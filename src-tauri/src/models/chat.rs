use serde::{Deserialize, Serialize};

/// chat://message 事件载荷
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessageEvent {
    /// 当前聊天请求 ID，用于前端过滤迟到/旧请求事件
    #[serde(rename = "requestId")]
    pub request_id: String,
    /// 原始 JSON 字符串（前端负责解析）
    pub json: String,
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
}
