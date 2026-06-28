//! WorkerSupervisor：每 agent 判活状态机（Hermes §6.3）。
//!
//! 职责：维护每个 agent 的 `last_activity_at`、`open_tool_uses`、`status`、
//! `structured` 标记和 `started_at`，并在 [`WorkerSupervisor::reap`] 里把满足
//! 判活条件的 Running agent 标记为 [`WorkerStatus::Suspect`]。
//!
//! Coordinator 的 watcher（Task 9）会把 `AgentEvent` 喂给 [`WorkerSupervisor::on_event`]，
//! 并定时调 [`WorkerSupervisor::reap`]；本模块只实现状态机 + reap，**不**接入
//! Coordinator（那是后续任务）。
//!
//! # 两档判活（**不混用**）
//!
//! runtime 的 [`super::RuntimeCapabilities::structured_events`] 决定走哪一档：
//!
//! - **结构化档**（`structured == true`，对应 SDK runtime）：
//!   任意 `AgentEvent` → 刷新 `last_activity_at`；`ToolUse{id}` → 入 `open_tool_uses`；
//!   `ToolResult{tool_use_id}` → 出；`NeedsInput` → [`WorkerStatus::WaitingInput`]
//!   （永不被超时杀）。reap 条件：**activity-timeout + `open_tool_uses.is_empty()`
//!   + 非 WaitingInput + 进程存活** → Suspect。`max_turn_ms` 硬兜底**不适用**
//!   ——精准信号统治。
//!
//! - **降级档**（`structured == false`，对应 bare-CLI runtime，只吐文本）：
//!   没有 tool_use/tool_result/NeedsInput 信号可用，规则退化为
//!   「**有输出=活、进程存活=没崩、max_turn_ms 硬兜底**」：
//!   reap 条件：**进程存活** AND
//!   （`now - last_activity_at > activity_timeout` **OR**
//!   `now - started_at > max_turn_ms`）→ Suspect。
//!   不查 `open_tool_uses` / `WaitingInput`（CLI 根本不发这些事件）；
//!   Dead + 静默**不算** Suspect（Dead 由别处作为 Failed 处理）。
//!
//! 两档在 [`WorkerSupervisor::reap`] 里按 per-agent `structured` 标记分流，互不混用：
//! 同一个 agent 从 [`WorkerSupervisor::register`] 起就钉死档次（结构化/降级），
//! 不会在运行中途切换。

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, Utc};

use super::runtime::{AgentEvent, AgentHandle, AgentRuntime, Liveness};

/// 降级判活的硬兜底超时（设计 §6.3）。
///
/// 仅对 `structured_events == false` 的 runtime 生效：即使 agent 一直在吐文本，
/// 只要单轮运行超过这个硬天花板就被 reap。结构化 agent **不**走这个兜底——
/// 它们由 open_tool_uses / WaitingInput 精准信号统治，max_turn_ms 不适用。
pub const DEFAULT_MAX_TURN_MS: Duration = Duration::minutes(10);

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
    /// **非空 = 有工具还在执行，不能判 Suspect**（仅结构化档用）。
    open_tool_uses: HashSet<String>,
    status: WorkerStatus,
    /// 是否走结构化判活档（`register` 时由 runtime 的
    /// [`super::RuntimeCapabilities::structured_events`] 决定，整个生命周期不变）。
    /// true=结构化档；false=降级档。两档不混用。
    structured: bool,
    /// agent 本次运行起点（`register` 时戳）。仅降级档用：`now - started_at > max_turn_ms`
    /// 触发硬兜底 Suspect。
    started_at: DateTime<Utc>,
}

impl WorkerState {
    fn new_running(now: DateTime<Utc>, structured: bool) -> Self {
        Self {
            last_activity_at: now,
            open_tool_uses: HashSet::new(),
            status: WorkerStatus::Running,
            structured,
            started_at: now,
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

    /// 注册一个新 agent worker（status = Running，last_activity_at = started_at = now）。
    /// 已存在同 id 则覆盖（Coordinator 重派同 agent_id 时用）。
    ///
    /// `structured` 决定该 agent 走哪一档判活（设计 §6.3 + §4 capability marker）：
    /// - `true`：结构化档（SDK runtime，发 tool_use/tool_result/NeedsInput）。
    /// - `false`：降级档（bare-CLI runtime，只吐文本——有输出=活、进程存活=没崩、
    ///   `max_turn_ms` 硬兜底）。
    /// 档次在 `register` 时钉死，整个生命周期不变（**两档不混用**）。
    /// Coordinator（后续任务）传 `runtime.capabilities().structured_events`。
    pub fn register(&self, agent_id: &str, structured: bool) {
        let now = Utc::now();
        let mut workers = self.workers.lock().unwrap();
        workers.insert(
            agent_id.to_string(),
            WorkerState::new_running(now, structured),
        );
    }

    /// 处理来自 watcher 的一个 `AgentEvent`，更新对应 agent 的判活状态。
    ///
    /// 任意事件都刷新 `last_activity_at`；之后按事件变体做状态迁移
    /// （详见模块顶部规则表）。未注册的 agent_id 会被自动注册为 Running。
    pub async fn on_event(&self, agent_id: &str, event: &AgentEvent) {
        let now = Utc::now();
        let mut workers = self.workers.lock().unwrap();
        // 自动注册默认走结构化档（Coordinator 正常流程会先 register 显式标档次；
        // 走到这里说明事件早于 register，按更保守的结构化档处理）。
        let state = workers
            .entry(agent_id.to_string())
            .or_insert_with(|| WorkerState::new_running(now, true));
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
    /// **两档分流（per-agent `structured` 标记，互不混用）：**
    ///
    /// - 结构化档（`structured == true`，设计 §6.3 精准信号）：全部满足才 Suspect——
    ///   1. `status == Running`，
    ///   2. `(now - last_activity_at) > activity_timeout`，
    ///   3. **非** WaitingInput（WaitingInput 永不被 reap），
    ///   4. `open_tool_uses.is_empty()`，
    ///   5. `runtime.liveness(handle) == Liveness::Alive`
    ///      （**Dead + 静默不算 Suspect**——Dead 由别处作为 Failed 处理）。
    ///   `max_turn_ms` 硬兜底**不适用**——精准信号统治。
    ///
    /// - 降级档（`structured == false`，设计 §6.3 退化规则）：进程存活 AND
    ///   （`(now - last_activity_at) > activity_timeout` OR
    ///   `(now - started_at) > max_turn_ms`）→ Suspect。
    ///   「**有输出=活、进程存活=没崩、max_turn_ms 硬兜底**」。
    ///   不查 `open_tool_uses` / `WaitingInput`（CLI 根本不发这些事件）；
    ///   Dead + 静默**不算** Suspect（同结构化档）。
    ///
    /// 返回顺序按 agent_id 字典序（确定性，便于测试）。
    pub async fn reap(
        &self,
        now: DateTime<Utc>,
        activity_timeout: Duration,
        max_turn_ms: Duration,
    ) -> Vec<String> {
        // 先挑出本轮候选（持锁时间短）：按 per-agent 档次分流，应用对应档的判活规则。
        // 进程存活探针放到锁外做（避免 await 持锁），后面再做 TOCTOU 双检。
        let candidate_ids: Vec<String> = {
            let workers = self.workers.lock().unwrap();
            workers
                .iter()
                .filter(|(_, state)| state.status == WorkerStatus::Running)
                .filter(|(_, state)| {
                    if state.structured {
                        // 结构化档：activity-timeout + 无 open tool_use（WaitingInput 已被上面 Running 过滤掉）。
                        (now - state.last_activity_at) > activity_timeout
                            && state.open_tool_uses.is_empty()
                    } else {
                        // 降级档：activity-timeout OR 硬兜底 max_turn_ms（不查 open_tool_uses/WaitingInput）。
                        (now - state.last_activity_at) > activity_timeout
                            || (now - state.started_at) > max_turn_ms
                    }
                })
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
                    // TOCTOU 双检：状态可能在探针期间被 on_event 改变。
                    // 仍按档次分流，保证两档不混用。
                    if state.status != WorkerStatus::Running {
                        continue;
                    }
                    let still_suspect = if state.structured {
                        state.open_tool_uses.is_empty()
                            && (now - state.last_activity_at) > activity_timeout
                    } else {
                        (now - state.last_activity_at) > activity_timeout
                            || (now - state.started_at) > max_turn_ms
                    };
                    if still_suspect {
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
        sup.register("a1", true);
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
        let reaped = sup
            .reap(now, Duration::seconds(5), DEFAULT_MAX_TURN_MS)
            .await;
        assert!(reaped.is_empty(), "open tool_use 必须免疫 Suspect");
        assert_eq!(sup.status_of("a1"), Some(WorkerStatus::Running));
    }

    // 用例 2：ToolResult 闭合后 → 可以被 Suspect。
    #[tokio::test]
    async fn tool_result_closes_then_can_be_suspect() {
        let (sup, _rt) = supervisor(Liveness::Alive);
        sup.register("a1", true);
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
        let reaped = sup
            .reap(now, Duration::seconds(5), DEFAULT_MAX_TURN_MS)
            .await;
        assert_eq!(reaped, vec!["a1".to_string()]);
        assert_eq!(sup.status_of("a1"), Some(WorkerStatus::Suspect));
    }

    // 用例 3：WaitingInput 永不被 reap。
    #[tokio::test]
    async fn waiting_input_is_never_reaped() {
        let (sup, _rt) = supervisor(Liveness::Alive);
        sup.register("a1", true);
        sup.on_event("a1", &AgentEvent::NeedsInput).await;
        assert_eq!(sup.status_of("a1"), Some(WorkerStatus::WaitingInput));

        let now = Utc::now() + Duration::seconds(60);
        let reaped = sup
            .reap(now, Duration::seconds(5), DEFAULT_MAX_TURN_MS)
            .await;
        assert!(reaped.is_empty(), "WaitingInput 永不被超时杀");
        assert_eq!(sup.status_of("a1"), Some(WorkerStatus::WaitingInput));
    }

    // 用例 4：静默 + 存活 + 无 open tool_use → Suspect。
    #[tokio::test]
    async fn silent_alive_no_open_tool_use_is_suspect() {
        let (sup, _rt) = supervisor(Liveness::Alive);
        sup.register("a1", true);
        // 无任何事件 → 无 open tool_use。

        let now = Utc::now() + Duration::seconds(60);
        let reaped = sup
            .reap(now, Duration::seconds(5), DEFAULT_MAX_TURN_MS)
            .await;
        assert_eq!(reaped, vec!["a1".to_string()]);
        assert_eq!(sup.status_of("a1"), Some(WorkerStatus::Suspect));
    }

    // 用例 5：Dead + 静默 → **不**是 Suspect（Dead 由别处作为 Failed 处理）。
    #[tokio::test]
    async fn dead_silent_is_not_suspect() {
        let (sup, _rt) = supervisor(Liveness::Dead);
        sup.register("a1", true);

        let now = Utc::now() + Duration::seconds(60);
        let reaped = sup
            .reap(now, Duration::seconds(5), DEFAULT_MAX_TURN_MS)
            .await;
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

        sup.register("done", true);
        sup.on_event(
            "done",
            &AgentEvent::Done {
                success: true,
                files_modified: vec![],
            },
        )
        .await;
        assert_eq!(sup.status_of("done"), Some(WorkerStatus::Done));

        sup.register("failed", true);
        sup.on_event(
            "failed",
            &AgentEvent::Failed {
                error: "boom".to_string(),
            },
        )
        .await;
        assert_eq!(sup.status_of("failed"), Some(WorkerStatus::Failed));

        let now = Utc::now() + Duration::seconds(60);
        let reaped = sup
            .reap(now, Duration::seconds(5), DEFAULT_MAX_TURN_MS)
            .await;
        assert!(reaped.is_empty(), "Done/Failed 不再被 reap");
        assert_eq!(sup.status_of("done"), Some(WorkerStatus::Done));
        assert_eq!(sup.status_of("failed"), Some(WorkerStatus::Failed));
    }

    // 用例 7：任意事件刷新活动时间 → reap 前刚有事件的 agent 不会被收。
    #[tokio::test]
    async fn any_event_refreshes_activity() {
        let (sup, _rt) = supervisor(Liveness::Alive);
        sup.register("a1", true);

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
        let reaped = sup
            .reap(now, Duration::seconds(5), DEFAULT_MAX_TURN_MS)
            .await;
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
        sup.register("b", true);
        sup.register("a", true);
        sup.register("c", true);
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
        let reaped = sup
            .reap(now, Duration::seconds(5), DEFAULT_MAX_TURN_MS)
            .await;
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

    // ── 降级档（structured == false）判活：有输出=活 / 进程存活=没崩 / max_turn_ms 硬兜底 ──

    // 降级用例 1：刚有输出（TextDelta）+ 活 + 在 activity_timeout 内 → 不被收。
    #[tokio::test]
    async fn degraded_recent_output_alive_not_suspect() {
        let (sup, _rt) = supervisor(Liveness::Alive);
        sup.register("cli1", false);
        // 降级 agent 发一个 TextDelta（"有输出=活"）。
        sup.on_event("cli1", &AgentEvent::TextDelta("hello".to_string()))
            .await;

        // now 取事件后很近的时间——活动时间未超 activity_timeout。
        let now = Utc::now() + Duration::milliseconds(10);
        let reaped = sup
            .reap(now, Duration::seconds(5), DEFAULT_MAX_TURN_MS)
            .await;
        assert!(reaped.is_empty(), "降级 agent 刚有输出 + 活 → 不是 Suspect");
        assert_eq!(sup.status_of("cli1"), Some(WorkerStatus::Running));
    }

    // 降级用例 2：静默 > activity_timeout + 活 → Suspect（退化判活：没输出 = 卡）。
    #[tokio::test]
    async fn degraded_silent_alive_is_suspect() {
        let (sup, _rt) = supervisor(Liveness::Alive);
        sup.register("cli1", false);

        let now = Utc::now() + Duration::seconds(60);
        let reaped = sup
            .reap(now, Duration::seconds(5), DEFAULT_MAX_TURN_MS)
            .await;
        assert_eq!(reaped, vec!["cli1".to_string()]);
        assert_eq!(sup.status_of("cli1"), Some(WorkerStatus::Suspect));
    }

    // 降级用例 3：仍在吐文本（last_activity_at 新鲜）但单轮超 max_turn_ms 硬兜底 → Suspect。
    // 证明硬兜底独立于 activity_timeout：哪怕 agent 一直在输出，跑过天花板就收。
    #[tokio::test]
    async fn degraded_hard_backstop_fires_even_with_fresh_activity() {
        let (sup, _rt) = supervisor(Liveness::Alive);
        sup.register("cli1", false);
        // 关键：模拟「一直在吐文本」——把 last_activity_at 拉到很新，
        // 但把 started_at 拉到很久以前，使 (now - started_at) > max_turn_ms。
        let now = Utc::now();
        {
            let mut workers = sup.workers.lock().unwrap();
            let s = workers.get_mut("cli1").unwrap();
            s.started_at = now - Duration::minutes(30); // 远超 max_turn_ms
            s.last_activity_at = now; // 但活动时间很新（一直在输出）
        }

        // activity_timeout 故意设很大（5 分钟），证明不是 activity_timeout 触发的。
        let reaped = sup
            .reap(now, Duration::minutes(5), Duration::minutes(10))
            .await;
        assert_eq!(
            reaped,
            vec!["cli1".to_string()],
            "降级档硬兜底：max_turn_ms 触发 Suspect，即使 last_activity_at 新鲜"
        );
        assert_eq!(sup.status_of("cli1"), Some(WorkerStatus::Suspect));
    }

    // 降级用例 4：进程 Dead（即使超 max_turn_ms）→ **不**是 Suspect。
    // 证明降级档同样遵守「Dead + 静默不算 Suspect」——Dead 由别处作 Failed 处理。
    #[tokio::test]
    async fn degraded_dead_not_suspect_even_past_max_turn() {
        let (sup, _rt) = supervisor(Liveness::Dead);
        sup.register("cli1", false);

        let now = Utc::now() + Duration::minutes(30);
        let reaped = sup
            .reap(now, Duration::seconds(5), Duration::minutes(10))
            .await;
        assert!(reaped.is_empty(), "降级档 Dead 即使超 max_turn_ms 也不是 Suspect");
        assert_eq!(
            sup.status_of("cli1"),
            Some(WorkerStatus::Running),
            "Dead 不在 reap 里被改状态——Failed 由别处处理"
        );
    }

    // 两档不混用 — 用例 A：结构化档**忽略** max_turn_ms 硬兜底。
    // 结构化 agent 有 open tool_use 时即使超 max_turn_ms 也**不**被收
    // （精准信号统治；max_turn_ms 只对降级档生效）。
    #[tokio::test]
    async fn structured_ignores_max_turn_ms_when_open_tool_use_shields() {
        let (sup, _rt) = supervisor(Liveness::Alive);
        sup.register("sdk1", true);
        sup.on_event(
            "sdk1",
            &AgentEvent::ToolUse {
                id: "t1".to_string(),
                name: "Read".to_string(),
            },
        )
        .await;

        // 把 started_at 拉到很久以前，模拟「单轮运行很久」。
        {
            let mut workers = sup.workers.lock().unwrap();
            workers.get_mut("sdk1").unwrap().started_at =
                Utc::now() - Duration::minutes(60);
        }

        let now = Utc::now() + Duration::seconds(1);
        // max_turn_ms 故意设很小（5 秒），started_at 已经超了 60 分钟。
        let reaped = sup
            .reap(now, Duration::seconds(5), Duration::seconds(5))
            .await;
        assert!(
            reaped.is_empty(),
            "结构化档：open tool_use 屏蔽 Suspect，max_turn_ms 硬兜底不适用"
        );
        assert_eq!(sup.status_of("sdk1"), Some(WorkerStatus::Running));
    }

    // 两档不混用 — 用例 B：结构化档即使 last_activity_at 新鲜 + 超 max_turn_ms，
    // 只要满足结构化判活条件（无 open tool_use + activity-timeout）才被收。
    // 这里给一个结构化 agent：无 open tool_use、超 activity_timeout、超 max_turn_ms
    // → 被 Suspect（但被收是因为 activity-timeout，不是 max_turn_ms）。
    #[tokio::test]
    async fn structured_reaped_by_activity_timeout_not_max_turn() {
        let (sup, _rt) = supervisor(Liveness::Alive);
        sup.register("sdk1", true);

        let now = Utc::now() + Duration::minutes(30);
        let reaped = sup
            .reap(now, Duration::seconds(5), Duration::minutes(10))
            .await;
        assert_eq!(reaped, vec!["sdk1".to_string()]);
        assert_eq!(sup.status_of("sdk1"), Some(WorkerStatus::Suspect));
    }
}
