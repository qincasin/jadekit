import { vi, describe, it, expect, beforeEach } from 'vitest';
import * as hermesService from './hermesService';
import {
  isRunEvent,
  isTaskEvent,
  isAgentEvent,
  getEventChannel,
  OrchestrationEvent,
  AGENT_STATUS,
  AGENT_ACTIVITY,
} from '../types/hermes';

// Mock Tauri's invoke API
vi.mock('@tauri-apps/api/core', () => {
  return {
    invoke: vi.fn(),
  };
});

import { invoke } from '@tauri-apps/api/core';

describe('Hermes Service and Contract Types', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('Tauri Command Invocation', () => {
    it('run: invokes hermes_run with correct arguments', async () => {
      const mockInvoke = vi.mocked(invoke);
      mockInvoke.mockResolvedValueOnce('run_abc123');

      const runId = await hermesService.run('deploy app', { maxConcurrent: 3, pollIntervalMs: 1000 });

      expect(mockInvoke).toHaveBeenCalledWith('hermes_run', {
        goal: 'deploy app',
        opts: { maxConcurrent: 3, pollIntervalMs: 1000 },
      });
      expect(runId).toBe('run_abc123');
    });

    it('taskList: invokes hermes_task_list with filters', async () => {
      const mockInvoke = vi.mocked(invoke);
      const mockTasks = [
        {
          id: 'task_1',
          parentId: null,
          spec: 'do X',
          status: 'ready',
          deps: [],
          result: null,
          createdAt: '2026-06-29T00:00:00Z',
          completedAt: null,
        },
      ];
      mockInvoke.mockResolvedValueOnce(mockTasks);

      const tasks = await hermesService.taskList({ status: 'ready', ready: true });

      expect(mockInvoke).toHaveBeenCalledWith('hermes_task_list', {
        filter: { status: 'ready', ready: true },
      });
      expect(tasks).toEqual(mockTasks);
    });

    it('dispatchShow: invokes hermes_dispatch_show with dispatchId', async () => {
      const mockInvoke = vi.mocked(invoke);
      const mockDispatch = {
        id: 'disp_1',
        taskId: 'task_1',
        assignee: 'agent_1',
        status: 'dispatched',
        failureCount: 0,
        lastHeartbeatAt: '2026-06-29T00:01:00Z',
        lastFailure: null,
        dispatchedAt: '2026-06-29T00:00:00Z',
        completedAt: null,
        createdAt: '2026-06-29T00:00:00Z',
      };
      mockInvoke.mockResolvedValueOnce(mockDispatch);

      const dispatch = await hermesService.dispatchShow('disp_1');

      expect(mockInvoke).toHaveBeenCalledWith('hermes_dispatch_show', {
        dispatchId: 'disp_1',
      });
      expect(dispatch).toEqual(mockDispatch);
    });

    it('gateResolve: invokes hermes_gate_resolve with gateId and resolution', async () => {
      const mockInvoke = vi.mocked(invoke);
      mockInvoke.mockResolvedValueOnce(undefined);

      await hermesService.gateResolve('gate_1', 'approved');

      expect(mockInvoke).toHaveBeenCalledWith('hermes_gate_resolve', {
        gateId: 'gate_1',
        resolution: 'approved',
      });
    });

    it('gateShow: propagates backend errors instead of fabricating fallback data', async () => {
      const mockInvoke = vi.mocked(invoke);
      mockInvoke.mockRejectedValueOnce(new Error('missing gate'));

      await expect(hermesService.gateShow('gate_missing')).rejects.toThrow('missing gate');

      expect(mockInvoke).toHaveBeenCalledWith('hermes_gate_show', {
        gateId: 'gate_missing',
      });
    });

    it('runStop: invokes hermes_run_stop with runId', async () => {
      const mockInvoke = vi.mocked(invoke);
      mockInvoke.mockResolvedValueOnce(undefined);

      await hermesService.runStop('run_123');

      expect(mockInvoke).toHaveBeenCalledWith('hermes_run_stop', {
        runId: 'run_123',
      });
    });

    it('runCancel: invokes hermes_run_cancel with runId', async () => {
      const mockInvoke = vi.mocked(invoke);
      mockInvoke.mockResolvedValueOnce(undefined);

      await hermesService.runCancel('run_123');

      expect(mockInvoke).toHaveBeenCalledWith('hermes_run_cancel', {
        runId: 'run_123',
      });
    });

    it('runShow: invokes hermes_run_show with runId and returns RunShowDto', async () => {
      const mockInvoke = vi.mocked(invoke);
      const mockRunShow = {
        id: 'run_123',
        goal: 'deploy app',
        status: 'running',
        createdAt: '2026-06-29T00:00:00Z',
        completedAt: null,
        taskCount: 5,
        completedCount: 2,
      };
      mockInvoke.mockResolvedValueOnce(mockRunShow);

      const runShow = await hermesService.runShow('run_123');

      expect(mockInvoke).toHaveBeenCalledWith('hermes_run_show', {
        runId: 'run_123',
      });
      expect(runShow).toEqual(mockRunShow);
    });

    it('agentList: invokes hermes_agent_list and returns DispatchDto[]', async () => {
      const mockInvoke = vi.mocked(invoke);
      const mockActiveAgents = [
        {
          id: 'disp_1',
          taskId: 'task_1',
          assignee: 'agent_1',
          status: 'dispatched',
          failureCount: 0,
          lastHeartbeatAt: '2026-06-29T00:01:00Z',
          lastFailure: null,
          dispatchedAt: '2026-06-29T00:00:00Z',
          completedAt: null,
          createdAt: '2026-06-29T00:00:00Z',
        },
      ];
      mockInvoke.mockResolvedValueOnce(mockActiveAgents);

      const activeAgents = await hermesService.agentList();

      expect(mockInvoke).toHaveBeenCalledWith('hermes_agent_list');
      expect(activeAgents).toEqual(mockActiveAgents);
    });

    it('agentAbort: invokes hermes_agent_abort with agentId', async () => {
      const mockInvoke = vi.mocked(invoke);
      const mockDispatch = {
        id: 'disp_1',
        taskId: 'task_1',
        assignee: 'agent_1',
        status: 'failed',
        failureCount: 1,
        lastHeartbeatAt: '2026-06-29T00:01:00Z',
        lastFailure: 'aborted by user',
        dispatchedAt: '2026-06-29T00:00:00Z',
        completedAt: '2026-06-29T00:02:00Z',
        createdAt: '2026-06-29T00:00:00Z',
      };
      mockInvoke.mockResolvedValueOnce(mockDispatch);

      const dispatch = await hermesService.agentAbort('agent_1');

      expect(mockInvoke).toHaveBeenCalledWith('hermes_agent_abort', {
        agentId: 'agent_1',
      });
      expect(dispatch).toEqual(mockDispatch);
    });

    it('runCleanup: invokes hermes_run_cleanup and returns SweepReportDto', async () => {
      const mockInvoke = vi.mocked(invoke);
      const mockReport = {
        removed: 3,
        retained: 1,
      };
      mockInvoke.mockResolvedValueOnce(mockReport);

      const report = await hermesService.runCleanup('run_123');

      expect(mockInvoke).toHaveBeenCalledWith('hermes_run_cleanup', {
        runId: 'run_123',
      });
      expect(report).toEqual(mockReport);
    });

    it('workerTranscript: invokes hermes_worker_transcript with agentId', async () => {
      const mockInvoke = vi.mocked(invoke);
      const mockTranscript = [
        { role: 'assistant', content: '完成脚本化 worker 回放' },
      ];
      mockInvoke.mockResolvedValueOnce(mockTranscript);

      const transcript = await hermesService.workerTranscript('agent_1');

      expect(mockInvoke).toHaveBeenCalledWith('hermes_worker_transcript', {
        agentId: 'agent_1',
      });
      expect(transcript).toEqual(mockTranscript);
    });
  });

  describe('Event Guards and Helpers', () => {
    const runEvent: OrchestrationEvent = {
      kind: 'run',
      runId: 'run_1',
      goal: 'test goal',
      status: 'running',
      error: null,
    };

    const taskEvent: OrchestrationEvent = {
      kind: 'task',
      runId: 'run_1',
      taskId: 'task_1',
      status: 'completed',
      dispatchId: 'disp_1',
    };

    const agentEvent: OrchestrationEvent = {
      kind: 'agent',
      runId: 'run_1',
      agentId: 'agent_1',
      taskId: 'task_1',
      status: AGENT_STATUS.WORKING,
      activity: AGENT_ACTIVITY.THINKING,
    };

    it('identifies run events correctly using isRunEvent', () => {
      expect(isRunEvent(runEvent)).toBe(true);
      expect(isRunEvent(taskEvent)).toBe(false);
      expect(isRunEvent(agentEvent)).toBe(false);
    });

    it('identifies task events correctly using isTaskEvent', () => {
      expect(isTaskEvent(runEvent)).toBe(false);
      expect(isTaskEvent(taskEvent)).toBe(true);
      expect(isTaskEvent(agentEvent)).toBe(false);
    });

    it('identifies agent events correctly using isAgentEvent', () => {
      expect(isAgentEvent(runEvent)).toBe(false);
      expect(isAgentEvent(taskEvent)).toBe(false);
      expect(isAgentEvent(agentEvent)).toBe(true);
    });

    it('returns correct event channel for each event type using getEventChannel', () => {
      expect(getEventChannel(runEvent)).toBe('hermes://run');
      expect(getEventChannel(taskEvent)).toBe('hermes://task');
      expect(getEventChannel(agentEvent)).toBe('hermes://agent');
    });
  });
});
