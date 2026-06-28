// 扇出计划纯函数：prompt + 选兵 → N 个 Agent 描述（各自 worktree）。无副作用，便于测试。
export interface FanoutPick {
    providerId: string;
    chatProvider: 'claude' | 'codex';
    model: string;
}

export interface FanoutAgentPlan {
    agentId: string;
    worktreeName: string;
    pick: FanoutPick;
}

export interface FanoutPlan {
    groupId: string;
    prompt: string;
    agents: FanoutAgentPlan[];
}

const safe = (value: string): string => (
    value.replace(/[^A-Za-z0-9._-]/g, '-').slice(0, 24)
);

export function buildFanoutPlan(
    prompt: string,
    picks: FanoutPick[],
    makeId: () => string = () => crypto.randomUUID(),
): FanoutPlan {
    const groupId = makeId();
    const groupShort = groupId.slice(0, 8);
    const agents = picks.map((pick, index) => ({
        agentId: makeId(),
        worktreeName: `fanout-${safe(groupShort)}-${index}-${safe(pick.model)}`,
        pick,
    }));
    return {groupId, prompt, agents};
}
