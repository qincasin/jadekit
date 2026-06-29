export const AGENT_STATUS = {
  WORKING: 'working',
  NEEDS_ATTENTION: 'needs-attention',
  DONE: 'done',
  INTERRUPTED: 'interrupted',
} as const;

export type AgentStatus = typeof AGENT_STATUS[keyof typeof AGENT_STATUS];

export const AGENT_ACTIVITY = {
  TOOL_USE: 'tool_use',
  TEXT: 'text',
  THINKING: 'thinking',
} as const;

export type AgentActivity = typeof AGENT_ACTIVITY[keyof typeof AGENT_ACTIVITY];

export const TASK_STATUS = {
  PENDING: 'pending',
  READY: 'ready',
  DISPATCHED: 'dispatched',
  COMPLETED: 'completed',
  FAILED: 'failed',
  BLOCKED: 'blocked',
} as const;

export type TaskStatus = typeof TASK_STATUS[keyof typeof TASK_STATUS];

export const DISPATCH_STATUS = {
  PENDING: 'pending',
  DISPATCHED: 'dispatched',
  COMPLETED: 'completed',
  FAILED: 'failed',
  CIRCUIT_BROKEN: 'circuit_broken',
} as const;

export type DispatchStatus = typeof DISPATCH_STATUS[keyof typeof DISPATCH_STATUS];

export const RUN_STATUS = {
  IDLE: 'idle',
  RUNNING: 'running',
  COMPLETED: 'completed',
  FAILED: 'failed',
  CANCELLED: 'cancelled',
} as const;

export type RunStatus = typeof RUN_STATUS[keyof typeof RUN_STATUS];

export interface TaskDto {
  id: string;
  parentId: string | null;
  spec: string;
  status: string;
  deps: string[];
  result: string | null;
  createdAt: string;
  completedAt: string | null;
}

export interface DispatchDto {
  id: string;
  taskId: string;
  assignee: string | null;
  status: string;
  failureCount: number;
  lastHeartbeatAt: string | null;
  lastFailure: string | null;
  dispatchedAt: string | null;
  completedAt: string | null;
  createdAt: string;
}

export interface RunShowDto {
  id: string;
  goal: string;
  status: string;
  createdAt: string;
  completedAt: string | null;
  taskCount: number;
  completedCount: number;
}

export interface SweepReportDto {
  removed: number;
  retained: number;
}

export interface HermesRunOpts {
  maxConcurrent?: number | null;
  pollIntervalMs?: number | null;
  repoRoot?: string | null;
}

export interface TaskListFilterDto {
  status?: string | null;
  ready?: boolean | null;
}

export type OrchestrationEvent =
  | { kind: 'run'; runId: string; goal: string; status: string; error: string | null }
  | { kind: 'task'; runId: string; taskId: string; status: string; dispatchId: string | null }
  | {
      kind: 'agent';
      runId: string;
      agentId: string;
      taskId: string | null;
      status: string;
      activity: string | null;
    };

export function isRunEvent(event: OrchestrationEvent): event is OrchestrationEvent & { kind: 'run' } {
  return event.kind === 'run';
}

export function isTaskEvent(event: OrchestrationEvent): event is OrchestrationEvent & { kind: 'task' } {
  return event.kind === 'task';
}

export function isAgentEvent(event: OrchestrationEvent): event is OrchestrationEvent & { kind: 'agent' } {
  return event.kind === 'agent';
}

export const HERMES_EVENT_CHANNELS = {
  RUN: 'hermes://run',
  TASK: 'hermes://task',
  AGENT: 'hermes://agent',
} as const;

export function getEventChannel(event: OrchestrationEvent): string {
  switch (event.kind) {
    case 'run':
      return HERMES_EVENT_CHANNELS.RUN;
    case 'task':
      return HERMES_EVENT_CHANNELS.TASK;
    case 'agent':
      return HERMES_EVENT_CHANNELS.AGENT;
  }
}

export interface JudgeVerdictDto {
  winnerIndex: number;
  scores: number[];
  reason: string;
  candidates: {
    index: number;
    agentId: string;
  }[];
}

export interface InterventionGateDto {
  id: string;
  taskId: string;
  question: string;
  options: string[];
  status: 'pending' | 'resolved' | 'timeout';
  resolution?: string | null;
  createdAt?: string;
  resolvedAt?: string | null;
}


