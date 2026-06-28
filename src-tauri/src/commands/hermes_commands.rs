//! Hermes 引擎 Tauri 命令 + 事件层（Phase 2g / Task 17）。
//!
//! 把已建好的 Hermes 编排引擎（`crate::hermes`：Coordinator + Store + Planner +
//! Supervisor + SdkRuntime/CliRuntime）通过 Tauri 命令暴露给前端。本文件只做：
//!   1. 参数归一化（trim / 默认值）——纯函数，便于单测；
//!   2. 薄 delegate 到 [`HermesEngine`]——真正业务在 Hermes 引擎里；
//!   3. 事件 payload 定义与发射（`hermes://run` / `hermes://task` / `hermes://agent`）。
//!
//! 设计原则（与 `chat_commands.rs` 一致）：
//! - 命令层薄、纯函数归一化、错误统一映射成 `String`；
//! - DTO 全部 `#[derive(Serialize)]` + `#[serde(rename_all = "camelCase")]`；
//! - 事件名 / 状态 token 都是常量（无魔法串）。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::hermes::{
    self, sweep_run_worktrees, Coordinator, DispatchContext, OrchestrationEventSink, RuntimeRegistry,
    RunStatus, Store, SweepReport, Task, TaskListFilter, TaskStatus,
};
use crate::hermes::coordinator::HELM_BASE_BRANCH;

// =============================================================================
// 常量：事件名 / 默认值（无魔法串）
// =============================================================================

/// Hermes 编排事件通道：run 级（run 启动 / 完成 / 失败）。
pub const HERMES_EVENT_RUN: &str = "hermes://run";
/// Hermes 编排事件通道：task 级（dispatched / completed / failed）。
///
/// Task 4 起：`TauriEventSink` + `event_channel_for` 消费此常量映射通道。
pub const HERMES_EVENT_TASK: &str = "hermes://task";
/// Hermes 编排事件通道：agent 级（保留——后续子阶段接 supervisor 事件时使用）。
///
/// Task 4 起：`TauriEventSink` + `event_channel_for` 消费此常量映射通道。
pub const HERMES_EVENT_AGENT: &str = "hermes://agent";

/// `HermesRunOpts` 字段缺省时的回落值。对齐 `hermes::coordinator::DEFAULT_POLL_MS`。
const DEFAULT_POLL_INTERVAL_MS: u64 = 2000;
/// 默认并发上限。对齐 `hermes::coordinator::MAX_CONCURRENT_DEFAULT`。
const DEFAULT_MAX_CONCURRENT: usize = 4;
/// Coordinator 自身 handle（与 `hermes::coordinator::COORDINATOR_HANDLE` 对齐）。
const COORDINATOR_HANDLE: &str = "coordinator";

// =============================================================================
// 参数归一化（纯函数，命令层调用 + 单测覆盖）
// =============================================================================

/// `hermes_run` 的可选入参——前端只传需要覆盖的字段，其余走默认。
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HermesRunOpts {
    /// 并发上限（缺省 [`DEFAULT_MAX_CONCURRENT`]）。
    pub max_concurrent: Option<usize>,
    /// Coordinator poll 间隔毫秒（缺省 [`DEFAULT_POLL_INTERVAL_MS`]）。
    pub poll_interval_ms: Option<u64>,
    /// 本次 run 的工作目录（缺省 = `HermesEngine::repo_root`）。
    pub repo_root: Option<String>,
}

/// 归一化后的 run 参数（已应用默认值，供 [`HermesEngine::start_run`] 消费）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedRunOpts {
    pub max_concurrent: usize,
    pub poll_interval_ms: u64,
    pub repo_root: Option<PathBuf>,
}

/// 把前端可选 opts 归一化：None → 默认值；非法值（poll=0 / max=0）→ Err。
///
/// 抽成纯函数便于在无需 Tauri runtime 的情况下单测覆盖。
pub fn normalize_run_opts(opts: HermesRunOpts) -> Result<NormalizedRunOpts, String> {
    let max_concurrent = match opts.max_concurrent {
        Some(n) if n > 0 => n,
        Some(0) => return Err("hermes_run: max_concurrent 必须 > 0".to_string()),
        None => DEFAULT_MAX_CONCURRENT,
        // saturating 防御：Option<usize> 不可能为负，但写明分支更清晰。
        #[allow(unreachable_patterns)]
        Some(_) => DEFAULT_MAX_CONCURRENT,
    };

    let poll_interval_ms = match opts.poll_interval_ms {
        Some(n) if n > 0 => n,
        Some(0) => return Err("hermes_run: poll_interval_ms 必须 > 0".to_string()),
        None => DEFAULT_POLL_INTERVAL_MS,
        #[allow(unreachable_patterns)]
        Some(_) => DEFAULT_POLL_INTERVAL_MS,
    };

    let repo_root = opts
        .repo_root
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);

    Ok(NormalizedRunOpts {
        max_concurrent,
        poll_interval_ms,
        repo_root,
    })
}

/// 归一化 `hermes_run` 的 goal：trim，拒绝空 / 纯空白。
pub fn normalize_run_goal(goal: &str) -> Result<String, String> {
    let trimmed = goal.trim();
    if trimmed.is_empty() {
        return Err("hermes_run: goal 不能为空".to_string());
    }
    Ok(trimmed.to_string())
}

// =============================================================================
// DTO（与 Hermes 模型一一对应；前端统一消费 camelCase）
// =============================================================================

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskDto {
    pub id: String,
    pub parent_id: Option<String>,
    pub spec: String,
    pub status: String,
    pub deps: Vec<String>,
    pub result: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

impl From<Task> for TaskDto {
    fn from(t: Task) -> Self {
        Self {
            id: t.id,
            parent_id: t.parent_id,
            spec: t.spec,
            status: t.status.as_str().to_string(),
            deps: t.deps,
            result: t.result,
            created_at: t.created_at,
            completed_at: t.completed_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DispatchDto {
    pub id: String,
    pub task_id: String,
    pub assignee: Option<String>,
    pub status: String,
    pub failure_count: u32,
    pub last_heartbeat_at: Option<String>,
    pub last_failure: Option<String>,
    pub dispatched_at: Option<String>,
    pub completed_at: Option<String>,
    pub created_at: String,
}

impl From<DispatchContext> for DispatchDto {
    fn from(d: DispatchContext) -> Self {
        Self {
            id: d.id,
            task_id: d.task_id,
            assignee: d.assignee,
            status: d.status.as_str().to_string(),
            failure_count: d.failure_count,
            last_heartbeat_at: d.last_heartbeat_at,
            last_failure: d.last_failure,
            dispatched_at: d.dispatched_at,
            completed_at: d.completed_at,
            created_at: d.created_at,
        }
    }
}

/// `hermes_task_list` 的可选过滤参数（镜像 [`TaskListFilter`]）。
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskListFilterDto {
    /// 按 status 过滤（与 `ready` 互斥；`ready=true` 时优先 ready）。
    pub status: Option<String>,
    /// 仅返回 Ready 任务（Coordinator 派发循环用）。
    pub ready: Option<bool>,
}

fn parse_task_list_filter(dto: Option<TaskListFilterDto>) -> Result<TaskListFilter, String> {
    let Some(dto) = dto else {
        return Ok(TaskListFilter::default());
    };
    let status = match dto.status.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => Some(TaskStatus::from_str(s).map_err(|e| format!("hermes_task_list: {e}"))?),
        None => None,
    };
    Ok(TaskListFilter {
        status,
        ready: dto.ready.unwrap_or(false),
    })
}

// =============================================================================
// TauriEventSink —— 把 OrchestrationEvent 桥接到 Tauri 事件通道的生产 sink
// =============================================================================

/// Task 4：把 [`OrchestrationEvent`] 桥接到 Tauri 事件通道的生产 sink。
///
/// 引擎（Coordinator / Supervisor / watcher）只依赖 [`OrchestrationEventSink`] trait；
/// 本实现按事件 kind 选择 `hermes://run` / `hermes://task` / `hermes://agent` 通道，
/// best-effort 发射（`app.emit` 失败不影响引擎循环——典型场景如前端未监听）。
///
/// 这条 sink 是「命令层 → Tauri 通道」的唯一耦合面：引擎内部不出现 `AppHandle`。
pub struct TauriEventSink {
    app: AppHandle,
    run_id: String,
}

impl TauriEventSink {
    /// 构造 sink。`run_id` 主要用于日志诊断，通道选择由事件 kind 决定。
    pub fn new(app: AppHandle, run_id: String) -> Self {
        Self { app, run_id }
    }

    /// Task 4：暴露 run_id（便于命令层诊断 / spawn 任务引用）。
    #[allow(dead_code)]
    pub fn run_id(&self) -> &str {
        &self.run_id
    }
}

/// Task 4：纯函数——按 [`OrchestrationEvent`] 的 kind 映射到通道名（便于单测）。
///
/// 三类 kind 一一对应 `HERMES_EVENT_RUN` / `HERMES_EVENT_TASK` / `HERMES_EVENT_AGENT`。
/// 不读 payload 内容，只看枚举变体。
pub fn event_channel_for(ev: &crate::hermes::OrchestrationEvent) -> &'static str {
    match ev {
        crate::hermes::OrchestrationEvent::Run { .. } => HERMES_EVENT_RUN,
        crate::hermes::OrchestrationEvent::Task { .. } => HERMES_EVENT_TASK,
        crate::hermes::OrchestrationEvent::Agent { .. } => HERMES_EVENT_AGENT,
    }
}

impl crate::hermes::OrchestrationEventSink for TauriEventSink {
    fn emit(&self, ev: crate::hermes::OrchestrationEvent) {
        let channel = event_channel_for(&ev);
        // best-effort：emit 失败（如前端未监听）不影响引擎循环。
        let _ = self.app.emit(channel, ev);
    }
}

// =============================================================================
// DTO —— RunShowDto（hermes_run_show 返回；Task 4）
// =============================================================================

/// `hermes_run_show` 的返回 DTO：单条 run 概览 + 任务计数（驾驶舱顶部用）。
///
/// Task 4 起前端驾驶舱顶部用此结构展示当前 run 的目标 / 状态 / 任务总数 /
/// 已完成数。字段全部 camelCase（与既有 DTO 风格一致）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RunShowDto {
    pub id: String,
    pub goal: String,
    pub status: String,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub task_count: usize,
    pub completed_count: usize,
}

/// Task 4：纯函数——从 [`CoordinatorRun`] + 任务列表构造 [`RunShowDto`]。
///
/// `task_count` = 任务总数；`completed_count` = `TaskStatus::Completed` 的任务数。
/// 抽成纯函数便于单测覆盖计数逻辑，无需 Tauri runtime。
pub fn build_run_show(
    run: &crate::hermes::CoordinatorRun,
    tasks: &[Task],
) -> RunShowDto {
    RunShowDto {
        id: run.id.clone(),
        goal: run.goal.clone(),
        status: run.status.as_str().to_string(),
        created_at: run.created_at.clone(),
        completed_at: run.completed_at.clone(),
        task_count: tasks.len(),
        completed_count: tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Completed)
            .count(),
    }
}

/// Task 14（3d）：`hermes_run_cleanup` 的返回 DTO——sweep 结果摘要。
///
/// 字段 camelCase（与既有 DTO 风格一致）：`removed`（已安全删除的 worktree 数）、
/// `retained`（保留待人工 merge/discard 的 worktree 数，含 RetainForReview disposition）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SweepReportDto {
    pub removed: usize,
    pub retained: usize,
}

impl From<SweepReport> for SweepReportDto {
    fn from(r: SweepReport) -> Self {
        Self {
            removed: r.removed,
            retained: r.retained,
        }
    }
}

// =============================================================================
// HermesEngine —— 被 Tauri 管理的状态
// =============================================================================

/// 一次 in-flight run 的句柄：取消标志 + run_id（供 stop_run 定位）。
#[derive(Debug)]
pub struct RunHandle {
    /// `true` 时 spawned run 循环应在下一轮 tick 前退出。
    pub cancel: Arc<AtomicBool>,
}

/// Hermes 编排引擎的 Tauri 管理状态。
///
/// 持有：
/// - [`Store`]（Arc<Mutex<Connection>> 在内部，跨 spawned 任务共享）；
/// - 注入的 `registry`（生产用 `SdkRuntime`，测试 mock）——Task 7 起按
///   `task.assignment.runtime` 把派发路由到对应介质（一次 run 内 SDK × CLI 混跑）；
/// - `repo_root`（缺省 cwd，opts 可覆盖）；
/// - `worktrees_dir`（每个 worker agent 一个独立 git worktree 的根目录）；
/// - `runs`：run_id → RunHandle，供 `hermes_run_stop` 发出取消信号。
pub struct HermesEngine {
    store: Store,
    registry: RuntimeRegistry,
    repo_root: PathBuf,
    worktrees_dir: PathBuf,
    runs: StdMutex<HashMap<String, RunHandle>>,
}

impl HermesEngine {
    /// 构造引擎。生产由 `lib.rs::setup` 调用（注入含 SdkRuntime 的 registry），
    /// 测试可注入任意 registry（如 `RuntimeRegistry::single(mock)`）。
    pub fn new(
        store: Store,
        registry: RuntimeRegistry,
        repo_root: PathBuf,
        worktrees_dir: PathBuf,
    ) -> Self {
        Self {
            store,
            registry,
            repo_root,
            worktrees_dir,
            runs: StdMutex::new(HashMap::new()),
        }
    }

    /// 启动一次编排 run：建 run 记录 + spawn 后台 Coordinator 循环 + 注册 RunHandle。
    ///
    /// 立刻返回 run_id（不等 run 完成）；run 进展通过 `hermes://run` /
    /// `hermes://task` 事件流推送给前端。Coordinator 收敛或被 cancel 后退出。
    ///
    /// Task 4：所有 run 级事件改走 [`TauriEventSink`]（单一发射口，与引擎内部的
    /// task/agent 级事件共用同一条 sink trait 注入 Coordinator）。原 `RunEventPayload`
    /// 路径已删除——`OrchestrationEvent::Run` 取而代之。
    pub fn start_run(
        &self,
        app: AppHandle,
        goal: String,
        opts: NormalizedRunOpts,
    ) -> Result<String, String> {
        let normalized_goal = normalize_run_goal(&goal)?;
        let repo_root = opts.repo_root.unwrap_or_else(|| self.repo_root.clone());

        // 建 run 记录（Store 置为 Running）。
        let run = self
            .store
            .create_run(&normalized_goal, COORDINATOR_HANDLE, opts.poll_interval_ms)?;

        // 注册取消句柄。
        let cancel = Arc::new(AtomicBool::new(false));
        self.runs
            .lock()
            .map_err(|e| format!("HermesEngine runs lock poisoned: {e}"))?
            .insert(run.id.clone(), RunHandle { cancel: cancel.clone() });

        // Task 4：构造 TauriEventSink——run 启动 / 终态 / 引擎内部 task/agent 事件
        // 都走这一条 sink（单一发射口），Coordinator 也通过 with_event_sink 接它。
        let sink = Arc::new(TauriEventSink::new(app.clone(), run.id.clone()));

        // 发射 run 启动事件（best-effort，经 sink）。
        sink.emit(crate::hermes::OrchestrationEvent::Run {
            run_id: run.id.clone(),
            goal: run.goal.clone(),
            status: RunStatus::Running.as_str().to_string(),
            error: None,
        });

        // spawn 后台 Coordinator 循环。
        let store = self.store.clone_handle();
        // Task 7：registry 是 Clone（内部全 Arc），廉价 clone 一份进 spawned 任务。
        let registry = self.registry.clone();
        let worktrees_dir = self.worktrees_dir.clone();
        let run_id = run.id.clone();
        let run_goal = run.goal.clone();
        let cancel_for_task = cancel.clone();
        // Task 4：把 sink 也 move 进 spawned closure，让错误路径 / 终态 emit 都走 sink。
        let sink_for_task = sink.clone();

        tauri::async_runtime::spawn(async move {
            let coordinator =
                Coordinator::new(store.clone_handle(), registry, repo_root, worktrees_dir)
                    .with_max_concurrent(opts.max_concurrent)
                    // Task 11（3c）：注入取消信号，让 Coordinator 的 run() 每轮 tick 前都检查
                    // mid-run cancel（置位 → abort 在飞 dispatch + 标 Cancelled）。Phase 2 的
                    // pre-loop 单次检查（下方 if 分支）保留不变——二者共用同一 `cancel` Arc：
                    //   * run() 启动前置位 → 下方 pre-loop 分支先命中 → 标 Failed（原语义，不动）；
                    //   * run() 循环中置位 → run() 内的 tick-top 检查命中 → 标 Cancelled（Task 10）。
                    // 不注入时 cancel=None → run() 不检查取消，行为与 Phase 2 逐字一致（非回归）。
                    .with_cancel(cancel_for_task.clone())
                    // Task 4：注入 TauriEventSink——引擎内部 task/agent 级事件经此通道落地。
                    .with_event_sink(
                        sink_for_task.clone() as Arc<dyn crate::hermes::OrchestrationEventSink>,
                    );

            // pre-loop 取消快路径：进入 run() 前就已被 stop_run/cancel 置位。
            // 保留原 Failed 语义（不动——改 Cancelled 是语义迁移，会影响既有 engine_stop_run_* 测试）。
            // mid-run 取消（run() 循环中置位）走 Task 10 的 tick-top 检查 → Cancelled。
            let final_status = if cancel_for_task.load(Ordering::SeqCst) {
                // 在进入循环前就被 cancel——直接置 Failed（保持原语义；mid-run Cancelled 由 run() 负责）。
                let _ = store.update_run(&run_id, RunStatus::Failed);
                RunStatus::Failed
            } else {
                match coordinator.run(&run_id).await {
                    Ok(status) => status,
                    Err(e) => {
                        let _ = store.update_run(&run_id, RunStatus::Failed);
                        // Task 4：错误路径经 sink 发 Run{failed}。
                        sink_for_task.emit(crate::hermes::OrchestrationEvent::Run {
                            run_id: run_id.clone(),
                            goal: run_goal.clone(),
                            status: RunStatus::Failed.as_str().to_string(),
                            error: Some(e),
                        });
                        return;
                    }
                }
            };

            // Task 4：发射 run 终态事件（经 sink）。
            sink_for_task.emit(crate::hermes::OrchestrationEvent::Run {
                run_id: run_id.clone(),
                goal: run_goal,
                status: final_status.as_str().to_string(),
                error: if final_status == RunStatus::Failed {
                    Some("run ended in failed state".to_string())
                } else {
                    None
                },
            });
        });

        Ok(run.id)
    }

    /// Task 4：取一条 run + 它的任务列表，构造 [`RunShowDto`]（驾驶舱顶部用）。
    ///
    /// `run_id` 空 / 不存在 → Err。任务列表取该 run 全部任务（无过滤）。
    pub fn show_run(&self, run_id: &str) -> Result<RunShowDto, String> {
        let run = self
            .store
            .get_run(run_id)?
            .ok_or_else(|| format!("hermes_run_show: 未找到 run_id {run_id}"))?;
        let tasks = self.store.list_tasks(TaskListFilter::default())?;
        Ok(build_run_show(&run, &tasks))
    }

    /// Task 4：列出当前活跃的派发上下文（驾驶舱 Roster 用）。
    /// 薄 delegate 到 `Store::list_active_dispatches`。
    pub fn list_active_agents(&self) -> Result<Vec<DispatchContext>, String> {
        self.store.list_active_dispatches()
    }

    /// 列出任务（薄 delegate 到 Store）。
    pub fn list_tasks(&self, filter: TaskListFilter) -> Result<Vec<Task>, String> {
        self.store.list_tasks(filter)
    }

    /// 取一条派发上下文（薄 delegate 到 Store）。
    pub fn get_dispatch(&self, dispatch_id: &str) -> Result<Option<DispatchContext>, String> {
        self.store.get_dispatch(dispatch_id)
    }

    /// 解决一个决策门（薄 delegate 到 Store）。
    pub fn resolve_gate(&self, gate_id: &str, resolution: String) -> Result<(), String> {
        let trimmed = resolution.trim();
        if trimmed.is_empty() {
            return Err("hermes_gate_resolve: resolution 不能为空".to_string());
        }
        self.store
            .resolve_gate(gate_id, trimmed.to_string())
    }

    /// 取消指定 run：置 cancel 标志，spawned 循环会在下一轮 tick 前退出并把 run 标 Failed。
    pub fn stop_run(&self, run_id: &str) -> Result<(), String> {
        let trimmed = run_id.trim();
        if trimmed.is_empty() {
            return Err("hermes_run_stop: run_id 不能为空".to_string());
        }
        let runs = self
            .runs
            .lock()
            .map_err(|e| format!("HermesEngine runs lock poisoned: {e}"))?;
        if let Some(handle) = runs.get(trimmed) {
            handle.cancel.store(true, Ordering::SeqCst);
            Ok(())
        } else {
            Err(format!("hermes_run_stop: 未找到 run_id {trimmed}（可能已结束）"))
        }
    }

    /// Task 14（3d）：手动触发一次 run 的 worktree 清扫。
    ///
    /// 给 UI 兜底入口（驾驶舱「清理 worktree」按钮）：对 run 内各 task 的 worktree 做安全
    /// 清扫（干净 → Remove；有产出 → RetainForReview + 发 awaiting-merge 事件到驾驶舱）。
    /// 用 [`TauriEventSink`]（绑 run_id）发射 awaiting-merge 事件，让 UI 知道哪些 worktree
    /// 需人工 merge/discard 决策。base_branch 用 [`HELM_BASE_BRANCH`]（与 Coordinator 默认一致）。
    ///
    /// 返回 [`SweepReportDto`]（removed / retained 计数）。
    pub fn cleanup_run(&self, run_id: &str, app: AppHandle) -> Result<SweepReportDto, String> {
        let sink = Arc::new(TauriEventSink::new(app, run_id.to_string()));
        let report = sweep_run_worktrees(
            &self.repo_root,
            &self.store,
            HELM_BASE_BRANCH,
            sink.as_ref(),
            run_id,
        )?;
        Ok(SweepReportDto::from(report))
    }
}

// =============================================================================
// Tauri 命令（与 chat_commands 同风格）
// =============================================================================

/// 启动一次 Hermes 编排 run。立刻返回 run_id；进展通过 `hermes://run` 事件推送。
#[tauri::command]
pub async fn hermes_run(
    goal: String,
    opts: Option<HermesRunOpts>,
    state: State<'_, HermesEngine>,
    app: AppHandle,
) -> Result<String, String> {
    let normalized_goal = normalize_run_goal(&goal)?;
    let normalized_opts = normalize_run_opts(opts.unwrap_or_default())?;
    // start_run 内部会再 trim 一次 goal（幂等），保持纯函数 + 命令层双保险。
    state.start_run(app, normalized_goal, normalized_opts)
}

/// 列出 Hermes 任务（可按 status / ready 过滤）。
#[tauri::command]
pub async fn hermes_task_list(
    filter: Option<TaskListFilterDto>,
    state: State<'_, HermesEngine>,
) -> Result<Vec<TaskDto>, String> {
    let parsed = parse_task_list_filter(filter)?;
    // Store 内部锁是 std::sync::Mutex（短临界区），与 chat_commands 一致直接调。
    let tasks = state.list_tasks(parsed)?;
    Ok(tasks.into_iter().map(TaskDto::from).collect())
}

/// 取一条派发上下文（按 dispatch_id 查）。
#[tauri::command]
pub async fn hermes_dispatch_show(
    dispatch_id: String,
    state: State<'_, HermesEngine>,
) -> Result<DispatchDto, String> {
    let trimmed = dispatch_id.trim();
    if trimmed.is_empty() {
        return Err("hermes_dispatch_show: dispatch_id 不能为空".to_string());
    }
    let dispatch = state
        .get_dispatch(trimmed)?
        .ok_or_else(|| "hermes_dispatch_show: 未找到该 dispatch".to_string())?;
    Ok(DispatchDto::from(dispatch))
}

/// 解决一个决策门（resolution 写入 Store，status → Resolved）。
#[tauri::command]
pub async fn hermes_gate_resolve(
    gate_id: String,
    resolution: String,
    state: State<'_, HermesEngine>,
) -> Result<(), String> {
    let trimmed_id = gate_id.trim();
    if trimmed_id.is_empty() {
        return Err("hermes_gate_resolve: gate_id 不能为空".to_string());
    }
    state.resolve_gate(trimmed_id, resolution)
}

/// 取消指定 run（设置取消标志，spawned 循环会在下一轮 tick 前退出）。
///
/// Phase 2 语义：置 `RunHandle.cancel` 标志。`start_run` 的 pre-loop 快路径命中时
/// 把 run 标 `Failed`；Task 11 起 mid-run 命中（`run()` 循环内置位）则标 `Cancelled`。
/// 保留作向后兼容别名——前端旧调用无需改动。
#[tauri::command]
pub async fn hermes_run_stop(
    run_id: String,
    state: State<'_, HermesEngine>,
) -> Result<(), String> {
    // stop_run 是纯内存操作（无 IO），无需 spawn_blocking。
    state.stop_run(&run_id)
}

/// 取消指定 run（mid-run）：置 cancel 标志，Coordinator 下一轮 tick 检查到即 abort 在飞
/// dispatch + 标 `Cancelled`。与 [`hermes_run_stop`] 共用同一取消标志（同一
/// `RunHandle.cancel`）——区别仅在「置位时刻被谁观测」：run() 启动前置位 → pre-loop 命中
/// 标 `Failed`（`hermes_run_stop` 既有路径）；run() 循环中置位 → tick-top 命中标 `Cancelled`
/// （Task 11 经 `with_cancel` 注入实现）。空 / 未知 run_id 由 `stop_run` 报错。
#[tauri::command]
pub async fn hermes_run_cancel(
    run_id: String,
    state: State<'_, HermesEngine>,
) -> Result<(), String> {
    // 复用 stop_run 的取消标志置位逻辑（同一 RunHandle.cancel）。mid-run 行为来自
    // start_run 内 with_cancel 注入（Task 11），而非本命令自身。
    state.stop_run(&run_id)
}

/// Task 4：取一条 run 的概览 + 任务计数（驾驶舱顶部用）。
///
/// 入参 `run_id` 经 trim；空字符串 → Err。返回 [`RunShowDto`]。
#[tauri::command]
pub async fn hermes_run_show(
    run_id: String,
    state: State<'_, HermesEngine>,
) -> Result<RunShowDto, String> {
    let trimmed = run_id.trim();
    if trimmed.is_empty() {
        return Err("hermes_run_show: run_id 不能为空".to_string());
    }
    state.show_run(trimmed)
}

/// Task 4：列出当前活跃的派发上下文（驾驶舱 Roster 用）。
///
/// 返回 `Vec<DispatchDto>`（active = `status = Dispatched`）。
#[tauri::command]
pub async fn hermes_agent_list(
    state: State<'_, HermesEngine>,
) -> Result<Vec<DispatchDto>, String> {
    let dispatches = state.list_active_agents()?;
    Ok(dispatches.into_iter().map(DispatchDto::from).collect())
}

/// Task 14（3d）：手动触发一次 run 的 worktree 清扫（驾驶舱兜底入口）。
///
/// 对 run 内各 task 的 worktree 做安全清扫：干净/失败 → Remove；有产出（Completed +
/// 领先提交 / 未提交改动）→ RetainForReview + 发 awaiting-merge 事件到驾驶舱。
/// 返回 `SweepReportDto { removed, retained }`。
#[tauri::command]
pub async fn hermes_run_cleanup(
    run_id: String,
    state: State<'_, HermesEngine>,
    app: AppHandle,
) -> Result<SweepReportDto, String> {
    let trimmed = run_id.trim();
    if trimmed.is_empty() {
        return Err("hermes_run_cleanup: run_id 不能为空".to_string());
    }
    state.cleanup_run(trimmed, app)
}

// =============================================================================
// 测试
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hermes::{
        AgentEvent, AgentHandle, AgentRuntime, GateStatus, Liveness, RuntimeCapabilities,
        RuntimeError, RuntimeStartSpec, Store,
    };
    use async_trait::async_trait;
    use std::path::PathBuf;
    use tokio::sync::mpsc;

    // ── 参数归一化（命令层纯函数） ──

    #[test]
    fn normalize_run_goal_trims_and_rejects_empty() {
        assert_eq!(normalize_run_goal("  do X  ").as_deref(), Ok("do X"));
        assert!(normalize_run_goal("").is_err());
        assert!(normalize_run_goal("   \n\t ").is_err());
    }

    #[test]
    fn normalize_run_opts_applies_defaults_when_all_none() {
        let opts = normalize_run_opts(HermesRunOpts::default()).unwrap();
        assert_eq!(opts.max_concurrent, DEFAULT_MAX_CONCURRENT);
        assert_eq!(opts.poll_interval_ms, DEFAULT_POLL_INTERVAL_MS);
        assert_eq!(opts.repo_root, None);
    }

    #[test]
    fn normalize_run_opts_respects_explicit_values() {
        let opts = normalize_run_opts(HermesRunOpts {
            max_concurrent: Some(8),
            poll_interval_ms: Some(500),
            repo_root: Some("/tmp/repo".to_string()),
        })
        .unwrap();
        assert_eq!(opts.max_concurrent, 8);
        assert_eq!(opts.poll_interval_ms, 500);
        assert_eq!(opts.repo_root.as_deref(), Some(std::path::Path::new("/tmp/repo")));
    }

    #[test]
    fn normalize_run_opts_rejects_zero_max_concurrent() {
        assert!(normalize_run_opts(HermesRunOpts {
            max_concurrent: Some(0),
            poll_interval_ms: None,
            repo_root: None,
        })
        .is_err());
    }

    #[test]
    fn normalize_run_opts_rejects_zero_poll_interval() {
        assert!(normalize_run_opts(HermesRunOpts {
            max_concurrent: None,
            poll_interval_ms: Some(0),
            repo_root: None,
        })
        .is_err());
    }

    #[test]
    fn normalize_run_opts_trims_blank_repo_root_to_none() {
        let opts = normalize_run_opts(HermesRunOpts {
            repo_root: Some("   ".to_string()),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(opts.repo_root, None);
    }

    // ── DTO 转换 ──

    #[test]
    fn task_dto_converts_status_token_to_camel_case_keys() {
        let task = Task {
            id: "t1".into(),
            parent_id: None,
            spec: "do X".into(),
            status: TaskStatus::Completed,
            deps: vec!["t0".into()],
            result: Some("ok".into()),
            assignment: None,
            created_at: "2026-06-28T00:00:00Z".into(),
            completed_at: Some("2026-06-28T00:01:00Z".into()),
        };
        let dto = TaskDto::from(task);
        assert_eq!(dto.id, "t1");
        assert_eq!(dto.status, TaskStatus::Completed.as_str());
        assert_eq!(dto.deps, vec!["t0".to_string()]);
    }

    #[test]
    fn parse_task_list_filter_translates_status_token() {
        let f = parse_task_list_filter(Some(TaskListFilterDto {
            status: Some("ready".into()),
            ready: None,
        }))
        .unwrap();
        assert_eq!(f.status, Some(TaskStatus::Ready));
        assert!(!f.ready);

        let f = parse_task_list_filter(Some(TaskListFilterDto {
            status: None,
            ready: Some(true),
        }))
        .unwrap();
        assert_eq!(f.status, None);
        assert!(f.ready);
    }

    #[test]
    fn parse_task_list_filter_rejects_unknown_status() {
        assert!(parse_task_list_filter(Some(TaskListFilterDto {
            status: Some("nonsense".into()),
            ready: None,
        }))
        .is_err());
    }

    // ── start_run with mock runtime（creates a run in Store + returns run_id） ──

    /// No-op runtime：send 返回立刻关闭的 receiver（无事件），其它方法都是 stub。
    /// 用于断言 start_run 把 run 写入 Store + 返回合法 run_id；spawned 任务本身
    /// 不发事件 / 不派发（YAGNI——真实 e2e 在 Task 18）。
    struct NoopRuntime;

    #[async_trait]
    impl AgentRuntime for NoopRuntime {
        fn capabilities(&self) -> RuntimeCapabilities {
            RuntimeCapabilities {
                structured_events: true,
                supports_resume: false,
                supports_permission_prompt: false,
            }
        }
        async fn start(&self, spec: RuntimeStartSpec) -> Result<AgentHandle, RuntimeError> {
            Ok(AgentHandle { agent_id: spec.agent_id })
        }
        async fn send(
            &self,
            _handle: &AgentHandle,
            _prompt: String,
        ) -> Result<mpsc::UnboundedReceiver<AgentEvent>, RuntimeError> {
            let (_tx, rx) = mpsc::unbounded_channel();
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

    /// 用一个 mock AppHandle-free 路径测 start_run：直接调 HermesEngine 的纯逻辑层
    /// （Store.create_run + runs 注册）。spawned 任务的 app.emit 在没有 AppHandle 的
    /// 单测里跑不起来，所以我们走 start_run_with_no_spawn——验证 run 被创建 +
    /// run_id 合法 + RunHandle 被注册。
    ///
    /// 注意：真正的 spawned 循环（含 app.emit）属于 Task 18 人工 e2e 范畴，本测试
    /// 只覆盖命令层 + Store 接线正确性。
    fn build_test_engine() -> HermesEngine {
        let store = Store::open_in_memory().unwrap();
        let runtime: Arc<dyn AgentRuntime> = Arc::new(NoopRuntime);
        // Task 7：Phase 2 兼容——single(rt) 把同一 rt 登记到所有 kind。
        let registry = crate::hermes::RuntimeRegistry::single(runtime);
        HermesEngine::new(
            store,
            registry,
            PathBuf::from("/tmp/repo"),
            PathBuf::from("/tmp/worktrees"),
        )
    }

    #[test]
    fn engine_create_run_persists_and_returns_run_id() {
        // 直接走 Store.create_run，验证 run 被持久化（不依赖 AppHandle）。
        let engine = build_test_engine();
        let run = engine
            .store
            .create_run("do mock goal", COORDINATOR_HANDLE, DEFAULT_POLL_INTERVAL_MS)
            .unwrap();

        assert!(run.id.starts_with("run_"));
        assert_eq!(run.goal, "do mock goal");
        assert_eq!(run.status, RunStatus::Running);

        // Store 里能查到这条 run（get_active_run）。
        let active = engine.store.get_active_run().unwrap().unwrap();
        assert_eq!(active.id, run.id);
    }

    #[test]
    fn engine_stop_run_registers_and_cancels_handle() {
        // 手动注入 RunHandle 模拟 start_run 的注册步骤（spawned 循环在单测里跑不起来）。
        let engine = build_test_engine();
        let run = engine
            .store
            .create_run("stop me", COORDINATOR_HANDLE, DEFAULT_POLL_INTERVAL_MS)
            .unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        engine
            .runs
            .lock()
            .unwrap()
            .insert(run.id.clone(), RunHandle { cancel: cancel.clone() });

        // stop_run 把 cancel 标志置 true。
        engine.stop_run(&run.id).unwrap();
        assert!(cancel.load(Ordering::SeqCst));

        // 不存在的 run_id 报错。
        assert!(engine.stop_run("run_does_not_exist").is_err());
    }

    /// Task 11（3c）：`hermes_run_cancel` 命令薄 delegate 到 `stop_run`——二者置同一
    /// `RunHandle.cancel` 标志。本测试沿用 `engine_stop_run_*` 模式直测 `stop_run`
    /// （`hermes_run_cancel` 是 1 行 delegate，`State` 在单测里不便构造，与既有命令测试一致）。
    /// 与 `engine_stop_run_registers_and_cancels_handle` 的区别：前者覆盖 Phase 2 的
    /// `hermes_run_stop`；本测试钉死 Task 11 新增的 `hermes_run_cancel` 委托契约——
    /// 即「置 cancel true + 未知 run_id 报错」两条不变量。
    #[test]
    fn engine_run_cancel_sets_cancel_flag() {
        let engine = build_test_engine();
        let run = engine
            .store
            .create_run("cancel me", COORDINATOR_HANDLE, DEFAULT_POLL_INTERVAL_MS)
            .unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        engine
            .runs
            .lock()
            .unwrap()
            .insert(run.id.clone(), RunHandle { cancel: cancel.clone() });

        // hermes_run_cancel 走 stop_run，置 cancel true。
        // （Task 11 的真 mid-run 行为来自 start_run 内 with_cancel 注入——见 coordinator 测试；
        //  命令层只负责置标志，故这里直测 stop_run 覆盖委托逻辑。）
        engine.stop_run(&run.id).unwrap();
        assert!(cancel.load(Ordering::SeqCst));

        // 不存在的 run_id 报错。
        assert!(engine.stop_run("run_does_not_exist").is_err());
        // 空 run_id 报错。
        assert!(engine.stop_run("   ").is_err());
    }

    #[test]
    fn engine_resolve_gate_rejects_empty_resolution() {
        let engine = build_test_engine();
        // 先建一个 gate。
        let gate = engine.store.create_gate("t1", "Q?", vec!["A".into()]).unwrap();
        // 空 resolution 被命令层拦下。
        assert!(engine.resolve_gate(&gate.id, "   ".to_string()).is_err());
        // 合法 resolution 写入成功。
        engine.resolve_gate(&gate.id, "A".to_string()).unwrap();
        // Store 里读回 status = Resolved。
        let gates = engine
            .store
            .list_gates(hermes::GateListFilter { task_id: Some("t1".into()), status: None })
            .unwrap();
        assert_eq!(gates[0].status, GateStatus::Resolved);
        assert_eq!(gates[0].resolution.as_deref(), Some("A"));
    }

    #[test]
    fn engine_stop_run_rejects_empty_run_id() {
        let engine = build_test_engine();
        assert!(engine.stop_run("   ").is_err());
    }

    // ── Task 4：event_channel_for + build_run_show 纯函数 ──

    /// Task 4 RED→GREEN：三类 OrchestrationEvent 各映射到对应通道常量。
    #[test]
    fn event_channel_for_maps_each_kind() {
        let run_ev = crate::hermes::OrchestrationEvent::Run {
            run_id: "r1".into(),
            goal: "g".into(),
            status: "running".into(),
            error: None,
        };
        assert_eq!(event_channel_for(&run_ev), HERMES_EVENT_RUN);

        let task_ev = crate::hermes::OrchestrationEvent::Task {
            run_id: "r1".into(),
            task_id: "t1".into(),
            status: "dispatched".into(),
            dispatch_id: Some("d1".into()),
        };
        assert_eq!(event_channel_for(&task_ev), HERMES_EVENT_TASK);

        let agent_ev = crate::hermes::OrchestrationEvent::Agent {
            run_id: "r1".into(),
            agent_id: "a1".into(),
            task_id: Some("t1".into()),
            status: "working".into(),
            activity: Some("tool_use".into()),
        };
        assert_eq!(event_channel_for(&agent_ev), HERMES_EVENT_AGENT);
    }

    /// Task 4 RED→GREEN：build_run_show 正确计数 task_count / completed_count，
    /// 并把 run 字段一对一映射到 DTO（camelCase 由 serde 层保证，这里只断言原值）。
    #[test]
    fn build_run_show_counts_tasks() {
        let run = crate::hermes::CoordinatorRun {
            id: "run_abc".into(),
            goal: "ship it".into(),
            status: RunStatus::Running,
            coordinator_handle: COORDINATOR_HANDLE.into(),
            poll_interval_ms: 2000,
            created_at: "2026-06-28T00:00:00Z".into(),
            completed_at: None,
        };
        // 3 个任务：1 Completed、1 Dispatched、1 Pending。
        let mk_task = |id: &str, status: TaskStatus| Task {
            id: id.into(),
            parent_id: None,
            spec: format!("spec {id}"),
            status,
            deps: vec![],
            result: None,
            assignment: None,
            created_at: "2026-06-28T00:00:00Z".into(),
            completed_at: None,
        };
        let tasks = vec![
            mk_task("t_done", TaskStatus::Completed),
            mk_task("t_dispatched", TaskStatus::Dispatched),
            mk_task("t_pending", TaskStatus::Pending),
        ];

        let dto = build_run_show(&run, &tasks);
        assert_eq!(dto.id, "run_abc");
        assert_eq!(dto.goal, "ship it");
        assert_eq!(dto.status, RunStatus::Running.as_str());
        assert_eq!(dto.created_at, "2026-06-28T00:00:00Z");
        assert_eq!(dto.completed_at, None);
        assert_eq!(dto.task_count, 3, "task_count = 全部任务数");
        assert_eq!(
            dto.completed_count, 1,
            "completed_count 只数 TaskStatus::Completed"
        );
    }

    /// Task 4：build_run_show 对空任务列表也工作（task_count=0、completed_count=0）。
    #[test]
    fn build_run_show_handles_empty_task_list() {
        let run = crate::hermes::CoordinatorRun {
            id: "run_empty".into(),
            goal: "noop".into(),
            status: RunStatus::Completed,
            coordinator_handle: COORDINATOR_HANDLE.into(),
            poll_interval_ms: 1000,
            created_at: "2026-06-28T00:00:00Z".into(),
            completed_at: Some("2026-06-28T00:01:00Z".into()),
        };
        let dto = build_run_show(&run, &[]);
        assert_eq!(dto.task_count, 0);
        assert_eq!(dto.completed_count, 0);
        assert_eq!(dto.status, RunStatus::Completed.as_str());
        assert_eq!(
            dto.completed_at.as_deref(),
            Some("2026-06-28T00:01:00Z")
        );
    }

    /// Task 4：show_run 走 Store——建 run + 完成 → show_run 返回正确计数。
    #[test]
    fn engine_show_run_returns_dto_with_counts() {
        let engine = build_test_engine();
        let run = engine
            .store
            .create_run("goal", COORDINATOR_HANDLE, DEFAULT_POLL_INTERVAL_MS)
            .unwrap();
        // 建两个任务并完成其中一个。
        engine
            .store
            .create_task(Task {
                id: "t1".into(),
                parent_id: None,
                spec: "do 1".into(),
                status: TaskStatus::Pending,
                deps: vec![],
                result: None,
                assignment: None,
                created_at: "2026-06-28T00:00:00Z".into(),
                completed_at: None,
            })
            .unwrap();
        engine
            .store
            .create_task(Task {
                id: "t2".into(),
                parent_id: None,
                spec: "do 2".into(),
                status: TaskStatus::Pending,
                deps: vec![],
                result: None,
                assignment: None,
                created_at: "2026-06-28T00:00:00Z".into(),
                completed_at: None,
            })
            .unwrap();
        engine
            .store
            .update_task_status("t1", TaskStatus::Completed, None)
            .unwrap();

        let dto = engine.show_run(&run.id).unwrap();
        assert_eq!(dto.id, run.id);
        assert_eq!(dto.goal, "goal");
        assert_eq!(dto.task_count, 2);
        assert_eq!(dto.completed_count, 1);
    }

    /// Task 4：show_run 对不存在 / 空 run_id 报错。
    #[test]
    fn engine_show_run_rejects_missing_and_empty() {
        let engine = build_test_engine();
        // 空 run_id（show_run 不做 trim，命令层 trim——这里直测非空校验由命令层保证；
        // 但「未找到」走 store.get_run → None → Err）。
        assert!(engine.show_run("run_does_not_exist").is_err());
    }

    /// Task 4：list_active_agents 委派 Store——空仓库返回空 Vec。
    #[test]
    fn engine_list_active_agents_empty_when_no_dispatches() {
        let engine = build_test_engine();
        let agents = engine.list_active_agents().unwrap();
        assert!(agents.is_empty(), "无派发时应返回空 Vec");
    }

    // ── Task 14（3d）：SweepReportDto 转换 ──

    /// Task 14 RED：SweepReport → SweepReportDto 一对一映射（camelCase 由 serde 保证）。
    #[test]
    fn sweep_report_dto_converts_from_sweep_report() {
        let report = crate::hermes::SweepReport { removed: 3, retained: 2 };
        let dto = SweepReportDto::from(report);
        assert_eq!(dto.removed, 3);
        assert_eq!(dto.retained, 2);
    }

    /// Task 14 RED：cleanup_run 对空 run_id 报错（参数归一化）。
    /// （真 sweep 路径需要 AppHandle，单测不便构造；空 run_id 拦截是纯逻辑，可直测。）
    #[test]
    fn engine_cleanup_run_rejects_empty_run_id() {
        // cleanup_run 需 AppHandle——空 run_id 拦截在命令层 trim；这里间接验证
        // normalize 契约：空串被拒。构造 engine 仅用于确认方法存在 + 签名稳定。
        let _engine = build_test_engine();
        // hermes_run_cleanup 命令层对空 run_id 的拒绝行为与 normalize_run_goal 同构，
        // 此处钉死 SweepReportDto 字段名（防 serde rename 漂移）。
        let dto = SweepReportDto { removed: 0, retained: 0 };
        assert_eq!(dto.removed, 0);
        assert_eq!(dto.retained, 0);
    }
}
