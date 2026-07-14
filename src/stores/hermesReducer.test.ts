import { describe, it, expect } from 'vitest';
import { reduceHermesEvent, HermesState } from './hermesReducer';
import { OrchestrationEvent } from '../types/hermes';

describe('reduceHermesEvent', () => {
  const getInitialState = (): HermesState => ({
    runs: {},
    tasks: {},
    agents: {},
  });

  describe('run events', () => {
    it('should add a new run', () => {
      const state = getInitialState();
      const event: OrchestrationEvent = {
        kind: 'run',
        runId: 'run-1',
        goal: 'Deploy system',
        status: 'running',
        error: null,
      };

      const nextState = reduceHermesEvent(state, event);
      expect(nextState.runs['run-1']).toEqual({
        id: 'run-1',
        goal: 'Deploy system',
        status: 'running',
        error: null,
      });
    });

    it('should update an existing run status', () => {
      const state: HermesState = {
        runs: {
          'run-1': { id: 'run-1', goal: 'Deploy system', status: 'running', error: null },
        },
        tasks: {},
        agents: {},
      };
      const event: OrchestrationEvent = {
        kind: 'run',
        runId: 'run-1',
        goal: 'Deploy system',
        status: 'completed',
        error: null,
      };

      const nextState = reduceHermesEvent(state, event);
      expect(nextState.runs['run-1'].status).toBe('completed');
    });

    it('should handle idempotency (duplicate run events)', () => {
      const state: HermesState = {
        runs: {
          'run-1': { id: 'run-1', goal: 'Deploy system', status: 'running', error: null },
        },
        tasks: {},
        agents: {},
      };
      const event: OrchestrationEvent = {
        kind: 'run',
        runId: 'run-1',
        goal: 'Deploy system',
        status: 'running',
        error: null,
      };

      const nextState = reduceHermesEvent(state, event);
      expect(nextState).toBe(state); // If state is unchanged, return identical reference
    });

    it('should prevent terminal status regression (completed, failed, cancelled)', () => {
      const statuses = ['completed', 'failed', 'cancelled'];
      for (const terminalStatus of statuses) {
        const state: HermesState = {
          runs: {
            'run-1': { id: 'run-1', goal: 'Deploy system', status: terminalStatus, error: 'some error' },
          },
          tasks: {},
          agents: {},
        };
        const event: OrchestrationEvent = {
          kind: 'run',
          runId: 'run-1',
          goal: 'Deploy system',
          status: 'running',
          error: null,
        };

        const nextState = reduceHermesEvent(state, event);
        expect(nextState.runs['run-1'].status).toBe(terminalStatus);
        // Error should also be preserved
        expect(nextState.runs['run-1'].error).toBe('some error');
      }
    });
  });

  describe('task events', () => {
    it('should add a new task', () => {
      const state = getInitialState();
      const event: OrchestrationEvent = {
        kind: 'task',
        runId: 'run-1',
        taskId: 'task-1',
        status: 'pending',
        dispatchId: null,
      };

      const nextState = reduceHermesEvent(state, event);
      expect(nextState.tasks['task-1']).toEqual({
        id: 'task-1',
        runId: 'run-1',
        status: 'pending',
        dispatchId: null,
      });
    });

    it('should update an existing task status and dispatchId', () => {
      const state: HermesState = {
        runs: {},
        tasks: {
          'task-1': { id: 'task-1', runId: 'run-1', status: 'pending', dispatchId: null },
        },
        agents: {},
      };
      const event: OrchestrationEvent = {
        kind: 'task',
        runId: 'run-1',
        taskId: 'task-1',
        status: 'completed',
        dispatchId: 'dispatch-1',
      };

      const nextState = reduceHermesEvent(state, event);
      expect(nextState.tasks['task-1'].status).toBe('completed');
      expect(nextState.tasks['task-1'].dispatchId).toBe('dispatch-1');
    });

    it('should handle task idempotency (duplicate task events)', () => {
      const state: HermesState = {
        runs: {},
        tasks: {
          'task-1': { id: 'task-1', runId: 'run-1', status: 'completed', dispatchId: 'dispatch-1' },
        },
        agents: {},
      };
      const event: OrchestrationEvent = {
        kind: 'task',
        runId: 'run-1',
        taskId: 'task-1',
        status: 'completed',
        dispatchId: 'dispatch-1',
      };

      const nextState = reduceHermesEvent(state, event);
      expect(nextState).toBe(state);
    });

    it('should prevent task terminal status regression (completed, failed)', () => {
      const terminalStatuses = ['completed', 'failed'];
      for (const terminalStatus of terminalStatuses) {
        const state: HermesState = {
          runs: {},
          tasks: {
            'task-1': { id: 'task-1', runId: 'run-1', status: terminalStatus, dispatchId: 'dispatch-1' },
          },
          agents: {},
        };
        const event: OrchestrationEvent = {
          kind: 'task',
          runId: 'run-1',
          taskId: 'task-1',
          status: 'dispatched',
          dispatchId: 'dispatch-2',
        };

        const nextState = reduceHermesEvent(state, event);
        expect(nextState.tasks['task-1'].status).toBe(terminalStatus);
        expect(nextState.tasks['task-1'].dispatchId).toBe('dispatch-1');
      }
    });
  });

  describe('agent events', () => {
    it('should add a new agent', () => {
      const state = getInitialState();
      const event: OrchestrationEvent = {
        kind: 'agent',
        runId: 'run-1',
        agentId: 'agent-1',
        taskId: 'task-1',
        status: 'working',
        activity: 'thinking',
      };

      const nextState = reduceHermesEvent(state, event);
      expect(nextState.agents['agent-1']).toEqual({
        id: 'agent-1',
        runId: 'run-1',
        taskId: 'task-1',
        status: 'working',
        activity: 'thinking',
      });
    });

    it('should update an existing agent', () => {
      const state: HermesState = {
        runs: {},
        tasks: {},
        agents: {
          'agent-1': { id: 'agent-1', runId: 'run-1', taskId: 'task-1', status: 'working', activity: 'thinking' },
        },
      };
      const event: OrchestrationEvent = {
        kind: 'agent',
        runId: 'run-1',
        agentId: 'agent-1',
        taskId: null,
        status: 'done',
        activity: null,
      };

      const nextState = reduceHermesEvent(state, event);
      expect(nextState.agents['agent-1']).toEqual({
        id: 'agent-1',
        runId: 'run-1',
        taskId: null,
        status: 'done',
        activity: null,
      });
    });

    it('should handle agent idempotency', () => {
      const state: HermesState = {
        runs: {},
        tasks: {},
        agents: {
          'agent-1': { id: 'agent-1', runId: 'run-1', taskId: 'task-1', status: 'working', activity: 'thinking' },
        },
      };
      const event: OrchestrationEvent = {
        kind: 'agent',
        runId: 'run-1',
        agentId: 'agent-1',
        taskId: 'task-1',
        status: 'working',
        activity: 'thinking',
      };

      const nextState = reduceHermesEvent(state, event);
      expect(nextState).toBe(state);
    });

    it('should prevent agent terminal status regression (done, interrupted)', () => {
      const terminalStatuses = ['done', 'interrupted'];
      for (const terminalStatus of terminalStatuses) {
        const state: HermesState = {
          runs: {},
          tasks: {},
          agents: {
            'agent-1': { id: 'agent-1', runId: 'run-1', taskId: null, status: terminalStatus, activity: null },
          },
        };
        const event: OrchestrationEvent = {
          kind: 'agent',
          runId: 'run-1',
          agentId: 'agent-1',
          taskId: 'task-2',
          status: 'working',
          activity: 'thinking',
        };

        const nextState = reduceHermesEvent(state, event);
        expect(nextState.agents['agent-1'].status).toBe(terminalStatus);
        expect(nextState.agents['agent-1'].taskId).toBeNull();
        expect(nextState.agents['agent-1'].activity).toBeNull();
      }
    });
  });
});
