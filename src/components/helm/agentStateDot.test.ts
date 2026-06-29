import { describe, it, expect } from 'vitest';
import { dotVisualFor } from './agentStateDot';

describe('dotVisualFor', () => {
  it('should map "working" status correctly', () => {
    expect(dotVisualFor('working')).toEqual({ kind: 'spinner', tone: 'working' });
  });

  it('should map "needs-attention" status correctly', () => {
    expect(dotVisualFor('needs-attention')).toEqual({ kind: 'dot', tone: 'amber' });
  });

  it('should map "done" status correctly', () => {
    expect(dotVisualFor('done')).toEqual({ kind: 'check', tone: 'emerald' });
  });

  it('should map "interrupted" status correctly', () => {
    expect(dotVisualFor('interrupted')).toEqual({ kind: 'square', tone: 'red' });
  });

  it('should map unknown status to fallback properties', () => {
    expect(dotVisualFor('unknown-status-here')).toEqual({ kind: 'dot', tone: 'amber' });
    expect(dotVisualFor(undefined as any)).toEqual({ kind: 'dot', tone: 'amber' });
  });
});
