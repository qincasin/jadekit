import {invoke} from '@tauri-apps/api/core';

export type ChatMcpLiveStatus = 'online' | 'offline' | 'timeout' | 'error' | 'unknown';

export interface ChatMcpStatusResult {
    serverId: string;
    status: ChatMcpLiveStatus;
    message?: string | null;
    latencyMs?: number | null;
}

export interface ChatMcpConnectivityState {
    checking: boolean;
    checkedAt: number | null;
    error: string | null;
    hasResults: boolean;
    resultByServerId: Record<string, ChatMcpStatusResult>;
}

interface BuildChatMcpConnectivityStateInput {
    checking: boolean;
    checkedAt?: number | null;
    error?: string | null;
    results?: ChatMcpStatusResult[];
}

const MCP_CONNECTIVITY_ERROR_MAX_LENGTH = 140;

export const EMPTY_CHAT_MCP_CONNECTIVITY_STATE: ChatMcpConnectivityState = {
    checking: false,
    checkedAt: null,
    error: null,
    hasResults: false,
    resultByServerId: {},
};

export function normalizeChatMcpConnectivityError(error?: string | null): string | null {
    const normalized = error?.replace(/\s+/g, ' ').trim() ?? '';
    if (!normalized) return null;
    if (normalized.length <= MCP_CONNECTIVITY_ERROR_MAX_LENGTH) return normalized;
    return `${normalized.slice(0, MCP_CONNECTIVITY_ERROR_MAX_LENGTH - 3)}...`;
}

export function buildChatMcpConnectivityState({
    checking,
    checkedAt = null,
    error = null,
    results = [],
}: BuildChatMcpConnectivityStateInput): ChatMcpConnectivityState {
    const resultByServerId = results.reduce<Record<string, ChatMcpStatusResult>>((acc, result) => {
        acc[result.serverId] = {
            ...result,
            message: normalizeChatMcpConnectivityError(result.message),
        };
        return acc;
    }, {});

    return {
        checking,
        checkedAt,
        error: normalizeChatMcpConnectivityError(error),
        hasResults: results.length > 0,
        resultByServerId,
    };
}

export async function checkChatMcpConnectivity(serverIds: string[]): Promise<ChatMcpStatusResult[]> {
    if (serverIds.length === 0) return [];
    return invoke<ChatMcpStatusResult[]>('check_mcp_status', {serverIds});
}
