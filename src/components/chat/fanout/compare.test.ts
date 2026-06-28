import {describe, expect, it} from 'vitest';
import {canMergeFanoutTab} from './compare';

describe('canMergeFanoutTab', () => {
    it('requires a worktree branch and no active request', () => {
        expect(canMergeFanoutTab({
            worktreeBranch: 'helm/fanout-a',
            activeRequestId: null,
            status: 'idle',
        })).toBe(true);

        expect(canMergeFanoutTab({
            worktreeBranch: null,
            activeRequestId: null,
            status: 'idle',
        })).toBe(false);

        expect(canMergeFanoutTab({
            worktreeBranch: 'helm/fanout-a',
            activeRequestId: 'req-1',
            status: 'running',
        })).toBe(false);
    });
});
