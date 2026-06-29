import React, { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { useHermesStore } from '../../stores/useHermesStore';
import { useChatStore } from '../../stores/useChatStore';
import { listWorktrees } from '../../services/worktreeService';
import { openChatPathInExplorer } from '../../utils/chatWorkspaceStatus';
import { showToast } from '../common/ToastContainer';
import { selectActiveAgent } from './sessionSelect';
import { sessionHeaderActions } from './sessionHeaderActions';
import { cn } from '../../utils/cn';
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

interface DummyMessage {
  id: string;
  role: string;
  content: string;
}

// Dummy transcript loader that currently returns null
async function loadWorkerTranscript(agentId: string): Promise<DummyMessage[] | null> {
  if (!agentId) return null;
  return null;
}

export const SessionPanel: React.FC = () => {
  const { t } = useTranslation();
  
  const selectedAgentId = useHermesStore((state) => state.selectedAgentId);
  const agents = useHermesStore((state) => state.agents);
  const agent = selectActiveAgent(agents, selectedAgentId);

  const currentCwd = useChatStore((state) => state.currentCwd);
  const setCurrentCwd = useChatStore((state) => state.setCurrentCwd);

  const [transcript, setTranscript] = useState<DummyMessage[] | null>(null);
  const [, setIsLoadingTranscript] = useState(false);

  useEffect(() => {
    if (selectedAgentId) {
      setIsLoadingTranscript(true);
      loadWorkerTranscript(selectedAgentId).then((res) => {
        setTranscript(res);
        setIsLoadingTranscript(false);
      });
    } else {
      setTranscript(null);
    }
  }, [selectedAgentId]);

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

  if (!selectedAgentId || !agent) {
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
            const buttonElement = (
              <button
                key={act.id}
                onClick={isJump ? handleJumpToWorktree : undefined}
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
          {transcript === null ? (
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

              {/* Placeholder banner */}
              <div className="mt-12 bg-base-200 border border-base-300 rounded-xl p-4 text-xs font-mono leading-relaxed text-base-content/70 text-center shadow-sm">
                <div className="font-bold text-base-content/60">
                  {t(
                    'helm.placeholderBanner',
                    '完整执行记录将在引擎接通后显示 / Full transcript will be shown after engine bridge is lit.'
                  )}
                </div>
              </div>
            </div>
          ) : (
            // Full transcript view (never loaded since loadWorkerTranscript returns null)
            <div className="text-xs font-mono text-base-content/60">
              Loaded transcript messages: {transcript.length}
            </div>
          )}
        </div>

        {/* Needs-Attention Gate Card */}
        {agent.status === 'needs-attention' && (
          <div className="mt-8 border border-amber-300 dark:border-amber-900 bg-amber-50/50 dark:bg-amber-950/15 rounded-xl p-5 shadow-sm flex flex-col gap-4 max-w-2xl mx-auto w-full transition-all duration-300">
            <div className="flex items-start gap-3">
              <AlertTriangle className="h-5 w-5 text-amber-500 flex-shrink-0 mt-0.5" />
              <div>
                <h4 className="text-sm font-semibold text-amber-800 dark:text-amber-300">
                  {t('helm.attentionGate.title', '需要人工介入 / Attention Required')}
                </h4>
                <p className="text-xs text-amber-700 dark:text-amber-400/90 mt-1 leading-relaxed">
                  {t(
                    'helm.attentionGate.description',
                    '该 Agent 已暂停，需要您进行评审或提供输入以继续执行。 / This agent has paused and requires your review or intervention to proceed.'
                  )}
                </p>
              </div>
            </div>
            
            <div className="flex items-center justify-between border-t border-amber-200 dark:border-amber-900/40 pt-4 mt-2">
              <span className="text-[10px] font-semibold text-amber-800/60 dark:text-amber-400/60 uppercase tracking-wider">
                {t('helm.attentionGate.actionRequired', '需采取行动 / Action Required')}
              </span>
              <div className="flex gap-2">
                <button 
                  className="btn btn-warning btn-xs rounded-lg font-semibold"
                  disabled
                >
                  Resume Agent (Phase 4.5)
                </button>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
};

export default SessionPanel;
