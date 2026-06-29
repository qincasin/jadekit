import { HermesRunOpts } from '../../types/hermes';

/**
 * Normalizes parameters for starting a Hermes run.
 * Validates that the goal is not empty/whitespace and that at least one agent is selected.
 * Normalizes maxConcurrent: ensures it is a positive integer, defaults to the number of
 * selected picks if missing, and clamps to the selected picks count.
 */
export function buildLaunch(
  goal: string,
  opts: HermesRunOpts,
  selectedPicks: string[]
): { goal: string; opts: HermesRunOpts } {
  if (!goal || !goal.trim()) {
    throw new Error('Goal cannot be empty');
  }

  if (!selectedPicks || selectedPicks.length === 0) {
    throw new Error('At least one agent must be selected');
  }

  let maxConcurrent = opts.maxConcurrent;

  if (maxConcurrent !== undefined && maxConcurrent !== null) {
    if (typeof maxConcurrent !== 'number' || maxConcurrent <= 0 || !Number.isInteger(maxConcurrent)) {
      throw new Error('maxConcurrent must be a positive integer');
    }
    if (maxConcurrent > selectedPicks.length) {
      maxConcurrent = selectedPicks.length;
    }
  } else {
    maxConcurrent = selectedPicks.length;
  }

  return {
    goal: goal.trim(),
    opts: {
      ...opts,
      maxConcurrent,
    },
  };
}
