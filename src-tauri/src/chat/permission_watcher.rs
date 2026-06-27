use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Runtime};

pub const DEFAULT_PERMISSION_SESSION_ID: &str = "default";

/// Polls the permission directory for request files written by the daemon
/// (claude-agent-sdk), parses them, and emits Tauri events to the frontend.
///
/// Protocol: daemon writes `request-<sessionId>-<requestId>.json`,
/// `ask-user-question-<sessionId>-<requestId>.json` or
/// `plan-approval-<sessionId>-<requestId>.json` → watcher reads + emits event →
/// frontend responds via Tauri command → writes response file → daemon reads.
pub struct PermissionWatcher<R: Runtime> {
    permission_dir: PathBuf,
    session_id: String,
    app: AppHandle<R>,
    stop: Arc<AtomicBool>,
}

// ─── Request Types ──────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AskUserQuestionRequest {
    pub request_id: String,
    #[serde(default)]
    pub session_id: String,
    pub tool_name: String,
    pub questions: Vec<Question>,
    pub timestamp: String,
    pub cwd: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    pub question: String,
    pub header: String,
    pub options: Vec<QuestionOption>,
    pub multi_select: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct QuestionOption {
    pub label: String,
    pub description: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PlanApprovalRequest {
    pub request_id: String,
    #[serde(default)]
    pub session_id: String,
    pub tool_name: String,
    pub plan: String,
    pub allowed_prompts: Vec<AllowedPrompt>,
    pub timestamp: String,
    pub cwd: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AllowedPrompt {
    pub tool: String,
    pub prompt: String,
}

fn empty_tool_inputs() -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::new())
}

fn deserialize_tool_inputs<'de, D>(deserializer: D) -> Result<serde_json::Value, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value =
        Option::<serde_json::Value>::deserialize(deserializer)?.unwrap_or_else(empty_tool_inputs);
    if value.is_object() {
        Ok(value)
    } else {
        Ok(empty_tool_inputs())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ToolPermissionRequest {
    pub request_id: String,
    #[serde(default)]
    pub session_id: String,
    pub tool_name: String,
    #[serde(
        default = "empty_tool_inputs",
        deserialize_with = "deserialize_tool_inputs"
    )]
    pub inputs: serde_json::Value,
    pub timestamp: String,
    pub cwd: String,
}

// ─── Response Types (for Tauri commands to serialize) ─────────────────────

#[derive(Serialize)]
struct AskUserQuestionResponse {
    #[serde(rename = "requestId")]
    request_id: String,
    answers: HashMap<String, String>,
}

#[derive(Serialize)]
struct PlanApprovalResponse {
    #[serde(rename = "requestId")]
    request_id: String,
    approved: bool,
    #[serde(rename = "targetMode")]
    target_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

#[derive(Serialize)]
struct ToolPermissionResponse {
    allow: bool,
}

fn normalize_session_id_value(session_id: &str, fallback: &str) -> String {
    let trimmed = session_id.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn permission_response_session_id(session_id: Option<String>) -> String {
    normalize_session_id_value(
        session_id.as_deref().unwrap_or_default(),
        DEFAULT_PERMISSION_SESSION_ID,
    )
}

fn fill_missing_request_session_id(session_id: &mut String, fallback: &str) {
    *session_id = normalize_session_id_value(session_id, fallback);
}

impl<R: Runtime> PermissionWatcher<R> {
    pub fn new(permission_dir: PathBuf, session_id: String, app: AppHandle<R>) -> Self {
        Self {
            permission_dir,
            session_id,
            app,
            stop: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Start polling in a background thread. Polls every 100ms until stop() is called.
    pub fn start(&self) {
        let dir = self.permission_dir.clone();
        let session_id = self.session_id.clone();
        let app = self.app.clone();
        let stop = self.stop.clone();

        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                Self::poll_once(&dir, &session_id, &app);
                std::thread::sleep(Duration::from_millis(100));
            }
        });
    }

    /// Stop the polling thread. Reserved for future watcher lifecycle control.
    #[allow(dead_code)]
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    /// Single poll cycle: scan for request files, parse, emit event, delete request.
    fn poll_once(permission_dir: &Path, session_id: &str, app: &AppHandle<R>) {
        // Scan AskUserQuestion requests
        if let Ok(entries) = std::fs::read_dir(permission_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with(&format!("ask-user-question-{}-", session_id))
                        && name.ends_with(".json")
                        && !name.contains("-response-")
                    {
                        Self::handle_ask_user_question(&path, session_id, app);
                    } else if name.starts_with(&format!("plan-approval-{}-", session_id))
                        && name.ends_with(".json")
                        && !name.contains("-response-")
                    {
                        Self::handle_plan_approval(&path, session_id, app);
                    } else if name.starts_with(&format!("request-{}-", session_id))
                        && name.ends_with(".json")
                    {
                        Self::handle_tool_permission(&path, session_id, app);
                    }
                }
            }
        }
    }

    fn handle_ask_user_question(file: &Path, session_id: &str, app: &AppHandle<R>) {
        match std::fs::read_to_string(file) {
            Ok(content) => match serde_json::from_str::<AskUserQuestionRequest>(&content) {
                Ok(mut req) => {
                    fill_missing_request_session_id(&mut req.session_id, session_id);
                    eprintln!("[PermissionWatcher] AskUserQuestion: {}", req.request_id);
                    let _ = app.emit("permission://ask-user-question", req);
                    // Delete request file after emitting (daemon won't re-read it).
                    let _ = std::fs::remove_file(file);
                }
                Err(e) => eprintln!("[PermissionWatcher] Parse error: {e}"),
            },
            Err(e) => eprintln!("[PermissionWatcher] Read error: {e}"),
        }
    }

    fn handle_plan_approval(file: &Path, session_id: &str, app: &AppHandle<R>) {
        match std::fs::read_to_string(file) {
            Ok(content) => match serde_json::from_str::<PlanApprovalRequest>(&content) {
                Ok(mut req) => {
                    fill_missing_request_session_id(&mut req.session_id, session_id);
                    eprintln!("[PermissionWatcher] PlanApproval: {}", req.request_id);
                    let _ = app.emit("permission://plan-approval", req);
                    let _ = std::fs::remove_file(file);
                }
                Err(e) => eprintln!("[PermissionWatcher] Parse error: {e}"),
            },
            Err(e) => eprintln!("[PermissionWatcher] Read error: {e}"),
        }
    }

    fn handle_tool_permission(file: &Path, session_id: &str, app: &AppHandle<R>) {
        match std::fs::read_to_string(file) {
            Ok(content) => match serde_json::from_str::<ToolPermissionRequest>(&content) {
                Ok(mut req) => {
                    fill_missing_request_session_id(&mut req.session_id, session_id);
                    eprintln!(
                        "[PermissionWatcher] ToolPermission: {} {}",
                        req.tool_name, req.request_id
                    );
                    let _ = app.emit("permission://tool", req);
                    let _ = std::fs::remove_file(file);
                }
                Err(e) => eprintln!("[PermissionWatcher] Parse error: {e}"),
            },
            Err(e) => eprintln!("[PermissionWatcher] Read error: {e}"),
        }
    }
}

/// Write AskUserQuestion response file.
pub fn write_ask_user_question_response(
    permission_dir: &Path,
    session_id: &str,
    request_id: &str,
    answers: HashMap<String, String>,
) -> Result<(), String> {
    let filename = format!(
        "ask-user-question-response-{}-{}.json",
        session_id, request_id
    );
    let path = permission_dir.join(filename);
    let resp = AskUserQuestionResponse {
        request_id: request_id.to_string(),
        answers,
    };
    let json = serde_json::to_string_pretty(&resp).map_err(|e| format!("序列化失败: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("写入响应文件失败: {e}"))?;
    Ok(())
}

/// Write PlanApproval response file.
pub fn write_plan_approval_response(
    permission_dir: &Path,
    session_id: &str,
    request_id: &str,
    approved: bool,
    target_mode: String,
    message: Option<String>,
) -> Result<(), String> {
    let filename = format!("plan-approval-response-{}-{}.json", session_id, request_id);
    let path = permission_dir.join(filename);
    let resp = PlanApprovalResponse {
        request_id: request_id.to_string(),
        approved,
        target_mode,
        message,
    };
    let json = serde_json::to_string_pretty(&resp).map_err(|e| format!("序列化失败: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("写入响应文件失败: {e}"))?;
    Ok(())
}

/// Write generic tool permission response file.
pub fn write_tool_permission_response(
    permission_dir: &Path,
    session_id: &str,
    request_id: &str,
    allow: bool,
) -> Result<(), String> {
    let filename = format!("response-{}-{}.json", session_id, request_id);
    let path = permission_dir.join(filename);
    let resp = ToolPermissionResponse { allow };
    let json = serde_json::to_string_pretty(&resp).map_err(|e| format!("序列化失败: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("写入响应文件失败: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_tool_permission_response_writes_bridge_allow_payload() {
        let dir = std::env::temp_dir().join(format!(
            "ccg-switch-permission-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).expect("create temp permission dir");

        write_tool_permission_response(&dir, "default", "perm-1", true).expect("write response");

        let path = dir.join("response-default-perm-1.json");
        let content = std::fs::read_to_string(&path).expect("read response file");
        let value: serde_json::Value = serde_json::from_str(&content).expect("parse response json");

        assert_eq!(value.get("allow").and_then(|v| v.as_bool()), Some(true));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tool_permission_request_allows_missing_inputs() {
        let request = serde_json::json!({
            "requestId": "perm-2",
            "toolName": "Bash",
            "timestamp": "2026-06-18T09:00:00.000Z",
            "cwd": "C:/guodevelop/ccg-switch"
        });

        let parsed: ToolPermissionRequest =
            serde_json::from_value(request).expect("missing inputs should default");

        assert_eq!(parsed.inputs, empty_tool_inputs());
    }

    #[test]
    fn permission_response_session_id_defaults_missing_or_blank() {
        assert_eq!(
            permission_response_session_id(None),
            DEFAULT_PERMISSION_SESSION_ID
        );
        assert_eq!(
            permission_response_session_id(Some(" \n\t ".to_string())),
            DEFAULT_PERMISSION_SESSION_ID
        );
    }

    #[test]
    fn permission_response_session_id_trims_custom_session() {
        assert_eq!(
            permission_response_session_id(Some("  session-custom  ".to_string())),
            "session-custom"
        );
    }

    #[test]
    fn ask_user_question_request_preserves_session_id() {
        let request = serde_json::json!({
            "requestId": "ask-1",
            "sessionId": "session-custom",
            "toolName": "AskUserQuestion",
            "questions": [],
            "timestamp": "2026-06-18T09:00:00.000Z",
            "cwd": "C:/guodevelop/ccg-switch"
        });

        let parsed: AskUserQuestionRequest =
            serde_json::from_value(request).expect("parse ask request");
        let value = serde_json::to_value(parsed).expect("serialize ask request");

        assert_eq!(
            value.get("sessionId").and_then(|v| v.as_str()),
            Some("session-custom")
        );
    }

    #[test]
    fn plan_approval_request_preserves_session_id() {
        let request = serde_json::json!({
            "requestId": "plan-1",
            "sessionId": "session-custom",
            "toolName": "ExitPlanMode",
            "plan": "1. Inspect\n2. Verify",
            "allowedPrompts": [],
            "timestamp": "2026-06-18T09:00:00.000Z",
            "cwd": "C:/guodevelop/ccg-switch"
        });

        let parsed: PlanApprovalRequest =
            serde_json::from_value(request).expect("parse plan request");
        let value = serde_json::to_value(parsed).expect("serialize plan request");

        assert_eq!(
            value.get("sessionId").and_then(|v| v.as_str()),
            Some("session-custom")
        );
    }

    #[test]
    fn tool_permission_request_preserves_session_id() {
        let request = serde_json::json!({
            "requestId": "perm-3",
            "sessionId": "session-custom",
            "toolName": "Bash",
            "inputs": {"command": "npm test"},
            "timestamp": "2026-06-18T09:00:00.000Z",
            "cwd": "C:/guodevelop/ccg-switch"
        });

        let parsed: ToolPermissionRequest =
            serde_json::from_value(request).expect("parse tool request");
        let value = serde_json::to_value(parsed).expect("serialize tool request");

        assert_eq!(
            value.get("sessionId").and_then(|v| v.as_str()),
            Some("session-custom")
        );
    }
}
