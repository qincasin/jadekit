//! Hermes 编排进度事件 —— 引擎对驾驶舱 / 前端的统一播报通道。
//!
//! 设计目的：
//! - 把 run / task / agent 三级进度统一到一个 [`OrchestrationEvent`] 枚举里，
//!   由 [`OrchestrationEventSink`] 的实现决定如何落地（Tauri emit / 测试收集 / no-op）。
//! - 引擎只依赖 sink trait，不认 `AppHandle`；生产实现在 `hermes_commands.rs`，
//!   测试用 [`NullEventSink`] 或自管收集器。这是 Phase 3a 的 keystone：解耦引擎与 UI，
//!   让 Phase 4 的驾驶舱可以独立迭代。
//!
//! 序列化约定：所有字段 camelCase，枚举以 `"kind"` 标签区分（`"run"` / `"task"` / `"agent"`），
//! 供 TypeScript 侧直接 `JSON.parse` 成判别联合。

use serde::Serialize;

// =============================================================================
// 事件枚举
// =============================================================================

/// 编排进度事件（引擎 → 前端）。三类通道统一走这一个枚举，
/// 由 [`OrchestrationEventSink`] 的实现决定怎么落地（Tauri emit / 测试收集 / no-op）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OrchestrationEvent {
    /// run 级：启动 / 完成 / 失败 / 取消。
    #[serde(rename_all = "camelCase")]
    Run {
        run_id: String,
        goal: String,
        status: String,
        error: Option<String>,
    },
    /// task 级：ready / dispatched / completed / failed / blocked。
    #[serde(rename_all = "camelCase")]
    Task {
        run_id: String,
        task_id: String,
        status: String,
        dispatch_id: Option<String>,
    },
    /// agent 级：worker 判活状态 + 最近活动类别（用于驾驶舱 AgentStateDot）。
    #[serde(rename_all = "camelCase")]
    Agent {
        run_id: String,
        agent_id: String,
        task_id: Option<String>,
        status: String,
        activity: Option<String>,
    },
}

// =============================================================================
// Sink 契约
// =============================================================================

/// 事件下游契约：引擎只调 [`OrchestrationEventSink::emit`]，不认 Tauri。
///
/// 这条 trait 是引擎与驾驶舱之间的唯一耦合面：生产实现在 `hermes_commands.rs`
/// 用 `AppHandle::emit` 落地，测试可以塞自管的收集器。引擎代码不出现 `AppHandle`，
/// 因此可以在无 Tauri 上下文的单测里完整驱动。
pub trait OrchestrationEventSink: Send + Sync {
    fn emit(&self, event: OrchestrationEvent);
}

// =============================================================================
// 默认实现：no-op
// =============================================================================

/// 默认无操作 sink —— 不注入时引擎零成本、行为与 Phase 2 完全一致。
///
/// 非回归不变量：`Coordinator`（或任何接 sink 的引擎组件）拿到的 sink 默认是它，
/// `emit` 立即返回、不分配、不持有任何状态，因此 Phase 2 已通过的引擎测试
/// 在不改一行代码的前提下应继续保持 byte-identical 行为。
pub struct NullEventSink;

impl OrchestrationEventSink for NullEventSink {
    fn emit(&self, _event: OrchestrationEvent) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_event_serializes_camel_case_with_kind_tag() {
        let ev = OrchestrationEvent::Run {
            run_id: "run_1".into(),
            goal: "g".into(),
            status: "running".into(),
            error: None,
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("\"kind\":\"run\""), "got {json}");
        assert!(json.contains("\"runId\":\"run_1\""), "got {json}");
    }

    #[test]
    fn agent_event_serializes_activity_and_status() {
        let ev = OrchestrationEvent::Agent {
            run_id: "run_1".into(),
            agent_id: "a1".into(),
            task_id: Some("t1".into()),
            status: "working".into(),
            activity: Some("tool_use".into()),
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(
            json.contains("\"kind\":\"agent\"") && json.contains("\"agentId\":\"a1\""),
            "got {json}"
        );
    }

    #[test]
    fn null_sink_is_noop() {
        NullEventSink.emit(OrchestrationEvent::Task {
            run_id: "r".into(),
            task_id: "t".into(),
            status: "ready".into(),
            dispatch_id: None,
        });
    }
}
