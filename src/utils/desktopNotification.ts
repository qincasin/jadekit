import {invoke, isTauri} from '@tauri-apps/api/core';

export type ChatTurnStopOutcome = 'success' | 'error' | 'aborted';

interface ChatTurnStopNotification {
    outcome: ChatTurnStopOutcome;
    provider: string;
    detail?: string | null;
}

const NOTIFICATION_DETAIL_MAX_LENGTH = 160;
const NOTIFICATION_TITLE = 'CCG Switch';

function providerLabel(provider: string): string {
    const normalized = provider.trim().toLowerCase();
    if (normalized === 'claude') return 'Claude';
    if (normalized === 'codex') return 'Codex';
    return provider.trim() || 'AI';
}

function normalizeNotificationDetail(detail?: string | null): string {
    const normalized = detail?.replace(/\s+/g, ' ').trim() ?? '';
    if (normalized.length <= NOTIFICATION_DETAIL_MAX_LENGTH) return normalized;
    return `${normalized.slice(0, NOTIFICATION_DETAIL_MAX_LENGTH - 1).trimEnd()}…`;
}

function isTauriRuntime(): boolean {
    try {
        return isTauri();
    } catch {
        return false;
    }
}

function showWebNotification(body: string): void {
    if (typeof globalThis.Notification === 'undefined') return;

    const NotificationApi = globalThis.Notification;
    const show = () => {
        try {
            new NotificationApi(NOTIFICATION_TITLE, {body});
        } catch {
            // System notifications are best-effort; never interrupt chat state updates.
        }
    };

    try {
        if (NotificationApi.permission === 'granted') {
            show();
            return;
        }

        if (NotificationApi.permission === 'default' && NotificationApi.requestPermission) {
            void NotificationApi.requestPermission()
                .then((permission) => {
                    if (permission === 'granted') show();
                })
                .catch(() => undefined);
        }
    } catch {
        // Ignore unsupported or denied notification environments.
    }
}

async function showNativeNotification(body: string): Promise<boolean> {
    if (!isTauriRuntime()) return false;

    try {
        await invoke('chat_show_system_notification', {
            title: NOTIFICATION_TITLE,
            body,
        });
        return true;
    } catch {
        return false;
    }
}

export function prepareChatTurnStoppedNotificationPermission(): void {
    if (isTauriRuntime()) return;
    if (typeof globalThis.Notification === 'undefined') return;

    const NotificationApi = globalThis.Notification;
    try {
        if (NotificationApi.permission === 'default' && NotificationApi.requestPermission) {
            void NotificationApi.requestPermission().catch(() => undefined);
        }
    } catch {
        // Permission prompts are best-effort and must never block chat actions.
    }
}

function notificationBody({outcome, provider, detail}: ChatTurnStopNotification): string {
    const label = providerLabel(provider);
    const detailText = normalizeNotificationDetail(detail);
    if (outcome === 'success') {
        return detailText ? `${label} 任务已完成：${detailText}` : `${label} 任务已完成。`;
    }
    if (outcome === 'aborted') {
        return detailText ? `${label} 输出已停止：${detailText}` : `${label} 输出已停止。`;
    }

    return detailText ? `${label} 任务已失败：${detailText}` : `${label} 任务已失败。`;
}

export function notifyChatTurnStopped(notification: ChatTurnStopNotification): void {
    const body = notificationBody(notification);
    if (isTauriRuntime()) {
        void showNativeNotification(body).then((shown) => {
            if (!shown) showWebNotification(body);
        });
        return;
    }

    showWebNotification(body);
}
