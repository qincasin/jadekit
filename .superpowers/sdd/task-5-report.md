# Task 5 Report — Orchestration Data Types and Status Enums

**Branch:** `feat/helm-phase2b-store`
**Commit:** `991fee2 feat(hermes): orchestration data types and status enums`
**Spec:** `docs/superpowers/specs/2026-06-27-helm-hermes-design.md` §7 (lines 224–253)
**Orca reference:** `/Users/jiaxing/code/github/orca/src/main/runtime/orchestration/types.ts` + `db.ts`

---

## What was defined

All in **`src-tauri/src/hermes/types.rs`** (new file, 477 lines incl. tests), re-exported from
**`src-tauri/src/hermes/mod.rs`**.

### Enums (each with `as_str(&self) -> &'static str` and `from_str(&str) -> Result<Self, String>`)

| Enum | Variants |
|------|----------|
| `MessageType` | Status, Dispatch, WorkerDone, MergeReady, Escalation, Handoff, DecisionGate, Heartbeat |
| `TaskStatus` | Pending, Ready, Dispatched, Completed, Failed, Blocked |
| `DispatchStatus` | Pending, Dispatched, Completed, Failed, CircuitBroken |
| `GateStatus` | Pending, Resolved, Timeout |
| `RunStatus` | Idle, Running, Completed, Failed (§7 rename of orca `CoordinatorStatus`) |
| `RuntimeKind` | Sdk, Cli (not in §7; added per brief for `AgentAssignment.runtime`) |

All enums derive `Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize`.
The DB layer (Task 6) will use `as_str`/`from_str` as the source of truth; serde falls
back to default Rust variant-name (de)serialization but is not the load-bearing path.

### Structs (all derive `Debug, Clone, PartialEq, Eq, Serialize, Deserialize`)

- **`Task`** — `id, parent_id, spec, status: TaskStatus, deps: Vec<String>, result, assignment: Option<AgentAssignment>, created_at, completed_at` (§7 shape verbatim).
- **`AgentAssignment`** — `runtime: RuntimeKind, tool: String, model: String` (§7 verbatim).
- **`DispatchContext`** — §7 core (`id, task_id, assignee: Option<String>, status: DispatchStatus, failure_count: u32, last_heartbeat_at: Option<String>`) + orca-elided (`last_failure, dispatched_at, completed_at, created_at`).
- **`Message`** — §7 field set (`id, from, to, type→kind, priority, thread_id, payload, read: bool, sequence: u64, created_at`) + orca `subject, body` (required by `insertMessage`). `kind` uses `#[serde(rename = "type")]` to keep JSON column `"type"` while avoiding the Rust reserved word.
- **`DecisionGate`** — `id, task_id, question, options: Vec<String>, resolution, status: GateStatus` (§7 verbatim).
- **`CoordinatorRun`** — `id, goal, status: RunStatus, coordinator_handle, poll_interval_ms: u64, created_at, completed_at`. Uses §7's `goal` (orca calls it `spec`); added orca's `coordinator_handle`/`created_at`/`completed_at` since §7 elides.

---

## TDD evidence

### RED (failing tests first)

**Command:**
```
cargo test --manifest-path src-tauri/Cargo.toml hermes::types
```
**Result:** 41 compile errors — every referenced enum/struct item undefined (`E0433: cannot find type`, `E0425: cannot find value`). Tail:
```
error[E0433]: cannot find type `RuntimeKind` in this scope
   --> src/hermes/types.rs:130:17
    |
130 |         assert!(RuntimeKind::from_str("nonsense").is_err());
    |                 ^^^^^^^^^^^ use of undeclared type `RuntimeKind`
...
error: could not compile `jadekit` (lib test) due to 41 previous errors; 3 warnings emitted
```
**Why it fails:** tests written first; implementation not yet present.

### GREEN (after implementation)

**Command:**
```
cargo test --manifest-path src-tauri/Cargo.toml hermes::types
```
**Result:** 12 passed / 0 failed.
```
running 12 tests
test hermes::types::tests::dispatch_status_from_str_rejects_unknown ... ok
test hermes::types::tests::dispatch_status_roundtrip ... ok
test hermes::types::tests::gate_status_from_str_rejects_unknown ... ok
test hermes::types::tests::gate_status_roundtrip ... ok
test hermes::types::tests::message_type_from_str_rejects_unknown ... ok
test hermes::types::tests::message_type_roundtrip ... ok
test hermes::types::tests::run_status_from_str_rejects_unknown ... ok
test hermes::types::tests::run_status_roundtrip ... ok
test hermes::types::tests::runtime_kind_from_str_rejects_unknown ... ok
test hermes::types::tests::runtime_kind_roundtrip ... ok
test hermes::types::tests::task_status_from_str_rejects_unknown ... ok
test hermes::types::tests::task_status_roundtrip ... ok
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 237 filtered out
```
Each enum gets two tests: full-variant `as_str→from_str` roundtrip + `from_str("nonsense")` errors.

### `cargo check`

```
$ cargo check --manifest-path src-tauri/Cargo.toml
...
Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.21s
```
**Clean — no errors.** Only dead-code warnings on the new types (`struct CoordinatorRun is never constructed`, etc.) which is expected — Tasks 6/7 will consume them.

---

## `as_str` token scheme

All tokens are **lowercase snake_case**, chosen to match orca's SQLite `CHECK` constraints verbatim so the Store (Task 6) can write/read enum values as plain DB strings without translation:

| Enum | Tokens |
|------|--------|
| MessageType | `status`, `dispatch`, `worker_done`, `merge_ready`, `escalation`, `handoff`, `decision_gate`, `heartbeat` |
| TaskStatus | `pending`, `ready`, `dispatched`, `completed`, `failed`, `blocked` |
| DispatchStatus | `pending`, `dispatched`, `completed`, `failed`, `circuit_broken` |
| GateStatus | `pending`, `resolved`, `timeout` |
| RunStatus | `idle`, `running`, `completed`, `failed` |
| RuntimeKind | `sdk`, `cli` |

`from_str` accepts **exactly** these tokens (case-sensitive); any other input — including `""` — returns `Err(format!("unknown <Enum>: <input>"))`.

---

## orca vs §7 reconciliations

1. **`CoordinatorRun` field name**: orca uses `spec`; §7 uses `goal`. **Followed §7** per brief. Struct doc notes the divergence.
2. **`CoordinatorStatus` → `RunStatus`**: §7 rename of orca's `CoordinatorStatus`. Followed §7.
3. **`DispatchContext.assignee`**: orca `assignee_handle: string | null`; §7 `assignee: Option<AgentId>`. Used `Option<String>` per task brief (handle = string).
4. **`DispatchContext` extra fields**: §7 elides with `/* … */`; orca reveals `last_failure`, `dispatched_at`, `completed_at`, `created_at`. **Added all four** — they're material for the dispatch lifecycle (circuit-breaker accounting + stale-dispatch detection).
5. **`Message` shape**: §7 gives field-name list only; orca `insertMessage` shows `subject`/`body` are required NOT NULL columns. **Added both** (defaulting to empty string is a Store concern, Task 6). **Skipped `delivered_at`** (YAGNI — push-on-idle dedup is not in §7; defer to the relevant sub-phase).
6. **`Task` extras**: orca has `task_title`, `display_name`, `created_by_terminal_handle` (added in schema v4/v5). **Not in §7 → omitted** per YAGNI; can be added back if/when the Coordinator needs them.
7. **`Task.deps`**: orca stores as JSON `TEXT`; Rust type is `Vec<String>` (cleaner API). Store layer will serialize.
8. **`Message.kind`**: Rust field `kind` with `#[serde(rename = "type")]` so the JSON column still reads `"type"` (matches orca + avoids the reserved word).

---

## Self-review

- **Completeness vs §7**: every enum and every struct from §7 lines 226–250 is present. Field-for-field, §7-named fields are 1:1.
- **No magic strings**: every enum value flows through `as_str`/`from_str`; no bare `"circuit_broken"` etc. anywhere outside those match arms.
- **YAGNI**: no Store/CRUD logic, no helpers beyond what the brief mandates (`as_str`/`from_str`). No `delivered_at`, no orca task-display fields. `RuntimeKind` is the only non-§7 addition, and the brief explicitly requires it.
- **Tests verify real roundtrip**: every variant is exercised through `as_str → from_str → Self`, plus a negative case per enum. PartialEq/Eq derived so the assertion is structural, not stringly.
- **serde**: structs + enums all derive `Serialize/Deserialize`. JSON uses default casing except `Message.kind → "type"`.
- **Comments**: module doc + per-enum + per-struct, in Chinese as specified. `CircuitBroken` documents the 3-failure threshold; `Ready` documents "deps all completed".
- **Timestamp convention**: `String` (ISO-8601) per §7; module doc calls out the divergence from `jadekit.db`'s INTEGER timestamps.

---

## Files changed

- `src-tauri/src/hermes/types.rs` — **new** (477 lines incl. tests)
- `src-tauri/src/hermes/mod.rs` — added `pub mod types;` and `pub use types::{...}` re-export

---

## Concerns

- None blocking. Minor: the workspace carries pre-existing dead-code warnings from `hermes/runtime.rs` (`NeedsInput` never constructed) — not introduced by this task, will be exercised in Phase 2c.
- Forward note for Task 6: when laying down the `messages` table, default `subject`/`body` to empty string (NOT NULL DEFAULT '') to mirror orca; `priority` should default to `"normal"` (could later become its own enum, but YAGNI for now per brief).
