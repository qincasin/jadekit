use crate::session_manager::{
    self, SessionMeta, UnifiedSessionMessage, UnifiedSessionMessageWindow,
};
use std::collections::HashMap;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameSessionTitleResult {
    pub title: String,
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn list_sessions(projectPath: String) -> Result<Vec<SessionMeta>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        session_manager::scan_sessions_for_project(&projectPath)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn chat_session_rename(
    providerId: String,
    sessionId: String,
    title: String,
) -> Result<RenameSessionTitleResult, String> {
    let title = tauri::async_runtime::spawn_blocking(move || {
        session_manager::rename_session_title(&providerId, &sessionId, &title)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))??;

    Ok(RenameSessionTitleResult {title})
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameProjectResult {
    pub name: String,
}

// ---- 项目元数据动作 ----

#[tauri::command]
#[allow(non_snake_case)]
pub async fn chat_project_set_pinned(projectPath: String, pinned: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        session_manager::workspace_metadata::set_project_pinned(&projectPath, pinned)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn chat_project_set_archived(projectPath: String, archived: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        session_manager::workspace_metadata::set_project_archived(&projectPath, archived)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn chat_project_remove(projectPath: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        session_manager::workspace_metadata::set_project_removed(&projectPath, true)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn chat_project_rename(
    projectPath: String,
    name: String,
) -> Result<RenameProjectResult, String> {
    let name = tauri::async_runtime::spawn_blocking(move || {
        session_manager::workspace_metadata::rename_project(&projectPath, &name)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))??;

    Ok(RenameProjectResult {name})
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn chat_project_mark_all_read(projectPath: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let sessions = session_manager::scan_sessions_for_project(&projectPath);
        let session_ids: Vec<String> =
            sessions.into_iter().map(|session| session.session_id).collect();
        session_manager::workspace_metadata::mark_sessions_read(&session_ids)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

// ---- 会话元数据动作 ----

#[tauri::command]
#[allow(non_snake_case)]
pub async fn chat_session_set_pinned(sessionId: String, pinned: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        session_manager::workspace_metadata::set_session_pinned(&sessionId, pinned)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn chat_session_set_archived(sessionId: String, archived: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        session_manager::workspace_metadata::set_session_archived(&sessionId, archived)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn chat_session_set_unread(sessionId: String, unread: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        session_manager::workspace_metadata::set_session_unread(&sessionId, unread)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

/// 轻量检测每个项目拥有哪些 provider（不读取标题/内容）
#[tauri::command]
#[allow(non_snake_case)]
pub async fn get_project_provider_map(
    projectPaths: Vec<String>,
) -> Result<HashMap<String, Vec<String>>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        session_manager::get_project_provider_map(&projectPaths)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn get_unified_session_messages(
    providerId: String,
    sourcePath: String,
) -> Result<Vec<UnifiedSessionMessage>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        session_manager::load_messages(&providerId, &sourcePath)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn get_unified_session_message_window(
    providerId: String,
    sourcePath: String,
    tailLimit: usize,
) -> Result<UnifiedSessionMessageWindow, String> {
    tauri::async_runtime::spawn_blocking(move || {
        session_manager::load_message_window(&providerId, &sourcePath, tailLimit)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn get_claude_subagent_session_messages(
    sessionId: String,
    sourcePath: String,
    agentId: Option<String>,
    description: Option<String>,
) -> Result<Vec<UnifiedSessionMessage>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        session_manager::load_claude_subagent_messages(
            &sessionId,
            &sourcePath,
            agentId.as_deref(),
            description.as_deref(),
        )
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}
