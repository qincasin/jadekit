import {
  OrchestrationEvent,
  isRunEvent,
  isTaskEvent,
  isAgentEvent,
} from '../types/hermes';

export interface RunState {
  id: string;
  goal: string;
  status: string;
  error: string | null;
}

export interface TaskState {
  id: string;
  runId?: string;
  status: string;
  dispatchId?: string | null;
  parentId?: string | null;
  spec?: string;
  deps?: string[];
  result?: string | null;
  createdAt?: string;
  completedAt?: string | null;
}

export interface AgentState {
  id: string;
  runId?: string;
  taskId?: string | null;
  status: string;
  activity?: string | null;
  assignee?: string | null;
  failureCount?: number;
  lastHeartbeatAt?: string | null;
  lastFailure?: string | null;
  dispatchedAt?: string | null;
  completedAt?: string | null;
  createdAt?: string;
}

export interface HermesState {
  runs: Record<string, RunState>;
  tasks: Record<string, TaskState>;
  agents: Record<string, AgentState>;
}

const GENERIC_RUN_FAILURE_ERROR = 'run ended in failed state';

export function reduceHermesEvent(state: HermesState, event: OrchestrationEvent): HermesState {
  if (isRunEvent(event)) {
    const existing = state.runs[event.runId];
    if (existing) {
      // Check if existing run is in a terminal state: completed, failed, cancelled
      if (
        existing.status === 'completed' ||
        existing.status === 'failed' ||
        existing.status === 'cancelled'
      ) {
        const hasConcreteFailureUpgrade =
          existing.status === 'failed' &&
          event.status === 'failed' &&
          (existing.error === null || existing.error === GENERIC_RUN_FAILURE_ERROR) &&
          event.error !== null &&
          event.error !== GENERIC_RUN_FAILURE_ERROR;
        if (!hasConcreteFailureUpgrade) {
          // Idempotency / terminal regression prevention.
          return state;
        }
      }
      // If nothing has changed, return the exact same state reference
      if (
        existing.goal === event.goal &&
        existing.status === event.status &&
        existing.error === event.error
      ) {
        return state;
      }
      return {
        ...state,
        runs: {
          ...state.runs,
          [event.runId]: {
            ...existing,
            goal: event.goal,
            status: event.status,
            error: event.error,
          },
        },
      };
    } else {
      return {
        ...state,
        runs: {
          ...state.runs,
          [event.runId]: {
            id: event.runId,
            goal: event.goal,
            status: event.status,
            error: event.error,
          },
        },
      };
    }
  }

  if (isTaskEvent(event)) {
    const existing = state.tasks[event.taskId];
    if (existing) {
      // Check if task is in a terminal state: completed, failed
      if (existing.status === 'completed' || existing.status === 'failed') {
        return state;
      }
      if (
        existing.status === event.status &&
        existing.dispatchId === event.dispatchId
      ) {
        return state;
      }
      return {
        ...state,
        tasks: {
          ...state.tasks,
          [event.taskId]: {
            ...existing,
            status: event.status,
            dispatchId: event.dispatchId,
          },
        },
      };
    } else {
      return {
        ...state,
        tasks: {
          ...state.tasks,
          [event.taskId]: {
            id: event.taskId,
            runId: event.runId,
            status: event.status,
            dispatchId: event.dispatchId,
          },
        },
      };
    }
  }

  if (isAgentEvent(event)) {
    const existing = state.agents[event.agentId];
    if (existing) {
      // Check if agent is in terminal state: done, interrupted
      if (existing.status === 'done' || existing.status === 'interrupted') {
        return state;
      }
      if (
        existing.taskId === event.taskId &&
        existing.status === event.status &&
        existing.activity === event.activity
      ) {
        return state;
      }
      return {
        ...state,
        agents: {
          ...state.agents,
          [event.agentId]: {
            ...existing,
            taskId: event.taskId,
            status: event.status,
            activity: event.activity,
          },
        },
      };
    } else {
      return {
        ...state,
        agents: {
          ...state.agents,
          [event.agentId]: {
            id: event.agentId,
            runId: event.runId,
            taskId: event.taskId,
            status: event.status,
            activity: event.activity,
          },
        },
      };
    }
  }

  return state;
}
