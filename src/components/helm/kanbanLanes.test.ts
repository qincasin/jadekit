import { describe, it, expect } from 'vitest';
import { laneFor } from './kanbanLanes';
import { TaskState } from '../../stores/hermesReducer';

describe('laneFor', () => {
  it('should map "pending" status to "pending"', () => {
    const task: TaskState = { id: '1', status: 'pending' };
    expect(laneFor(task)).toBe('pending');
  });

  it('should map "ready" status to "pending"', () => {
    const task: TaskState = { id: '2', status: 'ready' };
    expect(laneFor(task)).toBe('pending');
  });

  it('should map "dispatched" status to "running"', () => {
    const task: TaskState = { id: '3', status: 'dispatched' };
    expect(laneFor(task)).toBe('running');
  });

  it('should map "awaiting-merge" status to "review"', () => {
    const task: TaskState = { id: '4', status: 'awaiting-merge' };
    expect(laneFor(task)).toBe('review');
  });

  it('should map "completed" status to "done"', () => {
    const task: TaskState = { id: '5', status: 'completed' };
    expect(laneFor(task)).toBe('done');
  });

  it('should map "failed" status to "done"', () => {
    const task: TaskState = { id: '6', status: 'failed' };
    expect(laneFor(task)).toBe('done');
  });

  it('should handle fallback for unknown status (e.g. "pending")', () => {
    const task: TaskState = { id: '7', status: 'unknown' };
    expect(laneFor(task)).toBe('pending');
  });
});
