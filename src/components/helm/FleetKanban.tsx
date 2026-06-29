import React, { useState, useMemo } from 'react';
import { useHermesStore } from '../../stores/useHermesStore';
import { laneFor } from './kanbanLanes';
import { WorkerCard } from './WorkerCard';
import { TaskState, AgentState } from '../../stores/hermesReducer';
import { Terminal, Cpu } from 'lucide-react';
import { cn } from '../../utils/cn';

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
        <div className="w-[280px] flex-shrink-0 flex flex-col bg-base-200/35 rounded-xl p-3 border border-base-300/40 snap-align-start h-full overflow-hidden">
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
                />
              )}
            />
          </div>
        </div>

        {/* Running Lane */}
        <div className="w-[280px] flex-shrink-0 flex flex-col bg-base-200/35 rounded-xl p-3 border border-base-300/40 snap-align-start h-full overflow-hidden">
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
                />
              )}
            />
          </div>
        </div>

        {/* Review Lane */}
        <div className="w-[280px] flex-shrink-0 flex flex-col bg-base-200/35 rounded-xl p-3 border border-base-300/40 snap-align-start h-full overflow-hidden">
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
                />
              )}
            />
          </div>
        </div>

        {/* Completed Lane */}
        <div className="w-[280px] flex-shrink-0 flex flex-col bg-base-200/35 rounded-xl p-3 border border-base-300/40 snap-align-start h-full overflow-hidden">
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
                />
              )}
            />
          </div>
        </div>
      </div>
    </div>
  );
};

export default FleetKanban;
