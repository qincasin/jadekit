import {describe, expect, it} from 'vitest';
import {worktreeBadgeLabel} from './worktreeBadge';

describe('worktreeBadgeLabel', () => {
    it('shows branch and change count', () => {
        expect(worktreeBadgeLabel({branch: 'helm/task-a', diff: {filesChanged: 3}}))
            .toBe('helm/task-a · 3 changed');
    });

    it('omits count when no diff', () => {
        expect(worktreeBadgeLabel({branch: 'helm/task-a'})).toBe('helm/task-a');
    });
});
