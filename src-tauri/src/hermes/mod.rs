//! Hermes 编排引擎模块。
//!
//! 建立在 Helm（Phase 0/1/1b）之上，定义引擎与可插拔 agent 介质之间的契约。
//! 本模块为纯增量，不影响现有 `chat` 代码。

pub mod coordinator;
pub mod runtime;
pub mod sdk_runtime;
pub mod store;
pub mod supervisor;
pub mod types;

pub use runtime::{
    AgentEvent, AgentHandle, AgentRuntime, Liveness, RuntimeCapabilities, RuntimeError,
    RuntimeStartSpec,
};
pub use supervisor::{WorkerStatus, WorkerSupervisor};
pub use sdk_runtime::{parse_stream_line, SdkRuntime};
pub use store::{GateListFilter, InboxFilter, ReconcileReport, Store, TaskListFilter};
pub use types::{
    AgentAssignment, CoordinatorRun, DecisionGate, DispatchContext, DispatchStatus, GateStatus,
    Message, MessageType, RunStatus, RuntimeKind, Task, TaskStatus,
};
