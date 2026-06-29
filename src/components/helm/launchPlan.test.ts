import { describe, it, expect } from 'vitest';
import { buildLaunch } from './launchPlan';

describe('buildLaunch', () => {
  it('should reject empty goal inputs', () => {
    expect(() => buildLaunch('', {}, ['agent-1'])).toThrow('Goal cannot be empty');
    expect(() => buildLaunch('   ', {}, ['agent-1'])).toThrow('Goal cannot be empty');
  });

  it('should reject empty selected picks', () => {
    expect(() => buildLaunch('deploy app', {}, [])).toThrow('At least one agent must be selected');
  });

  it('should validate and normalize maxConcurrent logic', () => {
    // Normal case: maxConcurrent is valid and within range
    const res1 = buildLaunch('deploy app', { maxConcurrent: 2 }, ['agent-1', 'agent-2', 'agent-3']);
    expect(res1.opts.maxConcurrent).toBe(2);
    expect(res1.goal).toBe('deploy app');

    // Default case: maxConcurrent is missing, defaults to selectedPicks.length
    const res2 = buildLaunch('deploy app', {}, ['agent-1', 'agent-2']);
    expect(res2.opts.maxConcurrent).toBe(2);

    // Clamping case: maxConcurrent is greater than selectedPicks, clamp to selectedPicks.length
    const res3 = buildLaunch('deploy app', { maxConcurrent: 5 }, ['agent-1', 'agent-2']);
    expect(res3.opts.maxConcurrent).toBe(2);

    // Error case: maxConcurrent <= 0
    expect(() => buildLaunch('deploy app', { maxConcurrent: 0 }, ['agent-1'])).toThrow('maxConcurrent must be a positive integer');
    expect(() => buildLaunch('deploy app', { maxConcurrent: -1 }, ['agent-1'])).toThrow('maxConcurrent must be a positive integer');

    // Error case: maxConcurrent is not an integer
    expect(() => buildLaunch('deploy app', { maxConcurrent: 2.5 }, ['agent-1', 'agent-2', 'agent-3'])).toThrow('maxConcurrent must be a positive integer');
  });
});
