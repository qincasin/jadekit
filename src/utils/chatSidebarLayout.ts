export interface ChatSidebarLayoutState {
    sessionSidebarCollapsed: boolean;
    statusSidebarCollapsed: boolean;
}

export type ChatSidebarLayoutAction =
    | 'collapse-session-sidebar'
    | 'expand-session-sidebar'
    | 'collapse-status-sidebar'
    | 'expand-status-sidebar';

interface ChatSidebarLayoutActionLabelInput {
    action: ChatSidebarLayoutAction;
    translate: (key: string) => string;
}

export const CHAT_SIDEBAR_LAYOUT_STORAGE_KEY = 'ccg-chat-sidebar-layout';

export const DEFAULT_CHAT_SIDEBAR_LAYOUT_STATE: ChatSidebarLayoutState = {
    sessionSidebarCollapsed: false,
    statusSidebarCollapsed: false,
};

const CHAT_SIDEBAR_LAYOUT_ACTION_LABELS: Record<ChatSidebarLayoutAction, {key: string; fallback: string}> = {
    'collapse-session-sidebar': {
        key: 'chat.layout.collapseSessionSidebar',
        fallback: 'Collapse session sidebar',
    },
    'expand-session-sidebar': {
        key: 'chat.layout.expandSessionSidebar',
        fallback: 'Expand session sidebar',
    },
    'collapse-status-sidebar': {
        key: 'chat.layout.collapseStatusSidebar',
        fallback: 'Collapse status sidebar',
    },
    'expand-status-sidebar': {
        key: 'chat.layout.expandStatusSidebar',
        fallback: 'Expand status sidebar',
    },
};

function isChatSidebarLayoutState(value: unknown): value is ChatSidebarLayoutState {
    if (!value || typeof value !== 'object') return false;

    const candidate = value as Partial<ChatSidebarLayoutState>;
    return typeof candidate.sessionSidebarCollapsed === 'boolean'
        && typeof candidate.statusSidebarCollapsed === 'boolean';
}

export function loadChatSidebarLayoutState(): ChatSidebarLayoutState {
    try {
        const raw = window.localStorage.getItem(CHAT_SIDEBAR_LAYOUT_STORAGE_KEY);
        if (!raw) return DEFAULT_CHAT_SIDEBAR_LAYOUT_STATE;

        const parsed = JSON.parse(raw) as unknown;
        return isChatSidebarLayoutState(parsed)
            ? parsed
            : DEFAULT_CHAT_SIDEBAR_LAYOUT_STATE;
    } catch {
        return DEFAULT_CHAT_SIDEBAR_LAYOUT_STATE;
    }
}

export function saveChatSidebarLayoutState(state: ChatSidebarLayoutState): void {
    try {
        window.localStorage.setItem(CHAT_SIDEBAR_LAYOUT_STORAGE_KEY, JSON.stringify(state));
    } catch {
        // localStorage can be unavailable in restricted WebView/browser contexts.
    }
}

export function getChatSidebarLayoutActionLabel({
    action,
    translate,
}: ChatSidebarLayoutActionLabelInput): string {
    const label = CHAT_SIDEBAR_LAYOUT_ACTION_LABELS[action];
    const translated = translate(label.key);

    return translated && translated !== label.key ? translated : label.fallback;
}
