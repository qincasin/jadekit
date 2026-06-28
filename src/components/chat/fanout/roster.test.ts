import {describe, expect, it} from 'vitest';
import {rosterPicksFromProviders} from './roster';

describe('rosterPicksFromProviders', () => {
    it('keeps only Claude and Codex providers with model choices', () => {
        const picks = rosterPicksFromProviders([
            {
                id: 'claude-1',
                name: 'Claude Team',
                appType: 'claude',
                isActive: true,
                defaultOpusModel: 'claude-custom-opus',
            },
            {
                id: 'codex-1',
                name: 'Codex Team',
                appType: 'codex',
                isActive: true,
                defaultReasoningModel: 'gpt-custom',
            },
            {
                id: 'gemini-1',
                name: 'Gemini Team',
                appType: 'gemini',
                isActive: true,
            },
        ]);

        expect(picks.map((pick) => pick.chatProvider)).toEqual(['claude', 'codex']);
        expect(picks[0].models[0].id).toBe('claude-custom-opus');
        expect(picks[1].models[0].id).toBe('gpt-custom');
    });
});
