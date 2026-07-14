//! Hermes 编排引擎模块。
//!
//! 建立在 Helm（Phase 0/1/1b）之上，定义引擎与可插拔 agent 介质之间的契约。
//! 本模块为纯增量，不影响现有 `chat` 代码。

pub mod cli_runtime;
pub mod coordinator;
pub mod events;
pub mod mock_runtime;
pub mod planner;
pub mod run_lifecycle;
pub mod runtime;
pub mod runtime_registry;
pub mod sdk_runtime;
pub mod store;
pub mod supervisor;
pub mod types;

pub use runtime::{
    AgentEvent, AgentHandle, AgentRuntime, Liveness, RuntimeCapabilities, RuntimeError,
    RuntimeStartSpec,
};
pub use runtime_registry::RuntimeRegistry;
pub use coordinator::{Coordinator, TickOutcome};
pub use events::{NullEventSink, OrchestrationEvent, OrchestrationEventSink};
pub use supervisor::{DEFAULT_MAX_TURN_MS, WorkerStatus, WorkerSupervisor};
pub use sdk_runtime::{parse_stream_line, SdkRuntime};
pub use cli_runtime::CliRuntime;
pub use mock_runtime::ScriptedRuntime;
pub use store::{GateListFilter, InboxFilter, ReconcileReport, Store, TaskListFilter};
pub use types::{
    AgentAssignment, CoordinatorRun, DecisionGate, DispatchContext, DispatchStatus, GateStatus,
    Message, MessageType, RunStatus, RuntimeKind, Task, TaskStatus,
};
pub use planner::{
    build_judge_prompt, build_plan_prompt, build_replan_prompt, parse_judge_response,
    parse_plan_response, parse_replan_response, JudgeCandidate, JudgeVerdict, Planner,
    ReplanAction, ReplanDecision, Roster, RosterEntry,
};
pub use run_lifecycle::{
    decide_disposition, sweep_run_worktrees, SweepReport, WorktreeCleanupInput, WorktreeDisposition,
};
