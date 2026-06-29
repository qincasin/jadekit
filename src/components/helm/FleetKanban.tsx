import React, { useState, useMemo } from 'react';
import { useHermesStore } from '../../stores/useHermesStore';
import { useChatStore } from '../../stores/useChatStore';
import { laneFor } from './kanbanLanes';
import { WorkerCard } from './WorkerCard';
import { TaskState, AgentState } from '../../stores/hermesReducer';
import { Terminal, Cpu } from 'lucide-react';
import { cn } from '../../utils/cn';
import { dropActionFor, Lane } from './kanbanDrag';
import {
  closeAgent,
  removeWorktree,
  listWorktrees,
  mergeWorktree,
} from '../../services/worktreeService';

// Custom lightweight VirtualList implementation
interface VirtualListProps<T> {
  items: T[];
  itemHeight: number;
  renderItem: (item: T, index: number) => React.ReactNode;
  className?: string;
  virtualized?: boolean;
}

export function VirtualList<T>({
  items,
  itemHeight,
  renderItem,
  className,
  virtualized = false,
}: VirtualListProps<T>) {
  const containerRef = React.useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [containerHeight, setContainerHeight] = useState(300);

  React.useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const handleScroll = () => {
      setScrollTop(el.scrollTop);
    };
    el.addEventListener('scroll', handleScroll);

    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        setContainerHeight(entry.contentRect.height);
      }
    });
    observer.observe(el);

    return () => {
      el.removeEventListener('scroll', handleScroll);
      observer.disconnect();
    };
  }, []);

  if (!virtualized) {
    return (
      <div
        ref={containerRef}
        className={cn("overflow-y-auto space-y-2.5 p-1 h-full pr-1.5 scrollbar-thin", className)}
      >
        {items.map((item, index) => renderItem(item, index))}
      </div>
    );
  }

  const totalHeight = items.length * itemHeight;
  const startIndex = Math.max(0, Math.floor(scrollTop / itemHeight) - 2);
  const endIndex = Math.min(
    items.length - 1,
    Math.floor((scrollTop + containerHeight) / itemHeight) + 2
  );
  const visibleItems = items.slice(startIndex, endIndex + 1);
  const offsetY = startIndex * itemHeight;

  return (
    <div
      ref={containerRef}
      className={cn("overflow-y-auto relative h-full pr-1.5 scrollbar-thin", className)}
    >
      {items.length === 0 ? null : (
        <div style={{ height: totalHeight, width: '100%', position: 'relative' }}>
          <div
            style={{
              transform: `translateY(${offsetY}px)`,
              position: 'absolute',
              left: 0,
              right: 0,
              top: 0,
              padding: '4px',
            }}
            className="space-y-2.5"
          >
            {visibleItems.map((item, index) => renderItem(item, startIndex + index))}
          </div>
        </div>
      )}
    </div>
  );
}

export const FleetKanban: React.FC = () => {
  const tasks = useHermesStore((state) => state.tasks);
  const agents = useHermesStore((state) => state.agents);
  const [virtualized, setVirtualized] = useState(false);

  const currentCwd = useChatStore((state) => state.currentCwd);
  const [draggedTask, setDraggedTask] = useState<TaskState | null>(null);
  const [draggedFromLane, setDraggedFromLane] = useState<Lane | null>(null);

  const [modalConfig, setModalConfig] = useState<{
    isOpen: boolean;
    title: string;
    message: string;
    onConfirm: () => Promise<void> | void;
  }>({
    isOpen: false,
    title: '',
    message: '',
    onConfirm: () => {},
  });

  const taskList = useMemo(() => Object.values(tasks), [tasks]);
  const agentList = useMemo(() => Object.values(agents), [agents]);

  // Group tasks into lanes
  const lanes = useMemo(() => {
    const grouped = {
      pending: [] as TaskState[],
      running: [] as TaskState[],
      review: [] as TaskState[],
      done: [] as TaskState[],
    };

    for (const task of taskList) {
      const lane = laneFor(task);
      grouped[lane].push(task);
    }

    return grouped;
  }, [taskList]);

  if (taskList.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center text-center p-8 border-2 border-dashed border-base-300 rounded-2xl bg-base-200/20 my-auto mx-4 space-y-3">
        <div className="p-3 bg-base-200 rounded-full text-base-content/40">
          <Terminal className="h-6 w-6 animate-pulse" />
        </div>
        <div className="space-y-1">
          <p className="text-sm font-semibold text-base-content/85">
            还没有 agent，下达一个目标开始
          </p>
          <p className="text-xs text-base-content/50">
            No agents running yet. Dispatch a goal to begin.
          </p>
        </div>
      </div>
    );
  }

  // Find agent associated with task
  const getAgentForTask = (taskId: string): AgentState | undefined => {
    return agentList.find((a) => a.taskId === taskId);
  };

  const handleDragStart = (task: TaskState, fromLane: Lane) => {
    setDraggedTask(task);
    setDraggedFromLane(fromLane);
  };

  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
  };

  const handleDrop = (e: React.DragEvent, toLane: Lane) => {
    e.preventDefault();
    if (!draggedTask || !draggedFromLane) return;
    const task = draggedTask;
    const fromLane = draggedFromLane;

    setDraggedTask(null);
    setDraggedFromLane(null);

    const action = dropActionFor(fromLane, toLane, task);
    if (action === 'none') return;

    handleAction(action, task);
  };

  const handleAction = (
    action: 'cancel' | 'confirm-discard' | 'confirm-merge',
    task: TaskState
  ) => {
    const agent = getAgentForTask(task.id);

    if (action === 'cancel') {
      if (!agent) return;
      setModalConfig({
        isOpen: true,
        title: '取消任务 / Cancel Task',
        message: `确定要取消正在执行的任务吗？(ID: ${task.id})\nAre you sure you want to cancel the running task?`,
        onConfirm: async () => {
          try {
            await closeAgent({ agentId: agent.id, removeWorktree: false });
          } catch (err) {
            console.error('Failed to cancel agent:', err);
          }
        },
      });
    } else if (action === 'confirm-discard') {
      setModalConfig({
        isOpen: true,
        title: '丢弃任务 / Discard Task',
        message: `确定要丢弃此任务并删除其工作树吗？此操作无法撤销。(ID: ${task.id})\nAre you sure you want to discard this task and delete its worktree? This action cannot be undone.`,
        onConfirm: async () => {
          try {
            if (!currentCwd) return;
            const worktrees = await listWorktrees(currentCwd).catch(() => []);
            const targetwt = worktrees.find(
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
          } catch (err) {
            console.error('Failed to discard worktree:', err);
          }
        },
      });
    } else if (action === 'confirm-merge') {
      setModalConfig({
        isOpen: true,
        title: '合并任务 / Merge Task',
        message: `确定要合并分支 helm/${task.id} 的修改到主分支吗？(ID: ${task.id})\nAre you sure you want to merge modifications from branch helm/${task.id}?`,
        onConfirm: async () => {
          try {
            if (!currentCwd) return;
            const outcomeDto = await mergeWorktree(currentCwd, `helm/${task.id}`);
            if (outcomeDto.outcome === 'conflict') {
              window.alert(
                '合并冲突，已自动回滚，请手动查看差异\nMerge conflict detected. The merge was aborted.'
              );
            } else {
              const worktrees = await listWorktrees(currentCwd).catch(() => []);
              const targetwt = worktrees.find(
                (wt) => wt.branch === `helm/${task.id}` || wt.branch.endsWith(`helm/${task.id}`)
              );
              if (targetwt?.path) {
                await removeWorktree(currentCwd, targetwt.path, true);
              }
            }
          } catch (err) {
            console.error('Failed to merge worktree:', err);
          }
        },
      });
    }
  };

  return (
    <div className="flex flex-col h-full overflow-hidden">
      {/* Kanban Header with Virtualization Toggle */}
      <div className="flex items-center justify-between px-4 py-2 border-b border-base-300/50 bg-base-200/40 text-xs">
        <span className="flex items-center gap-1.5 font-semibold text-base-content/70">
          <Cpu className="h-3.5 w-3.5" />
          KANBAN VIEW
        </span>
        <label className="flex items-center gap-1.5 cursor-pointer select-none">
          <input
            type="checkbox"
            className="checkbox checkbox-xs checkbox-primary focus-visible:ring-1 focus-visible:ring-primary focus-visible:outline-none"
            checked={virtualized}
            onChange={(e) => setVirtualized(e.target.checked)}
          />
          <span className="text-base-content/60 font-medium">Virtualize lists</span>
        </label>
      </div>

      {/* Columns Container */}
      <div className="flex flex-row gap-3.5 overflow-x-auto p-4 flex-1 select-none snap-x scrollbar-thin">
        {/* Pending Lane */}
        <div
          onDragOver={handleDragOver}
          onDrop={(e) => handleDrop(e, 'pending')}
          className="w-[280px] flex-shrink-0 flex flex-col bg-base-200/35 rounded-xl p-3 border border-base-300/40 snap-align-start h-full overflow-hidden"
        >
          <div className="flex items-center justify-between mb-3 px-1">
            <span className="text-xs font-bold tracking-wide text-base-content/75">
              待派 / Pending
            </span>
            <span className="badge badge-sm badge-ghost font-mono text-[10px] font-bold">
              {lanes.pending.length}
            </span>
          </div>
          <div className="flex-1 overflow-hidden min-h-0">
            <VirtualList
              items={lanes.pending}
              itemHeight={118}
              virtualized={virtualized}
              renderItem={(task) => (
                <WorkerCard
                  key={task.id}
                  task={task}
                  agent={getAgentForTask(task.id)}
                  draggable
                  onDragStart={() => handleDragStart(task, 'pending')}
                  onAction={(action) => handleAction(action, task)}
                />
              )}
            />
          </div>
        </div>

        {/* Running Lane */}
        <div
          onDragOver={handleDragOver}
          onDrop={(e) => handleDrop(e, 'running')}
          className="w-[280px] flex-shrink-0 flex flex-col bg-base-200/35 rounded-xl p-3 border border-base-300/40 snap-align-start h-full overflow-hidden"
        >
          <div className="flex items-center justify-between mb-3 px-1">
            <span className="text-xs font-bold tracking-wide text-base-content/75">
              执行中 / Running
            </span>
            <span className="badge badge-sm badge-primary font-mono text-[10px] font-bold text-primary-content">
              {lanes.running.length}
            </span>
          </div>
          <div className="flex-1 overflow-hidden min-h-0">
            <VirtualList
              items={lanes.running}
              itemHeight={118}
              virtualized={virtualized}
              renderItem={(task) => (
                <WorkerCard
                  key={task.id}
                  task={task}
                  agent={getAgentForTask(task.id)}
                  draggable
                  onDragStart={() => handleDragStart(task, 'running')}
                  onAction={(action) => handleAction(action, task)}
                />
              )}
            />
          </div>
        </div>

        {/* Review Lane */}
        <div
          onDragOver={handleDragOver}
          onDrop={(e) => handleDrop(e, 'review')}
          className="w-[280px] flex-shrink-0 flex flex-col bg-base-200/35 rounded-xl p-3 border border-base-300/40 snap-align-start h-full overflow-hidden"
        >
          <div className="flex items-center justify-between mb-3 px-1">
            <span className="text-xs font-bold tracking-wide text-base-content/75">
              待评审 / Review
            </span>
            <span className="badge badge-sm badge-secondary font-mono text-[10px] font-bold text-secondary-content">
              {lanes.review.length}
            </span>
          </div>
          <div className="flex-1 overflow-hidden min-h-0">
            <VirtualList
              items={lanes.review}
              itemHeight={118}
              virtualized={virtualized}
              renderItem={(task) => (
                <WorkerCard
                  key={task.id}
                  task={task}
                  agent={getAgentForTask(task.id)}
                  draggable
                  onDragStart={() => handleDragStart(task, 'review')}
                  onAction={(action) => handleAction(action, task)}
                />
              )}
            />
          </div>
        </div>

        {/* Completed Lane */}
        <div
          onDragOver={handleDragOver}
          onDrop={(e) => handleDrop(e, 'done')}
          className="w-[280px] flex-shrink-0 flex flex-col bg-base-200/35 rounded-xl p-3 border border-base-300/40 snap-align-start h-full overflow-hidden"
        >
          <div className="flex items-center justify-between mb-3 px-1">
            <span className="text-xs font-bold tracking-wide text-base-content/75">
              已完成 / Completed
            </span>
            <span className="badge badge-sm badge-accent font-mono text-[10px] font-bold text-accent-content">
              {lanes.done.length}
            </span>
          </div>
          <div className="flex-1 overflow-hidden min-h-0">
            <VirtualList
              items={lanes.done}
              itemHeight={118}
              virtualized={virtualized}
              renderItem={(task) => (
                <WorkerCard
                  key={task.id}
                  task={task}
                  agent={getAgentForTask(task.id)}
                  draggable
                  onDragStart={() => handleDragStart(task, 'done')}
                  onAction={(action) => handleAction(action, task)}
                />
              )}
            />
          </div>
        </div>
      </div>

      {/* Confirmation Modal */}
      {modalConfig.isOpen && (
        <div className="modal modal-open z-50">
          <div className="modal-box bg-base-100 border border-base-300 shadow-xl max-w-sm rounded-xl">
            <h3 className="font-bold text-sm text-base-content/90 tracking-wide uppercase mb-2">
              {modalConfig.title}
            </h3>
            <p className="text-xs text-base-content/70 whitespace-pre-line leading-relaxed mb-6">
              {modalConfig.message}
            </p>
            <div className="modal-action gap-2 animate-none">
              <button
                type="button"
                className="btn btn-ghost btn-sm text-xs font-semibold focus:outline-none rounded-lg"
                onClick={() => setModalConfig((prev) => ({ ...prev, isOpen: false }))}
              >
                取消 / Cancel
              </button>
              <button
                type="button"
                className="btn btn-primary btn-sm text-xs font-semibold focus:outline-none rounded-lg"
                onClick={async () => {
                  setModalConfig((prev) => ({ ...prev, isOpen: false }));
                  await modalConfig.onConfirm();
                }}
              >
                确认 / Confirm
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

export default FleetKanban;

