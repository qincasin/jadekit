//! Hermes 编排数据模型（移植 orca，Rust 化）。
//!
//! 本文件定义 Hermes 引擎（编排数据层 + Coordinator/Supervisor/Planner）所依赖的
//! 核心枚举与结构体。设计来源：`docs/superpowers/specs/2026-06-27-helm-hermes-design.md` §7。
//! 完整字段集参考 orca `runtime/orchestration/types.ts`（其中 §7 以 `/* … */` 省略的字段，
//! 按 orca 对齐补充）。
//!
//! 约定：
//! - 所有枚举提供 `as_str`/`from_str` 集中映射；其余代码必须用枚举，禁止魔法串。
//! - 时间戳统一为 ISO-8601 字符串。
//!   注意：Hermes Store 是独立 SQLite db，不沿用 `jadekit.db` 的 INTEGER 时间戳约定。
//! - 所有结构体派生 `Serialize/Deserialize` 以便跨 Tauri 边界 + 以 JSON TEXT 持久化。
//! - `as_str` 的 token 全部为 snake_case（如 `"circuit_broken"`、`"worker_done"`），
//!   对齐 orca 的 SQLite CHECK 约束取值，便于直接读写 DB 字符串列。

use serde::{Deserialize, Serialize};

// =============================================================================
// 枚举：编排状态空间
// =============================================================================

/// Agent 间消息种类。对应 orca `MessageType`。
///
/// - `Status`：常规状态报告。
/// - `Dispatch`：Coordinator → Worker 的派发指令。
/// - `WorkerDone`：Worker 完成任务回报。
/// - `MergeReady`：产物已就绪、可合并。
/// - `Escalation`：升级（需人工/上层介入）。
/// - `Handoff`：Agent 间任务交接。
/// - `DecisionGate`：决策门请求/响应（配合 [`GateStatus`]）。
/// - `Heartbeat`：派发心跳（配合 [`DispatchContext::last_heartbeat_at`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MessageType {
    Status,
    Dispatch,
    WorkerDone,
    MergeReady,
    Escalation,
    Handoff,
    DecisionGate,
    Heartbeat,
}

impl MessageType {
    /// 稳定的 snake_case token，与 orca SQLite `messages.type` CHECK 列对齐。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Dispatch => "dispatch",
            Self::WorkerDone => "worker_done",
            Self::MergeReady => "merge_ready",
            Self::Escalation => "escalation",
            Self::Handoff => "handoff",
            Self::DecisionGate => "decision_gate",
            Self::Heartbeat => "heartbeat",
        }
    }

    /// 反序列化 token；仅接受 [`as_str`](Self::as_str) 定义的取值，其它一律报错。
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "status" => Ok(Self::Status),
            "dispatch" => Ok(Self::Dispatch),
            "worker_done" => Ok(Self::WorkerDone),
            "merge_ready" => Ok(Self::MergeReady),
            "escalation" => Ok(Self::Escalation),
            "handoff" => Ok(Self::Handoff),
            "decision_gate" => Ok(Self::DecisionGate),
            "heartbeat" => Ok(Self::Heartbeat),
            other => Err(format!("unknown MessageType: {other}")),
        }
    }
}

/// 任务生命周期状态。对应 orca `TaskStatus`。
///
/// - `Pending`：尚有未完成的依赖。
/// - `Ready`：依赖全部 `Completed`，等待派发。
/// - `Dispatched`：已派发给 Agent。
/// - `Completed`：成功完成（会触发下游 `Pending` → `Ready` 提升）。
/// - `Failed`：失败/熔断。
/// - `Blocked`：被决策门阻塞，等待外部回答。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    Ready,
    Dispatched,
    Completed,
    Failed,
    Blocked,
}

impl TaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Ready => "ready",
            Self::Dispatched => "dispatched",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Blocked => "blocked",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "pending" => Ok(Self::Pending),
            "ready" => Ok(Self::Ready),
            "dispatched" => Ok(Self::Dispatched),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "blocked" => Ok(Self::Blocked),
            other => Err(format!("unknown TaskStatus: {other}")),
        }
    }
}

/// 派发上下文状态。对应 orca `DispatchStatus`。
///
/// - `Pending`：已建上下文但尚未发出。
/// - `Dispatched`：Agent 正在工作。
/// - `Completed`：完成（任务侧会同步 `Completed`）。
/// - `Failed`：本次失败（任务侧退回 `Ready` 等待重派）。
/// - `CircuitBroken`：熔断——累计 3 次失败（[`DispatchContext::failure_count`] >= 3），
///   任务侧直接置为 `Failed`，不再重试。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DispatchStatus {
    Pending,
    Dispatched,
    Completed,
    Failed,
    CircuitBroken,
}

impl DispatchStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Dispatched => "dispatched",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::CircuitBroken => "circuit_broken",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "pending" => Ok(Self::Pending),
            "dispatched" => Ok(Self::Dispatched),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "circuit_broken" => Ok(Self::CircuitBroken),
            other => Err(format!("unknown DispatchStatus: {other}")),
        }
    }
}

/// 决策门状态。对应 orca `GateStatus`。
///
/// - `Pending`：等待外部回答。
/// - `Resolved`：已用 `resolution` 解决。
/// - `Timeout`：超时未回答。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GateStatus {
    Pending,
    Resolved,
    Timeout,
}

impl GateStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Resolved => "resolved",
            Self::Timeout => "timeout",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "pending" => Ok(Self::Pending),
            "resolved" => Ok(Self::Resolved),
            "timeout" => Ok(Self::Timeout),
            other => Err(format!("unknown GateStatus: {other}")),
        }
    }
}

/// Coordinator 运行状态。对应 orca `CoordinatorStatus`（§7 重命名为 `RunStatus`）。
///
/// - `Idle`：尚未启动（仅理论取值，建表默认）。
/// - `Running`：调度循环中。
/// - `Completed`：达成目标、正常结束。
/// - `Failed`：异常终止。
/// - `Cancelled`：被用户中途取消（Phase 3c 新增——mid-run cancel 信号置位后，
///   `run()` 独立分支直接 `update_run(Cancelled)` + emit `Run{cancelled}`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RunStatus {
    Idle,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl RunStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "idle" => Ok(Self::Idle),
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            other => Err(format!("unknown RunStatus: {other}")),
        }
    }
}

/// Agent 运行介质种类。§7 未列举，由本任务补齐（[`AgentAssignment::runtime`] 必需）。
///
/// - `Sdk`：通过 Claude Agent SDK 路径驱动（见 `sdk_runtime::SdkRuntime`）。
/// - `Cli`：通过 bare-CLI（PTY）驱动（Phase 2f 落地）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RuntimeKind {
    Sdk,
    Cli,
}

impl RuntimeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Sdk => "sdk",
            Self::Cli => "cli",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "sdk" => Ok(Self::Sdk),
            "cli" => Ok(Self::Cli),
            other => Err(format!("unknown RuntimeKind: {other}")),
        }
    }
}

// =============================================================================
// 结构体：编排持久化记录
// =============================================================================

/// 任务记录（`tasks` 表）。字段集对齐 §7；`assignment` 是 Hermes 相对 orca 的增量。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub parent_id: Option<String>,
    pub spec: String,
    pub status: TaskStatus,
    /// JSON 数组字符串形式存储依赖任务 id 列表（对齐 orca `deps TEXT`）。
    pub deps: Vec<String>,
    pub result: Option<String>,
    /// 选兵结果：该任务该派给什么样的 Agent（Planner 产出，Coordinator 消费）。
    pub assignment: Option<AgentAssignment>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

/// 派发规约——把一个任务绑定到一个具体 agent 配置（runtime + 工具 + 模型）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentAssignment {
    pub runtime: RuntimeKind,
    /// 工具/介质名（如 `"claude-sdk"`、`"claude-cli"`）。
    pub tool: String,
    /// 模型 id（如 `"sonnet"`、`"glm-5.2"`）。
    pub model: String,
}

/// 派发上下文（`dispatch_contexts` 表）——某次「任务 → Agent」派发的运行时账本。
///
/// 字段集：§7 列出的核心字段 + orca 补充的时间账本（`last_failure` / `dispatched_at` /
/// `completed_at` / `created_at`）。`assignee` 在 §7 写作 `Option<AgentId>`，
/// 此处按 task brief 统一为 `Option<String>`（handle 字符串）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DispatchContext {
    pub id: String,
    pub task_id: String,
    /// 被派发的 Agent handle；未发出前为 `None`。
    pub assignee: Option<String>,
    pub status: DispatchStatus,
    /// 累计失败次数；达到 3 触发 [`DispatchStatus::CircuitBroken`]（熔断）。
    pub failure_count: u32,
    pub last_heartbeat_at: Option<String>,
    // —— orca 补充字段（§7 以 `/* … */` 省略）——
    pub last_failure: Option<String>,
    pub dispatched_at: Option<String>,
    pub completed_at: Option<String>,
    pub created_at: String,
}

/// Agent 间消息（`messages` 表）。字段集融合 §7 行内注释 + orca `MessageRow`。
///
/// §7 仅给出字段名清单；orca 揭示 `subject`/`body` 是 `messages` 表必需列
/// （`insertMessage` 强制写入），故一并纳入。`delivered_at` 是 orca 的推送去重字段，
/// 当前 YAGNI 不纳入（待 push-on-idle 子相位再加）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub from: String,
    pub to: String,
    pub subject: String,
    pub body: String,
    #[serde(rename = "type")]
    pub kind: MessageType,
    pub priority: String,
    pub thread_id: Option<String>,
    pub payload: Option<String>,
    pub read: bool,
    pub sequence: u64,
    pub created_at: String,
}

/// 决策门（`decision_gates` 表）——需要外部回答时由 Coordinator 建立的人工关卡。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionGate {
    pub id: String,
    pub task_id: String,
    pub question: String,
    pub options: Vec<String>,
    pub resolution: Option<String>,
    pub status: GateStatus,
}

/// Coordinator 一次编排运行（`coordinator_runs` 表）。
///
/// 注：orca 此结构使用 `spec` 字段，§7 重命名为 `goal`。本实现遵循 task brief
/// 「以 §7 为准」的指示，采用 `goal`；同时在 orca 揭示的补充字段（`coordinator_handle`、
/// `created_at`、`completed_at`）上一并采纳，避免 Task 6 落表时回头补字段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoordinatorRun {
    pub id: String,
    pub goal: String,
    pub status: RunStatus,
    /// Coordinator 自身的 Agent handle（用于消息路由）。
    pub coordinator_handle: String,
    pub poll_interval_ms: u64,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── MessageType ──
    #[test]
    fn message_type_roundtrip() {
        for v in [
            MessageType::Status,
            MessageType::Dispatch,
            MessageType::WorkerDone,
            MessageType::MergeReady,
            MessageType::Escalation,
            MessageType::Handoff,
            MessageType::DecisionGate,
            MessageType::Heartbeat,
        ] {
            let s = v.as_str();
            assert_eq!(MessageType::from_str(s).unwrap(), v, "roundtrip {}", s);
        }
    }

    #[test]
    fn message_type_from_str_rejects_unknown() {
        assert!(MessageType::from_str("nonsense").is_err());
        assert!(MessageType::from_str("").is_err());
    }

    // ── TaskStatus ──
    #[test]
    fn task_status_roundtrip() {
        for v in [
            TaskStatus::Pending,
            TaskStatus::Ready,
            TaskStatus::Dispatched,
            TaskStatus::Completed,
            TaskStatus::Failed,
            TaskStatus::Blocked,
        ] {
            let s = v.as_str();
            assert_eq!(TaskStatus::from_str(s).unwrap(), v, "roundtrip {}", s);
        }
    }

    #[test]
    fn task_status_from_str_rejects_unknown() {
        assert!(TaskStatus::from_str("nonsense").is_err());
    }

    // ── DispatchStatus ──
    #[test]
    fn dispatch_status_roundtrip() {
        for v in [
            DispatchStatus::Pending,
            DispatchStatus::Dispatched,
            DispatchStatus::Completed,
            DispatchStatus::Failed,
            DispatchStatus::CircuitBroken,
        ] {
            let s = v.as_str();
            assert_eq!(DispatchStatus::from_str(s).unwrap(), v, "roundtrip {}", s);
        }
    }

    #[test]
    fn dispatch_status_from_str_rejects_unknown() {
        assert!(DispatchStatus::from_str("nonsense").is_err());
    }

    // ── GateStatus ──
    #[test]
    fn gate_status_roundtrip() {
        for v in [
            GateStatus::Pending,
            GateStatus::Resolved,
            GateStatus::Timeout,
        ] {
            let s = v.as_str();
            assert_eq!(GateStatus::from_str(s).unwrap(), v, "roundtrip {}", s);
        }
    }

    #[test]
    fn gate_status_from_str_rejects_unknown() {
        assert!(GateStatus::from_str("nonsense").is_err());
    }

    // ── RunStatus ──
    #[test]
    fn run_status_roundtrip() {
        for v in [
            RunStatus::Idle,
            RunStatus::Running,
            RunStatus::Completed,
            RunStatus::Failed,
            RunStatus::Cancelled,
        ] {
            let s = v.as_str();
            assert_eq!(RunStatus::from_str(s).unwrap(), v, "roundtrip {}", s);
        }
    }

    #[test]
    fn run_status_from_str_rejects_unknown() {
        assert!(RunStatus::from_str("nonsense").is_err());
    }

    // ── RuntimeKind ──
    #[test]
    fn runtime_kind_roundtrip() {
        for v in [RuntimeKind::Sdk, RuntimeKind::Cli] {
            let s = v.as_str();
            assert_eq!(RuntimeKind::from_str(s).unwrap(), v, "roundtrip {}", s);
        }
    }

    #[test]
    fn runtime_kind_from_str_rejects_unknown() {
        assert!(RuntimeKind::from_str("nonsense").is_err());
    }
}
