import { describe, expect, it } from 'vitest';
import {
  buildInspectorDiffReviewEdit,
  resolveJudgeRunId,
} from './inspectorData';

describe('inspector grounded data helpers', () => {
  it('resolveJudgeRunId uses the selected agent runId instead of the task id', () => {
    expect(
      resolveJudgeRunId({
        id: 'agent-1',
        runId: 'run-1',
        taskId: 'task-1',
        status: 'done',
      })
    ).toBe('run-1');
  });

  it('resolveJudgeRunId returns null when the agent has no runId', () => {
    expect(
      resolveJudgeRunId({
        id: 'agent-1',
        taskId: 'task-1',
        status: 'done',
      })
    ).toBeNull();
  });

  it('buildInspectorDiffReviewEdit does not fabricate a diff when no real diff summary exists', () => {
    expect(
      buildInspectorDiffReviewEdit({
        task: {
          id: 'task-1',
          status: 'completed',
        },
        diffSummary: null,
      })
    ).toBeUndefined();
  });

  it('buildInspectorDiffReviewEdit adapts real diff summary into ChatDiffReviewPane input', () => {
    const edit = buildInspectorDiffReviewEdit({
      task: {
        id: 'task-1',
        status: 'completed',
      },
      diffSummary: {
        filesChanged: 2,
        insertions: 12,
        deletions: 3,
      },
    });

    expect(edit).toMatchObject({
      toolId: 'helm-diff-task-1',
      displayPath: '2 files changed',
      additions: 12,
      deletions: 3,
      status: 'completed',
    });
  });
});
