// 事件 → tab 路由：优先按 agentId（多 agent 池下每 tab 稳定标识），
// 回退 requestId → tabKey 映射（兼容不带 agentId 的旧事件）。
//
// TabRef 形状与 ChatSessionTab 的 { key, agentId } 结构兼容，可直接传入。

export interface TabRef {
    key: string;
    agentId: string;
}

export interface IncomingEvent {
    agentId?: string;
    requestId?: string;
}

/**
 * 解析一条后端事件应归属哪个 tab。
 * - 优先 agentId：多 agent 池下每 tab 绑定独立 agentId，命中即归属该 tab。
 * - 回退 requestTabKeys：旧事件（无 agentId）按 requestId → tabKey 映射。
 * - 都命不中返回 undefined（调用方按需丢弃或回退活跃投影）。
 */
export function resolveTabForEvent(
    tabs: TabRef[],
    ev: IncomingEvent,
    requestTabKeys: Map<string, string>,
): string | undefined {
    if (ev.agentId) {
        const hit = tabs.find((tab) => tab.agentId === ev.agentId);
        if (hit) return hit.key;
    }
    if (ev.requestId) return requestTabKeys.get(ev.requestId);
    return undefined;
}
