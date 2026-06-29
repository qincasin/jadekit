import { create } from 'zustand';
import { listen } from '@tauri-apps/api/event';
import { TaskDto, DispatchDto, OrchestrationEvent } from '../types/hermes';
import { reduceHermesEvent, HermesState } from './hermesReducer';

export interface HermesStoreState extends HermesState {
  initRun: (runId: string, goal: string) => void;
  setTasks: (tasks: TaskDto[]) => void;
  setAgents: (agents: DispatchDto[]) => void;
  resetStore: () => void;
  subscribeEvents: () => Promise<() => void>;
}

let unlisteners: (() => void)[] = [];

export const useHermesStore = create<HermesStoreState>((set) => ({
  runs: {},
  tasks: {},
  agents: {},

  initRun: (runId: string, goal: string) => {
    set((state) => ({
      runs: {
        ...state.runs,
        [runId]: {
          id: runId,
          goal,
          status: 'running',
          error: null,
        },
      },
    }));
  },

  setTasks: (tasks: TaskDto[]) => {
    set((state) => {
      const nextTasks = { ...state.tasks };
      for (const t of tasks) {
        const existing = nextTasks[t.id];
        nextTasks[t.id] = {
          ...existing,
          id: t.id,
          status: t.status,
          parentId: t.parentId,
          spec: t.spec,
          deps: t.deps,
          result: t.result,
          createdAt: t.createdAt,
          completedAt: t.completedAt,
        };
      }
      return { tasks: nextTasks };
    });
  },

  setAgents: (agents: DispatchDto[]) => {
    set((state) => {
      const nextAgents = { ...state.agents };
      for (const a of agents) {
        const key = a.assignee || a.id;
        const existing = nextAgents[key];
        nextAgents[key] = {
          ...existing,
          id: key,
          taskId: a.taskId,
          status: a.status,
          assignee: a.assignee,
          failureCount: a.failureCount,
          lastHeartbeatAt: a.lastHeartbeatAt,
          lastFailure: a.lastFailure,
          dispatchedAt: a.dispatchedAt,
          completedAt: a.completedAt,
          createdAt: a.createdAt,
        };
      }
      return { agents: nextAgents };
    });
  },

  resetStore: () => {
    set({
      runs: {},
      tasks: {},
      agents: {},
    });
  },

  subscribeEvents: async () => {
    // Unsubscribe any prior listeners
    unlisteners.forEach((unlisten) => unlisten());
    unlisteners = [];

    const runUn = await listen<OrchestrationEvent & { kind: 'run' }>('hermes://run', (event) => {
      set((state) => reduceHermesEvent(state, event.payload));
    });

    const taskUn = await listen<OrchestrationEvent & { kind: 'task' }>('hermes://task', (event) => {
      set((state) => reduceHermesEvent(state, event.payload));
    });

    const agentUn = await listen<OrchestrationEvent & { kind: 'agent' }>('hermes://agent', (event) => {
      set((state) => reduceHermesEvent(state, event.payload));
    });

    unlisteners = [runUn, taskUn, agentUn];

    return () => {
      runUn();
      taskUn();
      agentUn();
    };
  },
}));
