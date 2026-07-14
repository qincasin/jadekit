import React, { useState, useEffect, useMemo } from 'react';
import { useHermesStore } from '../../stores/useHermesStore';
import { useChatStore } from '../../stores/useChatStore';
import { JudgeVerdictCard } from './JudgeVerdictCard';
import { judgeShow } from '../../services/hermesService';
import { JudgeVerdictDto } from '../../types/hermes';
import {
  listWorktrees,
  worktreeDiff,
  mergeWorktree,
  removeWorktree,
  closeAgent,
  HelmDiffSummary,
} from '../../services/worktreeService';
import { mergePreflight } from './mergeDiscard';
import ChatDiffReviewPane from '../chat/ChatDiffReviewPane';
import { selectActiveAgent } from './sessionSelect';
import { buildInspectorDiffReviewEdit, resolveJudgeRunId } from './inspectorData';
import { FileDiff, GitMerge, Trash2, AlertTriangle, Loader2 } from 'lucide-react';
import { cn } from '../../utils/cn';

export interface InspectorPanelProps {
  onClose?: () => void;
}

export const InspectorPanel: React.FC<InspectorPanelProps> = ({ onClose }) => {
  const selectedAgentId = useHermesStore((state) => state.selectedAgentId);
  const setSelectedAgentId = useHermesStore((state) => state.setSelectedAgentId);
  const agents = useHermesStore((state) => state.agents);
  const tasks = useHermesStore((state) => state.tasks);

  const agent = useMemo(() => selectActiveAgent(agents, selectedAgentId), [agents, selectedAgentId]);
  const task = useMemo(() => (agent?.taskId ? tasks[agent.taskId] : null), [agent, tasks]);

  const currentCwd = useChatStore((state) => state.currentCwd);

  const [diffSummary, setDiffSummary] = useState<HelmDiffSummary | null>(null);
  const [isLoadingDiff, setIsLoadingDiff] = useState(false);
  const [verdict, setVerdict] = useState<JudgeVerdictDto | null>(null);

  const [diffMode, setDiffMode] = useState<'unified' | 'split'>('unified');
  const [wrapLines, setWrapLines] = useState(false);

  // Modal State
  const [modalConfig, setModalConfig] = useState<{
    isOpen: boolean;
    title: string;
    message: string;
    isAlertOnly?: boolean;
    onConfirm: () => Promise<void> | void;
  }>({
    isOpen: false,
    title: '',
    message: '',
    isAlertOnly: false,
    onConfirm: () => {},
  });

  const fetchWorktreesAndDiff = async () => {
    if (!currentCwd || !task) return;
    try {
      setIsLoadingDiff(true);
      const wtList = await listWorktrees(currentCwd);

      const targetwt = wtList.find(
        (wt) => wt.branch === `helm/${task.id}` || wt.branch.endsWith(`helm/${task.id}`)
      );
      if (targetwt) {
        const diffSum = await worktreeDiff(targetwt.path);
        setDiffSummary(diffSum);
      } else {
        setDiffSummary(null);
      }
    } catch (err) {
      console.error('Error fetching worktree or diff:', err);
      setDiffSummary(null);
    } finally {
      setIsLoadingDiff(false);
    }
  };

  useEffect(() => {
    fetchWorktreesAndDiff();
  }, [currentCwd, task?.id]);

  useEffect(() => {
    let active = true;
    const loadVerdict = async () => {
      const runId = resolveJudgeRunId(agent);
      if (!runId) {
        setVerdict(null);
        return;
      }

      try {
        const v = await judgeShow(runId);
        if (active) {
          setVerdict(v);
        }
      } catch (err) {
        console.error('Error fetching judge verdict:', err);
        if (active) {
          setVerdict(null);
        }
      }
    };

    loadVerdict();

    return () => {
      active = false;
    };
  }, [agent]);

  const diffReviewEdit = useMemo(
    () => buildInspectorDiffReviewEdit({ task, diffSummary }),
    [task, diffSummary]
  );

  const handleMerge = () => {
    if (!task) return;
    const preflight = mergePreflight(task);
    if (!preflight.canMerge) {
      setModalConfig({
        isOpen: true,
        title: '合并校验失败 / Preflight Failed',
        message: `无法合并此任务：${preflight.reason || '任务状态不正确'}\nCannot merge task: ${preflight.reason || 'Incorrect task status.'}`,
        isAlertOnly: true,
        onConfirm: () => {},
      });
      return;
    }

    setModalConfig({
      isOpen: true,
      title: '合并任务 / Merge Task',
      message: `确定要合并分支 helm/${task.id} 的修改到主分支吗？(ID: ${task.id})\nAre you sure you want to merge modifications from branch helm/${task.id}?`,
      isAlertOnly: false,
      onConfirm: async () => {
        try {
          if (!currentCwd) return;
          const outcomeDto = await mergeWorktree(currentCwd, `helm/${task.id}`);
          if (outcomeDto.outcome === 'conflict') {
            setModalConfig({
              isOpen: true,
              title: '合并冲突 / Merge Conflict',
              message: '合并冲突，已自动回滚，请手动查看差异\nMerge conflict detected. The merge was aborted.',
              isAlertOnly: true,
              onConfirm: () => {},
            });
          } else {
            // Clean up worktree
            const wtList = await listWorktrees(currentCwd).catch(() => []);
            const targetwt = wtList.find(
              (wt) => wt.branch === `helm/${task.id}` || wt.branch.endsWith(`helm/${task.id}`)
            );
            if (targetwt?.path) {
              await removeWorktree(currentCwd, targetwt.path, true);
            }
            setSelectedAgentId(null);
          }
        } catch (err) {
          console.error('Failed to merge worktree:', err);
          setModalConfig({
            isOpen: true,
            title: '合并失败 / Merge Failed',
            message: `合并操作出错：${String(err)}\nAn error occurred during merge: ${String(err)}`,
            isAlertOnly: true,
            onConfirm: () => {},
          });
        }
      },
    });
  };

  const handleDiscard = () => {
    if (!task) return;

    setModalConfig({
      isOpen: true,
      title: '丢弃修改并删除工作树 / Discard Changes',
      message: `确定要丢弃此任务的所有修改并删除其工作树吗？此操作是破坏性的，且无法撤销！\nAre you sure you want to discard all changes and delete the worktree for task helm/${task.id}? This action is destructive and cannot be undone.`,
      isAlertOnly: false,
      onConfirm: async () => {
        try {
          if (!currentCwd) return;
          const wtList = await listWorktrees(currentCwd).catch(() => []);
          const targetwt = wtList.find(
            (wt) => wt.branch === `helm/${task.id}` || wt.branch.endsWith(`helm/${task.id}`)
          );
          const worktreePath = targetwt?.path || null;

          if (agent) {
            await closeAgent({
              agentId: agent.id,
              removeWorktree: true,
              repoRoot: currentCwd,
              worktreePath,
              force: true,
            });
          } else if (worktreePath) {
            await removeWorktree(currentCwd, worktreePath, true);
          }
          setSelectedAgentId(null);
        } catch (err) {
          console.error('Failed to discard worktree:', err);
          setModalConfig({
            isOpen: true,
            title: '丢弃失败 / Discard Failed',
            message: `丢弃操作出错：${String(err)}\nAn error occurred during discard: ${String(err)}`,
            isAlertOnly: true,
            onConfirm: () => {},
          });
        }
      },
    });
  };

  // If no agent/task selected
  if (!agent || !task) {
    return (
      <div className="flex flex-col h-full overflow-hidden bg-base-100">
        <div className="p-4 border-b border-base-300 flex justify-between items-center bg-base-200 flex-shrink-0">
          {onClose && (
            <button
              onClick={onClose}
              className="btn btn-ghost btn-xs h-6 min-h-0 focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
              aria-label="收起右栏"
            >
              ▶
            </button>
          )}
          <span className="font-bold text-sm tracking-wider uppercase text-base-content/80">
            检查器 / Inspector
          </span>
        </div>
        <div className="flex-1 flex flex-col items-center justify-center p-8 text-center select-none bg-base-100">
          <FileDiff className="h-10 w-10 text-base-content/30 mb-3 animate-pulse" />
          <h3 className="text-xs font-semibold text-base-content/80 mb-1">
            尚未选择待评审 worker / No worker selected for review.
          </h3>
          <p className="text-[10px] text-base-content/40 max-w-[240px] leading-relaxed">
            请选择 Review 泳道中处于 awaiting-merge 状态的 retained worktree 以审查 diff 及合并产物。
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full overflow-hidden bg-base-100">
      {/* Header */}
      <div className="p-4 border-b border-base-300 flex justify-between items-center bg-base-200 flex-shrink-0">
        {onClose && (
          <button
            onClick={onClose}
            className="btn btn-ghost btn-xs h-6 min-h-0 focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
            aria-label="收起右栏"
          >
            ▶
          </button>
        )}
        <span className="font-bold text-sm tracking-wider uppercase text-base-content/80">
          检查器 / Inspector
        </span>
      </div>

      {/* Meta Info & Stats */}
      <div className="p-3 bg-base-200/50 border-b border-base-300 flex flex-col gap-1.5 flex-shrink-0 text-xs">
        <div className="flex items-center justify-between">
          <span className="font-semibold text-base-content/70">分支 / Branch:</span>
          <span className="font-mono text-[10px] bg-base-300 px-1.5 py-0.5 rounded text-primary font-bold">
            helm/{task.id}
          </span>
        </div>
        <div className="flex items-center justify-between text-[11px] text-base-content/50">
          <span>Worker: {agent.id}</span>
          <span>Status: {task.status}</span>
        </div>
        {isLoadingDiff ? (
          <div className="flex items-center gap-1 text-[11px] text-base-content/40">
            <Loader2 className="h-3 w-3 animate-spin text-primary" />
            <span>Calculating differences...</span>
          </div>
        ) : (
          diffSummary && (
            <div className="flex items-center gap-2.5 font-mono text-[10px] bg-base-200 p-1.5 rounded mt-1">
              <span className="text-base-content/60">改动文件: {diffSummary.filesChanged}</span>
              <span className="text-success font-bold">+{diffSummary.insertions}</span>
              <span className="text-error font-bold">-{diffSummary.deletions}</span>
            </div>
          )
        )}
      </div>

      {/* Diff Review Pane */}
      <div className="flex-1 overflow-hidden min-h-0 relative flex flex-col">
        {diffReviewEdit ? (
          <ChatDiffReviewPane
            edit={diffReviewEdit}
            mode={diffMode}
            wrapLines={wrapLines}
            currentCwd={currentCwd}
            onModeChange={setDiffMode}
            onWrapLinesChange={setWrapLines}
          />
        ) : (
          <div className="p-8 text-center text-xs text-base-content/40">
            暂无可预览的真实 diff。
          </div>
        )}
      </div>

      {/* Judge Verdict Panel */}
      <div className="p-3 bg-base-200 border-t border-base-300 max-h-64 overflow-y-auto flex-shrink-0">
        <JudgeVerdictCard verdict={verdict} />
      </div>

      {/* Actions */}
      <div className="p-3.5 bg-base-200 border-t border-base-300 flex items-center justify-between gap-3 flex-shrink-0">
        <button
          type="button"
          onClick={handleDiscard}
          className="btn btn-sm flex-1 font-semibold text-xs border-red-300 text-red-600 hover:bg-red-50 dark:border-red-900/50 dark:text-red-400 dark:hover:bg-red-950/20 rounded-lg flex items-center justify-center gap-1.5"
        >
          <Trash2 className="h-3.5 w-3.5" />
          丢弃修改 / Discard
        </button>
        <button
          type="button"
          onClick={handleMerge}
          className="btn btn-primary btn-sm flex-1 font-semibold text-xs rounded-lg flex items-center justify-center gap-1.5"
        >
          <GitMerge className="h-3.5 w-3.5" />
          合并保留 / Merge
        </button>
      </div>

      {/* Confirmation Modal */}
      {modalConfig.isOpen && (
        <div className="modal modal-open z-50">
          <div className="modal-box bg-base-100 border border-base-300 shadow-xl max-w-sm rounded-xl">
            <h3 className="font-bold text-sm text-base-content/90 tracking-wide uppercase mb-2 flex items-center gap-1.5">
              <AlertTriangle className={cn("h-4 w-4", modalConfig.isAlertOnly ? "text-warning" : "text-error")} />
              {modalConfig.title}
            </h3>
            <p className="text-xs text-base-content/70 whitespace-pre-line leading-relaxed mb-6">
              {modalConfig.message}
            </p>
            <div className="modal-action gap-2 animate-none">
              {!modalConfig.isAlertOnly && (
                <button
                  type="button"
                  className="btn btn-ghost btn-sm text-xs font-semibold focus:outline-none rounded-lg"
                  onClick={() => setModalConfig((prev) => ({ ...prev, isOpen: false }))}
                >
                  取消 / Cancel
                </button>
              )}
              <button
                type="button"
                className="btn btn-primary btn-sm text-xs font-semibold focus:outline-none rounded-lg"
                onClick={async () => {
                  setModalConfig((prev) => ({ ...prev, isOpen: false }));
                  await modalConfig.onConfirm();
                }}
              >
                {modalConfig.isAlertOnly ? '关闭 / Close' : '确认 / Confirm'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

export default InspectorPanel;
