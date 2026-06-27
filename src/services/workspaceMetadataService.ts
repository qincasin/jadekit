import {invoke} from '@tauri-apps/api/core';

/**
 * Workspace 元数据动作的前端服务封装。
 *
 * 对应后端 `session_commands` 中的项目 / 会话置顶、归档、未读、重命名、移除命令，
 * 状态持久化在 `~/.jadekit/workspace-metadata.json`，不污染各 provider 原生历史。
 */

export interface RenameProjectResult {
    name: string;
}

export async function setProjectPinned(projectPath: string, pinned: boolean): Promise<void> {
    await invoke('chat_project_set_pinned', {projectPath: projectPath.trim(), pinned});
}

export async function setProjectArchived(projectPath: string, archived: boolean): Promise<void> {
    await invoke('chat_project_set_archived', {projectPath: projectPath.trim(), archived});
}

export async function removeProject(projectPath: string): Promise<void> {
    await invoke('chat_project_remove', {projectPath: projectPath.trim()});
}

export async function renameProject(projectPath: string, name: string): Promise<RenameProjectResult> {
    return invoke<RenameProjectResult>('chat_project_rename', {
        projectPath: projectPath.trim(),
        name: name.trim(),
    });
}

export async function markProjectAllRead(projectPath: string): Promise<void> {
    await invoke('chat_project_mark_all_read', {projectPath: projectPath.trim()});
}

export async function setSessionPinned(sessionId: string, pinned: boolean): Promise<void> {
    await invoke('chat_session_set_pinned', {sessionId: sessionId.trim(), pinned});
}

export async function setSessionArchived(sessionId: string, archived: boolean): Promise<void> {
    await invoke('chat_session_set_archived', {sessionId: sessionId.trim(), archived});
}

export async function setSessionUnread(sessionId: string, unread: boolean): Promise<void> {
    await invoke('chat_session_set_unread', {sessionId: sessionId.trim(), unread});
}
