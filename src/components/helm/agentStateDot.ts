import { AgentStatus } from '../../types/hermes';

export type AgentStateKind = 'spinner' | 'dot' | 'check' | 'square';
export type AgentStateTone = 'working' | 'amber' | 'emerald' | 'red';

export interface AgentStateVisual {
  kind: AgentStateKind;
  tone: AgentStateTone;
}

/**
 * Pure mapper function mapping status token to visual properties.
 * 
 * "working" -> spinner / working
 * "needs-attention" -> dot / amber
 * "done" -> check / emerald
 * "interrupted" -> square / red
 * Any unknown status -> dot / amber (fallback)
 */
export function dotVisualFor(status: AgentStatus | string): AgentStateVisual {
  switch (status) {
    case 'working':
      return { kind: 'spinner', tone: 'working' };
    case 'needs-attention':
      return { kind: 'dot', tone: 'amber' };
    case 'done':
      return { kind: 'check', tone: 'emerald' };
    case 'interrupted':
      return { kind: 'square', tone: 'red' };
    default:
      return { kind: 'dot', tone: 'amber' };
  }
}
