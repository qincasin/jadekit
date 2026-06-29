import { AgentState } from '../../stores/hermesReducer';

/**
 * Filter agents by ID, taskId, or status, and prioritize those with status "needs-attention".
 */
export function filterAgents(agents: AgentState[], query: string): AgentState[] {
  const cleanQuery = query.trim().toLowerCase();

  const filtered = cleanQuery === ''
    ? agents
    : agents.filter((agent) => {
        const idMatches = agent.id.toLowerCase().includes(cleanQuery);
        const taskIdMatches = agent.taskId
          ? agent.taskId.toLowerCase().includes(cleanQuery)
          : false;
        const statusMatches = agent.status.toLowerCase().includes(cleanQuery);

        return idMatches || taskIdMatches || statusMatches;
      });

  return [...filtered].sort((a, b) => {
    const aNeedsAttention = a.status === 'needs-attention';
    const bNeedsAttention = b.status === 'needs-attention';

    if (aNeedsAttention && !bNeedsAttention) {
      return -1;
    }
    if (!aNeedsAttention && bNeedsAttention) {
      return 1;
    }
    return 0;
  });
}
