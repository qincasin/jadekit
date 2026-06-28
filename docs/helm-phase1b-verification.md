# Helm x Hermes Phase 1b Verification

Date: 2026-06-28
Branch: `feat/helm-phase1b-fanout`

## Scope

Phase 1b adds heterogeneous fan-out on top of Phase 0/1 primitives:

- `diff_summary` counts untracked files.
- Worktree winner branch can be merged into the current repo with conflict abort.
- Frontend can build a fan-out plan, create one worktree per selected agent, send the same prompt, compare outputs, merge a winner, and discard agents after confirmation.
- LLM-judge was intentionally deferred as optional stretch for Phase 2/Planner.

## Automatic Gates

Run before handoff:

```bash
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml chat
npx vitest run src/stores/fanoutPlan.test.ts src/stores/fanoutGroup.test.ts src/components/chat/fanout/roster.test.ts src/components/chat/fanout/compare.test.ts src/stores/chatSendCwd.test.ts src/components/chat/worktreeBadge.test.ts
npm run build
git diff --check
```

## Manual E2E

Status: 待人工执行

Steps:

1. Open Chat on a Git repository.
2. Enable Fan-out, choose 2-3 Claude/Codex provider/model entries, and submit the same prompt.
3. Verify `git worktree list` shows multiple `helm/fanout-*` worktrees.
4. Verify each compare column streams only its own agent output.
5. Verify each agent tab shows the correct branch/diff badge after changes.
6. Pick one winner and merge it; confirm the main repo git log contains the winner change.
7. Create a merge conflict case; verify the merge reports conflict and the main repo is clean after automatic abort.
8. Discard remaining agents only after confirmation; verify worktrees are removed.

Manual observations are not filled here because no desktop app E2E run was performed in this session.
