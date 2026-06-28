//! `SdkRuntime` —— 把现有 ChatManager 的 send 路径包成 [`AgentRuntime`]。
//!
//! 这是 Hermes 引擎与 ai-bridge daemon 之间的「介质适配器」：Coordinator 等
//! 高层组件只通过 [`AgentRuntime`] trait 驱动 agent，本模块把 ChatManager
//! 的 `send_raw_stream`（返回原始 `StreamLine` 流的入口）适配成统一的
//! `AgentEvent` 流。
//!
//! 关键设计：
//! - **加法式适配**：`ChatManager::send`（spawn `chat://` 事件发射任务）行为
//!   完全不变；本模块走并存的 `send_raw_stream` 入口，自己消费 `StreamLine`。
//! - **纯函数解析**：`parse_stream_line` 把 daemon 的标签行（`[CONTENT_DELTA]`
//!   / `[MESSAGE]` / `[TOOL_USE]` / `[TOOL_RESULT]` / ...）解析成 `AgentEvent`，
//!   无 daemon 也可单测。
//! - **结构化事件**：daemon 输出带结构化 tool_use / tool_result，故
//!   `capabilities().structured_events = true`。

use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::chat::ChatManager;
use crate::chat::AgentId;

use super::runtime::{
    AgentEvent, AgentHandle, AgentRuntime, Liveness, RuntimeCapabilities, RuntimeError,
    RuntimeStartSpec,
};

// ===== 标签词汇表（与 src-tauri/resources/ai-bridge/services/claude/*.js 对齐）=====
//
// daemon stdout 形如 {"id":..,"line":"<LABEL> <payload>"}；`StreamLine::Line.text`
// 即 `<LABEL> <payload>` 整段。以下常量是 `text` 开头的方括号 token。
//
// 权威来源：
// - services/claude/message-sender.js:213,226,247  ([MESSAGE]/[TOOL_USE]/[TOOL_RESULT])
// - services/claude/stream-event-processor.js:59,134
// - services/claude/persistent-query-service.js:249 ([SUBAGENT_MESSAGE])

/// 文本增量：`[CONTENT_DELTA] "<json-string>"` → `TextDelta(<decoded>)`。
const LABEL_CONTENT_DELTA: &str = "[CONTENT_DELTA]";
/// 思考增量：`[THINKING_DELTA] "<json-string>"` → `Thinking(<decoded>)`。
const LABEL_THINKING_DELTA: &str = "[THINKING_DELTA]";
/// 完整 message 快照：仅在 tool_use 场景输出。检测到 tool_use block → `ToolUse`；
/// 否则取 text 内容降级为 `TextDelta`。
const LABEL_MESSAGE: &str = "[MESSAGE]";
/// 独立 tool_use 事件：`[TOOL_USE] {"id":..,"name":..}` → `ToolUse{id,name}`。
const LABEL_TOOL_USE: &str = "[TOOL_USE]";
/// tool_result 块：`[TOOL_RESULT] {"tool_use_id":..,"is_error":..}` → `ToolResult`。
const LABEL_TOOL_RESULT: &str = "[TOOL_RESULT]";
/// 子代理消息：Hermes 引擎不消费（路由由别处处理）→ `None`。
const LABEL_SUBAGENT_MESSAGE: &str = "[SUBAGENT_MESSAGE]";
/// prompt-enhancer 输出标签：与 Hermes send 路径无关 → `None`。
const LABEL_ENHANCED: &str = "[ENHANCED]";

/// 解析 daemon `StreamLine::Line.text` 为 [`AgentEvent`]。
///
/// 纯函数：不访问 daemon、不读 IO，便于无 daemon 单测。
///
/// 协议（与 ai-bridge 对齐）：
/// - `[CONTENT_DELTA] "<json-string>"` → `TextDelta(<decoded>)`；payload 是被
///   `JSON.stringify` 过的字符串，反序列化失败时降级为 trim 后的原文。
/// - `[THINKING_DELTA] "<json-string>"` → `Thinking(<decoded>)`（同上降级）。
/// - `[TOOL_USE] {"id":..,"name":..}` → `ToolUse{id,name}`。
/// - `[TOOL_RESULT] {"tool_use_id":..,"is_error":..}` → `ToolResult{tool_use_id,is_error}`。
/// - `[MESSAGE] <json>` → assistant/user message 快照：检测 tool_use block →
///   `ToolUse`；tool_result block → `ToolResult`；否则取 text 内容 → `TextDelta`；
///   空则 `None`。
/// - `[SUBAGENT_MESSAGE]` / `[ENHANCED]` 等引擎无关标签 → `None`。
/// - 未识别的行（无标签）→ 把整段 text 作为 `TextDelta`（兼容裸 stdout）。
///
/// 注：`StreamLine::Done{success,error}` **不是**文本标签，不在此解析——由
/// `SdkRuntime::send` 的消费循环直接映射成 `AgentEvent::Done`/`Failed`。
pub fn parse_stream_line(line: &str) -> Option<AgentEvent> {
    let trimmed = line.trim_start();
    macro_rules! after {
        ($label:expr) => {
            trimmed
                .strip_prefix($label)
                .map(|rest| rest.trim_start_matches(' ').trim())
        };
    }

    // [CONTENT_DELTA] "<json-string>"
    if let Some(payload) = after!(LABEL_CONTENT_DELTA) {
        return Some(AgentEvent::TextDelta(decode_json_string(payload)));
    }

    // [THINKING_DELTA] "<json-string>"
    if let Some(payload) = after!(LABEL_THINKING_DELTA) {
        return Some(AgentEvent::Thinking(decode_json_string(payload)));
    }

    // [TOOL_USE] {"id":..,"name":..}
    if let Some(payload) = after!(LABEL_TOOL_USE) {
        if let Ok(v) = serde_json::from_str::<Value>(payload) {
            let id = v
                .get("id")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let name = v
                .get("name")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            return Some(AgentEvent::ToolUse { id, name });
        }
        return None;
    }

    // [TOOL_RESULT] {"tool_use_id":..,"is_error":..}
    if let Some(payload) = after!(LABEL_TOOL_RESULT) {
        if let Ok(v) = serde_json::from_str::<Value>(payload) {
            return Some(parse_tool_result(&v));
        }
        return None;
    }

    // [MESSAGE] <json>：assistant/user message 快照
    if let Some(payload) = after!(LABEL_MESSAGE) {
        if payload.is_empty() {
            return None;
        }
        let Ok(v) = serde_json::from_str::<Value>(payload) else {
            return None;
        };
        return parse_message_snapshot(&v);
    }

    // 引擎不消费的标签：显式列出，避免被「未识别」分支误判成 TextDelta。
    if after!(LABEL_SUBAGENT_MESSAGE).is_some() || after!(LABEL_ENHANCED).is_some() {
        return None;
    }

    // 未识别（无方括号标签）：把整段 text 作为 TextDelta（兼容裸 stdout 噪声）。
    Some(AgentEvent::TextDelta(trimmed.to_string()))
}

/// 把 `[CONTENT_DELTA]` / `[THINKING_DELTA]` 的 payload 解码成字符串。
///
/// daemon 用 `JSON.stringify(delta)` 输出（payload 形如 `"hi"`）；优先按 JSON
/// 字符串反序列化，失败时降级为 trim 后的原文（兼容非 JSON 输出）。
fn decode_json_string(payload: &str) -> String {
    serde_json::from_str::<String>(payload).unwrap_or_else(|_| payload.trim().to_string())
}

/// 从 `[MESSAGE]` 快照中提取 tool_use / tool_result / text。
///
/// assistant message 形如 `{"type":"assistant","message":{"content":[{"type":"tool_use","id":..,"name":..},...]}}`；
/// user message 形如 `{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":..,"is_error":..}]}}`。
fn parse_message_snapshot(v: &Value) -> Option<AgentEvent> {
    // content 可能在 message.content 或顶层 content。
    let content = v
        .get("message")
        .and_then(|m| m.get("content"))
        .or_else(|| v.get("content"));

    if let Some(blocks) = content.and_then(|c| c.as_array()) {
        // 优先：tool_use block → ToolUse
        for b in blocks {
            if b.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                let id = b
                    .get("id")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string();
                let name = b
                    .get("name")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string();
                return Some(AgentEvent::ToolUse { id, name });
            }
        }
        // 次之：tool_result block → ToolResult
        for b in blocks {
            if b.get("type").and_then(|t| t.as_str()) == Some("tool_result") {
                return Some(parse_tool_result(b));
            }
        }
        // 否则：拼接所有 text block → TextDelta
        let mut buf = String::new();
        for b in blocks {
            if b.get("type").and_then(|t| t.as_str()) == Some("text") {
                if let Some(t) = b.get("text").and_then(|x| x.as_str()) {
                    buf.push_str(t);
                }
            }
        }
        if buf.is_empty() {
            return None;
        }
        return Some(AgentEvent::TextDelta(buf));
    }
    None
}

/// 从 tool_result block（`[TOOL_RESULT]` 或 `[MESSAGE]` 内）提取 `ToolResult`。
fn parse_tool_result(v: &Value) -> AgentEvent {
    let tool_use_id = v
        .get("tool_use_id")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let is_error = v
        .get("is_error")
        .and_then(|x| x.as_bool())
        .unwrap_or(false);
    AgentEvent::ToolResult {
        tool_use_id,
        is_error,
    }
}

/// 基于 [`ChatManager`] 的 [`AgentRuntime`] 实现：Hermes 引擎通过它与
/// ai-bridge daemon 通信。
///
/// 适配策略：`send` 调用 `ChatManager::send_raw_stream`（与现有 `send` 并存、
/// 不 spawn `chat://` 事件发射任务），拿到原始 `StreamLine` 接收端后 spawn 一个
/// 转发任务，逐行映射成 [`AgentEvent`] 推给调用方。其余生命周期方法直接
/// 代理给 ChatManager。
pub struct SdkRuntime {
    manager: Arc<ChatManager>,
}

impl SdkRuntime {
    pub fn new(manager: Arc<ChatManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl AgentRuntime for SdkRuntime {
    fn capabilities(&self) -> RuntimeCapabilities {
        // daemon 输出带结构化 tool_use / tool_result，故 structured_events=true
        // （liveness 判定可基于精确事件而非纯文本 + 进程存活）。
        // supports_resume / supports_permission_prompt 暂为 false：
        // permission-prompt → NeedsInput 的解析尚未接入，待后续子阶段补齐后再翻为 true。
        RuntimeCapabilities {
            structured_events: true,
            supports_resume: false,
            supports_permission_prompt: false,
        }
    }

    async fn start(&self, spec: RuntimeStartSpec) -> Result<AgentHandle, RuntimeError> {
        // 不主动启动 daemon：ChatManager 在首次 send_raw_stream 时会通过内部
        // running_client_for 懒启动（含心跳）。Hermes 视角下 start 只登记 agent_id。
        Ok(AgentHandle {
            agent_id: spec.agent_id,
        })
    }

    async fn send(
        &self,
        handle: &AgentHandle,
        prompt: String,
    ) -> Result<mpsc::UnboundedReceiver<AgentEvent>, RuntimeError> {
        // 复用 ChatManager 的 send_raw_stream：返回原始 StreamLine 接收端，
        // 不 spawn chat:// 事件发射任务（与现有 send 并存的加法式入口）。
        let agent_id: AgentId = handle.agent_id.clone();
        // method = "<provider>.send"，与 chat_commands::chat_send 对齐。
        // 注意：当前 SdkRuntime 假设 provider=claude（Hermes 2a 范围内）。
        // 真正多 provider 的支持会在 Coordinator 子阶段注入 spec.provider 后补齐。
        let method = "claude.send".to_string();
        let params = json!({
            "message": prompt,
            // cwd / model / sessionId 等参数由 RuntimeStartSpec 在 Coordinator
            // 子阶段注入；此处先给最小可跑形状（Claude daemon 容忍缺字段）。
            "streaming": true,
        });

        let (_req_id, mut rx) = self
            .manager
            .send_raw_stream(agent_id, method, params)
            .await
            .map_err(RuntimeError)?;

        let (tx, out_rx) = mpsc::unbounded_channel::<AgentEvent>();
        tokio::spawn(async move {
            use crate::chat::StreamLine;
            while let Some(item) = rx.recv().await {
                match item {
                    StreamLine::Line { text } => {
                        if let Some(ev) = parse_stream_line(&text) {
                            if tx.send(ev).is_err() {
                                break; // 调用方丢弃 receiver：停止转发。
                            }
                        }
                    }
                    StreamLine::Stderr { text: _ } => {
                        // stderr 不参与 Hermes 引擎决策，忽略。
                    }
                    StreamLine::Done { success, error } => {
                        let ev = if success {
                            AgentEvent::Done {
                                success: true,
                                files_modified: vec![],
                            }
                        } else {
                            AgentEvent::Failed {
                                error: error.unwrap_or_default(),
                            }
                        };
                        let _ = tx.send(ev);
                        break; // Done 是终止信号：转发后关闭 channel。
                    }
                }
            }
            // tx 在此 drop，out_rx 的 recv() 将返回 None，通知调用方流结束。
        });
        Ok(out_rx)
    }

    async fn abort(&self, handle: &AgentHandle) -> Result<(), RuntimeError> {
        self.manager
            .abort(handle.agent_id.clone())
            .await
            .map_err(RuntimeError)
    }

    async fn liveness(&self, handle: &AgentHandle) -> Liveness {
        if self.manager.is_running(&handle.agent_id).await {
            Liveness::Alive
        } else {
            Liveness::Dead
        }
    }

    async fn stop(&self, handle: &AgentHandle) -> Result<(), RuntimeError> {
        // ChatManager::close_agent 幂等且自带回收（停 daemon + 移出 pool + 停 watcher）。
        self.manager.close_agent(handle.agent_id.clone()).await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_delta_json_string_decodes_to_text_delta() {
        // daemon 输出形如 [CONTENT_DELTA] "hi"（payload 是 JSON.stringify 过的字符串）。
        let ev = parse_stream_line(r#"[CONTENT_DELTA] "hi""#).expect("应有事件");
        match ev {
            AgentEvent::TextDelta(t) => assert_eq!(t, "hi"),
            other => panic!("期望 TextDelta，实际 {other:?}"),
        }
    }

    #[test]
    fn content_delta_non_json_falls_back_to_raw() {
        // 非 JSON payload（极端兜底）：降级为 trim 后原文。
        let ev = parse_stream_line("[CONTENT_DELTA] hello world").expect("应有事件");
        match ev {
            AgentEvent::TextDelta(t) => assert_eq!(t, "hello world"),
            other => panic!("期望 TextDelta，实际 {other:?}"),
        }
    }

    #[test]
    fn message_with_tool_use_block_yields_tool_use() {
        // assistant message 带 tool_use block（daemon 在流式 tool_use 场景输出 [MESSAGE]）。
        let payload = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"tool_1","name":"Read"}]}}"#;
        let line = format!("[MESSAGE] {payload}");
        let ev = parse_stream_line(&line).expect("应有事件");
        match ev {
            AgentEvent::ToolUse { id, name } => {
                assert_eq!(id, "tool_1");
                assert_eq!(name, "Read");
            }
            other => panic!("期望 ToolUse，实际 {other:?}"),
        }
    }

    #[test]
    fn standalone_tool_use_label_yields_tool_use() {
        // daemon 还会独立输出 [TOOL_USE] {"id":..,"name":..}（message-sender.js:226）。
        let payload = r#"{"id":"tool_42","name":"Edit"}"#;
        let line = format!("[TOOL_USE] {payload}");
        let ev = parse_stream_line(&line).expect("应有事件");
        match ev {
            AgentEvent::ToolUse { id, name } => {
                assert_eq!(id, "tool_42");
                assert_eq!(name, "Edit");
            }
            other => panic!("期望 ToolUse，实际 {other:?}"),
        }
    }

    #[test]
    fn tool_result_label_yields_tool_result() {
        // daemon 输出 [TOOL_RESULT]（truncateToolResultBlock 保留 tool_use_id / is_error）。
        let payload = r#"{"type":"tool_result","tool_use_id":"tool_1","content":"ok","is_error":false}"#;
        let line = format!("[TOOL_RESULT] {payload}");
        let ev = parse_stream_line(&line).expect("应有事件");
        match ev {
            AgentEvent::ToolResult {
                tool_use_id,
                is_error,
            } => {
                assert_eq!(tool_use_id, "tool_1");
                assert!(!is_error);
            }
            other => panic!("期望 ToolResult，实际 {other:?}"),
        }
    }

    #[test]
    fn tool_result_error_flag_is_propagated() {
        let payload = r#"{"type":"tool_result","tool_use_id":"tool_x","is_error":true}"#;
        let line = format!("[TOOL_RESULT] {payload}");
        let ev = parse_stream_line(&line).expect("应有事件");
        match ev {
            AgentEvent::ToolResult {
                tool_use_id,
                is_error,
            } => {
                assert_eq!(tool_use_id, "tool_x");
                assert!(is_error);
            }
            other => panic!("期望 ToolResult，实际 {other:?}"),
        }
    }

    #[test]
    fn message_with_tool_result_block_yields_tool_result() {
        // user message 快照带 tool_result：[MESSAGE] 也能解析出 ToolResult。
        let payload = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"tu_9","is_error":false}]}}"#;
        let line = format!("[MESSAGE] {payload}");
        let ev = parse_stream_line(&line).expect("应有事件");
        match ev {
            AgentEvent::ToolResult { tool_use_id, .. } => {
                assert_eq!(tool_use_id, "tu_9");
            }
            other => panic!("期望 ToolResult，实际 {other:?}"),
        }
    }

    #[test]
    fn message_with_only_text_yields_text_delta() {
        // 纯文本 [MESSAGE]：拼接 text block（非流式 / 非常规路径）。
        let payload = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"hello"}]}}"#;
        let line = format!("[MESSAGE] {payload}");
        let ev = parse_stream_line(&line).expect("应有事件");
        match ev {
            AgentEvent::TextDelta(t) => assert_eq!(t, "hello"),
            other => panic!("期望 TextDelta，实际 {other:?}"),
        }
    }

    #[test]
    fn empty_message_payload_returns_none() {
        // [MESSAGE] 后无 payload：不输出事件。
        assert!(parse_stream_line("[MESSAGE]").is_none());
        assert!(parse_stream_line("[MESSAGE] ").is_none());
    }

    #[test]
    fn subagent_message_label_is_ignored() {
        // Hermes 引擎不消费子代理消息（路由由别处处理）。
        let payload = r#"{"parentToolUseId":"tu_1","message":{}}"#;
        let line = format!("[SUBAGENT_MESSAGE] {payload}");
        assert!(parse_stream_line(&line).is_none());
    }

    #[test]
    fn enhanced_label_is_ignored() {
        // prompt-enhancer 输出，与 send 路径无关。
        assert!(parse_stream_line("[ENHANCED] some text").is_none());
    }

    #[test]
    fn thinking_delta_decodes_to_thinking_event() {
        let ev = parse_stream_line(r#"[THINKING_DELTA] "pondering""#).expect("应有事件");
        match ev {
            AgentEvent::Thinking(t) => assert_eq!(t, "pondering"),
            other => panic!("期望 Thinking，实际 {other:?}"),
        }
    }

    #[test]
    fn unrecognized_line_falls_back_to_text_delta() {
        // 无方括号标签的裸文本：作为 TextDelta 转发（兼容 daemon 偶发裸 stdout）。
        let ev = parse_stream_line("plain text without label").expect("应有事件");
        match ev {
            AgentEvent::TextDelta(t) => assert_eq!(t, "plain text without label"),
            other => panic!("期望 TextDelta，实际 {other:?}"),
        }
    }

    #[test]
    fn label_matching_is_case_sensitive_and_bracketed() {
        // 标签必须带方括号；小写变体不算命中（避免误判）。
        let ev = parse_stream_line("content_delta not a label").expect("应有事件");
        match ev {
            AgentEvent::TextDelta(t) => assert_eq!(t, "content_delta not a label"),
            other => panic!("期望 TextDelta（未识别），实际 {other:?}"),
        }
    }

    #[test]
    fn empty_text_line_yields_empty_text_delta() {
        // 空字符串：trim 后为空，但仍是「未识别」分支 → TextDelta("")。
        // 这是可接受的（上层可按需忽略空 delta）。
        let ev = parse_stream_line("").expect("应有事件");
        match ev {
            AgentEvent::TextDelta(t) => assert_eq!(t, ""),
            other => panic!("期望 TextDelta，实际 {other:?}"),
        }
    }
}
