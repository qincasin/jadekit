//! Hermes Coordinator —— 确定性的单一轮询循环，驱动整个引擎。
//!
//! 设计来源：orca `runtime/orchestration/coordinator.ts`（line 1 注释解释了为何
//! 把消息处理 / 任务派发 / 网关 / 升级 / 收敛判定全部塞进一个类——避免 split-brain：
//! 轮询循环必须能在所有这些关注点上做出原子决策）。
//!
//! # 与 orca 的关键差异
//! - **无 LLM**：Phase 2c 的 Coordinator 是确定性的；Planner 在 Phase 2e 才注入 LLM。
//! - **介质抽象**：orca 的 `CoordinatorRuntime` 是 terminal/PTY；Hermes 抽象为
//!   [`AgentRuntime`](crate::hermes::runtime::AgentRuntime) trait，引擎只认这个。
//! - **worker_done 由 watcher 写入**：派发后 spawn 一个 watcher 任务排空事件流，
//!   `Done{success:true}` → 写 `worker_done` 消息；`Failed{error}` → `fail_dispatch` +
//!   熔断级联（Task 6 把这条 task 侧级联显式 deferred 给 Coordinator，这里 own 它）。
//!
//! # 确定性测试方法（关键 —— 不允许 flaky sleep）
//! 测试用 [`MockRuntime`]：其 `send` 在返回 receiver **之前**就把编程好的事件
//! 全部压入 `mpsc::unbounded_channel`，然后立刻 drop sender。这样 watcher 排空
//! 事件流时所有事件都已 buffer 完毕、sender 已关闭，watcher 收完即退出——
//! 下一轮 `tick` 看到的 inbox 必然已就绪。无需任何 `time::sleep`。

#![allow(dead_code)] // Task 9 是增量；lib 非 test 构建暂无消费者。

use crate::chat::WorktreeManager;
use crate::hermes::runtime::{AgentEvent, AgentRuntime, RuntimeStartSpec};
use crate::hermes::store::{InboxFilter, Store, TaskListFilter};
use crate::hermes::types::{
    DispatchContext, DispatchStatus, Message, MessageType, RunStatus, Task, TaskStatus,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

// =============================================================================
// 常量（不写魔法串；循环参数集中定义）
// =============================================================================

/// Coordinator 自身固定的 inbox handle（消息路由用）。整个引擎往这个地址投递
/// `worker_done` / `escalation` / `merge_ready` 等消息。
const COORDINATOR_HANDLE: &str = "coordinator";

/// 默认心跳超时阈值（秒）：派发上下文 `last_heartbeat_at` 超过此阈值未刷新，
/// 视为 stale，tick 阶段 ① 会 `fail_dispatch`（让熔断器递增）。
/// 取 120s —— 比常规心跳间隔（~30s）宽裕，避免误杀慢但健康的 worker。
const STALE_DISPATCH_THRESHOLD_SECS: i64 = 120;

/// `run()` 循环的最大迭代次数安全阀——防止卡死循环跑 forever（也保证测试必终止）。
const RUN_MAX_ITERATIONS: u32 = 1000;

/// 默认并发上限（对齐 orca `MAX_CONCURRENT_DEFAULT`）。
const MAX_CONCURRENT_DEFAULT: usize = 4;

/// 默认 poll 间隔（毫秒），对齐 orca `DEFAULT_POLL_MS`。
const DEFAULT_POLL_MS: u64 = 2000;

/// 默认模型 / provider（task.assignment 缺省时使用）。
const DEFAULT_MODEL: &str = "sonnet";
const DEFAULT_PROVIDER: &str = "claude";

// =============================================================================
// TickOutcome
// =============================================================================

/// 单轮 tick 的结果摘要，便于断言与日志。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TickOutcome {
    /// 本轮新派发的任务数。
    pub dispatched: usize,
    /// 本轮通过 worker_done 标记为 Completed 的任务数。
    pub completed: usize,
    /// 本轮因失败被标记为 Failed 的任务数（含熔断）。
    pub failed: usize,
    /// 是否已收敛：所有任务终态 + 无活跃派发。
    pub converged: bool,
}

// =============================================================================
// Coordinator
// =============================================================================

/// 确定性的编排循环。一个 Coordinator 实例对应一次 [`CoordinatorRun`]。
///
/// 不变量：
/// - 所有状态读写都过 [`Store`]（SQLite + Mutex），Coordinator 自身无业务状态字段
///   （YAGNI——Task 9 不维护 in-memory `completed_tasks` 等；都以 Store 为权威）。
/// - `tick` 是幂等的：重复调用得到等价结果（除非外部并发改了 Store）。
pub struct Coordinator {
    store: Store,
    runtime: Arc<dyn AgentRuntime>,
    repo_root: PathBuf,
    /// 每个 worker agent 一个独立 git worktree 的根目录（如 `repo_root/.helm/worktrees`）。
    worktrees_dir: PathBuf,
    max_concurrent: usize,
    /// 测试仪器：可选的「活跃派发并发采样器」。
    /// 派发时 current +1 并把 current 推进 peak；watcher 退出时 current -1。
    /// 生产代码留 `None`（零成本）；并发上限测试通过 [`Self::with_concurrency_sampler`] 注入。
    active_counter: Option<Arc<ConcurrencySampler>>,
}

impl Coordinator {
    pub fn new(
        store: Store,
        runtime: Arc<dyn AgentRuntime>,
        repo_root: PathBuf,
        worktrees_dir: PathBuf,
    ) -> Self {
        Self {
            store,
            runtime,
            repo_root,
            worktrees_dir,
            max_concurrent: MAX_CONCURRENT_DEFAULT,
            active_counter: None,
        }
    }

    /// 设置并发上限（测试 / 配置覆盖用）。
    #[allow(dead_code)]
    pub fn with_max_concurrent(mut self, n: usize) -> Self {
        self.max_concurrent = n;
        self
    }

    /// 注入并发采样器（测试仪器）。派发时 current+1 & 推进 peak；watcher 退出 dec。
    /// 生产代码不调用。
    #[cfg(test)]
    pub fn with_concurrency_sampler(mut self, sampler: Arc<ConcurrencySampler>) -> Self {
        self.active_counter = Some(sampler);
        self
    }

    /// 单轮调度。阶段（顺序固定，确定性）：
    ///
    /// 1. **回收 stale 派发**：心跳超时 → `fail_dispatch`（让熔断递增）。
    /// 2. **排空 inbox**：`worker_done` → `update_task_status(Completed)`（Store 同事务
    ///    把下游 Pending 提升为 Ready）+ mark_read。`escalation`/`merge_ready` 当前
    ///    最小处理（log/ignore，Task 9 不展开，Task 10+ 完善）。
    /// 3. **网关**：列出 pending gates（Task 9 不自动 resolve，留给人工 / Planner）。
    /// 4. **派发 ready 任务**：`list_tasks(ready=true)` 按 created_at 取下一条；
    ///    只要还有空闲并发槽，就建 worktree + `runtime.start` + `runtime.send`，
    ///    然后 spawn 一个 watcher 排空事件流。
    /// 5. **熔断**：`fail_dispatch` 由 Store 完成 3 次累计 → CircuitBroken；
    ///    本 Coordinator 负责 task 侧级联：dispatch 进入 CircuitBroken 后立刻把
    ///    对应 task 标 Failed（Task 6 显式把这条 deferred 给 Coordinator）。
    /// 6. 返回 [`TickOutcome`]；`converged` 当所有任务为终态 + 无活跃派发。
    pub async fn tick(&self) -> Result<TickOutcome, String> {
        let mut outcome = TickOutcome::default();

        // ── 阶段 ①：回收 stale 派发（心跳超时 → fail_dispatch + 熔断级联） ──
        // Task 10：正式接入 Store::get_stale_dispatches。每条 stale 派发都走
        // fail_dispatch_with_cascade——熔断器递增；累计达阈值则 task 标 Failed。
        outcome.failed += self.reclaim_stale_dispatches().await?;

        // ── 阶段 ②：排空 inbox ────────────────────────────────────────────
        outcome.completed += self.drain_inbox().await?;

        // ── 阶段 ③：网关（最小处理） ──────────────────────────────────────
        // Task 9：仅列出 pending gates 触发的 blocked 一致性已在 Store 维护；
        // 此处不做自动 resolve（人类 / Planner 决定）。
        // 实现为空 hook，保留为后续 Phase 2e 接入点。

        // ── 阶段 ④：派发 ready 任务 ──────────────────────────────────────
        outcome.dispatched += self.dispatch_ready_tasks().await?;

        // ── 阶段 ⑤：熔断级联 ─────────────────────────────────────────────
        // fail_dispatch 的熔断（3 次 → CircuitBroken）由 watcher 在事件到来时
        // 即时触发（见 spawn_worker_watcher）；此处无额外扫描。

        // ── 阶段 ⑥：收敛判定 ─────────────────────────────────────────────
        outcome.converged = self.check_convergence()?;

        Ok(outcome)
    }

    /// 阶段 ① 的实现：列出心跳超时的 dispatched 上下文，逐条 `fail_dispatch`。
    ///
    /// Task 10：通过 [`Store::get_stale_dispatches`] 取回 stale 派发，对每条调用
    /// [`fail_dispatch_with_cascade`]——
    /// - 若熔断（3 次累计）→ task 标 Failed；
    /// - 否则 task 退回 Ready 等待下一轮重派。
    /// 返回本轮被回收（failed）的派发数。
    ///
    /// 阈值用常量 [`STALE_DISPATCH_THRESHOLD_SECS`]（120s），不写魔法串。
    async fn reclaim_stale_dispatches(&self) -> Result<usize, String> {
        let stale = self
            .store
            .get_stale_dispatches(STALE_DISPATCH_THRESHOLD_SECS as u64)?;
        if stale.is_empty() {
            return Ok(0);
        }
        let mut reaped = 0usize;
        for ctx in stale {
            // 复用 watcher 的级联逻辑：熔断阈值达 → task=Failed；否则 task=Ready 重派。
            let error = format!(
                "stale dispatch: heartbeat timeout (>{secs}s)",
                secs = STALE_DISPATCH_THRESHOLD_SECS
            );
            let _ = fail_dispatch_with_cascade(
                &self.store,
                &ctx.id,
                &ctx.task_id,
                &error,
            );
            reaped += 1;
        }
        Ok(reaped)
    }

    /// 阶段 ②：读取 `COORDINATOR_HANDLE` 的未读 inbox，按 sequence 顺序处理。
    /// 返回本轮通过 worker_done 标记为 Completed 的任务数（用于 TickOutcome）。
    async fn drain_inbox(&self) -> Result<usize, String> {
        let messages = self.store.list_inbox(
            COORDINATOR_HANDLE,
            InboxFilter { unread_only: true },
        )?;
        if messages.is_empty() {
            return Ok(0);
        }

        let mut completed = 0usize;
        let mut read_seqs: Vec<i64> = Vec::new();

        for msg in &messages {
            match msg.kind {
                MessageType::WorkerDone => {
                    // 解析 payload { taskId, dispatchId, result? }
                    let (task_id, result_text) = parse_worker_done_payload(&msg.payload);
                    if let Some(task_id) = task_id {
                        // Store 的 update_task_status(Completed) 会在同一事务把
                        // 下游 Pending → Ready。
                        let task = self.store.get_task(&task_id)?;
                        if let Some(t) = task {
                            // 仅当任务当前不是终态时才更新（避免重复 worker_done 幂等性问题）。
                            if !matches!(
                                t.status,
                                TaskStatus::Completed | TaskStatus::Failed
                            ) {
                                self.store.update_task_status(
                                    &task_id,
                                    TaskStatus::Completed,
                                    result_text.as_deref(),
                                )?;
                                completed += 1;
                            }
                        }
                    }
                }
                MessageType::Escalation | MessageType::MergeReady => {
                    // Task 9：最小处理（接受消息、标已读、不展开策略）。
                    // Task 10+ 由 Supervisor / Planner 处理 escalation；
                    // MergeReady 由合并子相位处理。
                }
                MessageType::Dispatch
                | MessageType::Handoff
                | MessageType::DecisionGate
                | MessageType::Status
                | MessageType::Heartbeat => {
                    // Task 9：这些类型不在 Coordinator 的处理范围（Heartbeat 由
                    // watcher 直接更新 last_heartbeat_at，DecisionGate 由
                    // Planner / UI 创建）。最小处理：标已读。
                }
            }
            read_seqs.push(msg.sequence as i64);
        }

        if !read_seqs.is_empty() {
            self.store.mark_read_by_ids(&read_seqs)?;
        }

        Ok(completed)
    }

    /// 阶段 ④：派发 ready 任务到空闲槽（≤ max_concurrent）。
    /// 返回本轮新派发的任务数。
    async fn dispatch_ready_tasks(&self) -> Result<usize, String> {
        let ready = self
            .store
            .list_tasks(TaskListFilter { ready: true, ..Default::default() })?;
        if ready.is_empty() {
            return Ok(0);
        }

        // 当前并发槽 = max_concurrent - 已 Dispatched 任务数。
        let dispatched_count = self
            .store
            .list_tasks(TaskListFilter {
                status: Some(TaskStatus::Dispatched),
                ..Default::default()
            })?
            .len();
        let mut slots = self.max_concurrent.saturating_sub(dispatched_count);

        let mut newly_dispatched = 0usize;
        for task in ready {
            if slots == 0 {
                break;
            }
            slots -= 1;
            self.dispatch_one(&task).await?;
            newly_dispatched += 1;
        }

        Ok(newly_dispatched)
    }

    /// 派发单个任务：建 worktree + runtime.start + runtime.send + spawn watcher。
    ///
    /// 派发顺序（必须严格）：
    /// 1. 先 `update_task_status(Dispatched)` + `create_dispatch(status=Dispatched,
    ///    assignee=agent_id)`——确保 Store 一致后才起 agent。
    /// 2. `WorktreeManager::create` 建独立 worktree（git 子进程，无 Tauri 依赖）。
    /// 3. `runtime.start(RuntimeStartSpec{ cwd: worktree_path, ... })` 获取 handle。
    /// 4. `runtime.send(handle, task.spec + preamble)` 拿到事件流 receiver。
    /// 5. `tokio::spawn(watcher)` 排空事件流：Done{success:true} → 写 worker_done
    ///    消息（下轮 tick drain_inbox 时完成 task）；Failed{error} → fail_dispatch
    ///    + 熔断级联（task → Failed）。
    async fn dispatch_one(&self, task: &Task) -> Result<(), String> {
        // —— 1. 状态前置：task → Dispatched ——
        self.store
            .update_task_status(&task.id, TaskStatus::Dispatched, None)?;

        // —— 2. 建 worktree（每 task 一个隔离 git worktree）——
        // 重派清理：若 task 此前派发过（worktree 路径或 helm/<task_id> 分支已存在），
        // 先 force-remove 旧 worktree + 删旧分支，再建新的。这样 3 次失败熔断的
        // 测试可以重派（每次重派都建一个干净的 worktree）。
        self.cleanup_prior_worktree(&task.id);
        let worktree_info = WorktreeManager::create(&self.repo_root, &self.worktrees_dir, &task.id)
            .map_err(|e| format!("WorktreeManager::create({}) failed: {e}", task.id))?;

        // —— 3. runtime.start ——
        let (model, provider) = match &task.assignment {
            Some(a) => (a.model.clone(), a.tool.clone()),
            None => (DEFAULT_MODEL.to_string(), DEFAULT_PROVIDER.to_string()),
        };
        let start_spec = RuntimeStartSpec {
            agent_id: task.id.clone(),
            cwd: worktree_info.path.clone(),
            model,
            provider,
        };
        let handle = self
            .runtime
            .start(start_spec)
            .await
            .map_err(|e| format!("runtime.start({}) failed: {:?}", task.id, e))?;

        // —— 4. create_dispatch（status=Dispatched, assignee=agent_id）——
        // 关键：跨 dispatch carry-forward failure_count（对齐 orca
        // `createDispatchContext` 的 `MAX(failure_count)` 语义）。task 上次失败的
        // dispatch 已经累计过若干次失败；本次重派如果不带这个起点，3 次熔断
        // 永远到不了——每次新 dispatch 都从 0 开始。
        let carried_failures = self.store.latest_failure_count_for_task(&task.id)?;
        let dispatch_id = format!("disp_{}_{}", task.id, nanos_hex());
        let now = chrono::Utc::now().to_rfc3339();
        self.store.create_dispatch(DispatchContext {
            id: dispatch_id.clone(),
            task_id: task.id.clone(),
            assignee: Some(handle.agent_id.clone()),
            status: DispatchStatus::Dispatched,
            failure_count: carried_failures,
            last_heartbeat_at: Some(now.clone()),
            last_failure: None,
            dispatched_at: Some(now),
            completed_at: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        })?;

        // —— 5. runtime.send（task.spec + preamble）——
        let prompt = build_prompt(task, &dispatch_id);
        let mut rx = self.runtime.send(&handle, prompt).await.map_err(|e| {
            format!("runtime.send({}) failed: {:?}", task.id, e)
        })?;

        // —— 6. spawn watcher：排空事件流，把结果落回 Store ——
        //
        // 确定性来源：MockRuntime 在 send 返回前已把全部事件压入 channel 并 drop
        // sender；real SdkRuntime 也由它自己控制何时关闭 sender。watcher 收到
        // None（channel 关闭）即退出，不会无限挂起。
        let store = self.store_clone_for_watcher();
        let task_id = task.id.clone();
        let agent_id = handle.agent_id.clone();
        let dispatch_id_w = dispatch_id.clone();
        // 测试仪器：注入时 inc（同时推进 peak），watcher 退出时 dec。
        // 用 RAII guard 保证无论 watcher 如何结束（正常 None / panic）都 dec。
        let counter = self.active_counter.clone();
        if let Some(c) = &counter {
            c.on_dispatch();
        }
        tokio::spawn(async move {
            // RAII guard：watcher 退出时 dec 计数器（若注入）。
            let _guard = ActiveGuard(counter);
            while let Some(event) = rx.recv().await {
                match event {
                    AgentEvent::Done { success, .. } => {
                        if success {
                            // 写 worker_done：下轮 tick drain_inbox 会把 task 标 Completed。
                            let _ = write_worker_done(
                                &store,
                                &agent_id,
                                &task_id,
                                &dispatch_id_w,
                            );
                        } else {
                            // success=false：当作失败，递增熔断器。
                            let _ = fail_dispatch_with_cascade(
                                &store,
                                &dispatch_id_w,
                                &task_id,
                                "agent reported Done{success:false}",
                            );
                        }
                    }
                    AgentEvent::Failed { error } => {
                        let _ = fail_dispatch_with_cascade(
                            &store,
                            &dispatch_id_w,
                            &task_id,
                            &error,
                        );
                    }
                    // Task 9：TextDelta / ToolUse / ToolResult / Thinking / NeedsInput
                    // 等活动事件不改变状态机；heartbeat 刷新留给 Task 11 WorkerSupervisor。
                    _ => {}
                }
            }
        });

        Ok(())
    }

    /// 收敛判定：所有任务都为终态（Completed / Failed / Blocked）且无活跃派发。
    fn check_convergence(&self) -> Result<bool, String> {
        let all = self
            .store
            .list_tasks(TaskListFilter { ..Default::default() })?;
        if all.is_empty() {
            return Ok(true);
        }
        let all_terminal = all.iter().all(|t| {
            matches!(
                t.status,
                TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Blocked
            )
        });
        if !all_terminal {
            return Ok(false);
        }
        // 还要确认无活跃派发（理论上终态任务不会有 dispatched 上下文，但保守起见）。
        let active_dispatches = self
            .store
            .list_tasks(TaskListFilter {
                status: Some(TaskStatus::Dispatched),
                ..Default::default()
            })?
            .len();
        Ok(active_dispatches == 0)
    }

    /// 主循环：tick → 检查收敛 → 否则 sleep poll_interval → 重复。
    /// 带 `RUN_MAX_ITERATIONS` 安全阀，防止卡死循环跑 forever（也保证测试必终止）。
    pub async fn run(&self, run_id: &str) -> Result<RunStatus, String> {
        // 从 run 表读 poll_interval（如 run 不存在则用默认）。
        let poll_ms = self.poll_interval_for(run_id).unwrap_or(DEFAULT_POLL_MS);

        self.store.update_run(run_id, RunStatus::Running)?;

        for _ in 0..RUN_MAX_ITERATIONS {
            let outcome = self.tick().await?;
            if outcome.converged {
                let final_status = self.derive_final_status()?;
                self.store.update_run(run_id, final_status)?;
                return Ok(final_status);
            }
            tokio::time::sleep(std::time::Duration::from_millis(poll_ms)).await;
        }

        // 超过最大迭代仍未收敛——视为失败。
        self.store.update_run(run_id, RunStatus::Failed)?;
        Ok(RunStatus::Failed)
    }

    fn poll_interval_for(&self, run_id: &str) -> Option<u64> {
        // CoordinatorRun 表查找；找不到或出错均回退到默认。
        let active = self.store.get_active_run().ok()??;
        if active.id == run_id {
            Some(active.poll_interval_ms)
        } else {
            None
        }
    }

    fn derive_final_status(&self) -> Result<RunStatus, String> {
        let all = self
            .store
            .list_tasks(TaskListFilter { ..Default::default() })?;
        if all.iter().any(|t| t.status == TaskStatus::Failed) {
            return Ok(RunStatus::Failed);
        }
        Ok(RunStatus::Completed)
    }

    /// 给 watcher 用的 Store 句柄。当前 Store 内部是 `Arc<Mutex<Connection>>`，
    /// 但其公开 API 只暴露 `&self` 方法——为支持 spawn 'static future，需要
    /// 一个可克隆的句柄。当前实现：Store 持有内部 Arc，可安全克隆。
    fn store_clone_for_watcher(&self) -> Store {
        self.store.clone_handle()
    }

    /// 重派前清理旧 worktree：force-remove 工作树目录 + 删除 helm/<task_id> 分支。
    /// 失败一律忽略（最坏情况是 WorktreeManager::create 接着报错，由调用方处理）。
    fn cleanup_prior_worktree(&self, task_id: &str) {
        let expected_path = self.worktrees_dir.join(task_id);
        if expected_path.exists() {
            let _ = WorktreeManager::remove(&self.repo_root, &expected_path, true);
        }
        // 删除旧分支（helm/<task_id>），让 WorktreeManager::create 的 `-b` 能成功。
        // 用与 WorktreeManager 一致的 git 子进程风格；失败不阻塞。
        let branch = format!("{}{}", crate::chat::HELM_BRANCH_PREFIX, task_id);
        let _ = std::process::Command::new("git")
            .current_dir(&self.repo_root)
            .args(["branch", "-D", &branch])
            .output();
    }
}

// =============================================================================
// 辅助函数
// =============================================================================

/// 测试仪器：采样 Coordinator 同时活跃的派发数（活跃 = 已派发但 watcher 未退出）。
///
/// - `current`：当前活跃数。`on_dispatch` +1，watcher Drop 时 -1。
/// - `peak`：历史观测到的 `current` 最大值。`on_dispatch` +1 后用 `fetch_max` 推进。
///
/// 设计目标：让并发上限测试**确定性**断言「同时活跃派发 ≤ max_concurrent」，
/// 不依赖任何 `time::sleep` / 时序——纯原子操作。
pub struct ConcurrencySampler {
    current: AtomicU32,
    peak: AtomicU32,
}

impl ConcurrencySampler {
    pub fn new() -> Self {
        Self {
            current: AtomicU32::new(0),
            peak: AtomicU32::new(0),
        }
    }

    /// 派发时调用：current +1，并把新 current 推进 peak。
    pub fn on_dispatch(&self) {
        let now = self.current.fetch_add(1, Ordering::SeqCst) + 1;
        // fetch_max 是 stable since 1.45；用 CAS 循环兜底也无必要。
        self.peak.fetch_max(now, Ordering::SeqCst);
    }

    /// watcher 退出时调用：current -1。
    pub fn on_exit(&self) {
        self.current.fetch_sub(1, Ordering::SeqCst);
    }

    /// 测试读取历史观测到的并发峰值。
    pub fn peak(&self) -> u32 {
        self.peak.load(Ordering::SeqCst)
    }
}

/// watcher 的 RAII 计数器 guard：构造时持有 sampler，Drop 时调 on_exit。
/// 用于并发上限测试观测「同时活跃的派发数」。生产路径 `None` 是零成本。
struct ActiveGuard(Option<Arc<ConcurrencySampler>>);

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        if let Some(c) = self.0.as_ref() {
            c.on_exit();
        }
    }
}

/// 从 worker_done 消息 payload 中提取 (taskId, result?)。
/// payload 形如 `{"taskId":"...","dispatchId":"...","result":"..."}`。
fn parse_worker_done_payload(payload: &Option<String>) -> (Option<String>, Option<String>) {
    let Some(p) = payload else {
        return (None, None);
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(p) else {
        return (None, None);
    };
    let task_id = v.get("taskId").and_then(|s| s.as_str()).map(String::from);
    let result = v.get("result").map(|r| match r {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    });
    (task_id, result)
}

/// watcher 调用：写入一条 worker_done 消息到 Coordinator inbox。
fn write_worker_done(
    store: &Store,
    agent_id: &str,
    task_id: &str,
    dispatch_id: &str,
) -> Result<(), String> {
    let payload = serde_json::json!({
        "taskId": task_id,
        "dispatchId": dispatch_id,
        "result": null,
    })
    .to_string();
    let msg = Message {
        id: format!("msg_{}_{}", task_id, nanos_hex()),
        from: agent_id.to_string(),
        to: COORDINATOR_HANDLE.to_string(),
        subject: "Done".to_string(),
        body: String::new(),
        kind: MessageType::WorkerDone,
        priority: "normal".to_string(),
        thread_id: None,
        payload: Some(payload),
        read: false,
        sequence: 0, // Store 用 AUTOINCREMENT 覆盖
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    store.insert_message(msg)?;
    Ok(())
}

/// watcher 调用：失败一次派发 + 熔断级联。
///
/// 行为：
/// - dispatch 到 CircuitBroken（3 次累计）→ task 标 Failed（Task 6 把这条级联显式
///   deferred 给 Coordinator）。
/// - dispatch 未熔断（status=Failed，重试还有预算）→ task 退回 Ready，下一轮 tick
///   重派。重派时 [`Coordinator::dispatch_one`] 通过
///   [`Store::latest_failure_count_for_task`] 把历史 failure_count carry-forward
///   到新 dispatch（对齐 orca `createDispatchContext` 的 `MAX(failure_count)`
///   语义），所以跨多次 dispatch 的失败仍能累计到熔断阈值。
///
/// 返回是否触发了熔断。
fn fail_dispatch_with_cascade(
    store: &Store,
    dispatch_id: &str,
    task_id: &str,
    error: &str,
) -> Result<bool, String> {
    let updated = store.fail_dispatch(dispatch_id, error)?;
    let Some(ctx) = updated else {
        return Ok(false);
    };
    if ctx.status == DispatchStatus::CircuitBroken {
        // 熔断：把 task 标 Failed（Task 6 把这条级联显式 deferred 给 Coordinator）。
        store.update_task_status(task_id, TaskStatus::Failed, Some(error))?;
        return Ok(true);
    }
    // 未熔断：退回 Ready，下一轮重派（dispatch_one 会 carry-forward failure_count）。
    store.update_task_status(task_id, TaskStatus::Ready, None)?;
    Ok(false)
}

/// 构造派发给 agent 的 prompt（task.spec + 简短 preamble）。
/// Task 9 是确定性骨架——preamble 只含必要的回执路由信息；
/// 完整 preamble（drift / decision-gate / dev-mode）由 Task 11 WorkerSupervisor 注入。
fn build_prompt(task: &Task, dispatch_id: &str) -> String {
    format!(
        "--- TASK ---\n{spec}\n\n--- DISPATCH META ---\ntaskId: {tid}\ndispatchId: {did}\nreplyTo: {handle}\n---\n",
        spec = task.spec,
        tid = task.id,
        did = dispatch_id,
        handle = COORDINATOR_HANDLE,
    )
}

/// 简短纳秒级 hex（dispatch / message id 去重用）。与 store::uuid_v4_short 等价。
fn nanos_hex() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}")
}

// =============================================================================
// Tests —— TDD：先写失败测试（RED），再实现到 GREEN。确定性、无 flaky sleep。
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hermes::runtime::{
        AgentHandle, Liveness, RuntimeCapabilities, RuntimeError,
    };
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::process::Command;
    use tokio::sync::mpsc;

    // ── MockRuntime ───────────────────────────────────────────────────────
    //
    // 确定性来源：send() 在返回 receiver **之前**就把该 agent 编程好的全部事件
    // 压入 unbounded channel，然后立刻 drop sender。这样 watcher 在 recv() 上
    // 立刻能取到所有事件，收到 None（channel 关闭）即退出——下一轮 tick 看到
    // 的 inbox 必然已就绪，无需任何 time::sleep。

    /// 每个 agent_id 编程好的事件序列（按顺序发出）。
    type ProgrammedEvents = HashMap<String, Vec<AgentEvent>>;

    struct MockRuntime {
        events: std::sync::Mutex<ProgrammedEvents>,
    }

    impl MockRuntime {
        fn new() -> Self {
            Self {
                events: std::sync::Mutex::new(HashMap::new()),
            }
        }

        /// 编程某 agent 的事件流（按顺序消费）。
        /// 调用 `program("t1", vec![Done{success:true}])` 后，agent_id="t1" 的
        /// 第一次 send 会返回这些事件。
        fn program(&self, agent_id: &str, events: Vec<AgentEvent>) {
            self.events
                .lock()
                .unwrap()
                .insert(agent_id.to_string(), events);
        }
    }

    #[async_trait]
    impl AgentRuntime for MockRuntime {
        fn capabilities(&self) -> RuntimeCapabilities {
            RuntimeCapabilities {
                structured_events: true,
                supports_resume: false,
                supports_permission_prompt: false,
            }
        }

        async fn start(&self, spec: RuntimeStartSpec) -> Result<AgentHandle, RuntimeError> {
            Ok(AgentHandle {
                agent_id: spec.agent_id,
            })
        }

        async fn send(
            &self,
            handle: &AgentHandle,
            _prompt: String,
        ) -> Result<mpsc::UnboundedReceiver<AgentEvent>, RuntimeError> {
            let (tx, rx) = mpsc::unbounded_channel();
            // 关键：先把编程好的事件全部压入 channel，再 drop sender。
            // watcher 收到 None 即退出 —— 全程无 time::sleep。
            let mut map = self.events.lock().unwrap();
            if let Some(ev_list) = map.remove(&handle.agent_id) {
                for ev in ev_list {
                    let _ = tx.send(ev);
                }
            }
            drop(tx);
            Ok(rx)
        }

        async fn abort(&self, _handle: &AgentHandle) -> Result<(), RuntimeError> {
            Ok(())
        }

        async fn liveness(&self, _handle: &AgentHandle) -> Liveness {
            Liveness::Alive
        }

        async fn stop(&self, _handle: &AgentHandle) -> Result<(), RuntimeError> {
            Ok(())
        }
    }

    // ── 临时 git repo + worktrees dir 辅助（抄自 chat/worktree.rs tests）──

    fn git(dir: &std::path::Path, args: &[&str]) {
        let ok = Command::new("git")
            .current_dir(dir)
            .args(args)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {:?} failed", args);
    }

    fn init_repo(dir: &std::path::Path) {
        git(dir, &["init", "-q"]);
        git(dir, &["config", "user.email", "t@t.t"]);
        git(dir, &["config", "user.name", "t"]);
        std::fs::write(dir.join("README.md"), "hi").unwrap();
        git(dir, &["add", "."]);
        git(dir, &["commit", "-qm", "init"]);
    }

    /// 构造一个完整测试 fixture：temp repo + worktrees dir + Store（in-memory）+
    /// MockRuntime 包成 Arc<dyn AgentRuntime> + Coordinator。
    struct Fixture {
        _tmp: tempfile::TempDir,
        repo_root: PathBuf,
        worktrees_dir: PathBuf,
        store: Store,
        runtime: Arc<MockRuntime>,
    }

    impl Fixture {
        fn new() -> Self {
            let tmp = tempfile::tempdir().unwrap();
            let repo_root = tmp.path().join("repo");
            std::fs::create_dir_all(&repo_root).unwrap();
            init_repo(&repo_root);
            let worktrees_dir = tmp.path().join("worktrees");
            let store = Store::open_in_memory().unwrap();
            // 用 Arc<MockRuntime> 以便测试侧调 program()；同时 clone 成 trait object。
            let runtime = Arc::new(MockRuntime::new());
            Self {
                _tmp: tmp,
                repo_root,
                worktrees_dir,
                store,
                runtime,
            }
        }

        fn coordinator(&self) -> Coordinator {
            let runtime_clone: Arc<dyn AgentRuntime> = self.runtime.clone();
            Coordinator::new(
                self.store.clone_handle(),
                runtime_clone,
                self.repo_root.clone(),
                self.worktrees_dir.clone(),
            )
        }

        fn program(&self, agent_id: &str, events: Vec<AgentEvent>) {
            self.runtime.program(agent_id, events);
        }

        fn create_task(&self, id: &str, spec: &str, deps: Vec<&str>) {
            let task = Task {
                id: id.to_string(),
                parent_id: None,
                spec: spec.to_string(),
                status: TaskStatus::Pending, // create_task 会按 deps 推导覆盖
                deps: deps.into_iter().map(String::from).collect(),
                result: None,
                assignment: None,
                created_at: format!("2026-06-28T00:00:0{}Z", id.len() % 10),
                completed_at: None,
            };
            self.store.create_task(task).unwrap();
        }

        /// 让 watcher 任务有机会被 executor 调度执行。
        /// 由于 MockRuntime 的 send 已把全部事件压入 channel 并 drop sender，
        /// watcher 的 recv() 立即返回事件然后返回 None；yield_now 让 tokio
        /// 把 watcher 跑完。 bounded 循环保证不会无限挂起。
        async fn yield_for_watchers(&self) {
            for _ in 0..16 {
                tokio::task::yield_now().await;
            }
        }
    }

    // ── 用例 1：单任务 → Done → Completed ─────────────────────────────────

    #[tokio::test]
    async fn single_task_done_completes() {
        let fx = Fixture::new();
        fx.create_task("t1", "implement feature A", vec![]);
        // 编程：t1 一开始就 emit Done{success:true}
        fx.program(
            "t1",
            vec![AgentEvent::Done {
                success: true,
                files_modified: vec![],
            }],
        );

        let _run = fx
            .store
            .create_run("do t1", COORDINATOR_HANDLE, 5)
            .unwrap();

        let coord = fx.coordinator();

        // 手动驱动 tick（确定性）：先 tick 派发，让 watcher 排空事件，再 tick 收敛。
        let o1 = coord.tick().await.unwrap();
        assert_eq!(o1.dispatched, 1, "第一轮应派发 1 个任务");
        // 让 watcher 写完 worker_done。
        fx.yield_for_watchers().await;

        let o2 = coord.tick().await.unwrap();
        assert_eq!(o2.completed, 1, "第二轮应完成 1 个任务");
        assert!(o2.converged, "应已收敛");

        let t1 = fx.store.get_task("t1").unwrap().unwrap();
        assert_eq!(
            t1.status,
            TaskStatus::Completed,
            "t1 必须为 Completed（worker_done → update_task_status）"
        );
    }

    // ── 用例 2：两任务带依赖 → 拓扑顺序（A 先完成，B 才 ready） ──────────

    #[tokio::test]
    async fn dependency_dag_dispatches_in_topological_order() {
        let fx = Fixture::new();
        fx.create_task("A", "first", vec![]);
        fx.create_task("B", "second", vec!["A"]);
        fx.program(
            "A",
            vec![AgentEvent::Done {
                success: true,
                files_modified: vec![],
            }],
        );
        fx.program(
            "B",
            vec![AgentEvent::Done {
                success: true,
                files_modified: vec![],
            }],
        );

        // 初始：B 必须 pending（依赖未完成）。
        let b0 = fx.store.get_task("B").unwrap().unwrap();
        assert_eq!(b0.status, TaskStatus::Pending, "B 初始应 Pending");

        let coord = fx.coordinator();

        // 第 1 轮：只有 A ready；B 仍 Pending。A 被派发。
        let o1 = coord.tick().await.unwrap();
        assert_eq!(o1.dispatched, 1, "第 1 轮应只派发 A");
        let a_after_1 = fx.store.get_task("A").unwrap().unwrap();
        assert_eq!(a_after_1.status, TaskStatus::Dispatched, "A 应已 Dispatched");
        let b_after_1 = fx.store.get_task("B").unwrap().unwrap();
        assert_eq!(
            b_after_1.status,
            TaskStatus::Pending,
            "B 必须仍 Pending（依赖 A 未完成）"
        );

        // 让 A 的 watcher 完成（写 worker_done）。
        fx.yield_for_watchers().await;

        // 第 2 轮：drain worker_done → A → Completed（Store 同事务把 B 提升 Ready）。
        //         此时 B ready，立即被派发。
        let o2 = coord.tick().await.unwrap();
        assert_eq!(o2.completed, 1, "第 2 轮应完成 A");
        let a_after_2 = fx.store.get_task("A").unwrap().unwrap();
        assert_eq!(a_after_2.status, TaskStatus::Completed, "A 应已 Completed");
        let b_after_2 = fx.store.get_task("B").unwrap().unwrap();
        assert_eq!(
            b_after_2.status,
            TaskStatus::Dispatched,
            "B 应已 Ready 并被派发（同轮）"
        );

        // 让 B 的 watcher 完成。
        fx.yield_for_watchers().await;

        // 第 3 轮：drain B 的 worker_done → B Completed → 收敛。
        let o3 = coord.tick().await.unwrap();
        assert_eq!(o3.completed, 1, "第 3 轮应完成 B");
        assert!(o3.converged, "应已收敛");
        let b_after_3 = fx.store.get_task("B").unwrap().unwrap();
        assert_eq!(b_after_3.status, TaskStatus::Completed);
    }

    // ── 用例 3：连续 3 次 Failed → CircuitBroken + task Failed + 不再派发 ─

    #[tokio::test]
    async fn three_failures_break_circuit_and_mark_task_failed() {
        let fx = Fixture::new();
        fx.create_task("F", "risky work", vec![]);

        // 编程：F 的 watcher 会在一次 send 中收到 3 次 Failed。
        // 但每个 dispatch 只对应一次 send —— 所以需要 3 个 dispatch 各失败一次。
        // 由于派发用 agent_id == task.id，且 MockRuntime 每次 send 消费一个编程条目，
        // 我们要为同一个 agent_id 准备多次 send 的数据。
        // 改设计：让 program() 在被消费后再次 send 时返回下一批。
        //
        // 更简单的做法：编程 agent "F" 返回 1 个 Failed，让 Coordinator 派发 3 次。
        // 但 task 一旦 Dispatched 就不会再被 list_tasks(ready) 选中——需要让失败后
        // 任务回退到 Ready（fail_dispatch 在未熔断时不应阻止重派）。
        //
        // 这暴露了一个设计点：watcher 在 fail_dispatch 未熔断时应把 task 退回 Ready，
        // 让下一轮 tick 重派。Task 6 的 fail_dispatch 不动 task 表（显式留给
        // Coordinator）——所以这条退回逻辑要在 fail_dispatch_with_cascade 里实现。
        //
        // 为保持本测试聚焦熔断行为，编程 3 个独立的 dispatch 失败：
        // 由于 task.id 固定，但每轮 send 会从 program map 里 drain（remove），
        // 我们需要在 watcher 完成后重新 program。
        fx.program(
            "F",
            vec![AgentEvent::Failed {
                error: "boom-1".to_string(),
            }],
        );

        let coord = fx.coordinator();

        // 第 1 轮：派发 F → watcher 失败 1 次（未熔断，task 退回 Ready）。
        let o1 = coord.tick().await.unwrap();
        assert_eq!(o1.dispatched, 1, "第 1 轮派发 F");
        fx.yield_for_watchers().await;
        let f_after_1 = fx.store.get_task("F").unwrap().unwrap();
        assert_eq!(
            f_after_1.status,
            TaskStatus::Ready,
            "未熔断时失败后 task 应回退 Ready（待重派）"
        );

        // 第 2 轮：再次派发 F（重新 program）→ 失败 2 次。
        fx.program(
            "F",
            vec![AgentEvent::Failed {
                error: "boom-2".to_string(),
            }],
        );
        let o2 = coord.tick().await.unwrap();
        assert_eq!(o2.dispatched, 1, "第 2 轮重派 F");
        fx.yield_for_watchers().await;

        // 第 3 轮：再次派发 F → 失败 3 次 → 熔断。
        fx.program(
            "F",
            vec![AgentEvent::Failed {
                error: "boom-3".to_string(),
            }],
        );
        let o3 = coord.tick().await.unwrap();
        assert_eq!(o3.dispatched, 1, "第 3 轮重派 F（即将熔断）");
        fx.yield_for_watchers().await;

        // 此时熔断应已触发；task 应为 Failed。
        let f_final = fx.store.get_task("F").unwrap().unwrap();
        assert_eq!(
            f_final.status,
            TaskStatus::Failed,
            "3 次失败后 task 必须 Failed（熔断级联）"
        );

        // 找到该 task 的最新 dispatch，确认是 CircuitBroken。
        // Store 当前无 list_dispatches_by_task，但我们可以查 ready 列表：
        // 熔断后 task=Failed，list_tasks(ready=true) 必须不返回它。
        let ready = fx
            .store
            .list_tasks(TaskListFilter { ready: true, ..Default::default() })
            .unwrap();
        assert!(
            !ready.iter().any(|t| t.id == "F"),
            "Failed 任务绝不能再出现在 ready 列表（不再派发）"
        );

        // 再跑一轮 tick：不应派发任何东西；应已收敛。
        let o4 = coord.tick().await.unwrap();
        assert_eq!(o4.dispatched, 0, "Failed 任务不能再被派发");
        assert!(o4.converged, "应已收敛");
    }

    // ── 辅助单测：parse_worker_done_payload ────────────────────────────────

    #[test]
    fn parse_worker_done_payload_extracts_task_id_and_result() {
        let payload = Some(
            serde_json::json!({
                "taskId": "t9",
                "dispatchId": "disp_t9_abc",
                "result": "all good"
            })
            .to_string(),
        );
        let (task_id, result) = parse_worker_done_payload(&payload);
        assert_eq!(task_id.as_deref(), Some("t9"));
        assert_eq!(result.as_deref(), Some("all good"));
    }

    #[test]
    fn parse_worker_done_payload_handles_missing_payload() {
        let (task_id, result) = parse_worker_done_payload(&None);
        assert!(task_id.is_none());
        assert!(result.is_none());
    }

    #[test]
    fn parse_worker_done_payload_handles_missing_result_field() {
        let payload = Some(
            serde_json::json!({
                "taskId": "t9",
                "dispatchId": "disp_t9_abc",
            })
            .to_string(),
        );
        let (task_id, result) = parse_worker_done_payload(&payload);
        assert_eq!(task_id.as_deref(), Some("t9"));
        assert!(result.is_none(), "缺 result 字段 → None（task 仍 Completed）");
    }

    // ── 用例 4：并行波次 —— 同时活跃派发 ≤ max_concurrent，且全部最终 Completed ─
    //
    // 并发上限是 Coordinator 的关键不变量。此测试构造 5 个相互独立的 ready 任务，
    // 把 max_concurrent=2，注入 ConcurrencySampler；驱动 tick → yield → tick 直到收敛，
    // 断言 sampler.peak() ≤ 2 且 5 个任务全部 Completed。
    //
    // 确定性来源：
    // - MockRuntime 预加载事件 + drop sender → watcher 排空即退出，无 sleep。
    // - 并发计数通过原子操作捕获 peak，与调度时序无关。
    #[tokio::test]
    async fn parallel_dispatch_waves_respect_max_concurrent() {
        let fx = Fixture::new();
        // 5 个独立 ready 任务（无依赖），全部会 emit Done{success:true}。
        for i in 1..=5 {
            let id = format!("p{i}");
            fx.create_task(&id, &format!("parallel work {i}"), vec![]);
            fx.program(
                &id,
                vec![AgentEvent::Done {
                    success: true,
                    files_modified: vec![],
                }],
            );
        }

        let sampler = Arc::new(ConcurrencySampler::new());
        let coord = fx
            .coordinator()
            .with_max_concurrent(2)
            .with_concurrency_sampler(sampler.clone());

        // 驱动循环：tick → yield → tick → ... 直到收敛。安全上限防卡死。
        let mut converged = false;
        for _ in 0..20 {
            let o = coord.tick().await.unwrap();
            fx.yield_for_watchers().await;
            if o.converged {
                converged = true;
                break;
            }
        }
        assert!(converged, "20 轮内必须收敛");

        // 关键不变量：任意时刻同时活跃派发 ≤ max_concurrent(=2)。
        let peak = sampler.peak();
        assert!(
            peak <= 2,
            "并发上限被违反：peak active = {peak}，max_concurrent = 2"
        );
        // 至少应该观测到过 2 个并发（5 个任务、max=2，必定有同时活跃窗口）。
        // 注：在单线程 mock runtime + yield_now 模型下，最坏只观测到 1（如果 dispatch_one
        // 严格串行 + watcher 在 yield 前已退出）。我们只断言「不超过」，不断言「至少」，
        // 因为后者依赖调度器细节而不够确定。

        // 全部 5 个任务必须最终 Completed。
        for i in 1..=5 {
            let id = format!("p{i}");
            let t = fx.store.get_task(&id).unwrap().unwrap();
            assert_eq!(
                t.status,
                TaskStatus::Completed,
                "任务 {id} 必须最终 Completed"
            );
        }
    }

    // ── 用例 5：形式收敛 —— N 个任务全 Done → run() 后 run=Completed ──────────
    //
    // 验证 `run()` 在 `check_convergence` 为 true 时把 run 状态置为 Completed 并退出。
    #[tokio::test]
    async fn run_marks_run_completed_on_full_convergence() {
        let fx = Fixture::new();
        // 3 个独立任务，全部成功完成。
        for i in 1..=3 {
            let id = format!("c{i}");
            fx.create_task(&id, &format!("conv work {i}"), vec![]);
            fx.program(
                &id,
                vec![AgentEvent::Done {
                    success: true,
                    files_modified: vec![],
                }],
            );
        }

        let run = fx
            .store
            .create_run("convergence run", COORDINATOR_HANDLE, 5)
            .unwrap();
        let coord = fx.coordinator().with_max_concurrent(4);

        let final_status = coord.run(&run.id).await.unwrap();

        // run 状态必须是 Completed（无 Failed 任务 → derive_final_status = Completed）。
        assert_eq!(
            final_status,
            RunStatus::Completed,
            "全部任务 Done 后 run 必须为 Completed"
        );

        // 从 Store 直接验证 run 行也被持久化为 Completed。
        let active = fx.store.get_active_run().unwrap();
        assert!(
            active.is_none(),
            "Completed run 不再是 active（status != Running）"
        );

        // 全部任务 Completed。
        for i in 1..=3 {
            let id = format!("c{i}");
            let t = fx.store.get_task(&id).unwrap().unwrap();
            assert_eq!(t.status, TaskStatus::Completed, "任务 {id} 必须 Completed");
        }
    }

    // ── 用例 6：stale-dispatch reaping —— 心跳超时派发被 fail_dispatch ─────────
    //
    // 构造一个已派发的任务，把它的 last_heartbeat_at 回拨到阈值之外，且 mock **不**
    // emit Done。tick 阶段 ① 应通过 Store::get_stale_dispatches 把它回收 →
    // fail_dispatch_with_cascade。断言 dispatch 不再是 Dispatched 状态。
    #[tokio::test]
    async fn stale_dispatch_is_reaped_via_heartbeat_timeout() {
        let fx = Fixture::new();
        fx.create_task("S", "work that stalls", vec![]);
        // 关键：不 program Done —— 这个 agent 假死。
        // （即使 program 为空，MockRuntime.send 仍返回一个立刻关闭的 receiver，
        // watcher 立刻 None 退出，不会写 worker_done。）

        let coord = fx.coordinator();

        // 第 1 轮：派发 S。watcher 会立刻收到 None 退出（无 worker_done）。
        let o1 = coord.tick().await.unwrap();
        assert_eq!(o1.dispatched, 1, "第 1 轮派发 S");
        fx.yield_for_watchers().await;

        // 找到刚创建的 dispatch，把它的心跳回拨到 STALE 阈值之外。
        // 阈值 = STALE_DISPATCH_THRESHOLD_SECS = 120s；回拨到 1 小时前。
        let stale_hb = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
        // 心跳未回拨前不应被视为 stale。
        let fresh = fx
            .store
            .get_stale_dispatches(STALE_DISPATCH_THRESHOLD_SECS as u64)
            .unwrap();
        assert!(fresh.is_empty(), "心跳未回拨：dispatch 不应被识别为 stale");

        // Coordinator 用 nanos_hex 自动生成 dispatch id（不可预测）；
        // 通过 Store 的测试 helper 按 task_id 定位当前 dispatched 上下文。
        let disp_id = fx.store.find_active_dispatch_id_for_test("S").unwrap();
        fx.store
            .set_dispatch_heartbeat_for_test(&disp_id, &stale_hb)
            .unwrap();

        // 现在 get_stale_dispatches 应命中。
        let stale = fx
            .store
            .get_stale_dispatches(STALE_DISPATCH_THRESHOLD_SECS as u64)
            .unwrap();
        assert_eq!(stale.len(), 1, "回拨心跳后应识别为 stale");
        assert_eq!(stale[0].id, disp_id);

        // 第 2 轮 tick：阶段 ① 应回收 stale dispatch。
        let o2 = coord.tick().await.unwrap();
        // reclaim_stale_dispatches 计入 outcome.failed（被回收的派发数）。
        assert!(o2.failed >= 1, "stale dispatch 应被回收（计入 failed）");

        // dispatch 不再是 Dispatched（应为 Failed —— 1 次失败未熔断）。
        let d = fx.store.get_dispatch(&disp_id).unwrap().unwrap();
        assert_ne!(
            d.status,
            DispatchStatus::Dispatched,
            "stale dispatch 不应仍为 Dispatched（已被回收）"
        );
        assert_eq!(
            d.status,
            DispatchStatus::Failed,
            "首次失败未熔断 → Failed（task 退回 Ready 等重派）"
        );
    }
}
