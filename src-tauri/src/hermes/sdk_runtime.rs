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
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::sync::mpsc;

use crate::chat::ChatManager;
use crate::chat::AgentId;

use super::runtime::{
    AgentEvent, AgentHandle, AgentRuntime, Liveness, RuntimeCapabilities, RuntimeError,
    RuntimeStartSpec,
};

fn build_sdk_send_request(
    spec: &RuntimeStartSpec,
    prompt: String,
) -> Result<(String, Value), RuntimeError> {
    let provider = match spec.provider.as_str() {
        "claude" | "codex" => spec.provider.as_str(),
        other => return Err(RuntimeError(format!("unsupported Hermes provider: {other}"))),
    };
    Ok((
        format!("{provider}.send"),
        json!({
            "message": prompt,
            "model": spec.model,
            "cwd": spec.cwd,
            "streaming": true,
        }),
    ))
}

// ===== 标签词汇表（与 src-tauri/resources/ai-bridge/services/claude/*.js 对齐）=====
//
// daemon stdout 形如 {"id":..,"line":"<LABEL> <payload>"}；`StreamLine::Line.text`
// 即 `<LABEL> <payload>` 整段。以下常量是 `text` 开头的方括号 token。
//
// 权威来源：
// - services/claude/message-sender.js:213,226,247  ([MESSAGE]/[TOOL_USE]/[TOOL_RESULT])
// - services/claude/stream-event-processor.js:59,134
// - services/claude/persistent-query-service.js:249 ([SUBAGENT_MESSAGE])
//
// 关键事实（daemon.js:199-245）：在活跃请求期间，daemon 把 `process.stdout.write`
// **和** `console.log` 都改写成 `{id, line: <text>}` NDJSON，最终落到 Rust 端的
// `StreamLine::Line`。`console.error` 才走 `{id, stderr: <text>}` → `StreamLine::Stderr`
// （已由 SdkRuntime::send 忽略）。因此 console.log 类的诊断标签也会进入本解析器，
// 必须显式 deny，否则会污染结构化 AgentEvent 流。

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

/// 元数据/诊断标签集合：这些标签**不是** agent 输出，统一返回 `None`。
///
/// 维护原则：daemon 任何只服务于可观测性/会话状态/重试控制流的标签都应列入此表。
/// 新增标签时优先考虑加到这里（返回 None），而不是让它落到 default-deny 分支——
/// 显式列出让意图清晰、便于审计。
///
/// 已知来源（按 daemon.js + services/ 全量 grep 确认）：
/// - `[USAGE]` — message-utils.js:177, utils/usage-utils.js:72（token 用量）
/// - `[SESSION_ID]` — message-sender.js:256, message-sender-anthropic.js:77
/// - `[THREAD_ID]` — codex/codex-event-handler.js:670
/// - `[STREAM_START]` / `[STREAM_END]` — message-sender.js:152,353,417,
///   persistent-query-service.js:293,345,521
/// - `[MESSAGE_START]` / `[MESSAGE_END]` — message-sender-anthropic.js:76,174,233,
///   message-sender.js:357
/// - `[MODEL_ENV]` — utils/model-utils.js:126,129,135
/// - `[RESUMING]` / `[RESUME_WAIT]` — message-sender.js:115,117
/// - `[RETRY]` — message-sender.js:316,348,389,390
/// - `[DIAG]` — message-sender.js:128,443,444
/// - `[DEBUG]` — message-sender.js / message-sender-anthropic.js 多处
/// - `[WARN]` / `[WARNING]` — codex-utils.js debugLog, message-sender.js:458,
///   persistent-query-service.js:190, message-rewind.js:43
/// - `[BLOCK_RESET]` — stream-event-processor.js:43, message-sender.js:173
/// - `[CONTENT]` — stream-event-processor.js:105, message-sender.js:270,
///   message-sender-anthropic.js:159,210（**与 `[CONTENT_DELTA]` 不同**：前者是
///   截断的错误内容预览，后者是真正的文本增量）
/// - `[THINKING]` / `[THINKING_START]` / `[THINKING_HINT]` — message-sender.js:201,288,
///   message-sender-anthropic.js, codex/message-service.js:328（与 `[THINKING_DELTA]`
///   不同：前者是诊断/提示，后者是真正的思考增量）
/// - `[PERM_DEBUG]` — codex-utils.js:52,68, codex/message-service.js:168,181,182
///   （通过 debugLog → console.log 输出）
/// - `[SUBAGENT_MESSAGE]` — persistent-query-service.js:249（路由由别处处理）
/// - `[ENHANCED]` — prompt-enhancer.js:470,494,498（与 send 路径无关）
const METADATA_LABELS: &[&str] = &[
    "[USAGE]",
    "[SESSION_ID]",
    "[THREAD_ID]",
    "[STREAM_START]",
    "[STREAM_END]",
    "[MESSAGE_START]",
    "[MESSAGE_END]",
    "[MODEL_ENV]",
    "[RESUMING]",
    "[RESUME_WAIT]",
    "[RETRY]",
    "[DIAG]",
    "[DEBUG]",
    "[WARN]",
    "[WARNING]",
    "[BLOCK_RESET]",
    "[CONTENT]",
    "[THINKING]",
    "[THINKING_START]",
    "[THINKING_HINT]",
    "[PERM_DEBUG]",
    "[SUBAGENT_MESSAGE]",
    "[ENHANCED]",
];

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
/// - **元数据/诊断标签**（见 [`METADATA_LABELS`]）→ `None`。
/// - **未知 `[LABEL] ...` 行**（既不在内容集合、也不在元数据集合）→ `None`
///   （default-deny：daemon 任何新加的诊断标签都不应自动混入 agent 输出流，
///   比猜测成 `TextDelta` 更安全）。
/// - **无方括号标签的裸文本** → `TextDelta(<text>)`（兼容偶发的裸 stdout；
///   daemon.js 全量观察下，所有结构化输出都带标签，此分支主要作防御性兜底）。
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
        return normalize_message_snapshot(&v).map(|json| AgentEvent::MessageRaw { json });
    }

    // 已知的元数据/诊断标签：返回 None，不污染结构化 AgentEvent 流。
    if METADATA_LABELS
        .iter()
        .any(|label| trimmed.starts_with(label))
    {
        return None;
    }

    // 未知 `[LABEL] ...` 行：default-deny。比猜测成 TextDelta 更安全——daemon
    // 新加的任何诊断/元数据标签都不会自动混入 agent 输出流，保护 structured_events
    // 契约。判定方式：行首是 `[`、且紧随其后到第一个 `]` 之间全部是 ASCII 大写
    // 字母/数字/下划线（即 daemon 的标签命名约定）。
    if let Some(close) = trimmed.find(']') {
        let inner = &trimmed[1..close];
        if close >= 2
            && inner
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        {
            return None;
        }
    }

    // 无方括号标签的裸文本：作为 TextDelta 转发（兼容偶发的裸 stdout）。
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
fn normalize_message_snapshot(v: &Value) -> Option<String> {
    let role = v.get("type")?.as_str()?;
    if !matches!(role, "assistant" | "user") {
        return None;
    }
    let blocks = v.get("message")?.get("content")?.as_array()?;
    if blocks.iter().any(|block| block.get("type").and_then(Value::as_str).is_none()) {
        return None;
    }
    // Canonical JSON preserves every daemon field while ensuring UI receives data, never HTML.
    serde_json::to_string(v).ok()
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
    start_specs: Mutex<HashMap<String, RuntimeStartSpec>>,
}

impl SdkRuntime {
    pub fn new(manager: Arc<ChatManager>) -> Self {
        Self {
            manager,
            start_specs: Mutex::new(HashMap::new()),
        }
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
        build_sdk_send_request(&spec, String::new())?;
        let agent_id = spec.agent_id.clone();
        self.start_specs
            .lock()
            .map_err(|e| RuntimeError(format!("SdkRuntime start specs lock poisoned: {e}")))?
            .insert(agent_id.clone(), spec);
        Ok(AgentHandle { agent_id })
    }

    async fn send(
        &self,
        handle: &AgentHandle,
        prompt: String,
    ) -> Result<mpsc::UnboundedReceiver<AgentEvent>, RuntimeError> {
        // 复用 ChatManager 的 send_raw_stream：返回原始 StreamLine 接收端，
        // 不 spawn chat:// 事件发射任务（与现有 send 并存的加法式入口）。
        let agent_id: AgentId = handle.agent_id.clone();
        let spec = self
            .start_specs
            .lock()
            .map_err(|e| RuntimeError(format!("SdkRuntime start specs lock poisoned: {e}")))?
            .get(&handle.agent_id)
            .cloned()
            .ok_or_else(|| RuntimeError(format!("SdkRuntime missing start spec for agent {}", handle.agent_id)))?;
        let (method, params) = build_sdk_send_request(&spec, prompt)?;

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
                                // files_modified 恒为空：StreamLine 协议不携带
                                // 文件变更信息，daemon 的 done 信号只有 success/error。
                                // 文件级变更检测必须由 Coordinator 在 done 后做
                                // 工作区 diff（或读 vcs 状态）——此处不是解析器 bug。
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
        self.start_specs
            .lock()
            .map_err(|e| RuntimeError(format!("SdkRuntime start specs lock poisoned: {e}")))?
            .remove(&handle.agent_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_snapshot_preserves_validated_raw_payload() {
        let payload = r#"{"type":"assistant","uuid":"msg-1","message":{"content":[{"type":"text","text":"hello"},{"type":"tool_use","id":"tool-1","name":"read","input":{"path":"src/lib.rs"}}]}}"#;

        let event = parse_stream_line(&format!("[MESSAGE] {payload}")).expect("MESSAGE event");

        match event {
            AgentEvent::MessageRaw { json } => assert_eq!(json, payload),
            other => panic!("expected raw message, got {other:?}"),
        }
    }

    #[test]
    fn sdk_request_builder_routes_codex_with_model_and_cwd() {
        let spec = RuntimeStartSpec {
            agent_id: "worker-1".to_string(),
            cwd: std::path::PathBuf::from("/tmp/worktree"),
            model: "gpt-5.3-codex".to_string(),
            provider: "codex".to_string(),
        };

        let (method, params) = build_sdk_send_request(&spec, "implement it".to_string())
            .expect("codex is a supported Hermes SDK provider");

        assert_eq!(method, "codex.send");
        assert_eq!(params["message"], "implement it");
        assert_eq!(params["model"], "gpt-5.3-codex");
        assert_eq!(params["cwd"], "/tmp/worktree");
        assert_eq!(params["streaming"], true);
    }

    #[test]
    fn sdk_request_builder_rejects_unknown_provider() {
        let spec = RuntimeStartSpec {
            agent_id: "worker-1".to_string(),
            cwd: std::path::PathBuf::from("/tmp/worktree"),
            model: "unknown".to_string(),
            provider: "gemini".to_string(),
        };

        let err = build_sdk_send_request(&spec, "implement it".to_string())
            .expect_err("unknown providers must never be routed through Claude");

        assert!(err.0.contains("unsupported Hermes provider"));
    }

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

    // ===== 元数据/诊断标签必须返回 None（default-deny 闸门）=====
    //
    // 背景：daemon 把大量诊断/元数据标签（[USAGE]/[SESSION_ID]/[DIAG]/...）通过
    // console.log → process.stdout.write 写入活跃请求的 StreamLine::Line。
    // 这些不是 agent 输出，绝不能污染结构化 AgentEvent 流（否则 Coordinator/
    // Supervisor 的 structured_events=true 信号会被破坏）。
    // 参见 src-tauri/resources/ai-bridge/daemon.js:199-245（stdout 拦截 +
    // console.log 重定向）。

    #[test]
    fn usage_metadata_label_is_dropped() {
        // [USAGE] 由 message-utils.js:177 / utils/usage-utils.js:72 经 stdout 输出。
        assert!(parse_stream_line(r#"[USAGE] {"input_tokens":10,"output_tokens":5}"#).is_none());
    }

    #[test]
    fn session_id_metadata_label_is_dropped() {
        // [SESSION_ID] 由 message-sender.js:256 / message-sender-anthropic.js:77 输出。
        assert!(parse_stream_line("[SESSION_ID] abc-123").is_none());
    }

    #[test]
    fn stream_start_metadata_label_is_dropped() {
        // [STREAM_START] 由 message-sender.js:152 / persistent-query-service.js:293 输出。
        assert!(parse_stream_line("[STREAM_START]").is_none());
    }

    #[test]
    fn stream_end_metadata_label_is_dropped() {
        // [STREAM_END] 由 message-sender.js:353,417 / persistent-query-service.js:345,521 输出。
        assert!(parse_stream_line("[STREAM_END]").is_none());
    }

    #[test]
    fn message_start_end_metadata_labels_are_dropped() {
        // [MESSAGE_START]/[MESSAGE_END] 由 message-sender-anthropic.js:76,174,233
        // 及 message-sender.js:357 输出。
        assert!(parse_stream_line("[MESSAGE_START]").is_none());
        assert!(parse_stream_line("[MESSAGE_END]").is_none());
    }

    #[test]
    fn model_env_metadata_label_is_dropped() {
        // [MODEL_ENV] 由 utils/model-utils.js:126,129,135 输出（环境变量注入日志）。
        assert!(parse_stream_line("[MODEL_ENV] Set ANTHROPIC_DEFAULT_OPUS_MODEL = gpt-4o").is_none());
    }

    #[test]
    fn resuming_and_resume_wait_metadata_labels_are_dropped() {
        // [RESUMING]/[RESUME_WAIT] 由 message-sender.js:115,117 输出。
        assert!(parse_stream_line("[RESUMING] old-session-id").is_none());
        assert!(parse_stream_line("[RESUME_WAIT] Waiting for session file to appear...").is_none());
    }

    #[test]
    fn retry_metadata_label_is_dropped() {
        // [RETRY] 由 message-sender.js:316,348,389,390 输出（自动重试日志）。
        assert!(parse_stream_line("[RETRY] Attempt 1/3 after error: timeout").is_none());
    }

    #[test]
    fn diag_metadata_label_is_dropped() {
        // [DIAG] 由 message-sender.js:128,443,444 输出（诊断日志）。
        assert!(parse_stream_line("[DIAG] ========== sendMessage() START ==========").is_none());
    }

    #[test]
    fn debug_metadata_label_is_dropped() {
        // [DEBUG] 由 message-sender-anthropic.js / message-sender.js 大量输出。
        assert!(parse_stream_line("[DEBUG] Model: gpt-4o").is_none());
    }

    #[test]
    fn warn_and_warning_metadata_labels_are_dropped() {
        // [WARN]/[WARNING] 由 codex-utils.js debugLog / message-sender.js:458 等输出。
        assert!(parse_stream_line("[WARN] something").is_none());
        assert!(parse_stream_line("[WARNING] chdir failed: EPERM").is_none());
    }

    #[test]
    fn block_reset_metadata_label_is_dropped() {
        // [BLOCK_RESET] 由 stream-event-processor.js:43 / message-sender.js:173 输出。
        assert!(parse_stream_line("[BLOCK_RESET]").is_none());
    }

    #[test]
    fn content_metadata_label_distinct_from_content_delta_is_dropped() {
        // [CONTENT]（截断的错误内容预览）与 [CONTENT_DELTA] 不同：前者是诊断，
        // 后者是 agent 文本增量。由 stream-event-processor.js:105 /
        // message-sender.js:270 / message-sender-anthropic.js:159,210 输出。
        assert!(parse_stream_line("[CONTENT] Error: rate limited").is_none());
    }

    #[test]
    fn thinking_metadata_label_distinct_from_thinking_delta_is_dropped() {
        // [THINKING]/[THINKING_START]/[THINKING_HINT] 是思考相关的诊断/提示，
        // 与 [THINKING_DELTA]（真正的思考增量）不同。分别由
        // message-sender.js:288,201、message-sender-anthropic.js、
        // codex/message-service.js:328 输出。
        assert!(parse_stream_line("[THINKING] pondering").is_none());
        assert!(parse_stream_line("[THINKING_START]").is_none());
        assert!(parse_stream_line("[THINKING_HINT] Codex did not return reasoning items.").is_none());
    }

    #[test]
    fn thread_id_metadata_label_is_dropped() {
        // [THREAD_ID] 由 codex/codex-event-handler.js:670 输出。
        assert!(parse_stream_line("[THREAD_ID] thread_abc").is_none());
    }

    #[test]
    fn perm_debug_metadata_label_is_dropped() {
        // [PERM_DEBUG] 由 codex-utils.js:52,68 / codex/message-service.js:168,181,182
        // 通过 debugLog → console.log 输出（CODEX_DEBUG_LEVEL >= 4 时）。
        assert!(parse_stream_line("[PERM_DEBUG] Codex permission config: {}").is_none());
    }

    #[test]
    fn unknown_bracket_label_is_dropped_default_deny() {
        // 未知 [LABEL] 行：default-deny（返回 None）。比猜测成 TextDelta 更安全，
        // 因为 daemon 任何新加的诊断标签都不应自动混入 agent 输出流。
        assert!(parse_stream_line("[SOME_FUTURE_TAG] payload").is_none());
        assert!(parse_stream_line("[NEW_META] x").is_none());
    }
}
