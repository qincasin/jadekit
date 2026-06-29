import { AgentState } from '../../stores/hermesReducer';

/**
 * Pure selector to extract the active agent from the agents dictionary.
 */
export function selectActiveAgent(
  agents: Record<string, AgentState>,
  selectedId: string | null
): AgentState | null {
  if (!selectedId) return null;
  return agents[selectedId] || null;
}

/**
 * Determines whether to display the activity timeline fallback.
 */
export function shouldFallbackToActivityTimeline(transcript: any[] | null): boolean {
  return transcript === null;
}
