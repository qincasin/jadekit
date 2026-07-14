import { AgentState, RunState } from '../../stores/hermesReducer';

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
  return transcript === null || transcript.length === 0;
}

/**
 * Picks the run that should be visible while no worker session is selected yet.
 */
export function selectVisibleRun(runs: Record<string, RunState>): RunState | null {
  const values = Object.values(runs);
  if (values.length === 0) return null;

  const running = values.filter((run) => run.status === 'running');
  if (running.length > 0) {
    return running[running.length - 1];
  }

  return values[values.length - 1];
}
