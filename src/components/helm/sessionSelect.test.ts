import { describe, it, expect } from 'vitest';
import { selectActiveAgent, selectVisibleRun, shouldFallbackToActivityTimeline } from './sessionSelect';
import { AgentState, RunState } from '../../stores/hermesReducer';

describe('sessionSelect selectors and fallbacks', () => {
  describe('selectActiveAgent', () => {
    it('should return null when selectedAgentId is null', () => {
      const agents: Record<string, AgentState> = {
        'agent-1': { id: 'agent-1', status: 'working' }
      };
      expect(selectActiveAgent(agents, null)).toBeNull();
    });

    it('should return the agent when selectedAgentId is present and exists', () => {
      const agents: Record<string, AgentState> = {
        'agent-1': { id: 'agent-1', status: 'working' },
        'agent-2': { id: 'agent-2', status: 'done' }
      };
      expect(selectActiveAgent(agents, 'agent-2')).toEqual({ id: 'agent-2', status: 'done' });
    });

    it('should return null when selectedAgentId does not exist in agents map', () => {
      const agents: Record<string, AgentState> = {
        'agent-1': { id: 'agent-1', status: 'working' }
      };
      expect(selectActiveAgent(agents, 'agent-unknown')).toBeNull();
    });
  });

  describe('shouldFallbackToActivityTimeline', () => {
    it('should return true when transcript is null', () => {
      expect(shouldFallbackToActivityTimeline(null)).toBe(true);
    });

    it('should return true when transcript is empty array', () => {
      expect(shouldFallbackToActivityTimeline([])).toBe(true);
    });

    it('should return false when transcript has messages', () => {
      expect(shouldFallbackToActivityTimeline([{ id: 'msg-1', role: 'user', content: 'hello' } as any])).toBe(false);
    });
  });

  describe('selectVisibleRun', () => {
    it('should return null when there are no runs', () => {
      expect(selectVisibleRun({})).toBeNull();
    });

    it('should prefer a running run over terminal runs', () => {
      const runs: Record<string, RunState> = {
        'run-1': { id: 'run-1', goal: 'old', status: 'completed', error: null },
        'run-2': { id: 'run-2', goal: 'current', status: 'running', error: null },
      };

      expect(selectVisibleRun(runs)?.id).toBe('run-2');
    });

    it('should return the latest inserted run when none are running', () => {
      const runs: Record<string, RunState> = {
        'run-1': { id: 'run-1', goal: 'old', status: 'completed', error: null },
        'run-2': { id: 'run-2', goal: 'failed', status: 'failed', error: 'planner failed' },
      };

      expect(selectVisibleRun(runs)?.id).toBe('run-2');
    });
  });
});
