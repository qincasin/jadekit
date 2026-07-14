# Repository Guidelines

## Project Structure & Module Organization
This repository is a Tauri desktop app with a React + TypeScript frontend.

- `src/`: frontend application code.
- `src/pages/`: route-level pages.
- `src/components/`: reusable UI (`common/`, `layout/`, `mcp/`).
- `src/services/`: frontend service layer for Tauri command calls.
- `src/stores/`: Zustand state stores (pattern: `useXStore.ts`).
- `src/types/`: shared TypeScript types.
- `src/locales/`: i18n resources (`en.json`, `zh.json`).
- `src-tauri/`: Rust backend (`models/`, `services/`, `utils/`, command wiring in `lib.rs`).
- `public/`: static files. `dist/` and `src-tauri/target/` are generated outputs; do not edit manually.

## Code Exploration (prefer CodeGraph over blind grep)
This repo ships a **CodeGraph** index at `.codegraph/codegraph.db` (gitignored — do not commit). When exploring code, prefer CodeGraph over repo-wide `grep`/`find` for structural questions (where a symbol is defined, who calls it, what a change impacts) — one call returns blast radius + line-numbered source + call paths.
- MCP tools (preferred when installed): `codegraph_explore`, `codegraph_node`. Install via `codegraph install`.
- CLI equivalents (same output): `codegraph explore <query>`, `codegraph node <name>`, `codegraph query <search>`, `codegraph callers <symbol>`, `codegraph callees <symbol>`, `codegraph impact <symbol>`. Run `codegraph sync` after edits to refresh the index.
- Still use `grep` for plain text / string literals / comments / config values — CodeGraph indexes symbols, not arbitrary text.

## Build, Test, and Development Commands
- `npm install`: install JavaScript dependencies.
- `npm run dev`: run Vite frontend locally.
- `npm run tauri dev`: run the full desktop app (frontend + Rust backend).
- `npm run build`: type-check and build frontend (`tsc && vite build`).
- `npm run tauri build`: create production desktop bundles.
- `cargo check --manifest-path src-tauri/Cargo.toml`: validate Rust compilation quickly.
- `npm run bump <major|minor|patch> "description"`: after user approval, bump all app/package versions with the unified version script.

## Coding Style & Naming Conventions
- TypeScript is strict (`strict`, `noUnusedLocals`, `noUnusedParameters` enabled).
- React components use PascalCase file names (example: `TokenUsagePage.tsx`).
- Zustand stores follow `useXStore` naming.
- Frontend service files use `*Service.ts`; Rust service modules use `*_service.rs`.
- Keep `src-tauri/src/lib.rs` command handlers thin; place business logic in `services/`.
- Match the formatting style already used in the file you modify to keep diffs clean.

## Testing Guidelines
- No dedicated frontend test script is currently defined in `package.json`; add tests alongside new features when introducing test infrastructure.
- Rust tests run with `cargo test --manifest-path src-tauri/Cargo.toml`.
- For new logic, cover at least one happy path and one error/edge path.
- Target 80%+ coverage for newly added, testable modules.

## Commit & Pull Request Guidelines
- This workspace snapshot does not include `.git` metadata. Use Conventional Commit style:
  `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`.
- Keep each commit focused on a single concern.
- After a feature or fix is complete, ask the user whether this completed work should bump the version. Do not choose automatically; follow [docs/versioning.md](docs/versioning.md) or [docs/versioning-cn.md](docs/versioning-cn.md), and let the user select `major` (x), `minor` (y), or `patch` (z), plus snapshot or release.
- The unified version script is `scripts/bump-version.js`; do not hand-edit version numbers across files unless the user explicitly asks for a one-off correction.
- PRs should include:
  - concise change summary and scope,
  - verification evidence (for example, `npm run build`, `cargo check`),
  - screenshots/GIFs for UI updates,
  - linked issue or task reference when available.
