use serde::{Deserialize, Serialize};

/// chat://message 事件载荷
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessageEvent {
    /// 当前聊天请求 ID，用于前端过滤迟到/旧请求事件
    #[serde(rename = "requestId")]
    pub request_id: String,
    /// 原始 JSON 字符串（前端负责解析）
    pub json: String,
    /// 产出该事件的 agent id；附加字段，缺省时向后兼容旧前端。
    #[serde(rename = "agentId", skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
}

/// chat://subagent-message 事件载荷。
///
/// 子代理(Task)消息从主流里分流出来，按 `parent_tool_use_id`(= 父 Task 工具块
/// 的 tool_use id)路由到对应的子代理卡片，不进主 transcript。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentMessageEvent {
    /// 当前聊天请求 ID。
    #[serde(rename = "requestId")]
    pub request_id: String,
    /// 父 Task 工具块的 tool_use id（路由键）。
    #[serde(rename = "parentToolUseId")]
    pub parent_tool_use_id: String,
    /// 子代理单条消息的原始 JSON 字符串（前端负责解析）。
    pub json: String,
    /// 产出该事件的 agent id；附加字段，缺省时向后兼容旧前端。
    #[serde(rename = "agentId", skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{ChatMessageEvent, SubagentMessageEvent};

    #[test]
    fn chat_message_event_serializes_optional_agent_id() {
        let with_agent = ChatMessageEvent {
            request_id: "r1".into(),
            json: "{}".into(),
            agent_id: Some("agent-7".into()),
        };
        let value = serde_json::to_value(&with_agent).unwrap();
        assert_eq!(value.get("agentId").and_then(|v| v.as_str()), Some("agent-7"));

        let without_agent = ChatMessageEvent {
            request_id: "r1".into(),
            json: "{}".into(),
            agent_id: None,
        };
        let value = serde_json::to_value(&without_agent).unwrap();
        assert!(
            value.get("agentId").is_none(),
            "缺省时不输出 agentId（向后兼容旧事件）"
        );
    }

    #[test]
    fn subagent_message_event_serializes_optional_agent_id() {
        let ev = SubagentMessageEvent {
            request_id: "r1".into(),
            parent_tool_use_id: "tu-1".into(),
            json: "{}".into(),
            agent_id: Some("agent-7".into()),
        };
        let value = serde_json::to_value(&ev).unwrap();
        assert_eq!(value.get("agentId").and_then(|v| v.as_str()), Some("agent-7"));
    }
}
