import { vi, describe, it, expect, beforeEach } from 'vitest';

// Mock Tauri's event API before importing the store
vi.mock('@tauri-apps/api/event', () => {
  const listenFn = vi.fn().mockImplementation((_channel, _callback) => {
    return Promise.resolve(() => {});
  });
  return {
    listen: listenFn,
  };
});

vi.mock('@tauri-apps/api/core', () => {
  return {
    invoke: vi.fn(),
  };
});

import { useHermesStore } from './useHermesStore';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { TaskDto, DispatchDto } from '../types/hermes';

describe('useHermesStore', () => {
  beforeEach(() => {
    useHermesStore.getState().resetStore();
    vi.clearAllMocks();
  });

  it('initRun: should initialize a run in the state', () => {
    const store = useHermesStore.getState();
    store.initRun('run-123', 'My goal description');

    const updatedState = useHermesStore.getState();
    expect(updatedState.runs['run-123']).toEqual({
      id: 'run-123',
      goal: 'My goal description',
      status: 'running',
      error: null,
    });
  });

  it('setTasks: should merge tasks into state', () => {
    const store = useHermesStore.getState();
    const task: TaskDto = {
      id: 'task-1',
      parentId: null,
      spec: 'Install dependencies',
      status: 'pending',
      deps: [],
      result: null,
      createdAt: '2026-06-29T00:00:00Z',
      completedAt: null,
    };

    store.setTasks([task]);

    let state = useHermesStore.getState();
    expect(state.tasks['task-1']).toEqual({
      id: 'task-1',
      parentId: null,
      spec: 'Install dependencies',
      status: 'pending',
      deps: [],
      result: null,
      createdAt: '2026-06-29T00:00:00Z',
      completedAt: null,
    });

    // Test merging: set task again with status change, it should update while maintaining other properties
    const updatedTask: TaskDto = {
      ...task,
      status: 'completed',
      completedAt: '2026-06-29T01:00:00Z',
    };
    useHermesStore.getState().setTasks([updatedTask]);

    state = useHermesStore.getState();
    expect(state.tasks['task-1'].status).toBe('completed');
    expect(state.tasks['task-1'].completedAt).toBe('2026-06-29T01:00:00Z');
  });

  it('setAgents: should merge agents into state', () => {
    const store = useHermesStore.getState();
    const agent: DispatchDto = {
      id: 'dispatch-1',
      taskId: 'task-1',
      assignee: 'agent-alice',
      status: 'working',
      failureCount: 0,
      lastHeartbeatAt: '2026-06-29T00:05:00Z',
      lastFailure: null,
      dispatchedAt: '2026-06-29T00:00:00Z',
      completedAt: null,
      createdAt: '2026-06-29T00:00:00Z',
    };

    store.setAgents([agent]);

    let state = useHermesStore.getState();
    expect(state.agents['agent-alice']).toEqual({
      id: 'agent-alice',
      taskId: 'task-1',
      assignee: 'agent-alice',
      status: 'working',
      failureCount: 0,
      lastHeartbeatAt: '2026-06-29T00:05:00Z',
      lastFailure: null,
      dispatchedAt: '2026-06-29T00:00:00Z',
      completedAt: null,
      createdAt: '2026-06-29T00:00:00Z',
    });

    // Test fallback when assignee is null (should use ID)
    const agentNoAssignee: DispatchDto = {
      ...agent,
      id: 'dispatch-anonymous',
      assignee: null,
    };
    useHermesStore.getState().setAgents([agentNoAssignee]);

    state = useHermesStore.getState();
    expect(state.agents['dispatch-anonymous'].id).toBe('dispatch-anonymous');
  });

  it('resetStore: should clear all runs, tasks, and agents', () => {
    const store = useHermesStore.getState();
    store.initRun('run-123', 'My goal');
    store.resetStore();

    const state = useHermesStore.getState();
    expect(state.runs).toEqual({});
    expect(state.tasks).toEqual({});
    expect(state.agents).toEqual({});
  });

  it('subscribeEvents: should register Tauri event listeners and return unlisten callbacks', async () => {
    const mockUnlistenRun = vi.fn();
    const mockUnlistenTask = vi.fn();
    const mockUnlistenAgent = vi.fn();

    const mockListen = vi.mocked(listen);
    mockListen.mockImplementation((channel) => {
      if (channel === 'hermes://run') return Promise.resolve(mockUnlistenRun);
      if (channel === 'hermes://task') return Promise.resolve(mockUnlistenTask);
      if (channel === 'hermes://agent') return Promise.resolve(mockUnlistenAgent);
      return Promise.resolve(() => {});
    });

    const store = useHermesStore.getState();
    const unsubscribe = await store.subscribeEvents();

    expect(mockListen).toHaveBeenCalledWith('hermes://run', expect.any(Function));
    expect(mockListen).toHaveBeenCalledWith('hermes://task', expect.any(Function));
    expect(mockListen).toHaveBeenCalledWith('hermes://agent', expect.any(Function));

    unsubscribe();

    expect(mockUnlistenRun).toHaveBeenCalled();
    expect(mockUnlistenTask).toHaveBeenCalled();
    expect(mockUnlistenAgent).toHaveBeenCalled();
  });

  it('subscribeEvents: should return a noop cleanup when Tauri event registration is unavailable', async () => {
    const mockListen = vi.mocked(listen);
    mockListen.mockRejectedValueOnce(new Error('event API unavailable'));

    const unsubscribe = await useHermesStore.getState().subscribeEvents();

    expect(unsubscribe).not.toThrow();
  });

  it('refreshSnapshot: should hydrate tasks and active dispatches from Hermes read commands', async () => {
    const task: TaskDto = {
      id: 'task-1',
      parentId: null,
      spec: 'Implement Helm cockpit',
      status: 'ready',
      deps: [],
      result: null,
      createdAt: '2026-06-29T00:00:00Z',
      completedAt: null,
    };
    const agent: DispatchDto = {
      id: 'dispatch-1',
      taskId: 'task-1',
      assignee: 'codex-01',
      status: 'dispatched',
      failureCount: 0,
      lastHeartbeatAt: '2026-06-29T00:05:00Z',
      lastFailure: null,
      dispatchedAt: '2026-06-29T00:00:00Z',
      completedAt: null,
      createdAt: '2026-06-29T00:00:00Z',
    };

    const mockInvoke = vi.mocked(invoke);
    mockInvoke.mockImplementation((command) => {
      if (command === 'hermes_task_list') return Promise.resolve([task]);
      if (command === 'hermes_agent_list') return Promise.resolve([agent]);
      return Promise.reject(new Error(`unexpected command: ${String(command)}`));
    });

    await useHermesStore.getState().refreshSnapshot();

    const state = useHermesStore.getState();
    expect(mockInvoke).toHaveBeenCalledWith('hermes_task_list', { filter: undefined });
    expect(mockInvoke).toHaveBeenCalledWith('hermes_agent_list');
    expect(state.tasks['task-1']?.spec).toBe('Implement Helm cockpit');
    expect(state.agents['codex-01']?.taskId).toBe('task-1');
    expect(state.agents['codex-01']?.status).toBe('working');
    expect(state.selectedAgentId).toBe('codex-01');
  });

  it('refreshSnapshotUntilHydrated: should retry until Hermes dispatch data appears', async () => {
    vi.useFakeTimers();

    const task: TaskDto = {
      id: 'task-2',
      parentId: null,
      spec: 'Build cockpit data recovery',
      status: 'dispatched',
      deps: [],
      result: null,
      createdAt: '2026-06-29T00:00:00Z',
      completedAt: null,
    };
    const agent: DispatchDto = {
      id: 'dispatch-2',
      taskId: 'task-2',
      assignee: 'codex-02',
      status: 'dispatched',
      failureCount: 0,
      lastHeartbeatAt: '2026-06-29T00:05:00Z',
      lastFailure: null,
      dispatchedAt: '2026-06-29T00:00:00Z',
      completedAt: null,
      createdAt: '2026-06-29T00:00:00Z',
    };

    let taskListCalls = 0;
    const mockInvoke = vi.mocked(invoke);
    mockInvoke.mockImplementation((command) => {
      if (command === 'hermes_task_list') {
        taskListCalls += 1;
        return Promise.resolve(taskListCalls === 1 ? [] : [task]);
      }
      if (command === 'hermes_agent_list') {
        return Promise.resolve(taskListCalls === 1 ? [] : [agent]);
      }
      return Promise.reject(new Error(`unexpected command: ${String(command)}`));
    });

    const refresh = useHermesStore
      .getState()
      .refreshSnapshotUntilHydrated({ attempts: 2, delayMs: 10 });
    await vi.advanceTimersByTimeAsync(10);
    await refresh;

    const state = useHermesStore.getState();
    expect(taskListCalls).toBe(2);
    expect(state.tasks['task-2']?.spec).toBe('Build cockpit data recovery');
    expect(state.agents['codex-02']?.status).toBe('working');

    vi.useRealTimers();
  });

  it('refreshSnapshot: should keep the cockpit open when Hermes read commands are unavailable', async () => {
    const mockInvoke = vi.mocked(invoke);
    mockInvoke.mockRejectedValue(new Error('command not available'));

    await expect(useHermesStore.getState().refreshSnapshot()).resolves.toBeUndefined();

    const state = useHermesStore.getState();
    expect(state.tasks).toEqual({});
    expect(state.agents).toEqual({});
  });

  it('refreshSnapshot: should remove stale agents and clear their selection when the active dispatch snapshot is empty', async () => {
    const staleAgent: DispatchDto = {
      id: 'dispatch-stale',
      taskId: 'task-stale',
      assignee: 'codex-stale',
      status: 'dispatched',
      failureCount: 0,
      lastHeartbeatAt: '2026-06-29T00:05:00Z',
      lastFailure: null,
      dispatchedAt: '2026-06-29T00:00:00Z',
      completedAt: null,
      createdAt: '2026-06-29T00:00:00Z',
    };
    const mockInvoke = vi.mocked(invoke);
    mockInvoke.mockImplementation((command) => {
      if (command === 'hermes_task_list' || command === 'hermes_agent_list') {
        return Promise.resolve([]);
      }
      return Promise.reject(new Error(`unexpected command: ${String(command)}`));
    });

    useHermesStore.getState().setAgents([staleAgent]);
    useHermesStore.getState().setSelectedAgentId('codex-stale');

    await useHermesStore.getState().refreshSnapshot();

    const state = useHermesStore.getState();
    expect(state.agents).toEqual({});
    expect(state.selectedAgentId).toBeNull();
  });
});
