import {describe, expect, it} from 'vitest';
import {resolveSendCwd} from './chatSendCwd';

describe('resolveSendCwd', () => {
    it('prefers worktreePath', () => {
        expect(resolveSendCwd({worktreePath: '/wt/a', cwd: '/proj'})).toBe('/wt/a');
    });

    it('falls back to tab cwd then arg', () => {
        expect(resolveSendCwd({cwd: '/proj'})).toBe('/proj');
        expect(resolveSendCwd({}, '/fallback')).toBe('/fallback');
    });
});
