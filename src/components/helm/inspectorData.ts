import type { AgentState, TaskState } from '../../stores/hermesReducer';
import type { HelmDiffSummary } from '../../services/worktreeService';
import type { ChatStatusEditSummary } from '../../utils/chatStatusSummary';

export function resolveJudgeRunId(agent: AgentState | null | undefined): string | null {
  return agent?.runId?.trim() || null;
}

export function buildInspectorDiffReviewEdit({
  task,
  diffSummary,
}: {
  task: TaskState | null | undefined;
  diffSummary: HelmDiffSummary | null | undefined;
}): ChatStatusEditSummary | undefined {
  if (!task || !diffSummary) {
    return undefined;
  }

  return {
    toolId: `helm-diff-${task.id}`,
    displayPath: `${diffSummary.filesChanged} files changed`,
    openPath: '',
    additions: diffSummary.insertions,
    deletions: diffSummary.deletions,
    status: 'completed',
    diffPreviewLines: [],
  };
}
