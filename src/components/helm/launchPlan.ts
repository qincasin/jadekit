import { HermesRosterEntry, HermesRunOpts } from '../../types/hermes';

export interface HelmRosterPick {
  providerId: string;
  providerName: string;
  chatProvider: 'claude' | 'codex';
  model: string;
}

function rosterEntryFromPick(pick: HelmRosterPick): HermesRosterEntry {
  return {
    runtime: 'sdk',
    provider: pick.chatProvider,
    model: pick.model,
    label: pick.providerName,
    costHint: pick.chatProvider === 'claude' ? 'mid' : 'low',
  };
}

/**
 * Normalizes parameters for starting a Hermes run.
 * Validates that the goal is not empty/whitespace and that at least one agent is selected.
 * Normalizes maxConcurrent: ensures it is a positive integer, defaults to the number of
 * selected picks if missing, and clamps to the selected picks count.
 */
export function buildLaunch(
  goal: string,
  opts: HermesRunOpts,
  selectedPicks: HelmRosterPick[],
  repoRoot: string,
): { goal: string; opts: HermesRunOpts } {
  if (!goal || !goal.trim()) {
    throw new Error('Goal cannot be empty');
  }

  if (!selectedPicks || selectedPicks.length === 0) {
    throw new Error('At least one agent must be selected');
  }

  const normalizedRepoRoot = repoRoot.trim();
  if (!normalizedRepoRoot) {
    throw new Error('A Git repository root is required');
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
      repoRoot: normalizedRepoRoot,
      roster: selectedPicks.map(rosterEntryFromPick),
    },
  };
}
