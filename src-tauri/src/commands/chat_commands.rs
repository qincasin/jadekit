//! Tauri commands for interactive chat (Claude Code / Codex).
//!
//! Thin command layer: parse args, delegate to ChatManager, map errors to
//! String. Follows the project's command-layer convention.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, State};

use crate::chat::{ChatManager, DiffSummary, WorktreeInfo, WorktreeManager};

const HELM_WORKTREES_DIR_NAME: &str = "helm-worktrees";

/// 当前聊天工作目录的只读工作区状态。
#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
pub struct ChatWorkspaceStatus {
    pub is_git_repository: bool,
    pub git_root: Option<String>,
    pub git_branch: Option<String>,
}

/// 一个 Git 本地分支。
#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChatGitBranch {
    pub name: String,
    pub current: bool,
}

/// 一个 Helm worktree（给前端展示与绑定 cwd）。
#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeInfoDto {
    pub path: String,
    pub branch: String,
}

/// worktree 相对 HEAD 的 diff 摘要。
#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiffSummaryDto {
    pub files_changed: u32,
    pub insertions: u32,
    pub deletions: u32,
}

/// 一个工作目录文件项（供 `@` 文件引用补全使用）。
#[derive(serde::Serialize)]
pub struct WorkspaceFile {
    /// 相对工作目录的路径（用 `/` 分隔，跨平台统一）
    pub rel_path: String,
    /// 文件名
    pub name: String,
    /// 是否为目录
    pub is_dir: bool,
}

/// 一个 Slash 命令补全项，格式对齐 cc-gui 的 SlashCommandRegistry。
#[derive(serde::Serialize)]
pub struct SlashCommandItem {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: String,
}

/// Shared chat manager, stored in Tauri managed state.
pub struct ChatState {
    pub manager: ChatManager,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SystemNotificationPayload {
    title: String,
    body: String,
}

fn normalize_system_notification_payload(
    title: &str,
    body: &str,
) -> Result<SystemNotificationPayload, String> {
    let title = title.trim().to_string();
    if title.is_empty() {
        return Err("发送系统通知失败: 标题不能为空".to_string());
    }

    Ok(SystemNotificationPayload {
        title,
        body: body.trim().to_string(),
    })
}

#[allow(dead_code)]
fn windows_terminal_start_args<'a>(
    project_dir: Option<&'a str>,
    command: Option<&'a str>,
) -> Vec<&'a str> {
    let mut args: Vec<&'a str> = vec!["/c", "start", ""];
    if let Some(dir) = project_dir.filter(|dir| !dir.is_empty()) {
        args.extend(["/D", dir]);
    }
    args.extend(["cmd", "/K"]);
    if let Some(command) = command.filter(|command| !command.is_empty()) {
        args.push(command);
    }
    args
}

fn empty_chat_workspace_status() -> ChatWorkspaceStatus {
    ChatWorkspaceStatus {
        is_git_repository: false,
        git_root: None,
        git_branch: None,
    }
}

fn find_git_entry(start: &Path) -> Option<(PathBuf, PathBuf)> {
    let mut current = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start.to_path_buf()
    };

    loop {
        let git_entry = current.join(".git");
        if git_entry.exists() {
            return Some((current, git_entry));
        }
        if !current.pop() {
            return None;
        }
    }
}

fn resolve_git_dir(repo_root: &Path, git_entry: &Path) -> Option<PathBuf> {
    if git_entry.is_dir() {
        return Some(git_entry.to_path_buf());
    }

    let content = std::fs::read_to_string(git_entry).ok()?;
    let git_dir = content.trim().strip_prefix("gitdir:")?.trim();
    if git_dir.is_empty() {
        return None;
    }

    let path = PathBuf::from(git_dir);
    Some(if path.is_absolute() {
        path
    } else {
        repo_root.join(path)
    })
}

fn read_git_branch(git_dir: &Path) -> Option<String> {
    let head = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let head = head.trim();
    if head.is_empty() {
        return None;
    }

    if let Some(reference) = head.strip_prefix("ref:") {
        let reference = reference.trim();
        return Some(
            reference
                .strip_prefix("refs/heads/")
                .unwrap_or(reference)
                .to_string(),
        )
        .filter(|branch| !branch.is_empty());
    }

    if head.len() >= 7 {
        return Some(head.chars().take(7).collect());
    }

    None
}

fn resolve_chat_workspace_status(cwd: Option<String>) -> Result<ChatWorkspaceStatus, String> {
    let Some(cwd) = cwd
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return Ok(empty_chat_workspace_status());
    };

    let path = PathBuf::from(cwd);
    if !path.exists() {
        return Ok(empty_chat_workspace_status());
    }

    let Some((repo_root, git_entry)) = find_git_entry(&path) else {
        return Ok(empty_chat_workspace_status());
    };

    let git_branch = resolve_git_dir(&repo_root, &git_entry)
        .as_deref()
        .and_then(read_git_branch);

    Ok(ChatWorkspaceStatus {
        is_git_repository: true,
        git_root: Some(repo_root.to_string_lossy().to_string()),
        git_branch,
    })
}

fn resolve_existing_chat_path(path: String, label: &str) -> Result<PathBuf, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(format!("{label}不能为空"));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(format!("{label}包含非法控制字符"));
    }

    let path = PathBuf::from(trimmed);
    if !path.exists() {
        return Err(format!("{label}不存在: {trimmed}"));
    }
    Ok(path)
}

fn resolve_existing_chat_directory(path: String, label: &str) -> Result<PathBuf, String> {
    let path = resolve_existing_chat_path(path, label)?;
    if !path.is_dir() {
        return Err(format!("{label}不是目录: {}", path.to_string_lossy()));
    }
    Ok(path)
}

fn repo_worktrees_dir(repo_root: &Path) -> Result<PathBuf, String> {
    let repo_name = repo_root
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("repo");
    crate::services::app_paths::data_subdir(HELM_WORKTREES_DIR_NAME)
        .map(|root| root.join(repo_name))
        .map_err(|e| format!("定位 Helm worktrees 目录失败: {e}"))
}

fn worktree_info_dto(info: WorktreeInfo) -> WorktreeInfoDto {
    WorktreeInfoDto {
        path: info.path.to_string_lossy().to_string(),
        branch: info.branch,
    }
}

fn diff_summary_dto(summary: DiffSummary) -> DiffSummaryDto {
    DiffSummaryDto {
        files_changed: summary.files_changed,
        insertions: summary.insertions,
        deletions: summary.deletions,
    }
}

fn normalize_chat_git_branch_name(branch_name: &str) -> Result<String, String> {
    let branch = branch_name.trim();
    if branch.is_empty() {
        return Err("创建 Git 分支失败: 分支名不能为空".to_string());
    }
    if branch.starts_with('-') {
        return Err("创建 Git 分支失败: 分支名不能以 - 开头".to_string());
    }
    if branch.chars().any(char::is_control) {
        return Err("创建 Git 分支失败: 分支名包含非法控制字符".to_string());
    }
    if branch.contains("..")
        || branch.contains("//")
        || branch.contains('\\')
        || branch.contains(' ')
        || branch.contains('~')
        || branch.contains('^')
        || branch.contains(':')
        || branch.contains('?')
        || branch.contains('*')
        || branch.contains('[')
        || branch.ends_with('/')
        || branch.ends_with('.')
        || branch.ends_with(".lock")
        || branch == "@{"
        || branch.contains("@{")
    {
        return Err("创建 Git 分支失败: 分支名格式不合法".to_string());
    }
    Ok(branch.to_string())
}

fn resolve_git_repository(cwd: &Path) -> Result<(PathBuf, PathBuf), String> {
    let Some((repo_root, git_entry)) = find_git_entry(cwd) else {
        return Err("当前工作目录不是 Git 仓库".to_string());
    };
    let Some(git_dir) = resolve_git_dir(&repo_root, &git_entry) else {
        return Err("无法解析 Git 目录".to_string());
    };
    Ok((repo_root, git_dir))
}

fn collect_refs_heads(
    root: &Path,
    current_branch: Option<&str>,
    prefix: &str,
    out: &mut Vec<ChatGitBranch>,
) -> Result<(), String> {
    let entries = std::fs::read_dir(root).map_err(|e| format!("读取 Git 分支失败: {e}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();
        if file_name.is_empty() {
            continue;
        }
        let branch_name = if prefix.is_empty() {
            file_name
        } else {
            format!("{prefix}/{file_name}")
        };
        if path.is_dir() {
            collect_refs_heads(&path, current_branch, &branch_name, out)?;
        } else {
            out.push(ChatGitBranch {
                current: current_branch == Some(branch_name.as_str()),
                name: branch_name,
            });
        }
    }
    Ok(())
}

fn list_chat_git_branches_for_path(cwd: &Path) -> Result<Vec<ChatGitBranch>, String> {
    let (repo_root, git_dir) = resolve_git_repository(cwd)?;
    let current_branch = read_git_branch(&git_dir);
    let output = Command::new("git")
        .arg("-C")
        .arg(&repo_root)
        .args([
            "branch",
            "--format=%(if)%(HEAD)%(then)*%(else) %(end)%(refname:short)",
        ])
        .output();

    if let Ok(output) = output {
        if output.status.success() {
            let mut branches: Vec<ChatGitBranch> = String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter_map(|line| {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        return None;
                    }
                    let current = trimmed.starts_with('*');
                    let name = trimmed.trim_start_matches('*').trim().to_string();
                    if name.is_empty() {
                        return None;
                    }
                    Some(ChatGitBranch { name, current })
                })
                .collect();
            branches.sort_by(|a, b| a.name.cmp(&b.name));
            return Ok(branches);
        }
    }

    let refs_heads = git_dir.join("refs").join("heads");
    if !refs_heads.exists() {
        return Ok(Vec::new());
    }

    let mut branches = Vec::new();
    collect_refs_heads(&refs_heads, current_branch.as_deref(), "", &mut branches)?;
    branches.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(branches)
}

/// 解析命令入参 agent_id：None 或空 → 默认 agent；否则归一化（去空白/路径分隔符）。
fn resolve_agent_id(agent_id: Option<String>) -> crate::chat::AgentId {
    crate::chat::sanitize_agent_id(agent_id.as_deref().unwrap_or(crate::chat::DEFAULT_AGENT_ID))
}

/// 计算某 session（= agent）的 permission 响应目录：`<permission_root>/<session_id>`。
/// 多 agent 下每个 daemon 只读自己子目录的响应文件，故响应必须落到对应子目录。
fn agent_permission_dir(root: &Path, session_id: &str) -> PathBuf {
    root.join(session_id)
}

/// Send a chat message and stream the response via "chat://stream"/"chat://done".
///
/// `agent_id` 缺省回退默认 agent（兼容旧前端）。`provider` is "claude" or "codex".
/// `command` is the ai-bridge verb, e.g. "send". `params` is the payload
/// (message, sessionId, model, cwd, …). Returns the request id for correlating
/// streamed events.
#[tauri::command]
pub async fn chat_send(
    agent_id: Option<String>,
    provider: String,
    command: String,
    params: Value,
    state: State<'_, ChatState>,
) -> Result<String, String> {
    let agent = resolve_agent_id(agent_id);
    let method = format!("{provider}.{command}");
    state.manager.send(agent, method, params).await
}

/// Abort the current in-flight turn for the given agent (`agent_id` 缺省回退默认 agent)。
#[tauri::command]
pub async fn chat_abort(
    agent_id: Option<String>,
    state: State<'_, ChatState>,
) -> Result<(), String> {
    let agent = resolve_agent_id(agent_id);
    state.manager.abort(agent).await
}

/// Whether the given agent's daemon is running (`agent_id` 缺省回退默认 agent)。
#[tauri::command]
pub async fn chat_is_running(
    agent_id: Option<String>,
    state: State<'_, ChatState>,
) -> Result<bool, String> {
    let agent = resolve_agent_id(agent_id);
    Ok(state.manager.is_running(&agent).await)
}

/// Explicitly start the daemon (otherwise it starts lazily on first send).
/// `agent_id` 缺省回退默认 agent。
#[tauri::command]
pub async fn chat_start_daemon(
    agent_id: Option<String>,
    state: State<'_, ChatState>,
) -> Result<(), String> {
    // A no-op send path: starting happens inside the manager's lazy init.
    // We trigger it via is_running which forces client init only on send, so
    // instead expose a dedicated warm-up by sending a heartbeat-like start.
    let agent = resolve_agent_id(agent_id);
    state.manager.warm_up(agent).await
}

/// 发送系统级桌面通知，用于聊天任务完成/失败/中断后的右下角提示。
#[tauri::command]
pub fn chat_show_system_notification(
    app: AppHandle,
    title: String,
    body: String,
) -> Result<(), String> {
    use tauri_plugin_notification::NotificationExt;

    let payload = normalize_system_notification_payload(&title, &body)?;

    app.notification()
        .builder()
        .title(payload.title)
        .body(payload.body)
        .show()
        .map_err(|e| format!("发送系统通知失败: {e}"))
}

/// 返回当前 Chat 工作目录的 Git 状态；非 Git 或路径不可读时返回空状态。
#[tauri::command]
pub fn chat_workspace_status(cwd: Option<String>) -> Result<ChatWorkspaceStatus, String> {
    resolve_chat_workspace_status(cwd)
}

/// 在系统资源管理器中打开 Chat 相关路径。
#[tauri::command]
pub fn chat_open_path_in_explorer(app: AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;

    let path = resolve_existing_chat_path(path, "路径")?;
    app.opener()
        .open_path(path.to_string_lossy().as_ref(), None::<String>)
        .map_err(|e| format!("在资源管理器中打开失败: {e}"))
}

/// 列出当前 Chat 工作目录所在 Git 仓库的本地分支。
#[tauri::command]
pub fn chat_git_list_branches(cwd: String) -> Result<Vec<ChatGitBranch>, String> {
    let cwd = resolve_existing_chat_directory(cwd, "工作目录")?;
    list_chat_git_branches_for_path(&cwd)
}

/// 创建并检出一个新的 Git 分支，成功后返回最新工作区状态。
#[tauri::command]
pub fn chat_git_create_and_checkout_branch(
    cwd: String,
    branch_name: String,
) -> Result<ChatWorkspaceStatus, String> {
    let cwd = resolve_existing_chat_directory(cwd, "工作目录")?;
    let branch_name = normalize_chat_git_branch_name(&branch_name)?;
    let (repo_root, _) = resolve_git_repository(&cwd)?;

    let output = Command::new("git")
        .arg("-C")
        .arg(&repo_root)
        .args(["checkout", "-b"])
        .arg(&branch_name)
        .output()
        .map_err(|e| format!("创建 Git 分支失败: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            "创建 Git 分支失败".to_string()
        } else {
            format!("创建 Git 分支失败: {stderr}")
        });
    }

    resolve_chat_workspace_status(Some(repo_root.to_string_lossy().to_string()))
}

/// 为 Helm agent 创建独立 worktree，并返回其路径与分支名。
#[tauri::command]
pub fn helm_worktree_create(repo_root: String, name: String) -> Result<WorktreeInfoDto, String> {
    let cwd = resolve_existing_chat_directory(repo_root, "Git 仓库")?;
    let (repo_root, _) = resolve_git_repository(&cwd)?;
    let worktrees_dir = repo_worktrees_dir(&repo_root)?;
    WorktreeManager::create(&repo_root, &worktrees_dir, name.trim()).map(worktree_info_dto)
}

/// 删除 Helm worktree。非 force 时底层会先做脏检查，拒绝删除有改动的工作树。
#[tauri::command]
pub fn helm_worktree_remove(
    repo_root: String,
    worktree_path: String,
    force: bool,
) -> Result<(), String> {
    let cwd = resolve_existing_chat_directory(repo_root, "Git 仓库")?;
    let (repo_root, _) = resolve_git_repository(&cwd)?;
    let worktree_path = resolve_existing_chat_directory(worktree_path, "worktree 路径")?;
    WorktreeManager::remove(&repo_root, &worktree_path, force)
}

/// 列出指定 Git 仓库下的所有 worktree。
#[tauri::command]
pub fn helm_worktree_list(repo_root: String) -> Result<Vec<WorktreeInfoDto>, String> {
    let cwd = resolve_existing_chat_directory(repo_root, "Git 仓库")?;
    let (repo_root, _) = resolve_git_repository(&cwd)?;
    WorktreeManager::list(&repo_root)
        .map(|items| items.into_iter().map(worktree_info_dto).collect())
}

/// 返回 worktree 相对 HEAD 的 diff 摘要。
#[tauri::command]
pub fn helm_worktree_diff(worktree_path: String) -> Result<DiffSummaryDto, String> {
    let worktree_path = resolve_existing_chat_directory(worktree_path, "worktree 路径")?;
    WorktreeManager::diff_summary(&worktree_path).map(diff_summary_dto)
}

/// 关闭 Helm agent，并可选删除其 worktree（删除前由 WorktreeManager 做脏检查）。
#[tauri::command]
pub async fn helm_close_agent(
    agent_id: String,
    remove_worktree: bool,
    repo_root: Option<String>,
    worktree_path: Option<String>,
    force: bool,
    state: State<'_, ChatState>,
) -> Result<(), String> {
    let agent = resolve_agent_id(Some(agent_id));
    state.manager.close_agent(agent).await;

    if !remove_worktree {
        return Ok(());
    }

    let repo_root = repo_root.ok_or_else(|| "删除 worktree 需要 Git 仓库路径".to_string())?;
    let worktree_path = worktree_path.ok_or_else(|| "删除 worktree 需要 worktree 路径".to_string())?;
    let cwd = resolve_existing_chat_directory(repo_root, "Git 仓库")?;
    let (repo_root, _) = resolve_git_repository(&cwd)?;
    let worktree_path = resolve_existing_chat_directory(worktree_path, "worktree 路径")?;
    WorktreeManager::remove(&repo_root, &worktree_path, force)
}

/// 列出所有 SDK 的安装状态（Claude / Codex）。
#[tauri::command]
pub async fn chat_sdk_status(
    state: State<'_, ChatState>,
) -> Result<Vec<crate::chat::SdkStatus>, String> {
    state.manager.sdk_status().await
}

/// 返回 Node.js 运行环境状态。系统 Node 可用或私有 runtime 已安装时为 installed。
#[tauri::command]
pub async fn chat_node_runtime_status(
    state: State<'_, ChatState>,
) -> Result<crate::chat::NodeRuntimeStatus, String> {
    state.manager.node_runtime_status().await
}

/// 安装 CCG Switch 私有 Node.js runtime。
#[tauri::command]
pub async fn chat_install_node_runtime(
    state: State<'_, ChatState>,
) -> Result<crate::chat::NodeRuntimeStatus, String> {
    state.manager.install_node_runtime().await
}

/// 安装指定 SDK。npm 日志通过 "chat://sdk-install-log" 事件流式推送，
/// 结束时发 "chat://sdk-install-done"。
#[tauri::command]
pub async fn chat_install_sdk(
    sdk_id: String,
    version: Option<String>,
    state: State<'_, ChatState>,
) -> Result<(), String> {
    state.manager.install_sdk(sdk_id, version).await
}

/// 卸载指定 SDK。
#[tauri::command]
pub async fn chat_uninstall_sdk(sdk_id: String, state: State<'_, ChatState>) -> Result<(), String> {
    state.manager.uninstall_sdk(sdk_id).await
}

/// 重启 daemon（用于手动刷新 / SDK 安装后）。
/// `agent_id = None` 重启所有在跑 daemon（deps 目录共享）；指定则只重启该 agent。
#[tauri::command]
pub async fn chat_restart_daemon(
    agent_id: Option<String>,
    state: State<'_, ChatState>,
) -> Result<(), String> {
    let agent = agent_id.map(|raw| crate::chat::sanitize_agent_id(&raw));
    state.manager.restart_daemon(agent).await
}

/// 列出 Slash 命令，用于输入框 `/` 补全。
///
/// 内置命令对齐 cc-gui 的 SlashCommandRegistry，并额外扫描当前项目向上的
/// `.claude/commands/**/*.md`，避免补全长期依赖前端硬编码列表。
#[tauri::command]
pub fn chat_list_slash_commands(
    cwd: Option<String>,
    provider: Option<String>,
) -> Result<Vec<SlashCommandItem>, String> {
    Ok(
        crate::chat::list_slash_commands(provider.as_deref(), cwd.as_deref())
            .into_iter()
            .map(|command| SlashCommandItem {
                id: command.id,
                name: command.name,
                description: command.description,
                source: command.source,
            })
            .collect(),
    )
}

/// 响应 AskUserQuestion 权限请求。
///
/// `request_id` 来自 "permission://ask-user-question" 事件，`answers` 是
/// { "问题文本": "用户选择的答案" } 的 map。
#[tauri::command]
pub async fn permission_respond_ask_user_question(
    request_id: String,
    session_id: Option<String>,
    answers: std::collections::HashMap<String, String>,
    state: State<'_, ChatState>,
) -> Result<(), String> {
    let session_id = crate::chat::permission_response_session_id(session_id);
    // 多 agent：响应写到该 agent 的 permission 子目录（daemon 只读自己子目录）。
    let perm_dir = agent_permission_dir(
        &crate::chat::permission_dir(state.manager.app())?,
        &session_id,
    );
    let _ = std::fs::create_dir_all(&perm_dir);
    crate::chat::write_ask_user_question_response(&perm_dir, &session_id, &request_id, answers)
}

/// 响应普通工具权限请求。
///
/// `request_id` 来自 "permission://tool" 事件，`allow` 会写入
/// `response-<sessionId>-<requestId>.json`，供 ai-bridge 继续执行或拒绝工具。
#[tauri::command]
pub async fn permission_respond_tool(
    request_id: String,
    session_id: Option<String>,
    allow: bool,
    state: State<'_, ChatState>,
) -> Result<(), String> {
    let session_id = crate::chat::permission_response_session_id(session_id);
    let perm_dir = agent_permission_dir(
        &crate::chat::permission_dir(state.manager.app())?,
        &session_id,
    );
    let _ = std::fs::create_dir_all(&perm_dir);
    crate::chat::write_tool_permission_response(&perm_dir, &session_id, &request_id, allow)
}

/// 列出工作目录下的文件，用于 `@` 文件引用补全。
///
/// `dir` 为工作目录（缺省用用户主目录）；`query` 为已输入的过滤词（按文件名/
/// 相对路径子串匹配，大小写不敏感）。最多返回 50 项，跳过常见的重型目录
/// （node_modules / .git / target / dist 等）与隐藏目录，限制扫描深度防卡顿。
///
/// 注意：文件系统遍历是阻塞操作，必须放到 `spawn_blocking` 线程池执行，
/// 否则会阻塞 Tauri 主线程导致界面卡死（"未响应"）。
#[tauri::command]
pub async fn chat_list_workspace_files(
    dir: Option<String>,
    query: Option<String>,
) -> Result<Vec<WorkspaceFile>, String> {
    tauri::async_runtime::spawn_blocking(move || list_workspace_files_blocking(dir, query))
        .await
        .map_err(|e| format!("文件扫描任务失败: {e}"))?
}

/// 同步执行工作目录文件扫描（阻塞）。由 `chat_list_workspace_files` 在
/// 后台线程池调用，不可直接在命令层（主线程）调用。
fn list_workspace_files_blocking(
    dir: Option<String>,
    query: Option<String>,
) -> Result<Vec<WorkspaceFile>, String> {
    use std::path::PathBuf;

    let root: PathBuf = match dir {
        Some(d) if !d.trim().is_empty() => PathBuf::from(d),
        _ => dirs::home_dir().ok_or_else(|| "无法定位主目录".to_string())?,
    };
    let query = query.unwrap_or_default().to_lowercase();

    const SKIP_DIRS: &[&str] = &[
        "node_modules",
        ".git",
        "target",
        "dist",
        "build",
        ".next",
        ".cache",
        "__pycache__",
        ".venv",
        "vendor",
    ];
    const MAX_RESULTS: usize = 50;
    const MAX_DEPTH: usize = 6;

    let mut out: Vec<WorkspaceFile> = Vec::new();
    // 广度优先，避免单分支过深；用栈记录 (路径, 深度)。
    let mut stack: Vec<(PathBuf, usize)> = vec![(root.clone(), 0)];

    while let Some((cur, depth)) = stack.pop() {
        if out.len() >= MAX_RESULTS {
            break;
        }
        let entries = match std::fs::read_dir(&cur) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            // 跳过隐藏项与重型目录
            if name.starts_with('.') || SKIP_DIRS.contains(&name.as_str()) {
                continue;
            }
            let is_dir = path.is_dir();
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");

            let matches = query.is_empty()
                || rel.to_lowercase().contains(&query)
                || name.to_lowercase().contains(&query);
            if matches {
                out.push(WorkspaceFile {
                    rel_path: rel,
                    name,
                    is_dir,
                });
                if out.len() >= MAX_RESULTS {
                    break;
                }
            }
            if is_dir && depth < MAX_DEPTH {
                stack.push((path, depth + 1));
            }
        }
    }

    // 目录在前、文件在后，再按路径排序，便于浏览。
    out.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.rel_path.cmp(&b.rel_path),
    });
    Ok(out)
}

/// 一键润色当前输入的 Prompt。返回增强后的文本（失败返回 Err，前端保留原文）。
#[tauri::command]
pub async fn chat_enhance_prompt(
    prompt: String,
    model: String,
    state: State<'_, ChatState>,
) -> Result<String, String> {
    state.manager.enhance_prompt(prompt, model).await
}

/// 响应 PlanApproval 权限请求。
///
/// `approved`: 是否批准；`target_mode`: "default" / "auto" / "bypassPermissions"。
#[tauri::command]
pub async fn permission_respond_plan_approval(
    request_id: String,
    session_id: Option<String>,
    approved: bool,
    target_mode: String,
    message: Option<String>,
    state: State<'_, ChatState>,
) -> Result<(), String> {
    let session_id = crate::chat::permission_response_session_id(session_id);
    let perm_dir = agent_permission_dir(
        &crate::chat::permission_dir(state.manager.app())?,
        &session_id,
    );
    let _ = std::fs::create_dir_all(&perm_dir);
    crate::chat::write_plan_approval_response(
        &perm_dir,
        &session_id,
        &request_id,
        approved,
        target_mode,
        message,
    )
}

/// 在系统终端打开会话的工作目录。
///
/// - 路径验证：拒绝空路径、不存在的目录、控制字符。
/// - 根据平台调用系统终端：
///   - Windows: cmd /c start "" /D <projectDir> cmd /K
///   - macOS: osascript -e 'tell app "Terminal" to do script "cd <projectDir>"'
///   - Linux: 检测常见终端（gnome-terminal / konsole / xterm）
#[tauri::command]
pub fn chat_open_project_in_terminal(project_dir: String) -> Result<(), String> {
    let trimmed = project_dir.trim();
    if trimmed.is_empty() {
        return Err("工作目录路径为空".to_string());
    }
    if trimmed.contains(|c: char| c.is_control()) {
        return Err("工作目录路径包含非法字符".to_string());
    }

    let path = Path::new(trimmed);
    if !path.exists() {
        return Err(format!("工作目录不存在: {}", trimmed));
    }
    if !path.is_dir() {
        return Err(format!("路径不是目录: {}", trimmed));
    }

    #[cfg(target_os = "windows")]
    {
        let args = windows_terminal_start_args(Some(trimmed), None);
        Command::new("cmd")
            .args(args)
            .spawn()
            .map_err(|e| format!("无法打开终端: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "tell app \"Terminal\" to do script \"cd \\\"{}\\\"\"",
            trimmed
        );
        Command::new("osascript")
            .args(&["-e", &script])
            .spawn()
            .map_err(|e| format!("无法打开终端: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        let cmd = format!("cd \"{}\" && exec bash", trimmed);

        // 尝试检测常见终端
        let terminals = vec![
            ("gnome-terminal", vec!["--", "bash", "-c", &cmd]),
            ("konsole", vec!["-e", "bash", "-c", &cmd]),
            ("xterm", vec!["-e", "bash", "-c", &cmd]),
            ("x-terminal-emulator", vec!["-e", "bash", "-c", &cmd]),
        ];

        let mut opened = false;
        for (terminal, args) in terminals {
            if let Ok(mut child) = Command::new(terminal).args(&args).spawn() {
                let _ = child.wait();
                opened = true;
                break;
            }
        }

        if !opened {
            return Err("无法打开终端：未找到可用的终端模拟器".to_string());
        }
    }

    Ok(())
}

/// 在系统终端恢复会话继续对话。
///
/// - 直接使用会话的 resume_command（如 "claude --resume <id>" / "codex resume <id>"）。
/// - 如果提供 project_dir，Windows 通过 start /D 指定目录，其他平台先 cd 再执行命令。
/// - 根据平台调用系统终端执行命令。
#[tauri::command]
pub fn chat_resume_session_in_terminal(
    resume_command: String,
    project_dir: Option<String>,
) -> Result<(), String> {
    let trimmed_cmd = resume_command.trim();
    if trimmed_cmd.is_empty() {
        return Err("恢复命令为空".to_string());
    }
    if trimmed_cmd.contains(|c: char| c.is_control()) {
        return Err("恢复命令包含非法字符".to_string());
    }

    let valid_project_dir = project_dir.as_deref().map(str::trim).filter(|dir| {
        if dir.is_empty() {
            return false;
        }
        let path = Path::new(dir);
        path.exists() && path.is_dir()
    });

    #[cfg(target_os = "windows")]
    {
        let args = windows_terminal_start_args(valid_project_dir, Some(trimmed_cmd));
        Command::new("cmd")
            .args(args)
            .spawn()
            .map_err(|e| format!("无法打开终端: {}", e))?;
    }

    #[cfg(not(target_os = "windows"))]
    let full_cmd = {
        let mut cmd_parts = Vec::new();
        if let Some(dir) = valid_project_dir {
            cmd_parts.push(format!("cd \"{}\"", dir));
        }
        cmd_parts.push(trimmed_cmd.to_string());
        cmd_parts.join(" && ")
    };

    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "tell app \"Terminal\" to do script \"{}\"",
            full_cmd.replace("\"", "\\\"")
        );
        Command::new("osascript")
            .args(&["-e", &script])
            .spawn()
            .map_err(|e| format!("无法打开终端: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        let terminals = vec![
            (
                "gnome-terminal",
                vec!["--", "bash", "-c", &format!("{}; exec bash", full_cmd)],
            ),
            (
                "konsole",
                vec!["-e", "bash", "-c", &format!("{}; exec bash", full_cmd)],
            ),
            (
                "xterm",
                vec!["-e", "bash", "-c", &format!("{}; exec bash", full_cmd)],
            ),
            (
                "x-terminal-emulator",
                vec!["-e", "bash", "-c", &format!("{}; exec bash", full_cmd)],
            ),
        ];

        let mut opened = false;
        for (terminal, args) in terminals {
            if let Ok(mut child) = Command::new(terminal).args(&args).spawn() {
                let _ = child.wait();
                opened = true;
                break;
            }
        }

        if !opened {
            return Err("无法打开终端：未找到可用的终端模拟器".to_string());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};

    #[test]
    fn agent_permission_dir_routes_to_session_subdir() {
        let root = PathBuf::from("/tmp/perm");
        // 多 agent：响应文件必须落到该 agent 的 permission 子目录，daemon 才读得到。
        assert_eq!(
            agent_permission_dir(&root, "agent-7"),
            PathBuf::from("/tmp/perm/agent-7")
        );
        assert_eq!(
            agent_permission_dir(&root, "default"),
            PathBuf::from("/tmp/perm/default")
        );
    }

    fn unique_test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ccg-switch-{name}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        fs::create_dir_all(&dir).expect("create test dir");
        dir
    }

    fn write_file(path: &Path, text: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dir");
        }
        fs::write(path, text).expect("write test file");
    }

    #[test]
    fn normalizes_system_notification_payload_trims_title_and_body() -> Result<(), String> {
        let payload = normalize_system_notification_payload("  CCG Switch  ", "  done\n")?;

        assert_eq!(payload.title, "CCG Switch");
        assert_eq!(payload.body, "done");
        Ok(())
    }

    #[test]
    fn normalizes_system_notification_payload_rejects_empty_title() {
        let error =
            normalize_system_notification_payload(" \n\t ", "body").expect_err("empty title");

        assert_eq!(error, "发送系统通知失败: 标题不能为空");
    }

    #[test]
    fn normalizes_system_notification_payload_allows_empty_body() -> Result<(), String> {
        let payload = normalize_system_notification_payload("CCG Switch", " \n\t ")?;

        assert_eq!(payload.title, "CCG Switch");
        assert_eq!(payload.body, "");
        Ok(())
    }

    #[test]
    fn resolves_workspace_status_none_for_non_git_directory() -> Result<(), String> {
        let dir = unique_test_dir("workspace-status-no-git");
        let status = resolve_chat_workspace_status(Some(dir.to_string_lossy().to_string()))?;

        assert!(!status.is_git_repository);
        assert_eq!(status.git_branch, None);
        assert_eq!(status.git_root, None);

        fs::remove_dir_all(dir).ok();
        Ok(())
    }

    #[test]
    fn resolves_workspace_status_from_git_head_branch() -> Result<(), String> {
        let dir = unique_test_dir("workspace-status-git-branch");
        let nested = dir.join("packages").join("app");
        fs::create_dir_all(&nested).expect("create nested dir");
        write_file(
            &dir.join(".git").join("HEAD"),
            "ref: refs/heads/feature/chat-status\n",
        );

        let status = resolve_chat_workspace_status(Some(nested.to_string_lossy().to_string()))?;

        assert!(status.is_git_repository);
        assert_eq!(status.git_branch.as_deref(), Some("feature/chat-status"));
        assert_eq!(
            status.git_root.as_deref(),
            Some(dir.to_string_lossy().as_ref())
        );

        fs::remove_dir_all(dir).ok();
        Ok(())
    }

    #[test]
    fn resolves_workspace_status_from_gitdir_file() -> Result<(), String> {
        let dir = unique_test_dir("workspace-status-gitdir-file");
        let actual_git_dir = dir.join(".git-worktrees").join("app");
        write_file(&dir.join(".git"), "gitdir: .git-worktrees/app\n");
        write_file(
            &actual_git_dir.join("HEAD"),
            "ref: refs/heads/worktree/status-strip\n",
        );

        let status = resolve_chat_workspace_status(Some(dir.to_string_lossy().to_string()))?;

        assert!(status.is_git_repository);
        assert_eq!(status.git_branch.as_deref(), Some("worktree/status-strip"));
        assert_eq!(
            status.git_root.as_deref(),
            Some(dir.to_string_lossy().as_ref())
        );

        fs::remove_dir_all(dir).ok();
        Ok(())
    }

    #[test]
    fn normalizes_chat_git_branch_name_rejects_unsafe_values() {
        assert_eq!(
            normalize_chat_git_branch_name(" feature/workspace-switch ").as_deref(),
            Ok("feature/workspace-switch")
        );
        assert!(normalize_chat_git_branch_name(" ").is_err());
        assert!(normalize_chat_git_branch_name("-danger").is_err());
        assert!(normalize_chat_git_branch_name("feature\nbad").is_err());
        assert!(normalize_chat_git_branch_name("feature..bad").is_err());
        assert!(normalize_chat_git_branch_name("feature.lock").is_err());
    }

    #[test]
    fn windows_terminal_args_open_project_use_start_directory() {
        let project_dir = r"C:\guodevelop\ccg-switch\src-tauri";
        let args = windows_terminal_start_args(Some(project_dir), None);

        assert_eq!(
            args,
            vec!["/c", "start", "", "/D", project_dir, "cmd", "/K",]
        );
        assert!(!args.iter().any(|arg| arg.contains("cd /d")));
    }

    #[test]
    fn windows_terminal_args_resume_use_start_directory_and_resume_command() {
        let project_dir = r"C:\guodevelop\ccg-switch\src-tauri";
        let resume_command = "codex resume abc123";
        let args = windows_terminal_start_args(Some(project_dir), Some(resume_command));

        assert_eq!(
            args,
            vec![
                "/c",
                "start",
                "",
                "/D",
                project_dir,
                "cmd",
                "/K",
                resume_command,
            ]
        );
        assert!(!args.iter().any(|arg| arg.contains("cd /d")));
    }

    #[test]
    fn lists_chat_git_branches_from_refs_heads() -> Result<(), String> {
        let dir = unique_test_dir("chat-git-list-branches");
        write_file(
            &dir.join(".git").join("HEAD"),
            "ref: refs/heads/feature/chat-ui\n",
        );
        write_file(
            &dir.join(".git").join("refs").join("heads").join("main"),
            "0000000\n",
        );
        write_file(
            &dir.join(".git")
                .join("refs")
                .join("heads")
                .join("feature")
                .join("chat-ui"),
            "0000000\n",
        );

        let branches = list_chat_git_branches_for_path(&dir)?;

        assert_eq!(
            branches,
            vec![
                ChatGitBranch {
                    name: "feature/chat-ui".to_string(),
                    current: true,
                },
                ChatGitBranch {
                    name: "main".to_string(),
                    current: false,
                },
            ]
        );

        fs::remove_dir_all(dir).ok();
        Ok(())
    }
}
