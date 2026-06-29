import React, { useState, useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { useHermesStore } from '../../stores/useHermesStore';
import { buildLaunch } from './launchPlan';
import RosterPanel from './RosterPanel';
import * as hermesService from '../../services/hermesService';
import { reduceHermesEvent } from '../../stores/hermesReducer';
import { Loader2, Play, Sparkles } from 'lucide-react';

export default function HelmComposer() {
  const { t } = useTranslation();
  const [goal, setGoal] = useState('');
  const [selectedPicks, setSelectedPicks] = useState<string[]>([]);
  const [maxConcurrent, setMaxConcurrent] = useState<number | undefined>(undefined);
  const [launching, setLaunching] = useState(false);
  
  // Mock support status: null = checking, true = supported, false = unsupported
  const [isMockSupported, setIsMockSupported] = useState<boolean | null>(null);

  // Walkthrough simulation state
  const [isWalkthroughRunning, setIsWalkthroughRunning] = useState(false);
  const timeoutsRef = useRef<number[]>([]);

  // Check if hermes_run_mock is supported on mount
  useEffect(() => {
    const checkMockSupport = async () => {
      try {
        await hermesService.runMock('', { maxConcurrent: 1 });
        setIsMockSupported(true);
      } catch (err: any) {
        const errMsg = String(err).toLowerCase();
        if (errMsg.includes('not found') || errMsg.includes('missing') || errMsg.includes('unknown command')) {
          setIsMockSupported(false);
        } else {
          // If it throws a validation error like empty goal, the command exists!
          setIsMockSupported(true);
        }
      }
    };
    void checkMockSupport();

    // Subscribe to events on mount
    let unsubscribe: (() => void) | undefined;
    const subscribe = async () => {
      unsubscribe = await useHermesStore.getState().subscribeEvents();
    };
    void subscribe();

    return () => {
      if (unsubscribe) unsubscribe();
      // Clear timeouts on unmount
      timeoutsRef.current.forEach((timeoutId) => clearTimeout(timeoutId));
    };
  }, []);

  const clearAllTimeouts = () => {
    timeoutsRef.current.forEach((timeoutId) => clearTimeout(timeoutId));
    timeoutsRef.current = [];
    setIsWalkthroughRunning(false);
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

  const handleLaunch = async () => {
    if (!goal.trim() || selectedPicks.length === 0 || launching) return;
    setLaunching(true);

    try {
      const normalized = buildLaunch(goal, { maxConcurrent }, selectedPicks);
      const runId = await hermesService.run(normalized.goal, normalized.opts);
      
      // Save state using initRun
      useHermesStore.getState().initRun(runId, normalized.goal);
      
      setGoal('');
    } catch (err: any) {
      console.error('Launch failed:', err);
      alert(err.message || 'Failed to dispatch run to Hermes.');
    } finally {
      setLaunching(false);
    }
  };

  const handleRunMock = async () => {
    if (!goal.trim() || selectedPicks.length === 0 || launching) return;
    setLaunching(true);

    try {
      const normalized = buildLaunch(goal, { maxConcurrent }, selectedPicks);
      const runId = await hermesService.runMock(normalized.goal, normalized.opts);
      
      useHermesStore.getState().initRun(runId, normalized.goal);
      
      setGoal('');
    } catch (err: any) {
      console.error('Mock run failed:', err);
      alert(err.message || 'Failed to trigger mock run.');
    } finally {
      setLaunching(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      void handleLaunch();
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
          <div
            className={isMockSupported === false ? 'tooltip tooltip-top' : ''}
            data-tip={t('helm.tooltips.requiresPhase35', 'Requires Phase 3.5 Engine / 需引擎 Phase 3.5')}
          >
            <button
              type="button"
              onClick={handleRunMock}
              disabled={launching || !goal.trim() || selectedPicks.length === 0 || isMockSupported === false}
              className="btn btn-outline btn-xs rounded-lg gap-1 border border-base-300 hover:border-primary hover:bg-primary/5 hover:text-primary disabled:bg-base-200/50 disabled:text-base-content/30 disabled:border-base-200"
            >
              <Play className="h-3 w-3" />
              <span>{t('helm.composer.runMock', 'Run Mock / 演示运行')}</span>
            </button>
          </div>

          {/* Dispatch Button */}
          <button
            type="button"
            onClick={handleLaunch}
            disabled={launching || !goal.trim() || selectedPicks.length === 0}
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
    </div>
  );
}
