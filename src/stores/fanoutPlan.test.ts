import {describe, expect, it} from 'vitest';
import {buildFanoutPlan} from './fanoutPlan';

const picks = [
    {providerId: 'p1', chatProvider: 'claude' as const, model: 'claude-opus-4-8'},
    {providerId: 'p2', chatProvider: 'codex' as const, model: 'gpt-5-codex'},
];

describe('buildFanoutPlan', () => {
    it('creates one agent per pick with unique agentId and worktree name', () => {
        let n = 0;
        const plan = buildFanoutPlan('do X', picks, () => `id-${n++}`);
        expect(plan.agents).toHaveLength(2);
        expect(plan.prompt).toBe('do X');
        const ids = plan.agents.map((a) => a.agentId);
        expect(new Set(ids).size).toBe(2);
        const wts = plan.agents.map((a) => a.worktreeName);
        expect(new Set(wts).size).toBe(2);
        expect(wts.every((w) => /^[A-Za-z0-9._-]+$/.test(w))).toBe(true);
    });

    it('preserves pick provider/model on each agent', () => {
        const plan = buildFanoutPlan('x', picks, () => 'fixed');
        expect(plan.agents[0].pick.chatProvider).toBe('claude');
        expect(plan.agents[1].pick.model).toBe('gpt-5-codex');
    });
});
