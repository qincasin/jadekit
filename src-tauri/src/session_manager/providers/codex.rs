use super::utils::{
    home_dir, sanitize_session_markdown_text, sanitize_session_text, truncate_text,
};
use crate::session_manager::{
    MessageWindowBuilder, SessionMeta, UnifiedSessionMessage, UnifiedSessionMessageWindow,
};
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

const TOOL_RESULT_CONTENT: &str = "[tool_result]";

static CODEX_SESSION_META_CACHE: Lazy<Mutex<HashMap<PathBuf, CachedCodexSessionFile>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static CODEX_SESSION_FILE_LIST_CACHE: Lazy<Mutex<HashMap<PathBuf, CachedCodexSessionFileList>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Clone, Debug, PartialEq, Eq)]
struct CodexSessionFileStamp {
    len: u64,
    modified_at: i64,
    created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CodexSessionDirectoryStamp {
    path: PathBuf,
    modified_at: i64,
}

#[derive(Clone, Debug)]
struct CachedCodexSessionFileList {
    directory_stamps: Vec<CodexSessionDirectoryStamp>,
    files: Vec<PathBuf>,
}

#[derive(Clone, Debug)]
struct CachedCodexSessionFile {
    stamp: CodexSessionFileStamp,
    session_id: String,
    title: Option<String>,
    project_dir: Option<String>,
    created_at: i64,
    last_active_at: i64,
    source_path: String,
    title_scanned: bool,
}

impl CachedCodexSessionFile {
    fn matches_project(&self, normalized_project_filter: Option<&str>) -> bool {
        match normalized_project_filter {
            Some(filter) => self
                .project_dir
                .as_ref()
                .map(|dir| normalize_session_project_path(dir) == filter)
                .unwrap_or(false),
            None => true,
        }
    }

    fn needs_title_for(&self, normalized_project_filter: Option<&str>) -> bool {
        self.matches_project(normalized_project_filter) && !self.title_scanned
    }

    fn to_session_meta(&self) -> SessionMeta {
        SessionMeta {
            provider_id: "codex".to_string(),
            session_id: self.session_id.clone(),
            title: self.title.clone(),
            summary: None,
            project_dir: self.project_dir.clone(),
            created_at: self.created_at,
            last_active_at: self.last_active_at,
            source_path: self.source_path.clone(),
            resume_command: Some(format!("codex resume {}", self.session_id)),
            pinned: false,
            archived: false,
            unread: false,
        }
    }
}

/// 按项目路径扫描 Codex 会话（扫描全部文件但只返回匹配项目的会话）
pub fn scan_codex_sessions_for_project(project_path: &str) -> Vec<SessionMeta> {
    let normalized_target = normalize_session_project_path(project_path);
    scan_codex_sessions_inner(Some(&normalized_target))
}

/// 扫描 Codex 会话目录，返回所有会话元数据
fn scan_codex_sessions_inner(normalized_project_filter: Option<&str>) -> Vec<SessionMeta> {
    let home = match home_dir() {
        Some(h) => h,
        None => return Vec::new(),
    };

    let sessions_dir = home.join(".codex").join("sessions");
    if !sessions_dir.exists() {
        return Vec::new();
    }

    let uuid_re = Regex::new(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}")
        .expect("invalid regex");

    // Codex 会话存储在 YYYY/MM/DD/ 子目录下；文件列表可按目录 stamp 复用。
    let jsonl_files = collect_jsonl_files_cached(&sessions_dir);

    let mut sessions = Vec::new();

    for path in jsonl_files {
        let filename = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        // 从文件名中提取 UUID 作为 session_id
        let session_id = match uuid_re.find(&filename) {
            Some(m) => m.as_str().to_string(),
            None => continue,
        };

        let Some(session) =
            read_codex_session_meta_cached(&path, &session_id, normalized_project_filter)
        else {
            continue;
        };
        if session.matches_project(normalized_project_filter) {
            sessions.push(session.to_session_meta());
        }
    }

    sessions
}

fn normalize_session_project_path(path: &str) -> String {
    path.replace('\\', "/")
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

fn timestamp_millis(time: std::time::SystemTime) -> Option<i64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as i64)
}

fn codex_file_stamp(path: &Path) -> Option<CodexSessionFileStamp> {
    let metadata = fs::metadata(path).ok()?;
    let created_at = metadata
        .created()
        .ok()
        .and_then(timestamp_millis)
        .unwrap_or(0);
    let modified_at = metadata
        .modified()
        .ok()
        .and_then(timestamp_millis)
        .unwrap_or(created_at);

    Some(CodexSessionFileStamp {
        len: metadata.len(),
        modified_at,
        created_at,
    })
}

fn codex_directory_stamp(path: &Path) -> Option<CodexSessionDirectoryStamp> {
    let metadata = fs::metadata(path).ok()?;
    let modified_at = metadata
        .modified()
        .ok()
        .and_then(timestamp_millis)
        .unwrap_or(0);

    Some(CodexSessionDirectoryStamp {
        path: path.to_path_buf(),
        modified_at,
    })
}

fn collect_codex_session_directory_stamps(
    dir: &Path,
    stamps: &mut Vec<CodexSessionDirectoryStamp>,
) {
    let Some(stamp) = codex_directory_stamp(dir) else {
        return;
    };
    stamps.push(stamp);

    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    let mut child_dirs: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    child_dirs.sort();

    for child in child_dirs {
        collect_codex_session_directory_stamps(&child, stamps);
    }
}

fn codex_session_directory_stamps(sessions_dir: &Path) -> Option<Vec<CodexSessionDirectoryStamp>> {
    if !sessions_dir.is_dir() {
        return None;
    }

    let mut stamps = Vec::new();
    collect_codex_session_directory_stamps(sessions_dir, &mut stamps);
    if stamps.is_empty() {
        None
    } else {
        Some(stamps)
    }
}

fn get_cached_codex_session_file_list(
    sessions_dir: &Path,
    directory_stamps: &[CodexSessionDirectoryStamp],
) -> Option<Vec<PathBuf>> {
    CODEX_SESSION_FILE_LIST_CACHE
        .lock()
        .ok()
        .and_then(|cache| cache.get(sessions_dir).cloned())
        .filter(|cached| cached.directory_stamps.as_slice() == directory_stamps)
        .map(|cached| cached.files)
}

fn remember_cached_codex_session_file_list(
    sessions_dir: &Path,
    directory_stamps: Vec<CodexSessionDirectoryStamp>,
    files: Vec<PathBuf>,
) {
    if let Ok(mut cache) = CODEX_SESSION_FILE_LIST_CACHE.lock() {
        cache.insert(
            sessions_dir.to_path_buf(),
            CachedCodexSessionFileList {
                directory_stamps,
                files,
            },
        );
    }
}

fn collect_jsonl_files_cached(sessions_dir: &Path) -> Vec<PathBuf> {
    let Some(directory_stamps) = codex_session_directory_stamps(sessions_dir) else {
        let mut files = Vec::new();
        collect_jsonl_files(sessions_dir, &mut files);
        files.sort();
        return files;
    };

    if let Some(files) = get_cached_codex_session_file_list(sessions_dir, &directory_stamps) {
        return files;
    }

    let mut files = Vec::new();
    collect_jsonl_files(sessions_dir, &mut files);
    files.sort();
    remember_cached_codex_session_file_list(sessions_dir, directory_stamps, files.clone());
    files
}

fn get_cached_codex_session_file(
    path: &Path,
    stamp: &CodexSessionFileStamp,
) -> Option<CachedCodexSessionFile> {
    CODEX_SESSION_META_CACHE
        .lock()
        .ok()
        .and_then(|cache| cache.get(path).cloned())
        .filter(|cached| cached.stamp == *stamp)
}

fn remember_cached_codex_session_file(path: &Path, session: CachedCodexSessionFile) {
    if let Ok(mut cache) = CODEX_SESSION_META_CACHE.lock() {
        cache.insert(path.to_path_buf(), session);
    }
}

fn read_codex_session_meta_cached(
    path: &Path,
    session_id: &str,
    normalized_project_filter: Option<&str>,
) -> Option<CachedCodexSessionFile> {
    let stamp = codex_file_stamp(path)?;
    if let Some(cached) = get_cached_codex_session_file(path, &stamp) {
        if !cached.needs_title_for(normalized_project_filter) {
            return Some(cached);
        }
    }

    let parsed = read_codex_session_file_meta(path, session_id, stamp, normalized_project_filter)?;
    remember_cached_codex_session_file(path, parsed.clone());
    Some(parsed)
}

fn read_codex_session_file_meta(
    path: &Path,
    session_id: &str,
    stamp: CodexSessionFileStamp,
    normalized_project_filter: Option<&str>,
) -> Option<CachedCodexSessionFile> {
    let file = fs::File::open(path).ok()?;
    let reader = BufReader::new(file);
    let mut project_dir: Option<String> = None;
    let mut best_title: Option<String> = None;
    let mut fallback_title: Option<String> = None;
    let mut should_collect_title = normalized_project_filter.is_none();

    for line in reader.lines() {
        let line = match line {
            Ok(line) => line,
            Err(_) => continue,
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let json: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let line_type = json
            .get("type")
            .and_then(|value| value.as_str())
            .unwrap_or("");

        if line_type == "session_meta" {
            if let Some(payload) = json.get("payload") {
                if project_dir.is_none() {
                    project_dir = payload
                        .get("cwd")
                        .and_then(|value| value.as_str())
                        .map(|value| value.to_string());
                    should_collect_title = match (normalized_project_filter, project_dir.as_deref())
                    {
                        (Some(filter), Some(dir)) => normalize_session_project_path(dir) == filter,
                        (Some(_), None) => false,
                        (None, _) => true,
                    };
                    if normalized_project_filter.is_some() && !should_collect_title {
                        break;
                    }
                }
            }
            continue;
        }

        if line_type != "response_item" {
            continue;
        }
        if normalized_project_filter.is_some() && project_dir.is_none() {
            continue;
        }
        if !should_collect_title {
            continue;
        }
        if best_title.is_some() {
            break;
        }

        let Some(payload) = json.get("payload") else {
            continue;
        };
        let payload_type = payload.get("type").and_then(|value| value.as_str());
        let role = payload.get("role").and_then(|value| value.as_str());
        if payload_type != Some("message") || role != Some("user") {
            continue;
        }

        let Some(content) = payload.get("content") else {
            continue;
        };
        let text = extract_codex_content(content);
        if text.is_empty() {
            continue;
        }

        let cleaned = sanitize_session_text(&text);
        if cleaned.is_empty() {
            continue;
        }
        if is_system_instruction(&cleaned) {
            if fallback_title.is_none() {
                fallback_title = Some(truncate_text(&cleaned, 80));
            }
        } else {
            best_title = Some(truncate_text(&cleaned, 80));
        }
    }

    Some(CachedCodexSessionFile {
        stamp: stamp.clone(),
        session_id: session_id.to_string(),
        title: best_title.or(fallback_title),
        project_dir,
        created_at: stamp.created_at,
        last_active_at: stamp.modified_at,
        source_path: path.to_string_lossy().to_string(),
        title_scanned: should_collect_title,
    })
}

/// 加载 Codex 会话的消息列表
pub fn load_codex_messages(source_path: &str) -> Result<Vec<UnifiedSessionMessage>, String> {
    let file = fs::File::open(source_path).map_err(|e| format!("打开文件失败: {}", e))?;
    let reader = BufReader::new(file);
    let mut messages = Vec::new();

    for line in reader.lines() {
        let line = line.map_err(|e| format!("读取行失败: {}", e))?;
        if let Some(message) = parse_codex_history_line(&line, messages.len()) {
            messages.push(message);
        }
    }

    Ok(messages)
}

pub fn load_codex_message_window(
    source_path: &str,
    tail_limit: usize,
) -> Result<UnifiedSessionMessageWindow, String> {
    let file = fs::File::open(source_path).map_err(|e| format!("打开文件失败: {}", e))?;
    let reader = BufReader::new(file);
    let mut window = MessageWindowBuilder::new(tail_limit);

    for line in reader.lines() {
        let line = line.map_err(|e| format!("读取行失败: {}", e))?;
        if let Some(message) = parse_codex_history_line(&line, window.next_index()) {
            window.push(message);
        }
    }

    Ok(window.finish())
}

fn parse_codex_history_line(line: &str, message_index: usize) -> Option<UnifiedSessionMessage> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    let json: serde_json::Value = serde_json::from_str(line).ok()?;

    let line_type = json.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if line_type != "response_item" {
        return None;
    }

    let payload = json.get("payload")?;

    let ts = json
        .get("timestamp")
        .or_else(|| json.get("ts"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let payload_type = payload.get("type").and_then(|v| v.as_str());
    match payload_type {
        Some("message") => {
            let role = payload
                .get("role")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let role = normalize_role(role);

            let blocks = match payload.get("content") {
                Some(c) => codex_message_content_blocks(c),
                None => Vec::new(),
            };
            let content = extract_text_from_blocks(&blocks);
            if content.is_empty() && blocks.is_empty() {
                return None;
            }

            Some(UnifiedSessionMessage {
                role: role.to_string(),
                content,
                ts: ts.clone(),
                raw: if blocks.is_empty() {
                    None
                } else {
                    Some(message_raw(role, blocks, ts))
                },
            })
        }
        Some("reasoning") => {
            let thinking = extract_codex_reasoning(payload);
            if thinking.is_empty() {
                return None;
            }

            Some(UnifiedSessionMessage {
                role: "assistant".to_string(),
                content: String::new(),
                ts: ts.clone(),
                raw: Some(message_raw(
                    "assistant",
                    vec![json!({
                        "type": "thinking",
                        "thinking": thinking,
                    })],
                    ts,
                )),
            })
        }
        Some("function_call") | Some("custom_tool_call") => {
            let tool_id = codex_tool_id(payload, &format!("codex-tool-{}", message_index));
            let (name, input) = match extract_codex_patch(payload) {
                Some(patch) => ("apply_patch", json!({ "patch": patch })),
                None => {
                    let name = payload
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("function_call");
                    let input = if payload_type == Some("custom_tool_call") {
                        parse_codex_custom_tool_input(payload.get("input"))
                    } else {
                        parse_codex_arguments(payload.get("arguments"))
                    };
                    (name, input)
                }
            };

            Some(UnifiedSessionMessage {
                role: "assistant".to_string(),
                content: String::new(),
                ts: ts.clone(),
                raw: Some(message_raw(
                    "assistant",
                    vec![json!({
                        "type": "tool_use",
                        "id": tool_id,
                        "name": name,
                        "input": input,
                    })],
                    ts,
                )),
            })
        }
        Some("function_call_output") | Some("custom_tool_call_output") => {
            let tool_id = codex_tool_id(payload, &format!("codex-tool-result-{}", message_index));
            let output = extract_codex_output(payload);
            let is_error = payload
                .get("is_error")
                .and_then(|v| v.as_bool())
                .unwrap_or_else(|| {
                    payload
                        .get("status")
                        .and_then(|v| v.as_str())
                        .map(|status| status.eq_ignore_ascii_case("failed"))
                        .unwrap_or(false)
                });

            Some(UnifiedSessionMessage {
                role: "user".to_string(),
                content: TOOL_RESULT_CONTENT.to_string(),
                ts: ts.clone(),
                raw: Some(message_raw(
                    "user",
                    vec![json!({
                        "type": "tool_result",
                        "tool_use_id": tool_id,
                        "content": output,
                        "is_error": is_error,
                    })],
                    ts,
                )),
            })
        }
        _ => None,
    }
}

/// 从 Codex content 字段提取文本（支持 string 和 array 格式）
fn extract_codex_content(content: &serde_json::Value) -> String {
    if let Some(text) = content.as_str() {
        return text.to_string();
    }

    if let Some(items) = content.as_array() {
        let mut parts = Vec::new();
        for item in items {
            if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                if !text.trim().is_empty() {
                    parts.push(text.to_string());
                }
            }
        }
        return parts.join("\n");
    }

    String::new()
}

fn text_from_value(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return Some(text.to_string());
    }

    for key in ["text", "content", "output"] {
        if let Some(text) = value.get(key).and_then(|v| v.as_str()) {
            return Some(text.to_string());
        }
    }

    None
}

fn codex_message_content_blocks(content: &Value) -> Vec<Value> {
    if let Some(text) = content.as_str() {
        let clean = sanitize_session_markdown_text(text);
        if clean.is_empty() {
            return Vec::new();
        }
        return vec![json!({"type": "text", "text": clean})];
    }

    let Some(items) = content.as_array() else {
        return Vec::new();
    };
    let has_image = items.iter().any(|item| {
        let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
        matches!(item_type, "image" | "input_image")
    });

    items
        .iter()
        .filter_map(|item| {
            let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if matches!(item_type, "image" | "input_image") {
                return normalize_codex_image_block(item, item_type);
            }

            if matches!(item_type, "text" | "input_text" | "output_text") {
                let text = text_from_value(item)?;
                let clean = sanitize_session_markdown_text(&text);
                if clean.is_empty() {
                    return None;
                }
                if has_image && is_codex_image_placeholder_text(&clean) {
                    return None;
                }
                return Some(json!({"type": "text", "text": clean}));
            }

            None
        })
        .collect()
}

fn is_codex_image_placeholder_text(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.eq_ignore_ascii_case("</image>") {
        return true;
    }
    if !trimmed.to_ascii_lowercase().starts_with("<image") {
        return false;
    }
    let without_open = match trimmed.find('>') {
        Some(index) => &trimmed[index + 1..],
        None => return false,
    };
    without_open.trim().is_empty() || without_open.trim().eq_ignore_ascii_case("</image>")
}

fn normalize_codex_image_block(item: &Value, item_type: &str) -> Option<Value> {
    let mut object = item.as_object()?.clone();
    object.insert(
        "type".to_string(),
        json!(if item_type == "image" {
            "image"
        } else {
            "input_image"
        }),
    );
    Some(Value::Object(object))
}

fn extract_text_from_blocks(blocks: &[Value]) -> String {
    blocks
        .iter()
        .filter_map(|block| {
            if block.get("type").and_then(|v| v.as_str()) == Some("text") {
                block.get("text").and_then(|v| v.as_str())
            } else {
                None
            }
        })
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn extract_codex_reasoning(payload: &Value) -> String {
    if let Some(text) = payload.get("content").and_then(text_from_value) {
        return sanitize_session_markdown_text(&text);
    }

    payload
        .get("summary")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(text_from_value)
                .map(|text| sanitize_session_markdown_text(&text))
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn parse_codex_arguments(arguments: Option<&Value>) -> Value {
    let Some(arguments) = arguments else {
        return json!({});
    };

    if let Some(raw) = arguments.as_str() {
        if raw.trim().is_empty() {
            return json!({});
        }
        return match serde_json::from_str::<Value>(raw) {
            Ok(Value::Object(map)) => Value::Object(map),
            Ok(parsed) => json!({"arguments": parsed}),
            Err(_) => json!({"arguments": raw}),
        };
    }

    if arguments.is_object() {
        return arguments.clone();
    }

    json!({"arguments": arguments})
}

fn parse_codex_custom_tool_input(input: Option<&Value>) -> Value {
    let Some(input) = input else {
        return json!({});
    };

    if let Some(raw) = input.as_str() {
        if raw.trim().is_empty() {
            return json!({});
        }
        return json!({ "input": raw });
    }

    if input.is_object() {
        return input.clone();
    }

    json!({ "input": input })
}

fn codex_tool_id(payload: &Value, fallback: &str) -> String {
    payload
        .get("call_id")
        .or_else(|| payload.get("id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| fallback.to_string())
}

fn string_field<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(|v| v.as_str()))
}

fn extract_patch_from_exec_command(command: &str) -> Option<String> {
    let begin = command.find("*** Begin Patch")?;
    let end = command.rfind("*** End Patch")?;
    if end < begin {
        return None;
    }

    let end_index = end + "*** End Patch".len();
    Some(command[begin..end_index].to_string())
}

fn extract_patch_from_value(value: Option<&Value>) -> Option<String> {
    let value = value?;
    if let Some(text) = value.as_str() {
        return text.contains("*** Begin Patch").then(|| text.to_string());
    }

    let object = value.as_object()?;
    for key in ["patch", "input", "content", "command", "cmd", "arguments"] {
        if let Some(text) = object.get(key).and_then(|v| v.as_str()) {
            if text.contains("*** Begin Patch") {
                return if key == "command" || key == "cmd" || key == "arguments" {
                    extract_patch_from_exec_command(text).or_else(|| Some(text.to_string()))
                } else {
                    Some(text.to_string())
                };
            }
        }
    }

    None
}

fn extract_codex_patch(payload: &Value) -> Option<String> {
    let payload_type = payload.get("type").and_then(|v| v.as_str())?;
    let name = payload.get("name").and_then(|v| v.as_str()).unwrap_or("");

    if payload_type == "custom_tool_call" && name == "apply_patch" {
        return extract_patch_from_value(payload.get("input"));
    }

    if payload_type != "function_call" {
        return None;
    }

    if name == "apply_patch" {
        let arguments = parse_codex_arguments(payload.get("arguments"));
        return extract_patch_from_value(Some(&arguments));
    }

    if !matches!(name, "exec_command" | "shell_command" | "shell") {
        return None;
    }

    let arguments = parse_codex_arguments(payload.get("arguments"));
    if let Some(command) = string_field(&arguments, &["cmd", "command"]) {
        return extract_patch_from_exec_command(command);
    }

    if let Some(patch) = extract_patch_from_value(Some(&arguments)) {
        return Some(patch);
    }

    None
}

fn extract_codex_output(payload: &Value) -> String {
    let value = payload.get("output").or_else(|| payload.get("content"));
    let Some(value) = value else {
        return String::new();
    };

    if let Some(text) = text_from_value(value) {
        return text;
    }

    if let Some(items) = value.as_array() {
        let text = items
            .iter()
            .filter_map(text_from_value)
            .collect::<Vec<_>>()
            .join("\n");
        if !text.is_empty() {
            return text;
        }
    }

    value.to_string()
}

fn message_raw(role: &str, blocks: Vec<Value>, ts: Option<String>) -> Value {
    let mut raw = json!({
        "type": role,
        "message": {
            "content": blocks,
        },
    });

    if let Some(ts) = ts {
        raw["timestamp"] = json!(ts);
    }

    raw
}

/// 规范化角色名称
fn normalize_role(role: &str) -> &str {
    match role {
        "user" => "user",
        "assistant" | "system" => role,
        _ => "assistant",
    }
}

/// 判断消息是否为系统指令（如 AGENTS.md、README.md 等文档内容）
fn is_system_instruction(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.starts_with("<permissions instructions>")
        || trimmed.starts_with("<heartbeat>")
        || trimmed.starts_with("<environment_context>")
        || trimmed.starts_with("<workflow-state>")
        || trimmed.starts_with("<codex-mode>")
        || trimmed.starts_with("<app-context>")
        || trimmed.starts_with("<collaboration_mode>")
        || trimmed.starts_with("<skills_instructions>")
        || trimmed.starts_with("<plugins_instructions>")
        || trimmed.starts_with("<turn_aborted>")
        || trimmed.starts_with("<user_action>")
        || trimmed.starts_with("<subagent_notification>")
        || trimmed.starts_with("<agents-instructions>")
        || trimmed.starts_with("<skill>")
        || trimmed.starts_with("Filesystem sandboxing defines which files can be read or written.")
        || trimmed.starts_with("Tools are grouped by namespace")
    {
        return true;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("you are codex, a coding agent")
        || lower.starts_with("you are claude code")
        || lower.starts_with("you are an ai assistant accessed via an api")
    {
        return true;
    }
    // 以 markdown 标题开头（# XXX），通常是 AGENTS.md / README 等系统文档
    if trimmed.starts_with("# ") {
        return true;
    }
    // 包含常见指令文件名关键词
    let lower = trimmed.to_lowercase();
    if lower.starts_with("agents.md") || lower.starts_with("claude.md") {
        return true;
    }
    false
}

/// 轻量扫描：只读取每个 Codex 会话的 cwd 字段，返回去重的项目路径列表
pub fn scan_codex_project_dirs() -> Vec<String> {
    let home = match home_dir() {
        Some(h) => h,
        None => return Vec::new(),
    };
    let sessions_dir = home.join(".codex").join("sessions");
    if !sessions_dir.exists() {
        return Vec::new();
    }

    let files = collect_jsonl_files_cached(&sessions_dir);

    let mut dirs = std::collections::HashSet::new();
    for path in files {
        let file = match fs::File::open(&path) {
            Ok(f) => f,
            Err(_) => continue,
        };
        let reader = BufReader::new(file);
        // 只读前 5 行，cwd 在 session_meta 行
        for line in reader.lines().take(5).flatten() {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
                if json.get("type").and_then(|v| v.as_str()) == Some("session_meta") {
                    if let Some(cwd) = json
                        .get("payload")
                        .and_then(|p| p.get("cwd"))
                        .and_then(|v| v.as_str())
                    {
                        dirs.insert(cwd.to_string());
                        break;
                    }
                }
            }
        }
    }

    dirs.into_iter().collect()
}

/// 递归收集目录下所有 .jsonl 文件
fn collect_jsonl_files(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl_files(&path, files);
        } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            files.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    static CODEX_FILE_LIST_CACHE_TEST_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    fn write_temp_session(content: impl AsRef<[u8]>) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("ccg-switch-codex-{suffix}.jsonl"));
        fs::write(&path, content).expect("write temp codex session");
        path
    }

    fn clear_codex_session_meta_cache() {
        CODEX_SESSION_META_CACHE
            .lock()
            .expect("lock codex session cache")
            .clear();
    }

    fn clear_codex_session_file_list_cache() {
        CODEX_SESSION_FILE_LIST_CACHE
            .lock()
            .expect("lock codex session file list cache")
            .clear();
    }

    fn write_temp_session_tree() -> (PathBuf, PathBuf) {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("ccg-switch-codex-sessions-{suffix}"));
        let day_dir = root.join("2026").join("06").join("18");
        fs::create_dir_all(&day_dir).expect("create codex session tree");
        let session_path = day_dir.join("session-33333333-3333-3333-3333-333333333333.jsonl");
        fs::write(
            &session_path,
            codex_session_with_title("C:/project", "session title"),
        )
        .expect("write codex session tree file");
        (root, session_path)
    }

    fn codex_session_with_title(cwd: &str, title: &str) -> String {
        format!(r#"{{"type":"session_meta","payload":{{"cwd":"{cwd}"}}}}"#)
            + "\n"
            + &format!(
                r#"{{"type":"response_item","payload":{{"type":"message","role":"user","content":[{{"type":"input_text","text":"{title}"}}]}}}}"#
            )
    }

    #[test]
    fn codex_session_file_list_cache_reuses_when_directory_stamps_match() {
        let _guard = CODEX_FILE_LIST_CACHE_TEST_LOCK
            .lock()
            .expect("lock codex file list cache test");
        clear_codex_session_file_list_cache();
        let (root, _) = write_temp_session_tree();
        let stamps = codex_session_directory_stamps(&root).expect("directory stamps");
        let sentinel = root.join("cached-only.jsonl");

        remember_cached_codex_session_file_list(&root, stamps, vec![sentinel.clone()]);
        let files = collect_jsonl_files_cached(&root);
        fs::remove_dir_all(root).ok();

        assert_eq!(files, vec![sentinel]);
    }

    #[test]
    fn codex_session_file_list_cache_invalidates_when_directory_stamps_change() {
        let _guard = CODEX_FILE_LIST_CACHE_TEST_LOCK
            .lock()
            .expect("lock codex file list cache test");
        clear_codex_session_file_list_cache();
        let (root, session_path) = write_temp_session_tree();
        let stale_stamp = CodexSessionDirectoryStamp {
            path: root.clone(),
            modified_at: -1,
        };
        let stale_file = root.join("stale.jsonl");

        remember_cached_codex_session_file_list(&root, vec![stale_stamp], vec![stale_file]);
        let files = collect_jsonl_files_cached(&root);
        fs::remove_dir_all(root).ok();

        assert_eq!(files, vec![session_path]);
    }

    #[test]
    fn read_codex_session_file_meta_skips_title_for_mismatched_project() {
        let path = write_temp_session(codex_session_with_title(
            "C:/guodevelop/ccg-switch",
            "real user task",
        ));
        let stamp = codex_file_stamp(&path).expect("stamp");

        let cached = read_codex_session_file_meta(
            &path,
            "11111111-1111-1111-1111-111111111111",
            stamp,
            Some(&normalize_session_project_path("C:/other/project")),
        )
        .expect("session meta");
        fs::remove_file(path).ok();

        assert_eq!(
            cached.project_dir.as_deref(),
            Some("C:/guodevelop/ccg-switch")
        );
        assert_eq!(cached.title, None);
        assert!(!cached.title_scanned);
    }

    #[test]
    fn codex_session_meta_cache_invalidates_when_file_changes() {
        clear_codex_session_meta_cache();
        let path = write_temp_session(codex_session_with_title(
            "C:/guodevelop/ccg-switch",
            "first title",
        ));
        let filter = normalize_session_project_path("C:/guodevelop/ccg-switch");

        let first = read_codex_session_meta_cached(
            &path,
            "22222222-2222-2222-2222-222222222222",
            Some(&filter),
        )
        .expect("first session meta");
        assert_eq!(first.title.as_deref(), Some("first title"));

        fs::write(
            &path,
            codex_session_with_title("C:/guodevelop/ccg-switch", "second longer title"),
        )
        .expect("rewrite temp codex session");

        let second = read_codex_session_meta_cached(
            &path,
            "22222222-2222-2222-2222-222222222222",
            Some(&filter),
        )
        .expect("second session meta");
        fs::remove_file(path).ok();

        assert_eq!(second.title.as_deref(), Some("second longer title"));
    }

    #[test]
    fn load_codex_messages_converts_native_items_to_structured_blocks() {
        let path = write_temp_session(
            r#"{"type":"response_item","timestamp":"2026-06-17T08:00:00.000Z","payload":{"type":"reasoning","summary":[{"type":"summary_text","text":"inspect first"}],"content":null}}"#
                .to_owned()
                + "\n"
                + r#"{"type":"response_item","timestamp":"2026-06-17T08:00:01.000Z","payload":{"type":"function_call","name":"shell_command","arguments":"{\"command\":\"git status\"}","call_id":"call-1"}}"#
                + "\n"
                + r#"{"type":"response_item","timestamp":"2026-06-17T08:00:02.000Z","payload":{"type":"function_call_output","call_id":"call-1","output":"clean"}}"#,
        );

        let messages = load_codex_messages(&path.to_string_lossy()).expect("load messages");
        fs::remove_file(path).ok();

        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].role, "assistant");
        assert_eq!(
            messages[0].raw.as_ref().unwrap()["message"]["content"][0]["type"],
            "thinking"
        );
        assert_eq!(
            messages[1].raw.as_ref().unwrap()["message"]["content"][0]["type"],
            "tool_use"
        );
        assert_eq!(
            messages[1].raw.as_ref().unwrap()["message"]["content"][0]["input"]["command"],
            "git status"
        );
        assert_eq!(messages[2].role, "user");
        assert_eq!(messages[2].content, TOOL_RESULT_CONTENT);
        assert_eq!(
            messages[2].raw.as_ref().unwrap()["message"]["content"][0]["tool_use_id"],
            "call-1"
        );
    }

    #[test]
    fn load_codex_messages_preserves_markdown_line_breaks() {
        let path = write_temp_session(
            r#"{"type":"response_item","timestamp":"2026-06-17T08:00:00.000Z","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"**上轮进展与阻塞**\n记录里声称完成。\n\n- **本轮规划**：先定位根因。\n- **验证结果**：保留列表。"}]}}"#,
        );

        let messages = load_codex_messages(&path.to_string_lossy()).expect("load messages");
        fs::remove_file(path).ok();

        let expected = "**上轮进展与阻塞**\n记录里声称完成。\n\n- **本轮规划**：先定位根因。\n- **验证结果**：保留列表。";
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, expected);
        assert_eq!(
            messages[0].raw.as_ref().unwrap()["message"]["content"][0]["text"],
            expected
        );
    }

    #[test]
    fn load_codex_message_window_keeps_tail_and_stable_fallback_tool_ids() {
        let path = write_temp_session(
            r#"{"type":"response_item","timestamp":"2026-06-17T08:00:00.000Z","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"first"}]}}"#
                .to_owned()
                + "\n"
                + r#"{"type":"response_item","timestamp":"2026-06-17T08:00:01.000Z","payload":{"type":"function_call","name":"shell_command","arguments":"{\"command\":\"npm test\"}"}}"#
                + "\n"
                + r#"{"type":"response_item","timestamp":"2026-06-17T08:00:02.000Z","payload":{"type":"function_call_output","output":"ok"}}"#,
        );

        let full = load_codex_messages(&path.to_string_lossy()).expect("load full");
        let window = load_codex_message_window(&path.to_string_lossy(), 2).expect("load window");
        fs::remove_file(path).ok();

        assert_eq!(window.total_count, 3);
        assert_eq!(window.start_index, 1);
        assert!(!window.complete);
        assert_eq!(window.messages.len(), 2);
        assert_eq!(
            window.messages[0].raw.as_ref().unwrap()["message"]["content"][0]["id"],
            full[1].raw.as_ref().unwrap()["message"]["content"][0]["id"]
        );
        assert_eq!(
            window.messages[1].raw.as_ref().unwrap()["message"]["content"][0]["tool_use_id"],
            full[2].raw.as_ref().unwrap()["message"]["content"][0]["tool_use_id"]
        );
    }

    #[test]
    fn load_codex_messages_preserves_input_image_blocks() {
        let path = write_temp_session(
            r#"{"type":"response_item","timestamp":"2026-06-17T08:00:00.000Z","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"看下截图"},{"type":"input_image","image_url":"file:///C:/Users/Administrator/Pictures/screen.png","detail":"high"}]}}"#,
        );

        let messages = load_codex_messages(&path.to_string_lossy()).expect("load messages");
        fs::remove_file(path).ok();

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].content, "看下截图");
        assert_eq!(
            messages[0].raw.as_ref().unwrap()["message"]["content"][1]["type"],
            "input_image"
        );
        assert_eq!(
            messages[0].raw.as_ref().unwrap()["message"]["content"][1]["image_url"],
            "file:///C:/Users/Administrator/Pictures/screen.png"
        );
    }

    #[test]
    fn load_codex_messages_drops_image_placeholder_text_when_image_block_exists() {
        let path = write_temp_session(
            r#"{"type":"response_item","timestamp":"2026-06-17T08:00:00.000Z","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<image name=[Image #1]>"},{"type":"input_image","image_url":"file:///C:/Users/Administrator/Pictures/screen.png","detail":"high"},{"type":"input_text","text":"</image>"},{"type":"input_text","text":"截图里的按钮太大了"}]}}"#,
        );

        let messages = load_codex_messages(&path.to_string_lossy()).expect("load messages");
        fs::remove_file(path).ok();

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].content, "截图里的按钮太大了");
        let blocks = messages[0].raw.as_ref().unwrap()["message"]["content"]
            .as_array()
            .expect("content blocks");
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0]["type"], "input_image");
        assert_eq!(blocks[1]["type"], "text");
        assert_eq!(blocks[1]["text"], "截图里的按钮太大了");
    }

    #[test]
    fn load_codex_messages_converts_custom_apply_patch_to_edit_tool() {
        let path = write_temp_session(
            r#"{"type":"response_item","timestamp":"2026-06-17T08:00:00.000Z","payload":{"type":"custom_tool_call","call_id":"call-patch-1","name":"apply_patch","input":"*** Begin Patch\n*** Update File: src/lib.rs\n@@ -1,1 +1,2 @@\n fn old() {}\n+fn new() {}\n*** End Patch"}}"#
                .to_owned()
                + "\n"
                + r#"{"type":"response_item","timestamp":"2026-06-17T08:00:01.000Z","payload":{"type":"custom_tool_call_output","call_id":"call-patch-1","output":"Done!"}}"#,
        );

        let messages = load_codex_messages(&path.to_string_lossy()).expect("load messages");
        fs::remove_file(path).ok();

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "assistant");
        assert_eq!(
            messages[0].raw.as_ref().unwrap()["message"]["content"][0]["type"],
            "tool_use"
        );
        assert_eq!(
            messages[0].raw.as_ref().unwrap()["message"]["content"][0]["id"],
            "call-patch-1"
        );
        assert_eq!(
            messages[0].raw.as_ref().unwrap()["message"]["content"][0]["name"],
            "apply_patch"
        );
        assert!(
            messages[0].raw.as_ref().unwrap()["message"]["content"][0]["input"]["patch"]
                .as_str()
                .unwrap()
                .contains("*** Begin Patch")
        );
        assert_eq!(messages[1].role, "user");
        assert_eq!(
            messages[1].raw.as_ref().unwrap()["message"]["content"][0]["tool_use_id"],
            "call-patch-1"
        );
    }

    #[test]
    fn load_codex_messages_converts_exec_command_patch_to_edit_tool() {
        let path = write_temp_session(
            r#"{"type":"response_item","timestamp":"2026-06-17T08:00:00.000Z","payload":{"type":"function_call","call_id":"call-exec-1","name":"exec_command","arguments":"{\"cmd\":\"apply_patch <<'PATCH'\n*** Begin Patch\n*** Update File: src/pages/ChatPage.tsx\n@@ -10,1 +10,2 @@\n const oldValue = true;\n+const newValue = true;\n*** End Patch\nPATCH\"}"}}"#,
        );

        let messages = load_codex_messages(&path.to_string_lossy()).expect("load messages");
        fs::remove_file(path).ok();

        assert_eq!(messages.len(), 1);
        assert_eq!(
            messages[0].raw.as_ref().unwrap()["message"]["content"][0]["name"],
            "apply_patch"
        );
        assert!(
            messages[0].raw.as_ref().unwrap()["message"]["content"][0]["input"]["patch"]
                .as_str()
                .unwrap()
                .contains("src/pages/ChatPage.tsx")
        );
    }

    #[test]
    fn system_instruction_detects_codex_protocol_context() {
        assert!(is_system_instruction("<turn_aborted>"));
        assert!(is_system_instruction(
            "<user_action>\n<context>User initiated a review task.</context>\n</user_action>"
        ));
        assert!(is_system_instruction(
            "You are Codex, a coding agent based on GPT-5.\n\n# Tools\nTools are grouped by namespace."
        ));
        assert!(is_system_instruction(
            "<agents-instructions>\n# Global Instructions\n</agents-instructions>"
        ));
        assert!(!is_system_instruction("继续推进任务"));
    }
}
