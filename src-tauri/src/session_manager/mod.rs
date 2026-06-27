pub mod providers;
pub mod workspace_metadata;

use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_MESSAGE_WINDOW_LIMIT: usize = 500;
const SESSION_CUSTOM_TITLE_MAX_CHARS: usize = 50;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SessionMeta {
    pub provider_id: String,
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_dir: Option<String>,
    pub created_at: i64,
    pub last_active_at: i64,
    pub source_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume_command: Option<String>,
    /// 用户是否置顶该会话（来自 workspace 元数据）。
    #[serde(default)]
    pub pinned: bool,
    /// 用户是否归档该会话（来自 workspace 元数据）。
    #[serde(default)]
    pub archived: bool,
    /// 用户是否手动标记为未读（来自 workspace 元数据）。
    #[serde(default)]
    pub unread: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct UnifiedSessionMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ts: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UnifiedSessionMessageWindow {
    pub messages: Vec<UnifiedSessionMessage>,
    pub start_index: usize,
    pub total_count: usize,
    pub complete: bool,
}

pub(crate) fn normalize_message_window_limit(tail_limit: usize) -> usize {
    tail_limit.clamp(1, MAX_MESSAGE_WINDOW_LIMIT)
}

pub(crate) struct MessageWindowBuilder {
    limit: usize,
    total_count: usize,
    messages: VecDeque<UnifiedSessionMessage>,
}

impl MessageWindowBuilder {
    pub(crate) fn new(tail_limit: usize) -> Self {
        Self {
            limit: normalize_message_window_limit(tail_limit),
            total_count: 0,
            messages: VecDeque::new(),
        }
    }

    pub(crate) fn push(&mut self, message: UnifiedSessionMessage) {
        self.total_count += 1;
        self.messages.push_back(message);
        while self.messages.len() > self.limit {
            self.messages.pop_front();
        }
    }

    pub(crate) fn next_index(&self) -> usize {
        self.total_count
    }

    pub(crate) fn finish(self) -> UnifiedSessionMessageWindow {
        let messages: Vec<UnifiedSessionMessage> = self.messages.into_iter().collect();
        let start_index = self.total_count.saturating_sub(messages.len());
        UnifiedSessionMessageWindow {
            complete: start_index == 0,
            messages,
            start_index,
            total_count: self.total_count,
        }
    }
}

fn messages_to_window(
    messages: Vec<UnifiedSessionMessage>,
    tail_limit: usize,
) -> UnifiedSessionMessageWindow {
    let total_count = messages.len();
    let limit = normalize_message_window_limit(tail_limit);
    let start_index = total_count.saturating_sub(limit);
    UnifiedSessionMessageWindow {
        messages: messages.into_iter().skip(start_index).collect(),
        start_index,
        total_count,
        complete: start_index == 0,
    }
}

/// 按项目路径扫描会话，合并所有 provider 并按 last_active_at 降序排序
pub fn scan_sessions_for_project(project_path: &str) -> Vec<SessionMeta> {
    let mut all = Vec::new();
    all.extend(providers::claude::scan_claude_sessions_for_project(
        project_path,
    ));
    all.extend(providers::codex::scan_codex_sessions_for_project(
        project_path,
    ));
    all.extend(providers::gemini::scan_gemini_sessions_for_project(
        project_path,
    ));
    apply_session_custom_titles(&mut all);
    apply_session_metadata(&mut all);
    // 归档会话默认从列表中过滤掉，避免污染主会话区。
    all.retain(|session| !session.archived);
    // 先按置顶分组，再按最近活跃时间降序。
    all.sort_by(|a, b| {
        b.pinned
            .cmp(&a.pinned)
            .then_with(|| b.last_active_at.cmp(&a.last_active_at))
    });
    all
}

/// 把 workspace 元数据（置顶 / 归档 / 未读）应用到扫描出的会话上。
fn apply_session_metadata(sessions: &mut [SessionMeta]) {
    let metadata = workspace_metadata::load_session_metadata_map();
    if metadata.is_empty() {
        return;
    }
    for session in sessions {
        if let Some(meta) = metadata.get(&session.session_id) {
            session.pinned = meta.pinned;
            session.archived = meta.archived;
            session.unread = meta.unread;
        }
    }
}

fn normalize_session_title_id(session_id: &str) -> Result<String, String> {
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return Err("会话 ID 不能为空".to_string());
    }
    if session_id.chars().any(char::is_control) {
        return Err("会话 ID 包含非法控制字符".to_string());
    }
    Ok(session_id.to_string())
}

fn normalize_session_custom_title(title: &str) -> Result<String, String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("会话标题不能为空".to_string());
    }
    if title.chars().count() > SESSION_CUSTOM_TITLE_MAX_CHARS {
        return Err(format!(
            "会话标题不能超过 {SESSION_CUSTOM_TITLE_MAX_CHARS} 个字符"
        ));
    }
    if title.chars().any(char::is_control) {
        return Err("会话标题包含非法控制字符".to_string());
    }
    Ok(title.to_string())
}

fn timestamp_now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn codemoss_dir() -> Result<PathBuf, String> {
    crate::services::app_paths::data_dir().map_err(|e| e.to_string())
}

fn session_custom_titles_file() -> Result<PathBuf, String> {
    Ok(codemoss_dir()?.join("session-titles.json"))
}

fn load_session_title_map(file: &Path) -> Map<String, Value> {
    let Ok(text) = std::fs::read_to_string(file) else {
        return Map::new();
    };
    serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
}

fn save_session_title_map(file: &Path, titles: &Map<String, Value>) -> Result<(), String> {
    let parent = file
        .parent()
        .ok_or_else(|| "会话标题文件路径无效".to_string())?;
    std::fs::create_dir_all(parent).map_err(|e| format!("创建会话标题目录失败: {e}"))?;

    let tmp_file = file.with_file_name(format!(
        "{}.tmp",
        file.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("session-titles.json")
    ));
    let text = serde_json::to_string_pretty(titles)
        .map_err(|e| format!("序列化会话标题失败: {e}"))?;
    std::fs::write(&tmp_file, text).map_err(|e| format!("写入会话标题失败: {e}"))?;
    std::fs::rename(&tmp_file, file).map_err(|e| format!("保存会话标题失败: {e}"))?;
    Ok(())
}

fn rename_session_title_in_file(
    file: &Path,
    session_id: &str,
    title: &str,
    updated_at: i64,
) -> Result<String, String> {
    let session_id = normalize_session_title_id(session_id)?;
    let title = normalize_session_custom_title(title)?;
    let mut titles = load_session_title_map(file);
    titles.insert(
        session_id,
        serde_json::json!({
            "customTitle": title,
            "updatedAt": updated_at,
        }),
    );
    save_session_title_map(file, &titles)?;
    Ok(title)
}

fn apply_session_custom_titles_from_file(file: &Path, sessions: &mut [SessionMeta]) {
    let titles = load_session_title_map(file);
    for session in sessions {
        let Some(record) = titles.get(&session.session_id).and_then(|value| value.as_object())
        else {
            continue;
        };
        let Some(title) = record
            .get("customTitle")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|title| !title.is_empty())
        else {
            continue;
        };
        session.title = Some(title.to_string());
    }
}

fn apply_session_custom_titles(sessions: &mut [SessionMeta]) {
    let Ok(file) = session_custom_titles_file() else {
        return;
    };
    apply_session_custom_titles_from_file(&file, sessions);
}

pub fn rename_session_title(
    provider_id: &str,
    session_id: &str,
    title: &str,
) -> Result<String, String> {
    if provider_id.trim().is_empty() {
        return Err("Provider 不能为空".to_string());
    }
    let file = session_custom_titles_file()?;
    rename_session_title_in_file(&file, session_id, title, timestamp_now_millis())
}

/// 轻量扫描：返回每个项目路径拥有哪些 provider 的映射
/// 不读取标题/内容，只检查目录结构和 cwd 字段
pub fn get_project_provider_map(project_paths: &[String]) -> HashMap<String, Vec<String>> {
    let normalized: Vec<String> = project_paths
        .iter()
        .map(|p| p.replace('\\', "/").to_lowercase())
        .collect();

    let mut result: HashMap<String, Vec<String>> = HashMap::new();
    // 初始化每个项目
    for path in project_paths {
        result.insert(path.clone(), vec!["claude".to_string()]); // 所有项目都来自 Claude
    }

    // Codex: 快速扫描 cwd 字段
    let codex_projects = providers::codex::scan_codex_project_dirs();
    for codex_dir in &codex_projects {
        let norm = codex_dir.replace('\\', "/").to_lowercase();
        for (i, target) in normalized.iter().enumerate() {
            if norm == *target {
                let entry = result.entry(project_paths[i].clone()).or_default();
                if !entry.contains(&"codex".to_string()) {
                    entry.push("codex".to_string());
                }
            }
        }
    }

    // Gemini: 从 projects.json 读取
    let gemini_projects = providers::gemini::scan_gemini_project_dirs();
    for gemini_dir in &gemini_projects {
        let norm = gemini_dir.replace('\\', "/").to_lowercase();
        for (i, target) in normalized.iter().enumerate() {
            if norm == *target {
                let entry = result.entry(project_paths[i].clone()).or_default();
                if !entry.contains(&"gemini".to_string()) {
                    entry.push("gemini".to_string());
                }
            }
        }
    }

    result
}

/// 根据 provider_id 路由到对应 provider 加载消息
pub fn load_messages(
    provider_id: &str,
    source_path: &str,
) -> Result<Vec<UnifiedSessionMessage>, String> {
    match provider_id {
        "claude" => providers::claude::load_claude_messages(source_path),
        "codex" => providers::codex::load_codex_messages(source_path),
        "gemini" => providers::gemini::load_gemini_messages(source_path),
        _ => Err(format!("Unknown provider: {}", provider_id)),
    }
}

pub fn load_message_window(
    provider_id: &str,
    source_path: &str,
    tail_limit: usize,
) -> Result<UnifiedSessionMessageWindow, String> {
    match provider_id {
        "claude" => providers::claude::load_claude_message_window(source_path, tail_limit),
        "codex" => providers::codex::load_codex_message_window(source_path, tail_limit),
        "gemini" => {
            let messages = providers::gemini::load_gemini_messages(source_path)?;
            Ok(messages_to_window(messages, tail_limit))
        }
        _ => Err(format!("Unknown provider: {}", provider_id)),
    }
}

pub fn load_claude_subagent_messages(
    session_id: &str,
    source_path: &str,
    agent_id: Option<&str>,
    description: Option<&str>,
) -> Result<Vec<UnifiedSessionMessage>, String> {
    providers::claude::load_claude_subagent_messages(session_id, source_path, agent_id, description)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_test_titles_file(name: &str) -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        std::env::temp_dir()
            .join(format!("ccg-switch-session-title-{name}-{suffix}"))
            .join("session-titles.json")
    }

    fn session_meta(session_id: &str, title: Option<&str>) -> SessionMeta {
        SessionMeta {
            provider_id: "claude".to_string(),
            session_id: session_id.to_string(),
            title: title.map(str::to_string),
            summary: None,
            project_dir: Some("C:/workspace/ccg-switch".to_string()),
            created_at: 1,
            last_active_at: 2,
            source_path: format!("C:/sessions/{session_id}.jsonl"),
            resume_command: None,
            pinned: false,
            archived: false,
            unread: false,
        }
    }

    #[test]
    fn session_custom_title_file_matches_ai_bridge_contract() -> Result<(), String> {
        let file = unique_test_titles_file("contract");

        rename_session_title_in_file(&file, "session-1", " Custom Title ", 123)?;

        let saved = fs::read_to_string(&file).map_err(|e| e.to_string())?;
        let json: serde_json::Value = serde_json::from_str(&saved).map_err(|e| e.to_string())?;
        assert_eq!(json["session-1"]["customTitle"], "Custom Title");
        assert_eq!(json["session-1"]["updatedAt"], 123);

        let mut sessions = vec![session_meta("session-1", Some("Generated title"))];
        apply_session_custom_titles_from_file(&file, &mut sessions);
        assert_eq!(sessions[0].title.as_deref(), Some("Custom Title"));

        fs::remove_dir_all(file.parent().expect("test title dir")).ok();
        Ok(())
    }

    #[test]
    fn session_custom_title_rejects_empty_or_too_long_titles() {
        let file = unique_test_titles_file("validation");

        assert!(rename_session_title_in_file(&file, "session-1", "   ", 123).is_err());
        assert!(rename_session_title_in_file(&file, "session-1", &"x".repeat(51), 123).is_err());
    }
}
