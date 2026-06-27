export type ChatSessionSidebarPanelMode = 'project' | 'recent';

export interface ChatSessionSidebarState {
    panelMode: ChatSessionSidebarPanelMode;
    collapsedRecentProjectKeys: string[];
}

export const CHAT_SESSION_SIDEBAR_STATE_STORAGE_KEY = 'ccg-chat-session-sidebar-state';

export const DEFAULT_CHAT_SESSION_SIDEBAR_STATE: ChatSessionSidebarState = {
    panelMode: 'project',
    collapsedRecentProjectKeys: [],
};

function isChatSessionSidebarPanelMode(value: unknown): value is ChatSessionSidebarPanelMode {
    return value === 'project' || value === 'recent';
}

function isChatSessionSidebarState(value: unknown): value is ChatSessionSidebarState {
    if (!value || typeof value !== 'object') return false;

    const candidate = value as Partial<ChatSessionSidebarState>;
    return isChatSessionSidebarPanelMode(candidate.panelMode)
        && Array.isArray(candidate.collapsedRecentProjectKeys)
        && candidate.collapsedRecentProjectKeys.every((key) => typeof key === 'string');
}

export function loadChatSessionSidebarState(): ChatSessionSidebarState {
    try {
        const raw = window.localStorage.getItem(CHAT_SESSION_SIDEBAR_STATE_STORAGE_KEY);
        if (!raw) return DEFAULT_CHAT_SESSION_SIDEBAR_STATE;

        const parsed = JSON.parse(raw) as unknown;
        return isChatSessionSidebarState(parsed)
            ? parsed
            : DEFAULT_CHAT_SESSION_SIDEBAR_STATE;
    } catch {
        return DEFAULT_CHAT_SESSION_SIDEBAR_STATE;
    }
}

export function saveChatSessionSidebarState(state: ChatSessionSidebarState): void {
    try {
        window.localStorage.setItem(CHAT_SESSION_SIDEBAR_STATE_STORAGE_KEY, JSON.stringify(state));
    } catch {
        // localStorage can be unavailable in restricted WebView/browser contexts.
    }
}
