export interface FanoutMergeTabLike {
    worktreeBranch?: string | null;
    activeRequestId?: string | null;
    status?: string | null;
}

export function canMergeFanoutTab(tab: FanoutMergeTabLike): boolean {
    return Boolean(tab.worktreeBranch?.trim())
        && !tab.activeRequestId
        && tab.status !== 'running'
        && tab.status !== 'loading'
        && tab.status !== 'queued';
}
