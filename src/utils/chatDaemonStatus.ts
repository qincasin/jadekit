export type ChatDaemonStatusKind = 'ready' | 'starting' | 'offline' | 'error' | 'unknown';

export const CHAT_DAEMON_READY_TIMEOUT_ERROR_KEY = 'chat.daemon.readyTimeoutError';

interface ChatDaemonStatusInput {
    daemonReady: boolean;
    daemonStatus?: string | null;
    daemonReconnecting?: boolean;
}

interface ChatDaemonStatusTextInput extends ChatDaemonStatusInput {
    translate: (key: string) => string;
}

interface ChatDaemonDiagnosticInput extends ChatDaemonStatusInput {
    error?: string | null;
}

interface ChatDaemonDiagnosticDisplayInput {
    diagnosticText?: string | null;
    translate: (key: string) => string;
}

interface ChatDaemonReconnectLabelInput {
    daemonReconnecting?: boolean;
    translate: (key: string) => string;
}

const DAEMON_DIAGNOSTIC_MAX_LENGTH = 140;

function normalizeDiagnosticText(text?: string | null): string | null {
    const normalized = text?.replace(/\s+/g, ' ').trim() ?? '';
    if (!normalized) return null;
    if (normalized.length <= DAEMON_DIAGNOSTIC_MAX_LENGTH) return normalized;
    return `${normalized.slice(0, DAEMON_DIAGNOSTIC_MAX_LENGTH - 3)}...`;
}

function isGenericDaemonStatus(status: string): boolean {
    const normalized = status.trim().toLowerCase();
    return normalized === 'ready'
        || normalized === 'starting'
        || normalized === 'error'
        || normalized === 'failed'
        || normalized === 'shutdown'
        || normalized === 'offline';
}

export function getChatDaemonStatusKind({
    daemonReady,
    daemonStatus,
    daemonReconnecting = false,
}: ChatDaemonStatusInput): ChatDaemonStatusKind {
    if (daemonReady) return 'ready';
    if (daemonReconnecting) return 'starting';

    const status = daemonStatus?.trim().toLowerCase() ?? '';
    if (!status || status === 'ready') return 'starting';
    if (status === 'starting' || status.includes('starting')) return 'starting';
    if (
        status === 'shutdown'
        || status.includes('daemon exited')
        || status.includes('not running')
        || status.includes('offline')
    ) {
        return 'offline';
    }
    if (status.includes('error') || status.includes('failed')) return 'error';
    return 'unknown';
}

export function canReconnectChatDaemon(input: ChatDaemonStatusInput): boolean {
    const kind = getChatDaemonStatusKind(input);
    return kind === 'offline' || kind === 'error';
}

export function getChatDaemonStatusText(input: ChatDaemonStatusTextInput): string {
    const kind = getChatDaemonStatusKind(input);
    if (kind === 'unknown' && input.daemonStatus?.trim()) {
        return input.daemonStatus.trim();
    }

    const statusConfig: Record<Exclude<ChatDaemonStatusKind, 'unknown'>, {key: string; fallback: string}> = {
        ready: {key: 'chat.ready', fallback: 'Ready'},
        starting: {key: 'chat.starting', fallback: 'Starting'},
        offline: {key: 'chat.daemon.offline', fallback: 'Offline'},
        error: {key: 'chat.daemon.error', fallback: 'Daemon error'},
    };
    const config = statusConfig[kind === 'unknown' ? 'starting' : kind];
    const translated = input.translate(config.key);

    return translated && translated !== config.key ? translated : config.fallback;
}

export function getChatDaemonDiagnosticText(input: ChatDaemonDiagnosticInput): string | null {
    const kind = getChatDaemonStatusKind(input);
    if (kind === 'ready' || kind === 'starting') return null;

    const errorText = normalizeDiagnosticText(input.error);
    if (errorText) return errorText;

    const statusText = normalizeDiagnosticText(input.daemonStatus);
    if (!statusText || isGenericDaemonStatus(statusText)) return null;
    return statusText;
}

export function getChatDaemonDiagnosticDisplayText({
    diagnosticText,
    translate,
}: ChatDaemonDiagnosticDisplayInput): string | null {
    if (!diagnosticText) return null;
    if (diagnosticText !== CHAT_DAEMON_READY_TIMEOUT_ERROR_KEY) return diagnosticText;

    const fallback = 'Daemon did not become ready in time';
    const translated = translate(CHAT_DAEMON_READY_TIMEOUT_ERROR_KEY);

    return translated && translated !== CHAT_DAEMON_READY_TIMEOUT_ERROR_KEY ? translated : fallback;
}

export function getChatDaemonReconnectLabel({
    daemonReconnecting = false,
    translate,
}: ChatDaemonReconnectLabelInput): string {
    const key = daemonReconnecting ? 'chat.daemon.reconnecting' : 'chat.daemon.reconnect';
    const fallback = daemonReconnecting ? 'Reconnecting daemon' : 'Reconnect daemon';
    const translated = translate(key);

    return translated && translated !== key ? translated : fallback;
}

export function getChatDaemonReconnectShortLabel({
    daemonReconnecting = false,
    translate,
}: ChatDaemonReconnectLabelInput): string {
    const key = daemonReconnecting ? 'chat.daemon.reconnecting' : 'chat.daemon.reconnectShort';
    const fallback = daemonReconnecting ? 'Reconnecting' : 'Reconnect';
    const translated = translate(key);

    return translated && translated !== key ? translated : fallback;
}
