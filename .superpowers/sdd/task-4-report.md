# Task 4 Report

## Summary

- Added centralized frontend official provider constants matching backend ids:
  `__claude_official__` and `__codex_official__`.
- Added display-only official providers for Claude and Codex, prepended to provider lists.
- Official providers are inferred active when no custom provider for the same app is active.
- Official items keep the existing switch flow, while edit/delete/clone/drag actions are hidden.
- Added official badge and official-switch toast copy in zh/en locales.

## Verification

- RED: `npx tsc --noEmit` failed after wiring `mergeOfficialProviders` into the store before `src/config/providerConstants.ts` existed.
- GREEN/Gate: `npx tsc --noEmit` passed after implementation.

## Manual Reasoning

- Activating an official item calls the existing `switchProvider(provider.appType, provider.id)` flow.
- The official ids match the backend constants exactly, so Task 3b routes them through the cleanup branch.
- Custom providers remain editable/deletable/sortable; official synthetic providers do not call DB-only edit/delete/move flows.
