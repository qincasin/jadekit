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

import { useHermesStore } from './useHermesStore';
import { listen } from '@tauri-apps/api/event';
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
});
