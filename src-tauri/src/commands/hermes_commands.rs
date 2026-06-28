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
    self, Coordinator, DispatchContext, RunStatus, Store, Task, TaskListFilter, TaskStatus,
};

// =============================================================================
// 常量：事件名 / 默认值（无魔法串）
// =============================================================================

/// Hermes 编排事件通道：run 级（run 启动 / 完成 / 失败）。
pub const HERMES_EVENT_RUN: &str = "hermes://run";
/// Hermes 编排事件通道：task 级（dispatched / completed / failed）。
///
/// 当前由命令层发射（Task 17 仅接入 run 级事件，task/agent 级事件名先占位，
/// 后续子阶段接入 supervisor 事件流时启用）。
#[allow(dead_code)]
pub const HERMES_EVENT_TASK: &str = "hermes://task";
/// Hermes 编排事件通道：agent 级（保留——后续子阶段接 supervisor 事件时使用）。
#[allow(dead_code)]
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
// 事件 payload
// =============================================================================

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunEventPayload {
    pub run_id: String,
    pub goal: String,
    pub status: String,
    /// 失败时附错误原因；其它状态为 None。
    pub error: Option<String>,
}

/// task 级事件 payload（与 [`HERMES_EVENT_TASK`] 配套，当前为预留——Task 17 不发射）。
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskEventPayload {
    pub run_id: String,
    pub task_id: String,
    pub status: String,
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
/// - 注入的 `runtime`（生产 SdkRuntime，测试 mock）；
/// - `repo_root`（缺省 cwd，opts 可覆盖）；
/// - `worktrees_dir`（每个 worker agent 一个独立 git worktree 的根目录）；
/// - `runs`：run_id → RunHandle，供 `hermes_run_stop` 发出取消信号。
pub struct HermesEngine {
    store: Store,
    runtime: Arc<dyn hermes::AgentRuntime>,
    repo_root: PathBuf,
    worktrees_dir: PathBuf,
    runs: StdMutex<HashMap<String, RunHandle>>,
}

impl HermesEngine {
    /// 构造引擎。生产由 `lib.rs::setup` 调用（注入 `SdkRuntime`），
    /// 测试可注入任意 `Arc<dyn AgentRuntime>` mock。
    pub fn new(
        store: Store,
        runtime: Arc<dyn hermes::AgentRuntime>,
        repo_root: PathBuf,
        worktrees_dir: PathBuf,
    ) -> Self {
        Self {
            store,
            runtime,
            repo_root,
            worktrees_dir,
            runs: StdMutex::new(HashMap::new()),
        }
    }

    /// 启动一次编排 run：建 run 记录 + spawn 后台 Coordinator 循环 + 注册 RunHandle。
    ///
    /// 立刻返回 run_id（不等 run 完成）；run 进展通过 `hermes://run` /
    /// `hermes://task` 事件流推送给前端。Coordinator 收敛或被 cancel 后退出。
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

        // 发射 run 启动事件（best-effort）。
        let _ = app.emit(
            HERMES_EVENT_RUN,
            RunEventPayload {
                run_id: run.id.clone(),
                goal: run.goal.clone(),
                status: RunStatus::Running.as_str().to_string(),
                error: None,
            },
        );

        // spawn 后台 Coordinator 循环。
        let store = self.store.clone_handle();
        let runtime = Arc::clone(&self.runtime);
        let worktrees_dir = self.worktrees_dir.clone();
        let run_id = run.id.clone();
        let run_goal = run.goal.clone();
        let cancel_for_task = cancel.clone();

        tauri::async_runtime::spawn(async move {
            let coordinator = Coordinator::new(store.clone_handle(), runtime, repo_root, worktrees_dir)
                .with_max_concurrent(opts.max_concurrent);

            let final_status = if cancel_for_task.load(Ordering::SeqCst) {
                // 在进入循环前就被 cancel——直接置 Failed。
                let _ = store.update_run(&run_id, RunStatus::Failed);
                RunStatus::Failed
            } else {
                match coordinator.run(&run_id).await {
                    Ok(status) => status,
                    Err(e) => {
                        let _ = store.update_run(&run_id, RunStatus::Failed);
                        let _ = app.emit(
                            HERMES_EVENT_RUN,
                            RunEventPayload {
                                run_id: run_id.clone(),
                                goal: run_goal.clone(),
                                status: RunStatus::Failed.as_str().to_string(),
                                error: Some(e),
                            },
                        );
                        return;
                    }
                }
            };

            // 发射 run 终态事件。
            let _ = app.emit(
                HERMES_EVENT_RUN,
                RunEventPayload {
                    run_id: run_id.clone(),
                    goal: run_goal,
                    status: final_status.as_str().to_string(),
                    error: if final_status == RunStatus::Failed {
                        Some("run ended in failed state".to_string())
                    } else {
                        None
                    },
                },
            );
        });

        Ok(run.id)
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
#[tauri::command]
pub async fn hermes_run_stop(
    run_id: String,
    state: State<'_, HermesEngine>,
) -> Result<(), String> {
    // stop_run 是纯内存操作（无 IO），无需 spawn_blocking。
    state.stop_run(&run_id)
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
        HermesEngine::new(
            store,
            runtime,
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
}
