import { describe, it, expect } from 'vitest';
import { dropActionFor } from './kanbanDrag';
import { TaskState } from '../../stores/hermesReducer';

describe('dropActionFor', () => {
  it('Running to pending should trigger cancel', () => {
    const task: TaskState = { id: '1', status: 'dispatched' };
    expect(dropActionFor('running', 'pending', task)).toBe('cancel');
  });

  it('Running to done should trigger confirm-discard', () => {
    const task: TaskState = { id: '2', status: 'dispatched' };
    expect(dropActionFor('running', 'done', task)).toBe('confirm-discard');
  });

  it('Review (awaiting-merge) to done should trigger confirm-merge', () => {
    const task: TaskState = { id: '3', status: 'awaiting-merge' };
    expect(dropActionFor('review', 'done', task)).toBe('confirm-merge');
  });

  it('Review to pending should trigger confirm-discard', () => {
    const task: TaskState = { id: '4', status: 'awaiting-merge' };
    expect(dropActionFor('review', 'pending', task)).toBe('confirm-discard');
  });

  it('Other drops should return none', () => {
    const task: TaskState = { id: '5', status: 'pending' };
    expect(dropActionFor('pending', 'running', task)).toBe('none');
  });
});
