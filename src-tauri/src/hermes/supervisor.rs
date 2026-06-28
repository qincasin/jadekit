//! WorkerSupervisor：每 agent 判活状态机（Hermes §6.3）。
//!
//! 职责：维护每个 agent 的 `last_activity_at`、`open_tool_uses`、`status`，
//! 并在 [`WorkerSupervisor::reap`] 里把"超时静默 + 进程存活 + 无未闭合 tool_use
//! + 非 WaitingInput"的 Running agent 标记为 [`WorkerStatus::Suspect`]。
//!
//! Coordinator 的 watcher（Task 9）会把 `AgentEvent` 喂给 [`WorkerSupervisor::on_event`]，
//! 并定时调 [`WorkerSupervisor::reap`]；本模块只实现状态机 + reap，**不**接入
//! Coordinator（那是后续任务）。
//!
//! 关键规则（设计 §6.3）：
//! - 任意 `AgentEvent` → 刷新 `last_activity_at`。
//! - `ToolUse{id,..}` → 入 `open_tool_uses`；`ToolResult{tool_use_id,..}` → 出。
//!   **已发 ToolUse 未收 ToolResult = 正常「工具执行中」，不算卡死**。
//! - `NeedsInput` → [`WorkerStatus::WaitingInput`]（**永不被超时杀**）。
//! - `Done{..}` → [`WorkerStatus::Done`]（成功态，**不**触发熔断）。
//! - `Failed{..}` → [`WorkerStatus::Failed`]。
//! - reap 条件（全部满足才 Suspect）：
//!   1. 当前 status == [`WorkerStatus::Running`]，
//!   2. `(now - last_activity_at) > timeout`，
//!   3. **非** WaitingInput（WaitingInput 永不被 reap），
//!   4. `open_tool_uses` 为空，
//!   5. `runtime.liveness(handle) == Liveness::Alive`
//!      （**Dead + 静默不算 Suspect**——Dead 由别处作为 Failed 处理）。

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, Utc};

use super::runtime::{AgentEvent, AgentHandle, AgentRuntime, Liveness};

// ── WorkerStatus ────────────────────────────────────────────────────────────

/// 单个 agent worker 的判活状态。对应设计 §6.3。
///
/// - `Running`：正常工作中（建表默认）。
/// - `WaitingInput`：`NeedsInput` 等待用户/权限回答——**永不被超时杀**。
/// - `Done`：`Done{success=true}` 正常结束（不熔断）。
/// - `Failed`：`Failed` 或 `Done{success=false}` 或进程退出（熔断由 Coordinator 处理）。
/// - `Suspect`：超时静默 + 进程存活 + 无未闭合 tool_use + 非 WaitingInput
///   → 由 [`WorkerSupervisor::reap`] 标记，Coordinator 探活后 abort+重试。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkerStatus {
    Running,
    WaitingInput,
    Done,
    Failed,
    Suspect,
}

impl WorkerStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::WaitingInput => "waiting_input",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Suspect => "suspect",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "running" => Ok(Self::Running),
            "waiting_input" => Ok(Self::WaitingInput),
            "done" => Ok(Self::Done),
            "failed" => Ok(Self::Failed),
            "suspect" => Ok(Self::Suspect),
            other => Err(format!("unknown WorkerStatus: {other}")),
        }
    }
}

// ── WorkerState ─────────────────────────────────────────────────────────────

/// 单个 agent 的判活状态快照（supervisor 内部）。
#[derive(Debug, Clone)]
struct WorkerState {
    /// 任意 `AgentEvent` 都会刷新它。reap 用 `(now - last_activity_at)` 判超时。
    last_activity_at: DateTime<Utc>,
    /// 已发 `ToolUse` 但未收到对应 `ToolResult` 的 id 集合。
    /// **非空 = 有工具还在执行，不能判 Suspect**。
    open_tool_uses: HashSet<String>,
    status: WorkerStatus,
}

impl WorkerState {
    fn new_running(now: DateTime<Utc>) -> Self {
        Self {
            last_activity_at: now,
            open_tool_uses: HashSet::new(),
            status: WorkerStatus::Running,
        }
    }
}

// ── WorkerSupervisor ────────────────────────────────────────────────────────

/// 每 agent 判活状态机。线程安全（`Mutex<HashMap<agent_id, WorkerState>>`），
/// 事件来自 Coordinator 的 watcher 任务。
pub struct WorkerSupervisor {
    runtime: Arc<dyn AgentRuntime>,
    workers: Mutex<HashMap<String, WorkerState>>,
}

impl WorkerSupervisor {
    pub fn new(runtime: Arc<dyn AgentRuntime>) -> Self {
        Self {
            runtime,
            workers: Mutex::new(HashMap::new()),
        }
    }

    /// 注册一个新 agent worker（status = Running，last_activity_at = now）。
    /// 已存在同 id 则覆盖（Coordinator 重派同 agent_id 时用）。
    pub fn register(&self, agent_id: &str) {
        let now = Utc::now();
        let mut workers = self.workers.lock().unwrap();
        workers.insert(agent_id.to_string(), WorkerState::new_running(now));
    }

    /// 处理来自 watcher 的一个 `AgentEvent`，更新对应 agent 的判活状态。
    ///
    /// 任意事件都刷新 `last_activity_at`；之后按事件变体做状态迁移
    /// （详见模块顶部规则表）。未注册的 agent_id 会被自动注册为 Running。
    pub async fn on_event(&self, agent_id: &str, event: &AgentEvent) {
        let now = Utc::now();
        let mut workers = self.workers.lock().unwrap();
        let state = workers
            .entry(agent_id.to_string())
            .or_insert_with(|| WorkerState::new_running(now));
        // 任意事件都刷新活动时间（"timeout 内有任意 event → Running"）。
        state.last_activity_at = now;

        match event {
            AgentEvent::TextDelta(_) | AgentEvent::Thinking(_) => {
                // 仅刷新活动时间，状态不变。
            }
            AgentEvent::ToolUse { id, .. } => {
                // 工具调用开始：入 open_tool_uses（未闭合期间免疫 Suspect）。
                state.open_tool_uses.insert(id.clone());
            }
            AgentEvent::ToolResult { tool_use_id, .. } => {
                // 工具调用结束：出 open_tool_uses（忽略未知 id）。
                state.open_tool_uses.remove(tool_use_id);
            }
            AgentEvent::NeedsInput => {
                // 等待用户/权限回答——永不被超时杀。
                state.status = WorkerStatus::WaitingInput;
            }
            AgentEvent::Done { .. } => {
                // 正常结束（Done 是成功态，不在这里触发熔断；熔断由 Coordinator 处理）。
                state.status = WorkerStatus::Done;
            }
            AgentEvent::Failed { .. } => {
                state.status = WorkerStatus::Failed;
            }
        }
    }

    /// 扫描所有 worker，把满足 Suspect 条件的 Running agent 标记为 Suspect，
    /// 返回本轮被标记的 agent_id 列表。
    ///
    /// Suspect 条件（设计 §6.3，全部满足）：
    /// 1. 当前 `status == Running`；
    /// 2. `(now - last_activity_at) > timeout`；
    /// 3. `status != WaitingInput`（由 1 隐含，保险起见再查一次）；
    /// 4. `open_tool_uses.is_empty()`；
    /// 5. `runtime.liveness(handle) == Liveness::Alive`。
    ///
    /// 返回顺序按 agent_id 字典序（确定性，便于测试）。
    pub async fn reap(&self, now: DateTime<Utc>, timeout: Duration) -> Vec<String> {
        // 先挑出本轮候选（持锁时间短）：Running + 超时 + 无 open tool_use。
        let candidate_ids: Vec<String> = {
            let workers = self.workers.lock().unwrap();
            workers
                .iter()
                .filter(|(_, state)| state.status == WorkerStatus::Running)
                .filter(|(_, state)| (now - state.last_activity_at) > timeout)
                .filter(|(_, state)| state.open_tool_uses.is_empty())
                .map(|(id, _)| id.clone())
                .collect()
        };

        let mut suspects = Vec::new();
        for agent_id in candidate_ids {
            // 进程存活探针：Dead + 静默不算 Suspect（Dead 由别处作为 Failed 处理）。
            let liveness = self
                .runtime
                .liveness(&AgentHandle {
                    agent_id: agent_id.clone(),
                })
                .await;
            if liveness == Liveness::Alive {
                let mut workers = self.workers.lock().unwrap();
                if let Some(state) = workers.get_mut(&agent_id) {
                    // 双检：状态可能在探针期间被 on_event 改变。
                    if state.status == WorkerStatus::Running
                        && state.open_tool_uses.is_empty()
                        && (now - state.last_activity_at) > timeout
                    {
                        state.status = WorkerStatus::Suspect;
                        suspects.push(agent_id);
                    }
                }
            }
        }

        suspects.sort();
        suspects
    }

    /// 测试/观测辅助：读某 agent 当前状态（不含在公开契约里，主要为测试和日志）。
    #[cfg(test)]
    fn status_of(&self, agent_id: &str) -> Option<WorkerStatus> {
        self.workers
            .lock()
            .unwrap()
            .get(agent_id)
            .map(|s| s.status)
    }
}

// ── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hermes::{
        RuntimeCapabilities, RuntimeError, RuntimeStartSpec,
    };
    use async_trait::async_trait;
    use tokio::sync::mpsc;

    // ── 测试专用 MockRuntime：liveness 可按 agent_id 编程 ────────────────────
    //
    // 除 liveness 外其它方法都是 no-op（supervisor 只用 liveness）。

    struct MockRuntime {
        /// agent_id -> 编程好的 liveness 值。未编程默认 Alive。
        liveness_map: Mutex<HashMap<String, Liveness>>,
        /// 全局默认 liveness（未在 map 里时用）。
        default: Liveness,
    }

    impl MockRuntime {
        fn new(default: Liveness) -> Self {
            Self {
                liveness_map: Mutex::new(HashMap::new()),
                default,
            }
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
            _handle: &AgentHandle,
            _prompt: String,
        ) -> Result<mpsc::UnboundedReceiver<AgentEvent>, RuntimeError> {
            let (_tx, rx) = mpsc::unbounded_channel();
            Ok(rx)
        }
        async fn abort(&self, _handle: &AgentHandle) -> Result<(), RuntimeError> {
            Ok(())
        }
        async fn liveness(&self, handle: &AgentHandle) -> Liveness {
            self.liveness_map
                .lock()
                .unwrap()
                .get(&handle.agent_id)
                .copied()
                .unwrap_or(self.default)
        }
        async fn stop(&self, _handle: &AgentHandle) -> Result<(), RuntimeError> {
            Ok(())
        }
    }

    fn supervisor(default_liveness: Liveness) -> (WorkerSupervisor, Arc<MockRuntime>) {
        let rt = Arc::new(MockRuntime::new(default_liveness));
        let rt_dyn: Arc<dyn AgentRuntime> = rt.clone();
        (WorkerSupervisor::new(rt_dyn), rt)
    }

    // 用例 1：ToolUse 未闭合 → 即使静默超时 + 进程存活，也**不**判 Suspect。
    #[tokio::test]
    async fn open_tool_use_shields_from_suspect() {
        let (sup, _rt) = supervisor(Liveness::Alive);
        sup.register("a1");
        // 发一个 ToolUse 但没对应的 ToolResult。
        sup.on_event(
            "a1",
            &AgentEvent::ToolUse {
                id: "t1".to_string(),
                name: "Read".to_string(),
            },
        )
        .await;

        let now = Utc::now() + Duration::seconds(60);
        let reaped = sup.reap(now, Duration::seconds(5)).await;
        assert!(reaped.is_empty(), "open tool_use 必须免疫 Suspect");
        assert_eq!(sup.status_of("a1"), Some(WorkerStatus::Running));
    }

    // 用例 2：ToolResult 闭合后 → 可以被 Suspect。
    #[tokio::test]
    async fn tool_result_closes_then_can_be_suspect() {
        let (sup, _rt) = supervisor(Liveness::Alive);
        sup.register("a1");
        sup.on_event(
            "a1",
            &AgentEvent::ToolUse {
                id: "t1".to_string(),
                name: "Read".to_string(),
            },
        )
        .await;
        // 闭合 tool_use。
        sup.on_event(
            "a1",
            &AgentEvent::ToolResult {
                tool_use_id: "t1".to_string(),
                is_error: false,
            },
        )
        .await;

        // ToolResult 也刷新了 last_activity_at，所以要从 ToolResult 之后开始计时。
        let now = Utc::now() + Duration::seconds(60);
        let reaped = sup.reap(now, Duration::seconds(5)).await;
        assert_eq!(reaped, vec!["a1".to_string()]);
        assert_eq!(sup.status_of("a1"), Some(WorkerStatus::Suspect));
    }

    // 用例 3：WaitingInput 永不被 reap。
    #[tokio::test]
    async fn waiting_input_is_never_reaped() {
        let (sup, _rt) = supervisor(Liveness::Alive);
        sup.register("a1");
        sup.on_event("a1", &AgentEvent::NeedsInput).await;
        assert_eq!(sup.status_of("a1"), Some(WorkerStatus::WaitingInput));

        let now = Utc::now() + Duration::seconds(60);
        let reaped = sup.reap(now, Duration::seconds(5)).await;
        assert!(reaped.is_empty(), "WaitingInput 永不被超时杀");
        assert_eq!(sup.status_of("a1"), Some(WorkerStatus::WaitingInput));
    }

    // 用例 4：静默 + 存活 + 无 open tool_use → Suspect。
    #[tokio::test]
    async fn silent_alive_no_open_tool_use_is_suspect() {
        let (sup, _rt) = supervisor(Liveness::Alive);
        sup.register("a1");
        // 无任何事件 → 无 open tool_use。

        let now = Utc::now() + Duration::seconds(60);
        let reaped = sup.reap(now, Duration::seconds(5)).await;
        assert_eq!(reaped, vec!["a1".to_string()]);
        assert_eq!(sup.status_of("a1"), Some(WorkerStatus::Suspect));
    }

    // 用例 5：Dead + 静默 → **不**是 Suspect（Dead 由别处作为 Failed 处理）。
    #[tokio::test]
    async fn dead_silent_is_not_suspect() {
        let (sup, _rt) = supervisor(Liveness::Dead);
        sup.register("a1");

        let now = Utc::now() + Duration::seconds(60);
        let reaped = sup.reap(now, Duration::seconds(5)).await;
        assert!(reaped.is_empty(), "Dead+静默不算 Suspect");
        assert_eq!(
            sup.status_of("a1"),
            Some(WorkerStatus::Running),
            "Dead 不在 reap 里被改状态——Failed 由别处处理"
        );
    }

    // 用例 6：已经 Done / Failed 的 agent 不会被 re-ap。
    #[tokio::test]
    async fn done_and_failed_are_not_reaped() {
        let (sup, _rt) = supervisor(Liveness::Alive);

        sup.register("done");
        sup.on_event(
            "done",
            &AgentEvent::Done {
                success: true,
                files_modified: vec![],
            },
        )
        .await;
        assert_eq!(sup.status_of("done"), Some(WorkerStatus::Done));

        sup.register("failed");
        sup.on_event(
            "failed",
            &AgentEvent::Failed {
                error: "boom".to_string(),
            },
        )
        .await;
        assert_eq!(sup.status_of("failed"), Some(WorkerStatus::Failed));

        let now = Utc::now() + Duration::seconds(60);
        let reaped = sup.reap(now, Duration::seconds(5)).await;
        assert!(reaped.is_empty(), "Done/Failed 不再被 reap");
        assert_eq!(sup.status_of("done"), Some(WorkerStatus::Done));
        assert_eq!(sup.status_of("failed"), Some(WorkerStatus::Failed));
    }

    // 用例 7：任意事件刷新活动时间 → reap 前刚有事件的 agent 不会被收。
    #[tokio::test]
    async fn any_event_refreshes_activity() {
        let (sup, _rt) = supervisor(Liveness::Alive);
        sup.register("a1");

        // 在 "now" 之前注册（老活动时间），然后 reap 用一个较近的 now，
        // 但 reap 前再发一个事件刷新 last_activity_at。
        let event_time = Utc::now();
        // 手动把 last_activity_at 拉到很久以前：通过锁直接改。
        {
            let mut workers = sup.workers.lock().unwrap();
            workers
                .get_mut("a1")
                .unwrap()
                .last_activity_at = event_time - Duration::seconds(60);
        }
        // 现在发一个事件（会刷新 last_activity_at 到 Utc::now()）。
        sup.on_event("a1", &AgentEvent::TextDelta("hi".to_string()))
            .await;

        // reap 的 now 取事件之后的"很近"的时间——不会超时。
        let now = Utc::now() + Duration::milliseconds(10);
        let reaped = sup.reap(now, Duration::seconds(5)).await;
        assert!(reaped.is_empty(), "刚收到事件 → 未超时 → 不被 reap");
        assert_eq!(sup.status_of("a1"), Some(WorkerStatus::Running));
    }

    // 补充：WorkerStatus as_str/from_str 往返 + 不写魔法字符串。
    #[test]
    fn worker_status_roundtrip() {
        for v in [
            WorkerStatus::Running,
            WorkerStatus::WaitingInput,
            WorkerStatus::Done,
            WorkerStatus::Failed,
            WorkerStatus::Suspect,
        ] {
            let s = v.as_str();
            assert_eq!(WorkerStatus::from_str(s).unwrap(), v);
        }
        assert!(WorkerStatus::from_str("nonsense").is_err());
    }

    // 补充：reap 只收 Running 超时者；多个候选按字典序返回（确定性）。
    #[tokio::test]
    async fn reap_returns_sorted_multiple_suspects() {
        let (sup, _rt) = supervisor(Liveness::Alive);
        sup.register("b");
        sup.register("a");
        sup.register("c");
        // 把 c 改成 Done，不应被收。
        sup.on_event(
            "c",
            &AgentEvent::Done {
                success: true,
                files_modified: vec![],
            },
        )
        .await;

        let now = Utc::now() + Duration::seconds(60);
        let reaped = sup.reap(now, Duration::seconds(5)).await;
        assert_eq!(reaped, vec!["a".to_string(), "b".to_string()]);
    }

    // 补充：未注册 agent 收到事件会自动注册为 Running。
    #[tokio::test]
    async fn on_event_auto_registers() {
        let (sup, _rt) = supervisor(Liveness::Alive);
        sup.on_event("ghost", &AgentEvent::TextDelta("x".to_string()))
            .await;
        assert_eq!(sup.status_of("ghost"), Some(WorkerStatus::Running));
    }
}
