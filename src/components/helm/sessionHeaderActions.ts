import { AgentState } from '../../stores/hermesReducer';

export interface SessionHeaderAction {
  id: string;
  labelKey: string;
  disabled: boolean;
  tooltipKey?: string;
}

export function isAbortableActiveAgent(agent: AgentState | null | undefined): boolean {
  return !!agent?.taskId && (agent.status === 'working' || agent.status === 'needs-attention');
}

export function sessionHeaderActions(agent: AgentState | null | undefined): SessionHeaderAction[] {
  const hasTaskId = !!agent?.taskId;
  const canStopAgent = isAbortableActiveAgent(agent);

  return [
    {
      id: 'jumpToWorktree',
      labelKey: 'helm.actions.jumpToWorktree',
      disabled: !hasTaskId,
      tooltipKey: !hasTaskId ? 'helm.tooltips.jumpToWorktreeDisabled' : undefined,
    },
    {
      id: 'stop',
      labelKey: 'helm.actions.stop',
      disabled: !canStopAgent,
      tooltipKey: canStopAgent ? undefined : 'helm.tooltips.stopDisabled',
    },
    {
      id: 'cancel',
      labelKey: 'helm.actions.cancel',
      disabled: true,
      tooltipKey: 'helm.tooltips.cancelDisabled',
    },
  ];
}
