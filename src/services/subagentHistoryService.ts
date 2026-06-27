import {invoke} from '@tauri-apps/api/core';
import type {UnifiedSessionMessage} from '../types/session';

export interface LoadClaudeSubagentHistoryParams {
    sessionId: string;
    sourcePath?: string | null;
    agentId?: string | null;
    description?: string | null;
}

export async function loadClaudeSubagentHistory(
    params: LoadClaudeSubagentHistoryParams,
): Promise<UnifiedSessionMessage[]> {
    return invoke<UnifiedSessionMessage[]>('get_claude_subagent_session_messages', {
        sessionId: params.sessionId,
        sourcePath: params.sourcePath ?? undefined,
        agentId: params.agentId ?? undefined,
        description: params.description ?? undefined,
    });
}
