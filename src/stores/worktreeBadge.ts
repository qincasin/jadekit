export interface WorktreeBadgeInfo {
    branch: string;
    diff?: {
        filesChanged: number;
    } | null;
}

export function worktreeBadgeLabel(info: WorktreeBadgeInfo): string {
    const branch = info.branch.trim();
    const filesChanged = info.diff?.filesChanged ?? 0;
    if (filesChanged > 0) {
        return `${branch} · ${filesChanged} changed`;
    }
    return branch;
}
