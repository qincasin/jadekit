import type {MessageRaw} from './chat';

export interface SessionMeta {
    providerId: 'claude' | 'codex' | 'gemini';
    sessionId: string;
    title: string | null;
    summary: string | null;
    projectDir: string | null;
    createdAt: number;
    lastActiveAt: number;
    sourcePath: string;
    resumeCommand: string | null;
    pinned?: boolean;
    archived?: boolean;
    unread?: boolean;
}

export interface UnifiedSessionMessage {
    role: string;
    content: string;
    ts?: string;
    raw?: MessageRaw | null;
}

export interface UnifiedSessionMessageWindow {
    messages: UnifiedSessionMessage[];
    startIndex: number;
    totalCount: number;
    complete: boolean;
}

export interface ChatSessionLoadMetrics {
    sessionKey: string;
    providerId: 'claude' | 'codex';
    sourcePath: string;
    cacheHit: boolean;
    status: 'loading' | 'windowed' | 'complete' | 'error';
    startedAt: number;
    completedAt: number | null;
    elapsedMs: number | null;
    windowMessageCount: number;
    totalMessageCount: number | null;
    fullMessageCount: number | null;
    windowLoadMs: number | null;
    windowMapMs: number | null;
    fullLoadMs: number | null;
    fullMapMs: number | null;
    error: string | null;
}

export function getSessionSelectionKey(
    session: Pick<SessionMeta, 'providerId' | 'sourcePath'>,
): string {
    return `${session.providerId}::${session.sourcePath}`;
}

export type ProviderFilter = 'all' | 'claude' | 'codex' | 'gemini';
export type ViewMode = 'project' | 'all';
