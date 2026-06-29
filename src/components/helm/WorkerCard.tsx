import React from 'react';
import { Terminal, MoreVertical, CircleSlash, Trash2, GitMerge } from 'lucide-react';
import { HermesAgentStateDot } from './HermesAgentStateDot';
import { TaskState, AgentState } from '../../stores/hermesReducer';
import { laneFor } from './kanbanLanes';
import { cn } from '../../utils/cn';

export interface WorkerCardProps {
  task: TaskState;
  agent?: AgentState;
  onClick?: () => void;
  draggable?: boolean;
  onDragStart?: (e: React.DragEvent) => void;
  onAction?: (action: 'cancel' | 'confirm-discard' | 'confirm-merge') => void;
}

export const WorkerCard: React.FC<WorkerCardProps> = ({
  task,
  agent,
  onClick,
  draggable,
  onDragStart,
  onAction,
}) => {
  const isFailed = task.status === 'failed';
  const assignee = agent?.assignee || 'Planner';
  const failureCount = agent?.failureCount;
  const lane = laneFor(task);

  // Handle keyboard interaction (Enter / Space)
  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (onClick && (e.key === 'Enter' || e.key === ' ')) {
      e.preventDefault();
      onClick();
    }
  };

  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onClick}
      onKeyDown={handleKeyDown}
      draggable={draggable}
      onDragStart={onDragStart}
      className={cn(
        "group relative flex flex-col p-4 rounded-xl border bg-base-100 transition-all duration-200 select-none cursor-pointer text-left focus:outline-none focus-visible:ring-2 focus-visible:ring-primary focus-visible:ring-offset-2",
        isFailed
          ? "border-red-300 dark:border-red-900 bg-red-50/30 dark:bg-red-950/10 shadow-[0_0_8px_rgba(239,68,68,0.1)]"
          : "border-base-300 hover:border-base-400 hover:shadow-md dark:hover:border-base-content/20"
      )}
      aria-label={`Task ${task.id}: ${task.spec}`}
    >
      {/* Top right dropdown menu */}
      {(lane === 'running' || lane === 'review') && onAction && (
        <div
          className="dropdown dropdown-end absolute top-3 right-3 z-10"
          onClick={(e) => e.stopPropagation()}
          onKeyDown={(e) => e.stopPropagation()}
        >
          <label
            tabIndex={0}
            role="button"
            className="btn btn-ghost btn-xs btn-circle text-base-content/40 hover:text-base-content/70 hover:bg-base-200/60 focus:outline-none focus-visible:ring-1 focus-visible:ring-primary"
            aria-label="Task Action Menu"
          >
            <MoreVertical className="h-4 w-4" />
          </label>
          <ul
            tabIndex={0}
            className="dropdown-content menu p-1 shadow-lg bg-base-100 rounded-lg w-36 text-[11px] font-semibold border border-base-300/80 z-20"
          >
            {lane === 'running' && (
              <>
                <li>
                  <button
                    type="button"
                    onClick={() => onAction('cancel')}
                    className="flex items-center gap-1.5 py-1.5 px-2 hover:bg-base-200 rounded text-base-content/80 focus:outline-none w-full text-left"
                  >
                    <CircleSlash className="h-3.5 w-3.5 text-warning" />
                    <span>取消 / Cancel</span>
                  </button>
                </li>
                <li>
                  <button
                    type="button"
                    onClick={() => onAction('confirm-discard')}
                    className="flex items-center gap-1.5 py-1.5 px-2 hover:bg-red-50 hover:text-red-600 dark:hover:bg-red-950/20 dark:hover:text-red-400 rounded text-base-content/85 focus:outline-none w-full text-left"
                  >
                    <Trash2 className="h-3.5 w-3.5 text-red-500" />
                    <span>丢弃 / Discard</span>
                  </button>
                </li>
              </>
            )}
            {lane === 'review' && (
              <>
                <li>
                  <button
                    type="button"
                    onClick={() => onAction('confirm-merge')}
                    className="flex items-center gap-1.5 py-1.5 px-2 hover:bg-base-200 rounded text-base-content/80 focus:outline-none w-full text-left"
                  >
                    <GitMerge className="h-3.5 w-3.5 text-primary" />
                    <span>合并 / Merge</span>
                  </button>
                </li>
                <li>
                  <button
                    type="button"
                    onClick={() => onAction('confirm-discard')}
                    className="flex items-center gap-1.5 py-1.5 px-2 hover:bg-red-50 hover:text-red-600 dark:hover:bg-red-950/20 dark:hover:text-red-400 rounded text-base-content/85 focus:outline-none w-full text-left"
                  >
                    <Trash2 className="h-3.5 w-3.5 text-red-500" />
                    <span>丢弃 / Discard</span>
                  </button>
                </li>
              </>
            )}
          </ul>
        </div>
      )}

      {/* Top row: Terminal icon, Status dot, assignee, failure details */}
      <div className={cn("flex items-center justify-between gap-2 mb-2.5", (lane === 'running' || lane === 'review') && onAction && "pr-6")}>
        <div className="flex items-center gap-2">
          <Terminal className="h-4 w-4 text-base-content/60" />
          <HermesAgentStateDot status={agent?.status || task.status} className="flex-shrink-0" />
          <span className="text-[10px] font-mono tracking-wider text-base-content/50 uppercase">
            ID: {task.id}
          </span>
        </div>
        <div className="flex items-center gap-1.5">
          {/* Assignee Badge */}
          <span className="px-2 py-0.5 rounded text-[10px] font-mono font-medium bg-base-200 text-base-content/85 border border-base-300">
            {assignee}
          </span>
          {/* Failed Highlight Badge */}
          {isFailed && (
            <span className="px-2 py-0.5 rounded text-[10px] font-semibold bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400 border border-red-200 dark:border-red-900/50">
              Failed {failureCount !== undefined && failureCount > 0 ? `(${failureCount})` : ''}
            </span>
          )}
        </div>
      </div>

      {/* Git branch name */}
      <div className="mb-2">
        <span className="font-mono text-xs px-2 py-0.5 rounded bg-base-200/55 text-primary/95 dark:text-primary-focus border border-base-300/40 select-text">
          helm/{task.id}
        </span>
      </div>

      {/* Task spec text */}
      <p className="text-xs text-base-content/80 line-clamp-3 leading-relaxed break-words font-medium">
        {task.spec || 'No specification provided'}
      </p>
    </div>
  );
};

export default WorkerCard;

