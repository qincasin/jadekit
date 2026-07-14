import { describe, it, expect } from 'vitest';
import { buildLaunch, HelmRosterPick } from './launchPlan';

const claudePick: HelmRosterPick = {
  providerId: 'claude-official',
  providerName: 'Claude 官方订阅',
  chatProvider: 'claude',
  model: 'claude-opus-4-8',
};

const codexPick: HelmRosterPick = {
  providerId: 'glm-luster',
  providerName: 'glm-luster',
  chatProvider: 'codex',
  model: 'glm-5.2',
};

const repoRoot = '/workspace/jadekit';

describe('buildLaunch', () => {
  it('should reject empty goal inputs', () => {
    expect(() => buildLaunch('', {}, [claudePick], repoRoot)).toThrow('Goal cannot be empty');
    expect(() => buildLaunch('   ', {}, [claudePick], repoRoot)).toThrow('Goal cannot be empty');
  });

  it('should reject empty selected picks', () => {
    expect(() => buildLaunch('deploy app', {}, [], repoRoot)).toThrow('At least one agent must be selected');
  });

  it('should validate and normalize maxConcurrent logic', () => {
    // Normal case: maxConcurrent is valid and within range
    const res1 = buildLaunch('deploy app', { maxConcurrent: 2 }, [claudePick, codexPick, { ...codexPick, providerId: 'codex-2' }], repoRoot);
    expect(res1.opts.maxConcurrent).toBe(2);
    expect(res1.goal).toBe('deploy app');

    // Default case: maxConcurrent is missing, defaults to selectedPicks.length
    const res2 = buildLaunch('deploy app', {}, [claudePick, codexPick], repoRoot);
    expect(res2.opts.maxConcurrent).toBe(2);

    // Clamping case: maxConcurrent is greater than selectedPicks, clamp to selectedPicks.length
    const res3 = buildLaunch('deploy app', { maxConcurrent: 5 }, [claudePick, codexPick], repoRoot);
    expect(res3.opts.maxConcurrent).toBe(2);

    // Error case: maxConcurrent <= 0
    expect(() => buildLaunch('deploy app', { maxConcurrent: 0 }, [claudePick], repoRoot)).toThrow('maxConcurrent must be a positive integer');
    expect(() => buildLaunch('deploy app', { maxConcurrent: -1 }, [claudePick], repoRoot)).toThrow('maxConcurrent must be a positive integer');

    // Error case: maxConcurrent is not an integer
    expect(() => buildLaunch('deploy app', { maxConcurrent: 2.5 }, [claudePick, codexPick], repoRoot)).toThrow('maxConcurrent must be a positive integer');
  });

  it('should pass selected roster entries to Hermes run options', () => {
    const res = buildLaunch('deploy app', {}, [claudePick, codexPick], repoRoot);

    expect(res.opts.roster).toEqual([
      {
        runtime: 'sdk',
        provider: 'claude',
        model: 'claude-opus-4-8',
        label: 'Claude 官方订阅',
        costHint: 'mid',
      },
      {
        runtime: 'sdk',
        provider: 'codex',
        model: 'glm-5.2',
        label: 'glm-luster',
        costHint: 'low',
      },
    ]);
  });

  it('should include the normalized repository root in Hermes run options', () => {
    const res = buildLaunch('deploy app', {}, [claudePick], '  /workspace/jadekit  ');

    expect(res.opts.repoRoot).toBe('/workspace/jadekit');
  });

  it('should reject a missing repository root', () => {
    expect(() => buildLaunch('deploy app', {}, [claudePick], '  '))
      .toThrow('A Git repository root is required');
  });
});
