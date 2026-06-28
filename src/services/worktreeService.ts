import {invoke} from '@tauri-apps/api/core';

export interface HelmWorktreeInfo {
    path: string;
    branch: string;
}

export interface HelmDiffSummary {
    filesChanged: number;
    insertions: number;
    deletions: number;
}

export async function createWorktree(repoRoot: string, name: string): Promise<HelmWorktreeInfo> {
    return invoke<HelmWorktreeInfo>('helm_worktree_create', {
        repoRoot: repoRoot.trim(),
        name: name.trim(),
    });
}

export async function removeWorktree(
    repoRoot: string,
    worktreePath: string,
    force = false,
): Promise<void> {
    await invoke('helm_worktree_remove', {
        repoRoot: repoRoot.trim(),
        worktreePath: worktreePath.trim(),
        force,
    });
}

export async function listWorktrees(repoRoot: string): Promise<HelmWorktreeInfo[]> {
    return invoke<HelmWorktreeInfo[]>('helm_worktree_list', {
        repoRoot: repoRoot.trim(),
    });
}

export async function worktreeDiff(worktreePath: string): Promise<HelmDiffSummary> {
    return invoke<HelmDiffSummary>('helm_worktree_diff', {
        worktreePath: worktreePath.trim(),
    });
}

export async function closeAgent(options: {
    agentId: string;
    removeWorktree?: boolean;
    repoRoot?: string | null;
    worktreePath?: string | null;
    force?: boolean;
}): Promise<void> {
    await invoke('helm_close_agent', {
        agentId: options.agentId.trim(),
        removeWorktree: options.removeWorktree === true,
        repoRoot: options.repoRoot?.trim() || undefined,
        worktreePath: options.worktreePath?.trim() || undefined,
        force: options.force === true,
    });
}
