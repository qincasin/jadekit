import {describe, expect, it} from 'vitest';
import {worktreesToRollback} from './fanoutRollback';

describe('worktreesToRollback', () => {
    it('returns paths of all successfully created worktrees', () => {
        expect(worktreesToRollback([{path: '/wt/a'}, {path: '/wt/b'}])).toEqual(['/wt/a', '/wt/b']);
    });
    it('empty when nothing created', () => {
        expect(worktreesToRollback([])).toEqual([]);
    });
});
