import React, { useState, useEffect, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { useHermesStore } from '../../stores/useHermesStore';
import { useChatStore } from '../../stores/useChatStore';
import { listWorktrees } from '../../services/worktreeService';
import { openChatPathInExplorer } from '../../utils/chatWorkspaceStatus';
import { showToast } from '../common/ToastContainer';
import { selectActiveAgent, selectVisibleRun, shouldFallbackToActivityTimeline } from './sessionSelect';
import { isAbortableActiveAgent, sessionHeaderActions } from './sessionHeaderActions';
import { cn } from '../../utils/cn';
import { InterventionGateCard } from './InterventionGateCard';
import { InterventionGateDto, WorkerSessionDto, WorkerTranscriptDto } from '../../types/hermes';
import * as hermesService from '../../services/hermesService';
import MessageList from '../chat/MessageList';
import type { ChatMessage, MessageRaw } from '../../types/chat';

import {
  Activity,
  Play,
  Check,
  AlertTriangle,
  Loader2,
  FileText,
  Settings,
  XOctagon
} from 'lucide-react';

export const SessionPanel: React.FC = () => {
  const { t } = useTranslation();
  
  const selectedAgentId = useHermesStore((state) => state.selectedAgentId);
  const agents = useHermesStore((state) => state.agents);
  const runs = useHermesStore((state) => state.runs);
  const agent = selectActiveAgent(agents, selectedAgentId);
  const visibleRun = selectVisibleRun(runs);

  const currentCwd = useChatStore((state) => state.currentCwd);
  const setCurrentCwd = useChatStore((state) => state.setCurrentCwd);

  const [transcript, setTranscript] = useState<WorkerTranscriptDto | null>(null);
  const [history, setHistory] = useState<WorkerSessionDto[]>([]);
  const [selectedHistoryDispatchId, setSelectedHistoryDispatchId] = useState<string | null>(null);
  const [, setIsLoadingTranscript] = useState(false);

  useEffect(() => {
    if (selectedAgentId) {
      setIsLoadingTranscript(true);
      hermesService
        .workerTranscript(selectedAgentId)
        .then((res) => {
          setTranscript(res);
        })
        .catch((err) => {
          console.error('Failed to load worker transcript:', err);
          setTranscript(null);
        })
        .finally(() => {
          setIsLoadingTranscript(false);
        });
    } else {
      setTranscript(null);
    }
  }, [selectedAgentId]);

  useEffect(() => {
    hermesService.workerSessionList().then(setHistory).catch((err) => {
      console.error('Failed to load worker session history:', err);
      setHistory([]);
    });
  }, [visibleRun?.status]);

  useEffect(() => {
    if (!selectedHistoryDispatchId) return;
    hermesService.workerTranscript(selectedHistoryDispatchId).then(setTranscript).catch((err) => {
      console.error('Failed to load archived worker transcript:', err);
      setTranscript(null);
    });
  }, [selectedHistoryDispatchId]);

  const transcriptMessages = useMemo<ChatMessage[]>(() => (transcript?.entries ?? []).flatMap((entry, index) => {
    if (entry.kind !== 'messageRaw') return [];
    try {
      const raw = JSON.parse(entry.json) as MessageRaw;
      if ((raw.type !== 'assistant' && raw.type !== 'user') || !Array.isArray(raw.message?.content)) return [];
      return [{ id: raw.uuid ?? `worker-${index}`, role: raw.type, content: '', raw, createdAt: Date.parse(entry.createdAt) || index }];
    } catch { return []; }
  }), [transcript]);

  const [gate, setGate] = useState<InterventionGateDto | null>(null);

  useEffect(() => {
    if (agent && agent.status === 'needs-attention') {
      const fetchGate = async () => {
        try {
          const list = await hermesService.gateList({ taskId: agent.taskId || undefined });
          if (list && list.length > 0) {
            setGate(list[0]);
          } else {
            setGate(null);
          }
        } catch (e) {
          console.error('Failed to load intervention gate:', e);
          setGate(null);
        }
      };
      void fetchGate();
    } else {
      setGate(null);
    }
  }, [agent?.id, agent?.status, agent?.taskId]);

  const handleResolveGate = (resolution: 'approve' | 'reject', _comment: string) => {
    setGate(null);
    showToast(t('common.success', 'Success') + `: Gate resolved successfully with ${resolution}`, 'success');
  };


  const handleJumpToWorktree = async () => {
    if (!agent || !agent.taskId) return;
    if (!currentCwd) {
      showToast('No project directory configured in current workspace', 'error');
      return;
    }
    try {
      const worktrees = await listWorktrees(currentCwd);
      const targetBranch = `helm/${agent.taskId}`;
      const matched = worktrees.find(
        (wt) => wt.branch === targetBranch || wt.branch.endsWith(`/${targetBranch}`)
      );
      if (matched) {
        setCurrentCwd(matched.path);
        await openChatPathInExplorer(matched.path).catch((err) => {
          console.error('Failed to open explorer:', err);
        });
        showToast(t('common.success', 'Success') + `: Jumped to worktree branch: ${targetBranch}`, 'success');
      } else {
        showToast(`No worktree found for branch: ${targetBranch}`, 'warning');
      }
    } catch (err) {
      console.error('Failed to jump to worktree:', err);
      showToast(`Error finding worktree: ${String(err)}`, 'error');
    }
  };

  const handleStopAgent = async () => {
    if (!agent) return;

    const latestAgent = useHermesStore.getState().agents[agent.id];
    if (!isAbortableActiveAgent(latestAgent)) {
      showToast(t('helm.agentAlreadyFinished', 'This agent has already finished.'), 'info');
      return;
    }

    try {
      await hermesService.agentAbort(agent.id);
      await useHermesStore.getState().refreshSnapshot();
      showToast(t('common.success', 'Success') + ': Agent stopped', 'success');
    } catch (err) {
      console.error('Failed to stop agent:', err);
      showToast(`Failed to stop agent: ${String(err)}`, 'error');
    }
  };

  if (!selectedAgentId || !agent) {
    if (history.length > 0) {
      const selectedHistory = history.find((session) => session.dispatchId === selectedHistoryDispatchId) ?? null;
      return (
        <div className="flex-1 overflow-y-auto bg-base-100 p-6">
          <div className="mx-auto max-w-4xl">
            <label className="text-xs font-medium" htmlFor="hermes-history-worker">{t('helm.history.title')}</label>
            <select id="hermes-history-worker" className="select select-sm mt-1 w-full" value={selectedHistoryDispatchId ?? ''} onChange={(event) => setSelectedHistoryDispatchId(event.target.value || null)}>
              <option value="">{t('helm.history.select')}</option>
              {history.map((session) => <option key={session.dispatchId} value={session.dispatchId}>{session.taskId} {session.error ? t('helm.history.failed') : t('helm.history.completed')}</option>)}
            </select>
            {selectedHistory && <p className="mt-3 text-xs text-base-content/60">{selectedHistory.error ?? selectedHistory.finalResponse ?? t('helm.history.noFinal')}</p>}
            {selectedHistoryDispatchId && (transcriptMessages.length > 0 ? <MessageList messages={transcriptMessages} /> : <p className="mt-6 text-center text-xs text-base-content/55">{t('helm.history.empty')}</p>)}
          </div>
        </div>
      );
    }
    if (visibleRun) {
      const isFailed = visibleRun.status === 'failed';
      const isCompleted = visibleRun.status === 'completed';
      return (
        <div className="flex-1 flex flex-col items-center justify-center p-8 text-center bg-base-100 select-none h-full">
          <div className="max-w-xl rounded-lg border border-base-300 bg-base-100 px-6 py-5 shadow-sm">
            <div className="flex items-center justify-center mb-3">
              {isFailed ? (
                <XOctagon className="h-8 w-8 text-error" />
              ) : isCompleted ? (
                <Check className="h-8 w-8 text-success" />
              ) : (
                <Loader2 className="h-8 w-8 animate-spin text-primary" />
              )}
            </div>
            <h3 className="text-sm font-semibold text-base-content/85">
              {isFailed
                ? 'Hermes 运行失败'
                : isCompleted
                  ? 'Hermes 运行已完成'
                  : 'Hermes 正在规划任务'}
            </h3>
            <p className="mt-2 text-xs leading-relaxed text-base-content/60">
              {visibleRun.goal}
            </p>
            {visibleRun.error && (
              <p className="mt-3 rounded border border-error/25 bg-error/5 px-3 py-2 text-left text-xs text-error">
                {visibleRun.error}
              </p>
            )}
            {!isFailed && !isCompleted && (
              <p className="mt-3 text-[11px] text-base-content/45">
                Planner 正在拆解目标；任务或 Agent 出现后会自动切换到会话视图。
              </p>
            )}
          </div>
        </div>
      );
    }

    return (
      <div className="flex-1 flex flex-col items-center justify-center p-8 text-center bg-base-100 select-none h-full">
        <Activity className="h-12 w-12 text-base-content/30 mb-4 animate-pulse" />
        <h3 className="text-sm font-semibold text-base-content/75 max-w-md">
          {t('helm.emptyState', '请从左侧舰队看板选择一个 Agent / Select an agent from the kanban')}
        </h3>
      </div>
    );
  }

  // Generate Timeline Events
  const timelineEvents = [];
  const showActivityFallback = shouldFallbackToActivityTimeline(transcript?.entries ?? null);

  // 1. Initialized Event
  timelineEvents.push({
    id: 'init',
    title: t('helm.timeline.initialized', 'Agent Session Initialized'),
    desc: t('helm.timeline.initializedDesc', 'Agent workspace environment setup complete.'),
    time: agent.dispatchedAt || agent.createdAt || null,
    status: 'completed',
    icon: 'check',
  });

  // 2. Running / Current activity Event
  if (agent.status === 'working') {
    if (agent.activity === 'thinking') {
      timelineEvents.push({
        id: 'thinking',
        title: t('helm.timeline.thinking', 'Agent is thinking'),
        desc: t('helm.timeline.thinkingDesc', 'Analyzing instructions, exploring codebase, and planning actions.'),
        time: null,
        status: 'active',
        icon: 'thinking',
      });
    } else if (agent.activity === 'text') {
      timelineEvents.push({
        id: 'text',
        title: t('helm.timeline.text', 'Streaming Text Response'),
        desc: t('helm.timeline.textDesc', 'Formulating explanation and compiling feedback.'),
        time: null,
        status: 'active',
        icon: 'text',
      });
    } else if (agent.activity === 'tool_use') {
      timelineEvents.push({
        id: 'tool_use',
        title: t('helm.timeline.toolUse', 'Executing Tools'),
        desc: t('helm.timeline.toolUseDesc', 'Invoking system or workspace tools to complete tasks.'),
        time: null,
        status: 'active',
        icon: 'tool',
      });
    } else {
      timelineEvents.push({
        id: 'working',
        title: t('helm.timeline.working', 'Running Task'),
        desc: t('helm.timeline.workingDesc', 'Executing automated routine.'),
        time: null,
        status: 'active',
        icon: 'working',
      });
    }
  } else if (agent.status === 'needs-attention') {
    timelineEvents.push({
      id: 'needs-attention',
      title: t('helm.timeline.needsAttention', 'Attention Required'),
      desc: t('helm.timeline.needsAttentionDesc', 'Agent paused waiting for manual confirmation or user input.'),
      time: null,
      status: 'warning',
      icon: 'warning',
    });
  } else if (agent.status === 'done') {
    timelineEvents.push({
      id: 'done',
      title: t('helm.timeline.done', 'Task Completed'),
      desc: t('helm.timeline.doneDesc', 'All actions finished successfully.'),
      time: agent.completedAt || null,
      status: 'success',
      icon: 'success',
    });
  } else if (agent.status === 'interrupted') {
    timelineEvents.push({
      id: 'interrupted',
      title: t('helm.timeline.interrupted', 'Session Interrupted'),
      desc: t('helm.timeline.interruptedDesc', 'Execution stopped before completion.'),
      time: agent.completedAt || null,
      status: 'error',
      icon: 'error',
    });
  }

  const renderTimelineIcon = (iconName: string) => {
    switch (iconName) {
      case 'check':
        return <Check className="h-4 w-4" />;
      case 'thinking':
        return <Loader2 className="h-4 w-4 animate-spin" />;
      case 'text':
        return <FileText className="h-4 w-4" />;
      case 'tool':
        return <Settings className="h-4 w-4" />;
      case 'working':
        return <Play className="h-4 w-4 animate-pulse" />;
      case 'warning':
        return <AlertTriangle className="h-4 w-4 animate-bounce" />;
      case 'success':
        return <Check className="h-4 w-4" />;
      case 'error':
        return <XOctagon className="h-4 w-4" />;
      default:
        return <Play className="h-4 w-4" />;
    }
  };

  const actions = sessionHeaderActions(agent);

  return (
    <div className="flex flex-col flex-1 h-full overflow-hidden bg-base-100">
      {/* Session Panel Header */}
      <div className="p-4 border-b border-base-300 flex justify-between items-center bg-base-200">
        <div className="flex flex-col gap-0.5">
          <div className="flex items-center gap-2">
            <span className="text-xs font-bold text-base-content/50 uppercase tracking-wider">
              {t('helm.agentId', 'Agent ID')}:
            </span>
            <span className="font-mono text-xs font-semibold text-base-content">
              {agent.id}
            </span>
          </div>
          {agent.taskId && (
            <div className="flex items-center gap-2">
              <span className="text-xs font-bold text-base-content/50 uppercase tracking-wider">
                {t('helm.taskId', 'Task ID')}:
              </span>
              <span className="font-mono text-xs font-semibold text-primary">
                {agent.taskId}
              </span>
            </div>
          )}
        </div>
        
        {/* Action Buttons with tooltips */}
        <div className="flex items-center gap-2">
          {actions.map((act) => {
            const isJump = act.id === 'jumpToWorktree';
            const isStop = act.id === 'stop';
            const buttonElement = (
              <button
                key={act.id}
                onClick={isJump ? handleJumpToWorktree : isStop ? handleStopAgent : undefined}
                disabled={act.disabled}
                className={cn(
                  "btn btn-xs rounded-lg font-semibold",
                  isJump
                    ? "btn-primary text-primary-content"
                    : "btn-outline btn-neutral"
                )}
              >
                {t(act.labelKey, act.id === 'jumpToWorktree' ? 'Jump to Worktree' : act.id === 'stop' ? 'Stop / Abort' : 'Cancel')}
              </button>
            );

            if (act.tooltipKey) {
              return (
                <div
                  key={act.id}
                  className="tooltip tooltip-bottom"
                  data-tip={t(act.tooltipKey)}
                >
                  {buttonElement}
                </div>
              );
            }

            return buttonElement;
          })}
        </div>
      </div>

      {/* Main Stream Activity Area */}
      <div className="flex-1 overflow-y-auto p-6 flex flex-col justify-between">
        <div>
          {showActivityFallback ? (
            <div className="max-w-2xl mx-auto mt-4">
              <h3 className="text-sm font-semibold text-base-content/85 mb-6 uppercase tracking-wider">
                Activity Stream
              </h3>
              
              {/* Vertical Timeline */}
              <div className="flow-root pl-2">
                <ul className="-mb-8">
                  {timelineEvents.map((event, idx) => (
                    <li key={event.id}>
                      <div className="relative pb-8">
                        {idx !== timelineEvents.length - 1 ? (
                          <span
                            className="absolute top-4 left-4 -ml-px h-full w-0.5 bg-base-300 dark:bg-neutral-focus"
                            aria-hidden="true"
                          />
                        ) : null}
                        <div className="relative flex space-x-3">
                          <div>
                            <span
                              className={cn(
                                "h-8 w-8 rounded-full flex items-center justify-center ring-8 ring-base-100 text-white",
                                event.status === 'completed' && "bg-emerald-500",
                                event.status === 'active' && "bg-primary",
                                event.status === 'warning' && "bg-amber-500",
                                event.status === 'success' && "bg-emerald-500",
                                event.status === 'error' && "bg-red-500"
                              )}
                            >
                              {renderTimelineIcon(event.icon)}
                            </span>
                          </div>
                          <div className="flex-1 min-w-0 pt-1.5 flex justify-between space-x-4">
                            <div>
                              <p className="text-xs font-semibold text-base-content">
                                {event.title}
                              </p>
                              <p className="text-xs text-base-content/60 mt-0.5">
                                {event.desc}
                              </p>
                            </div>
                            {event.time && (
                              <div className="text-right text-[10px] whitespace-nowrap text-base-content/40">
                                <time>{new Date(event.time).toLocaleTimeString()}</time>
                              </div>
                            )}
                          </div>
                        </div>
                      </div>
                    </li>
                  ))}
                </ul>
              </div>

              {(transcript?.entries.length ?? 0) === 0 && <p className="mt-6 text-center text-xs text-base-content/55">{t('helm.history.empty')}</p>}
              {(transcript?.entries ?? []).filter((entry) => entry.kind === 'activity').map((entry, index) => (
                <p key={`${entry.createdAt}-${index}`} className="mt-2 text-xs text-base-content/65">{entry.text}</p>
              ))}
            </div>
          ) : (
            <MessageList messages={transcriptMessages} />
          )}
        </div>

        {/* Needs-Attention Gate Card */}
        {agent.status === 'needs-attention' && gate && (
          <div className="mt-8">
            <InterventionGateCard gate={gate} onResolve={handleResolveGate} />
          </div>
        )}
      </div>

    </div>
  );
};

export default SessionPanel;
