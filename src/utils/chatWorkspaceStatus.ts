import {invoke, isTauri} from '@tauri-apps/api/core';

export interface ChatWorkspaceStatus {
    isGitRepository: boolean;
    gitRoot: string | null;
    gitBranch: string | null;
}

export interface ChatGitBranch {
    name: string;
    current: boolean;
}

export interface RenameChatSessionResult {
    title: string;
}

interface RawChatWorkspaceStatus {
    is_git_repository?: boolean;
    git_root?: string | null;
    git_branch?: string | null;
}

export const EMPTY_CHAT_WORKSPACE_STATUS: ChatWorkspaceStatus = {
    isGitRepository: false,
    gitRoot: null,
    gitBranch: null,
};

function cleanString(value: unknown): string | null {
    if (typeof value !== 'string') return null;
    const trimmed = value.trim();
    return trimmed.length > 0 ? trimmed : null;
}

export function normalizeChatWorkspaceStatus(
    raw: RawChatWorkspaceStatus | null | undefined,
): ChatWorkspaceStatus {
    if (!raw) return EMPTY_CHAT_WORKSPACE_STATUS;

    return {
        isGitRepository: raw.is_git_repository === true,
        gitRoot: cleanString(raw.git_root),
        gitBranch: cleanString(raw.git_branch),
    };
}

export async function loadChatWorkspaceStatus(cwd?: string | null): Promise<ChatWorkspaceStatus> {
    try {
        const raw = await invoke<RawChatWorkspaceStatus>('chat_workspace_status', {
            cwd: cwd?.trim() || undefined,
        });
        return normalizeChatWorkspaceStatus(raw);
    } catch {
        return EMPTY_CHAT_WORKSPACE_STATUS;
    }
}

export async function listChatGitBranches(cwd: string): Promise<ChatGitBranch[]> {
    const raw = await invoke<ChatGitBranch[]>('chat_git_list_branches', {
        cwd: cwd.trim(),
    });

    return raw
        .map((branch) => ({
            name: cleanString(branch.name),
            current: branch.current === true,
        }))
        .filter((branch): branch is ChatGitBranch => branch.name !== null);
}

export async function createAndCheckoutChatGitBranch(
    cwd: string,
    branchName: string,
): Promise<ChatWorkspaceStatus> {
    const raw = await invoke<RawChatWorkspaceStatus>('chat_git_create_and_checkout_branch', {
        cwd: cwd.trim(),
        branchName: branchName.trim(),
    });
    return normalizeChatWorkspaceStatus(raw);
}

export async function openChatPathInExplorer(path: string): Promise<void> {
    await invoke('chat_open_path_in_explorer', {
        path: path.trim(),
    });
}

export async function renameChatSessionTitle(
    providerId: string,
    sessionId: string,
    title: string,
): Promise<RenameChatSessionResult> {
    return invoke<RenameChatSessionResult>('chat_session_rename', {
        providerId: providerId.trim(),
        sessionId: sessionId.trim(),
        title: title.trim(),
    });
}

/**
 * 打开原生文件夹选择对话框，返回所选目录绝对路径。
 *
 * 在 Tauri 运行时只走系统原生目录选择器（dialog 插件）；调用失败直接抛出，
 * 由调用方提示，避免静默退化成浏览器 prompt 误导用户。仅在非 Tauri 环境
 * （如纯浏览器测试）才回退 `window.prompt`，保证空环境不崩溃。
 */
export async function pickWorkspaceFolder(options?: {
    defaultPath?: string | null;
    title?: string;
    promptFallbackLabel?: string;
}): Promise<string | null> {
    const defaultPath = options?.defaultPath?.trim() || undefined;

    let runningInTauri = false;
    try {
        runningInTauri = isTauri();
    } catch {
        runningInTauri = false;
    }

    if (runningInTauri) {
        const {open} = await import('@tauri-apps/plugin-dialog');
        const selected = await open({
            directory: true,
            multiple: false,
            defaultPath,
            title: options?.title,
        });
        if (typeof selected === 'string') {
            const trimmed = selected.trim();
            return trimmed.length > 0 ? trimmed : null;
        }
        return null;
    }

    if (typeof window === 'undefined') return null;
    const promptLabel = options?.promptFallbackLabel ?? options?.title ?? 'Open folder path';
    const manual = window.prompt(promptLabel, defaultPath ?? '')?.trim();
    return manual && manual.length > 0 ? manual : null;
}
