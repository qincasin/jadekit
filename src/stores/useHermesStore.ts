import { create } from 'zustand';
import { listen } from '@tauri-apps/api/event';
import { TaskDto, DispatchDto, OrchestrationEvent } from '../types/hermes';
import { reduceHermesEvent, HermesState } from './hermesReducer';
import * as hermesService from '../services/hermesService';

export interface HermesStoreState extends HermesState {
  selectedAgentId: string | null;
  setSelectedAgentId: (id: string | null) => void;
  initRun: (runId: string, goal: string) => void;
  setTasks: (tasks: TaskDto[]) => void;
  setAgents: (agents: DispatchDto[]) => void;
  refreshSnapshot: () => Promise<void>;
  refreshSnapshotUntilHydrated: (opts?: { attempts?: number; delayMs?: number }) => Promise<void>;
  resetStore: () => void;
  subscribeEvents: () => Promise<() => void>;
}

let unlisteners: (() => void)[] = [];

export const useHermesStore = create<HermesStoreState>((set) => ({
  runs: {},
  tasks: {},
  agents: {},
  selectedAgentId: null,

  setSelectedAgentId: (id: string | null) => {
    set({ selectedAgentId: id });
  },

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
      const nextAgents: HermesState['agents'] = {};
      let firstAgentId: string | null = null;
      for (const a of agents) {
        const key = a.assignee || a.id;
        firstAgentId ??= key;
        const existing = nextAgents[key];
        nextAgents[key] = {
          ...existing,
          id: key,
          taskId: a.taskId,
          status: a.status === 'dispatched' ? 'working' : a.status,
          assignee: a.assignee,
          failureCount: a.failureCount,
          lastHeartbeatAt: a.lastHeartbeatAt,
          lastFailure: a.lastFailure,
          dispatchedAt: a.dispatchedAt,
          completedAt: a.completedAt,
          createdAt: a.createdAt,
        };
      }
      return {
        agents: nextAgents,
        selectedAgentId:
          state.selectedAgentId === null
            ? firstAgentId
            : nextAgents[state.selectedAgentId]
              ? state.selectedAgentId
              : null,
      };
    });
  },

  refreshSnapshot: async () => {
    const [tasksResult, agentsResult] = await Promise.allSettled([
      hermesService.taskList(),
      hermesService.agentList(),
    ]);

    if (tasksResult.status === 'fulfilled') {
      useHermesStore.getState().setTasks(tasksResult.value);
    } else {
      console.warn('Failed to refresh Hermes task snapshot:', tasksResult.reason);
    }

    if (agentsResult.status === 'fulfilled') {
      useHermesStore.getState().setAgents(agentsResult.value);
    } else {
      console.warn('Failed to refresh Hermes agent snapshot:', agentsResult.reason);
    }
  },

  refreshSnapshotUntilHydrated: async (opts) => {
    const attempts = Math.max(1, opts?.attempts ?? 6);
    const delayMs = Math.max(0, opts?.delayMs ?? 500);

    for (let attempt = 0; attempt < attempts; attempt += 1) {
      await useHermesStore.getState().refreshSnapshot();

      const state = useHermesStore.getState();
      if (Object.keys(state.tasks).length > 0 || Object.keys(state.agents).length > 0) {
        return;
      }

      if (attempt < attempts - 1 && delayMs > 0) {
        await new Promise((resolve) => globalThis.setTimeout(resolve, delayMs));
      }
    }
  },

  resetStore: () => {
    set({
      runs: {},
      tasks: {},
      agents: {},
      selectedAgentId: null,
    });
  },

  subscribeEvents: async () => {
    // Unsubscribe any prior listeners
    unlisteners.forEach((unlisten) => unlisten());
    unlisteners = [];

    try {
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
    } catch (err) {
      unlisteners.forEach((unlisten) => unlisten());
      unlisteners = [];
      console.warn('Failed to subscribe to Hermes events:', err);
      return () => {};
    }
  },
}));
