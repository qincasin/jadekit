export interface HermesLaunchRequestState {
  goal: string;
  rosterCount: number;
  launching: boolean;
  workspaceLoading: boolean;
}

export function canRequestHermesLaunch({
  goal,
  rosterCount,
  launching,
  workspaceLoading,
}: HermesLaunchRequestState): boolean {
  return Boolean(goal.trim()) && rosterCount > 0 && !launching && !workspaceLoading;
}
