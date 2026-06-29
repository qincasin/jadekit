import { describe, it, expect } from 'vitest';
import { AgentState } from '../../stores/hermesReducer';
import { filterAgents } from './jumpSearch';

const mockAgents: AgentState[] = [
  { id: 'agent-1', status: 'done', taskId: 'task-100' },
  { id: 'agent-2', status: 'needs-attention', taskId: 'task-200' },
  { id: 'agent-3', status: 'working', taskId: 'task-300' },
  { id: 'agent-4', status: 'needs-attention', taskId: 'task-400' },
  { id: 'special-agent', status: 'working', taskId: 'task-100' },
];

describe('filterAgents', () => {
  it('should return all agents sorted with needs-attention first when query is empty', () => {
    const result = filterAgents(mockAgents, '');
    expect(result.map(a => a.id)).toEqual(['agent-2', 'agent-4', 'agent-1', 'agent-3', 'special-agent']);
  });

  it('should filter agents by ID', () => {
    const result = filterAgents(mockAgents, 'special');
    expect(result.map(a => a.id)).toEqual(['special-agent']);
  });

  it('should filter agents by taskId', () => {
    const result = filterAgents(mockAgents, 'task-100');
    expect(result.map(a => a.id)).toContain('agent-1');
    expect(result.map(a => a.id)).toContain('special-agent');
    expect(result.length).toBe(2);
  });

  it('should filter agents by status and prioritize needs-attention', () => {
    const result = filterAgents(mockAgents, 'working');
    expect(result.map(a => a.id)).toEqual(['agent-3', 'special-agent']);

    const result2 = filterAgents(mockAgents, 'attention');
    expect(result2.map(a => a.id)).toEqual(['agent-2', 'agent-4']);
  });
});
