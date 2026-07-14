export interface SendCwdSource {
    worktreePath?: string | null;
    cwd?: string | null;
}

function cleanPath(value: string | null | undefined): string | undefined {
    const trimmed = value?.trim();
    return trimmed ? trimmed : undefined;
}

export function resolveSendCwd(tab: SendCwdSource, fallback?: string | null): string | undefined {
    return cleanPath(tab.worktreePath) ?? cleanPath(tab.cwd) ?? cleanPath(fallback);
}
