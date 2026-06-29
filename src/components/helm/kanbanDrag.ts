import { TaskState } from '../../stores/hermesReducer';

export type Lane = 'pending' | 'running' | 'review' | 'done';
export type DropAction = 'cancel' | 'confirm-discard' | 'confirm-merge' | 'none';

/**
 * Maps Kanban board drag and drop actions based on source lane, destination lane, and task.
 */
export function dropActionFor(from: Lane, to: Lane, _task: TaskState): DropAction {
  if (from === to) {
    return 'none';
  }

  if (from === 'running') {
    if (to === 'pending') {
      return 'cancel';
    }
    if (to === 'done') {
      return 'confirm-discard';
    }
  }

  if (from === 'review') {
    if (to === 'done') {
      return 'confirm-merge';
    }
    if (to === 'pending') {
      return 'confirm-discard';
    }
  }

  return 'none';
}
