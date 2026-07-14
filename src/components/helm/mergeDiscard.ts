import { TaskState } from '../../stores/hermesReducer';

/**
 * Validates whether a task can be merged.
 * Only tasks with status "awaiting-merge" can be merged.
 */
export function mergePreflight(task: TaskState): { canMerge: boolean; reason?: string } {
  if (task.status === 'awaiting-merge') {
    return { canMerge: true };
  }
  return {
    canMerge: false,
    reason: 'Task status must be awaiting-merge to merge',
  };
}
