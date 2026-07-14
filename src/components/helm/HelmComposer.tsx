import React, { useState, useEffect, useMemo, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { useHermesStore } from '../../stores/useHermesStore';
import { buildLaunch, HelmRosterPick } from './launchPlan';
import RosterPanel from './RosterPanel';
import * as hermesService from '../../services/hermesService';
import { reduceHermesEvent } from '../../stores/hermesReducer';
import { FolderOpen, Loader2, Play, Sparkles } from 'lucide-react';
import { useProviderStore } from '../../stores/useProviderStore';
import { rosterPicksFromProviders } from '../chat/fanout/roster';
import { useChatStore } from '../../stores/useChatStore';
import { pickWorkspaceFolder } from '../../utils/chatWorkspaceStatus';
import { resolveHermesWorkspaceRoot } from './hermesWorkspace';
import { extractHermesLaunchErrorReason, getHermesLaunchErrorMessageKey } from './hermesLaunchError';
import { canRequestHermesLaunch } from './hermesLaunchReadiness';

type HermesLaunchMode = 'real' | 'mock';

export default function HelmComposer() {
  const { t } = useTranslation();
  const [goal, setGoal] = useState('');
  const [selectedPicks, setSelectedPicks] = useState<string[]>([]);
  const [maxConcurrent, setMaxConcurrent] = useState<number | undefined>(undefined);
  const [launching, setLaunching] = useState(false);
  const [workspaceRoot, setWorkspaceRoot] = useState<string | null>(null);
  const [workspaceLoading, setWorkspaceLoading] = useState(false);
  const [workspaceError, setWorkspaceError] = useState<string | null>(null);
  const [launchError, setLaunchError] = useState<string | null>(null);
  const providers = useProviderStore((state) => state.providers);
  const currentCwd = useChatStore((state) => state.currentCwd);
  const setCurrentCwd = useChatStore((state) => state.setCurrentCwd);
  const roster = useMemo(() => rosterPicksFromProviders(providers), [providers]);
  const launchPicks = useMemo<HelmRosterPick[]>(
    () =>
      selectedPicks.flatMap((providerId) => {
        const pick = roster.find((item) => item.providerId === providerId);
        const model = pick?.models[0]?.id.trim();
        if (!pick || !model) return [];
        return [
          {
            providerId: pick.providerId,
            providerName: pick.providerName,
            chatProvider: pick.chatProvider,
            model,
          },
        ];
      }),
    [roster, selectedPicks]
  );
  
  // Walkthrough simulation state
  const [isWalkthroughRunning, setIsWalkthroughRunning] = useState(false);
  const timeoutsRef = useRef<number[]>([]);

  useEffect(() => {
    return () => {
      timeoutsRef.current.forEach((timeoutId) => clearTimeout(timeoutId));
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    const selectedFolder = currentCwd?.trim();

    if (!selectedFolder) {
      setWorkspaceRoot(null);
      return () => {
        cancelled = true;
      };
    }

    setWorkspaceRoot(null);
    void resolveHermesWorkspaceRoot(selectedFolder)
      .then((repoRoot) => {
        if (!cancelled) setWorkspaceRoot(repoRoot);
      })
      .catch(() => {
        if (!cancelled) setWorkspaceRoot(null);
      });

    return () => {
      cancelled = true;
    };
  }, [currentCwd]);

  const clearAllTimeouts = () => {
    timeoutsRef.current.forEach((timeoutId) => clearTimeout(timeoutId));
    timeoutsRef.current = [];
    setIsWalkthroughRunning(false);
  };

  const displayLaunchError = (error: unknown) => {
    const reason = extractHermesLaunchErrorReason(error);
    const messageKey = getHermesLaunchErrorMessageKey(reason);
    setLaunchError(
      messageKey === 'launchErrorUnknown'
        ? t(`helm.composer.${messageKey}`, { reason })
        : t(`helm.composer.${messageKey}`)
    );
  };

  const handleProjectFolderSelect = async (): Promise<string | null> => {
    try {
      const selectedFolder = await pickWorkspaceFolder({
        defaultPath: currentCwd,
        title: t('helm.composer.projectFolderDialogTitle'),
        promptFallbackLabel: t('helm.composer.projectFolderPrompt'),
      });
      if (!selectedFolder) return null;

      setWorkspaceLoading(true);
      setWorkspaceError(null);
      const repoRoot = await resolveHermesWorkspaceRoot(selectedFolder);
      if (!repoRoot) {
        setWorkspaceRoot(null);
        setCurrentCwd(null);
        setWorkspaceError(t('helm.composer.projectFolderInvalid'));
        return null;
      }

      setWorkspaceRoot(repoRoot);
      setCurrentCwd(repoRoot);
      return repoRoot;
    } catch {
      setWorkspaceRoot(null);
      setCurrentCwd(null);
      setWorkspaceError(t('helm.composer.projectFolderLoadFailed'));
      return null;
    } finally {
      setWorkspaceLoading(false);
    }
  };

  const startWalkthrough = () => {
    clearAllTimeouts();
    setIsWalkthroughRunning(true);

    const store = useHermesStore.getState();
    store.resetStore();

    const runId = 'walkthrough-run';
    const goalStr = goal.trim() || 'Design and implement a Trie-based HTTP routing library in Rust';

    // 1. Initialize the run
    store.initRun(runId, goalStr);

    // 2. Set the initial tasks and agents
    const initialTasks = [
      {
        id: 'task-01',
        parentId: null,
        spec: 'Analyze workspace layout and outline Trie router structure in src/lib.rs',
        status: 'ready',
        deps: [],
        result: null,
        createdAt: new Date().toISOString(),
        completedAt: null,
      },
      {
        id: 'task-02',
        parentId: 'task-01',
        spec: 'Implement router path matching and wildcard resolution in src/router.rs',
        status: 'pending',
        deps: ['task-01'],
        result: null,
        createdAt: new Date().toISOString(),
        completedAt: null,
      },
      {
        id: 'task-03',
        parentId: 'task-02',
        spec: 'Write comprehensive integration tests for route parameters and matching edge-cases',
        status: 'pending',
        deps: ['task-02'],
        result: null,
        createdAt: new Date().toISOString(),
        completedAt: null,
      },
    ];

    const initialAgents = [
      {
        id: 'claude-02',
        taskId: '',
        status: 'working',
        assignee: 'claude-02',
        failureCount: 0,
        lastHeartbeatAt: new Date().toISOString(),
        lastFailure: null,
        dispatchedAt: null,
        completedAt: null,
        createdAt: new Date().toISOString(),
      },
      {
        id: 'codex-07',
        taskId: '',
        status: 'working',
        assignee: 'codex-07',
        failureCount: 0,
        lastHeartbeatAt: new Date().toISOString(),
        lastFailure: null,
        dispatchedAt: null,
        completedAt: null,
        createdAt: new Date().toISOString(),
      },
    ];

    store.setTasks(initialTasks);
    store.setAgents(initialAgents);
    
    // Focus on the first agent
    store.setSelectedAgentId('claude-02');

    const pushTimeout = (fn: () => void, delay: number) => {
      const t = window.setTimeout(fn, delay);
      timeoutsRef.current.push(t);
    };

    // T = 1.5s: claude-02 starts task-01
    pushTimeout(() => {
      useHermesStore.setState((state) => {
        const s1 = reduceHermesEvent(state, {
          kind: 'task',
          runId,
          taskId: 'task-01',
          status: 'dispatched',
          dispatchId: 'claude-02',
        });
        return reduceHermesEvent(s1, {
          kind: 'agent',
          runId,
          agentId: 'claude-02',
          taskId: 'task-01',
          status: 'working',
          activity: 'thinking',
        });
      });
    }, 1500);

    // T = 3.5s: claude-02 executing tools on task-01
    pushTimeout(() => {
      useHermesStore.setState((state) => {
        return reduceHermesEvent(state, {
          kind: 'agent',
          runId,
          agentId: 'claude-02',
          taskId: 'task-01',
          status: 'working',
          activity: 'tool_use',
        });
      });
    }, 3500);

    // T = 5.5s: claude-02 completes task-01, task-02 becomes ready
    pushTimeout(() => {
      useHermesStore.setState((state) => {
        const s1 = reduceHermesEvent(state, {
          kind: 'task',
          runId,
          taskId: 'task-01',
          status: 'completed',
          dispatchId: 'claude-02',
        });
        const s2 = reduceHermesEvent(s1, {
          kind: 'task',
          runId,
          taskId: 'task-02',
          status: 'ready',
          dispatchId: null,
        });
        return reduceHermesEvent(s2, {
          kind: 'agent',
          runId,
          agentId: 'claude-02',
          taskId: null,
          status: 'done',
          activity: null,
        });
      });
    }, 5500);

    // T = 7.5s: Switch focus to codex-07, codex-07 starts task-02
    pushTimeout(() => {
      useHermesStore.getState().setSelectedAgentId('codex-07');
      useHermesStore.setState((state) => {
        const s1 = reduceHermesEvent(state, {
          kind: 'task',
          runId,
          taskId: 'task-02',
          status: 'dispatched',
          dispatchId: 'codex-07',
        });
        return reduceHermesEvent(s1, {
          kind: 'agent',
          runId,
          agentId: 'codex-07',
          taskId: 'task-02',
          status: 'working',
          activity: 'thinking',
        });
      });
    }, 7500);

    // T = 9.5s: codex-07 needs attention (awaits review / attention gate)
    pushTimeout(() => {
      useHermesStore.setState((state) => {
        const s1 = reduceHermesEvent(state, {
          kind: 'task',
          runId,
          taskId: 'task-02',
          status: 'awaiting-merge',
          dispatchId: 'codex-07',
        });
        return reduceHermesEvent(s1, {
          kind: 'agent',
          runId,
          agentId: 'codex-07',
          taskId: 'task-02',
          status: 'needs-attention',
          activity: null,
        });
      });
    }, 9500);

    // T = 12.5s: Resume from attention gate (simulate resume actions)
    pushTimeout(() => {
      useHermesStore.setState((state) => {
        const s1 = reduceHermesEvent(state, {
          kind: 'task',
          runId,
          taskId: 'task-02',
          status: 'dispatched',
          dispatchId: 'codex-07',
        });
        return reduceHermesEvent(s1, {
          kind: 'agent',
          runId,
          agentId: 'codex-07',
          taskId: 'task-02',
          status: 'working',
          activity: 'tool_use',
        });
      });
    }, 12500);

    // T = 14.5s: codex-07 completes task-02, task-03 becomes ready
    pushTimeout(() => {
      useHermesStore.setState((state) => {
        const s1 = reduceHermesEvent(state, {
          kind: 'task',
          runId,
          taskId: 'task-02',
          status: 'completed',
          dispatchId: 'codex-07',
        });
        const s2 = reduceHermesEvent(s1, {
          kind: 'task',
          runId,
          taskId: 'task-03',
          status: 'ready',
          dispatchId: null,
        });
        return reduceHermesEvent(s2, {
          kind: 'agent',
          runId,
          agentId: 'codex-07',
          taskId: null,
          status: 'done',
          activity: null,
        });
      });
    }, 14500);

    // T = 16.5s: Switch focus to claude-02, claude-02 starts task-03
    pushTimeout(() => {
      useHermesStore.getState().setSelectedAgentId('claude-02');
      useHermesStore.setState((state) => {
        const s1 = reduceHermesEvent(state, {
          kind: 'task',
          runId,
          taskId: 'task-03',
          status: 'dispatched',
          dispatchId: 'claude-02',
        });
        return reduceHermesEvent(s1, {
          kind: 'agent',
          runId,
          agentId: 'claude-02',
          taskId: 'task-03',
          status: 'working',
          activity: 'thinking',
        });
      });
    }, 16500);

    // T = 18.5s: claude-02 completes task-03, entire run completes
    pushTimeout(() => {
      useHermesStore.setState((state) => {
        const s1 = reduceHermesEvent(state, {
          kind: 'task',
          runId,
          taskId: 'task-03',
          status: 'completed',
          dispatchId: 'claude-02',
        });
        const s2 = reduceHermesEvent(s1, {
          kind: 'agent',
          runId,
          agentId: 'claude-02',
          taskId: null,
          status: 'done',
          activity: null,
        });
        return reduceHermesEvent(s2, {
          kind: 'run',
          runId,
          goal: goalStr,
          status: 'completed',
          error: null,
        });
      });
      setIsWalkthroughRunning(false);
    }, 18500);
  };

  const handleLaunch = async (mode: HermesLaunchMode) => {
    if (!canRequestHermesLaunch({
      goal,
      rosterCount: launchPicks.length,
      launching,
      workspaceLoading,
    })) return;

    const repoRoot = workspaceRoot ?? await handleProjectFolderSelect();
    if (!repoRoot) return;

    setLaunching(true);

    try {
      const normalized = buildLaunch(goal, { maxConcurrent }, launchPicks, repoRoot);
      const runId = mode === 'mock'
        ? await hermesService.runMock(normalized.goal, normalized.opts)
        : await hermesService.run(normalized.goal, normalized.opts);
      
      const store = useHermesStore.getState();
      store.initRun(runId, normalized.goal);
      void store.refreshSnapshotUntilHydrated();
      
      setGoal('');
      setLaunchError(null);
    } catch (error) {
      displayLaunchError(error);
    } finally {
      setLaunching(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      void handleLaunch('real');
    }
  };

  return (
    <div className="flex flex-col gap-4 rounded-xl border border-base-300 bg-base-100 p-4 shadow-sm">
      {/* Target input textarea */}
      <div className="flex flex-col gap-1">
        <textarea
          className="textarea textarea-bordered h-20 w-full text-xs leading-relaxed resize-none focus:outline-none focus:border-primary placeholder:text-base-content/40 bg-base-200/20"
          placeholder={t('helm.composer.goalPlaceholder', 'Enter goal for Hermes... / 输入 Hermes 运行目标...')}
          value={goal}
          onChange={(e) => setGoal(e.target.value)}
          onKeyDown={handleKeyDown}
          aria-label="Hermes goal input"
        />
        <div className="flex justify-between items-center text-[10px] text-base-content/50 px-1">
          <span>{t('helm.composer.metadataHelper', 'Press Ctrl+Enter to submit / 按 Ctrl+Enter 快速提交')}</span>
          <span>{goal.length} chars</span>
        </div>
      </div>

      {/* Roster & Fleet Selection list */}
      <RosterPanel selectedPicks={selectedPicks} onChange={setSelectedPicks} />

      {/* Options and buttons row */}
      <div className="flex flex-wrap items-center justify-between gap-3 border-t border-base-300/60 pt-3">
        <div className="flex min-w-0 items-center gap-2">
          <button
            type="button"
            onClick={() => void handleProjectFolderSelect()}
            disabled={workspaceLoading}
            className="btn btn-ghost btn-xs"
            title={t('helm.composer.selectProjectFolder')}
            aria-label={t('helm.composer.selectProjectFolder')}
          >
            {workspaceLoading ? <Loader2 className="h-3 w-3 animate-spin" /> : <FolderOpen className="h-3 w-3" />}
          </button>
          <span
            className="max-w-48 truncate text-xs text-base-content/60"
            title={workspaceRoot ?? t('helm.composer.noProjectFolder')}
          >
            {workspaceRoot ?? t('helm.composer.noProjectFolder')}
          </span>
        </div>

        {/* Max concurrent input */}
        <div className="flex items-center gap-2">
          <label className="text-xs font-medium text-base-content/70">
            {t('helm.composer.maxConcurrent', 'Max Concurrent Tasks / 最大并发任务数')}:
          </label>
          <input
            type="number"
            min={1}
            max={selectedPicks.length || 1}
            className="input input-bordered input-xs w-14 font-semibold text-center"
            value={maxConcurrent ?? ''}
            placeholder={selectedPicks.length > 0 ? String(selectedPicks.length) : '1'}
            onChange={(e) => {
              const val = parseInt(e.target.value, 10);
              setMaxConcurrent(isNaN(val) ? undefined : val);
            }}
          />
        </div>

        {/* Action Buttons */}
        <div className="flex items-center gap-2">
          {/* Walkthrough Demo Button */}
          <button
            type="button"
            onClick={isWalkthroughRunning ? clearAllTimeouts : startWalkthrough}
            className={`btn btn-xs rounded-lg gap-1 border transition-all duration-200 ${
              isWalkthroughRunning
                ? 'btn-error hover:bg-error/90 text-white'
                : 'btn-outline border-success text-success hover:bg-success hover:text-white'
            }`}
          >
            <Sparkles className="h-3 w-3" />
            <span>
              {isWalkthroughRunning
                ? t('helm.composer.walkthroughStop', 'Stop Walkthrough / 停止 Walkthrough')
                : t('helm.composer.walkthrough', 'Start Walkthrough Demo / 启动 Walkthrough 演示')}
            </span>
          </button>

          {/* Run Mock Button */}
          <div>
            <button
              type="button"
              onClick={() => void handleLaunch('mock')}
              disabled={!canRequestHermesLaunch({
                goal,
                rosterCount: launchPicks.length,
                launching,
                workspaceLoading,
              })}
              className="btn btn-outline btn-xs rounded-lg gap-1 border border-base-300 hover:border-primary hover:bg-primary/5 hover:text-primary disabled:bg-base-200/50 disabled:text-base-content/30 disabled:border-base-200"
            >
              <Play className="h-3 w-3" />
              <span>{t('helm.composer.runMock', 'Run Mock / 演示运行')}</span>
            </button>
          </div>

          {/* Dispatch Button */}
          <button
            type="button"
            onClick={() => void handleLaunch('real')}
            disabled={!canRequestHermesLaunch({
              goal,
              rosterCount: launchPicks.length,
              launching,
              workspaceLoading,
            })}
            className="btn btn-primary btn-xs rounded-lg gap-1 text-white shadow-sm font-semibold hover:shadow bg-gradient-to-r from-primary to-primary-focus border-none disabled:bg-base-200/50 disabled:text-base-content/30"
          >
            {launching ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <span>🚀</span>
            )}
            <span>
              {launching
                ? t('helm.composer.launching', 'Launching... / 启动中...')
                : t('helm.composer.dispatch', 'Dispatch to Hermes / 派发给 Hermes')}
            </span>
          </button>
        </div>
      </div>
      {launchError && <p role="alert" className="text-xs text-error">{launchError}</p>}
      {workspaceError && <p role="alert" className="text-xs text-error">{workspaceError}</p>}
    </div>
  );
}
