import { TaskState } from '../../stores/hermesReducer';

export function laneFor(task: TaskState): 'pending' | 'running' | 'review' | 'done' {
  switch (task.status) {
    case 'pending':
    case 'ready':
      return 'pending';
    case 'dispatched':
      return 'running';
    case 'awaiting-merge':
      return 'review';
    case 'completed':
    case 'failed':
      return 'done';
    default:
      return 'pending';
  }
}
