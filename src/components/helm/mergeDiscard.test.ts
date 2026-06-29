import { describe, it, expect } from 'vitest';
import { mergePreflight } from './mergeDiscard';
import { TaskState } from '../../stores/hermesReducer';

describe('mergePreflight', () => {
  it('should allow merging when status is awaiting-merge', () => {
    const task: TaskState = { id: 'task-1', status: 'awaiting-merge' };
    const result = mergePreflight(task);
    expect(result.canMerge).toBe(true);
    expect(result.reason).toBeUndefined();
  });

  it('should not allow merging when status is not awaiting-merge', () => {
    const task: TaskState = { id: 'task-2', status: 'pending' };
    const result = mergePreflight(task);
    expect(result.canMerge).toBe(false);
    expect(result.reason).toBe('Task status must be awaiting-merge to merge');
  });
});
