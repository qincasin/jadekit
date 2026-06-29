import { describe, it, expect } from 'vitest';
import { selectActiveAgent, shouldFallbackToActivityTimeline } from './sessionSelect';
import { AgentState } from '../../stores/hermesReducer';

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

    it('should return false when transcript is empty array or populated array', () => {
      expect(shouldFallbackToActivityTimeline([])).toBe(false);
      expect(shouldFallbackToActivityTimeline([{ id: 'msg-1', role: 'user', content: 'hello' } as any])).toBe(false);
    });
  });
});
