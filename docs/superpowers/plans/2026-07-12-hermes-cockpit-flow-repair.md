# Hermes Cockpit Flow Repair Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` task-by-task. Each task starts with a failing test and is reviewed before the next task.

**Goal:** Make a normal Helm cockpit run reliable from workspace selection through dispatch, permission handling, live status, worktree navigation, and terminal actions.

**Architecture:** The chat workspace remains the source for the selected repository, and its verified Git root is passed as `HermesRunOpts.repoRoot`. Hermes' active-dispatch snapshot is authoritative for cancellable workers, so refresh reconciles rather than merges stale active entries. Provider selection is carried end-to-end through the roster and `RuntimeStartSpec`, allowing the SDK bridge to call the selected `claude` or `codex` method with its model and worktree cwd.

**Tech Stack:** React, TypeScript, Zustand, Vitest, Tauri, Rust, Tokio.

## Global Constraints

- Default user-facing text is Chinese and matching English locale keys are added at the same time.
- Existing user changes in the dirty `feat/helm` workspace are preserved.
- No fake transcript, worktree, or judge data is shown in production.
- `hermes_agent_list` is the authoritative list of active dispatches; a worker not in that list cannot be stopped.
- A real run must use a verified Git workspace root; the app startup directory is never used as an implicit project directory.
- Tests are written and observed failing before implementation; run focused tests, then `npm test -- --run`, `npm run lint`, `npm run build`, Cargo tests/check, and `git diff --check` before completion.

---

### Task 1: Reconcile Active Dispatch Snapshots

**Files:**
- Modify: `src/stores/useHermesStore.ts`
- Modify: `src/stores/useHermesStore.test.ts`
- Modify: `src/components/helm/sessionHeaderActions.ts`
- Modify: `src/components/helm/sessionHeaderActions.test.ts`

**Outcome:** A refreshed empty active-dispatch result removes stale selectable agents, clears a stale selection, and disables stop unless the agent is represented by an active dispatch.

### Task 2: Bind Hermes Runs to a Verified Workspace

**Files:**
- Create: `src/components/helm/hermesWorkspace.ts`
- Create: `src/components/helm/hermesWorkspace.test.ts`
- Modify: `src/components/helm/HelmComposer.tsx`
- Modify: `src/components/helm/launchPlan.ts`
- Modify: `src/components/helm/launchPlan.test.ts`
- Modify: `src/locales/zh.json`
- Modify: `src/locales/en.json`

**Outcome:** The composer exposes a project-folder action, validates the chosen directory is a Git repository, stores its Git root in chat state, and passes that root to both real and mock Hermes runs.

### Task 3: Route Selected Providers Through the SDK Runtime

**Files:**
- Modify: `src/types/hermes.ts`
- Modify: `src/components/helm/launchPlan.ts`
- Modify: `src/components/helm/launchPlan.test.ts`
- Modify: `src-tauri/src/commands/hermes_commands.rs`
- Modify: `src-tauri/src/hermes/planner.rs`
- Modify: `src-tauri/src/hermes/coordinator.rs`
- Modify: `src-tauri/src/hermes/sdk_runtime.rs`
- Modify: `docs/helm-hermes-ui-contract.md`

**Outcome:** A Claude or Codex selection carries its provider token and model through roster assignment into `RuntimeStartSpec`; SDK calls use `<provider>.send` with `message`, `model`, `cwd`, and streaming enabled. Unsupported provider tokens are rejected at the command boundary before a run is created.

### Task 4: Exercise and Harden Terminal Actions

**Files:**
- Modify: `src/components/helm/SessionPanel.tsx`
- Create or modify focused tests beside the extracted action helpers
- Modify: `src/components/helm/FleetKanban.tsx` only if its cancellation path has the same stale-action race
- Modify: `docs/helm-hermes-ui-contract.md`

**Outcome:** Worktree navigation uses the verified workspace root, a just-finished agent is treated as no longer stoppable rather than surfaced as a technical failure, and the user sees a specific, localized outcome for each terminal action.

### Task 5: End-to-End Verification

**Files:**
- Modify only tests or documentation if verification exposes a real gap.

**Outcome:** Run the deterministic mock path through the real command/event/snapshot route, then perform one manual desktop run with a configured Git project and record any provider/permission failure in the cockpit instead of silently losing it.

### Task 6: Isolate Consecutive Hermes Runs and Preserve Failure Reasons

**Files:**
- Modify: `src-tauri/src/hermes/store.rs`
- Modify: `src-tauri/src/commands/hermes_commands.rs`
- Modify: `src-tauri/src/hermes/coordinator.rs`
- Modify: `src/stores/hermesReducer.ts`
- Modify: `src/components/helm/SessionPanel.tsx`
- Modify: `docs/helm-hermes-ui-contract.md`

**Outcome:** Starting a new run clears only terminal operational state from prior runs while preserving run history, refuses to overlap a still-running run, and sends a persisted root cause rather than the generic `run ended in failed state` when a terminal task failure makes a run fail.
