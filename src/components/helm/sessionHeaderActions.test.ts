import { describe, it, expect } from 'vitest';
import { isAbortableActiveAgent, sessionHeaderActions } from './sessionHeaderActions';
import { AgentState } from '../../stores/hermesReducer';

describe('sessionHeaderActions', () => {
  it('identifies only active agents with a dispatch task as abortable', () => {
    expect(isAbortableActiveAgent({
      id: 'working-agent',
      status: 'working',
      taskId: 'task-100',
    })).toBe(true);
    expect(isAbortableActiveAgent({
      id: 'attention-agent',
      status: 'needs-attention',
      taskId: 'task-101',
    })).toBe(true);
    expect(isAbortableActiveAgent({
      id: 'taskless-agent',
      status: 'working',
      taskId: null,
    })).toBe(false);
    expect(isAbortableActiveAgent({
      id: 'finished-agent',
      status: 'done',
      taskId: 'task-102',
    })).toBe(false);
    expect(isAbortableActiveAgent(null)).toBe(false);
  });

  it('should handle null/undefined agent', () => {
    const actions = sessionHeaderActions(null);
    expect(actions).toBeDefined();
    expect(actions.length).toBeGreaterThan(0);
    
    // Jump to worktree should be disabled
    const jumpAction = actions.find(a => a.id === 'jumpToWorktree');
    expect(jumpAction).toBeDefined();
    expect(jumpAction?.disabled).toBe(true);

    // Stop action should be disabled
    const stopAction = actions.find(a => a.id === 'stop');
    expect(stopAction).toBeDefined();
    expect(stopAction?.disabled).toBe(true);
    expect(stopAction?.tooltipKey).toBe('helm.tooltips.stopDisabled');
  });

  it('should disable jumpToWorktree if agent has no taskId', () => {
    const mockAgent: AgentState = {
      id: 'agent-1',
      status: 'working',
      taskId: null,
    };
    const actions = sessionHeaderActions(mockAgent);
    const jumpAction = actions.find(a => a.id === 'jumpToWorktree');
    expect(jumpAction?.disabled).toBe(true);
  });

  it('should enable jumpToWorktree if agent has taskId', () => {
    const mockAgent: AgentState = {
      id: 'agent-1',
      status: 'working',
      taskId: 'task-100',
    };
    const actions = sessionHeaderActions(mockAgent);
    const jumpAction = actions.find(a => a.id === 'jumpToWorktree');
    expect(jumpAction?.disabled).toBe(false);
  });

  it('should enable stop for active agents and keep run cancel disabled', () => {
    const mockAgent: AgentState = {
      id: 'agent-1',
      status: 'working',
      taskId: 'task-100',
    };
    const actions = sessionHeaderActions(mockAgent);
    
    const stopAction = actions.find(a => a.id === 'stop');
    expect(stopAction?.disabled).toBe(false);
    expect(stopAction?.tooltipKey).toBeUndefined();

    const cancelAction = actions.find(a => a.id === 'cancel');
    expect(cancelAction?.disabled).toBe(true);
    expect(cancelAction?.tooltipKey).toBe('helm.tooltips.cancelDisabled');
  });

  it('should disable stop for terminal agents', () => {
    const mockAgent: AgentState = {
      id: 'agent-1',
      status: 'done',
      taskId: 'task-100',
    };
    const actions = sessionHeaderActions(mockAgent);

    const stopAction = actions.find(a => a.id === 'stop');
    expect(stopAction?.disabled).toBe(true);
    expect(stopAction?.tooltipKey).toBe('helm.tooltips.stopDisabled');
  });

  it('should disable stop for a status-only agent without an active dispatch task', () => {
    const mockAgent: AgentState = {
      id: 'agent-1',
      status: 'working',
      taskId: null,
    };

    const actions = sessionHeaderActions(mockAgent);
    const stopAction = actions.find(a => a.id === 'stop');

    expect(stopAction?.disabled).toBe(true);
    expect(stopAction?.tooltipKey).toBe('helm.tooltips.stopDisabled');
  });
});
