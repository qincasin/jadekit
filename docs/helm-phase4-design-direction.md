# Helm Phase 4 Cockpit Design Direction

Status: draft for Phase 4a Task 1 review.

Scope: this document defines the visual direction for the Helm cockpit only. It does not implement production React or Rust code. Production UI text must go through `src/locales/zh.json` and `src/locales/en.json` with zh/en kept in sync. For this project/app context, Simplified Chinese is the default visible experience; the static mock uses Chinese copy to represent that default UX.

## Design Thesis

Helm is a command bridge for an active heterogeneous AI fleet, not a generic analytics dashboard. The first viewport must make these facts obvious:

- Multiple agents can run in parallel.
- Each agent has a tool, model, task, worktree, and live state.
- The user can steer the fleet through kanban intervention, session focus, grounded activity details, diff review, and roster/composer controls.

Signature elements:

- `AgentStateDot`: compact four-state liveness marker on every worker, stream row, and inspector header.
- Three-pane command bridge: fleet kanban on the left, selected agent session in the center, worktree inspector on the right.
- Orchestration pulse: a restrained real-time accent used only for active run progress and live stream motion.
- Dense operational metadata: model, branch, task id, diff stats, and dispatch id use mono/tabular treatment.

Backend grounding for Phase 4a:

- Hermes workers currently use `send_raw_stream`; the engine consumes raw worker output internally.
- The frontend currently receives coarse `hermes://agent` events with `status` and `activity` (`tool_use`, `text`, `thinking`), plus task/dispatch/run DTOs. It does not receive a full worker transcript or rich tool blocks yet.
- The design mock depicts the target experience after the Phase 3.5 `hermes_worker_transcript` bridge exists, using realistic fake transcript data so the intended cockpit shape can be reviewed.
- Production implementation before that bridge must not show fake transcript/tool data. It must fall back to a grounded activity timeline based on status/activity, task spec/result, dispatch info (`assignee`, `failureCount`, timestamps), and intervention slots.
- The center pane should include a clear note: "完整会话由 Phase 3.5 worker-transcript 桥提供；实现阶段在桥接通前显示活动流回退态。"

## Token System

Helm should sit on top of DaisyUI semantic tokens (`base-100`, `base-200`, `base-300`, `base-content`, `primary`, `warning`, `error`, `success`) rather than replacing the app theme. The following named hex values are design anchors for mockups and minimal `--helm-*` extensions.

| Token | Hex | Role |
|---|---:|---|
| `helm-ink` | `#18212f` | light theme cockpit text anchor, used through `base-content` when possible |
| `helm-panel` | `#f6f7fb` | light theme panel wash, equivalent to a soft `base-200` layer |
| `helm-primary` | `#3b82f6` | Jadekit/DaisyUI primary anchor from Tailwind config |
| `helm-pulse` | `#2dd4bf` | only dedicated Helm accent, for live orchestration pulse |
| `helm-amber` | `#f59e0b` | waiting for user or permission, maps to DaisyUI `warning` |
| `helm-red` | `#ef4444` | interrupted/reaped/aborted, maps to DaisyUI `error` |

Minimal custom properties:

```css
:root {
  --helm-pulse: #2dd4bf;
  --helm-lane-gap: 0.625rem;
  --helm-panel-min: 16rem;
}
```

Optional local aliases may be used in static artifacts, but production should prefer DaisyUI semantic variables:

```css
/* Static mock only; production should bind these to DaisyUI theme variables. */
--helm-surface: hsl(var(--b1));
--helm-surface-raised: hsl(var(--b2));
--helm-border: hsl(var(--b3));
--helm-text: hsl(var(--bc));
```

## Light And Dark Handling

Default light:

- The default Helm cockpit is the white/light Jadekit theme. The first loaded view should read as bright, operational, and dense rather than dark-console-first.
- Use Tailwind/DaisyUI classes for the app canvas, pane backgrounds, borders, and primary actions. The static mock pins primary to Jadekit's `#3b82f6`; production should inherit DaisyUI `primary`.
- In Jadekit light theme, `base-100` is the app canvas, `base-200` is the pane wash, and `base-300` can be used for quiet borders and separators.
- Worker cards sit one layer above lanes with low shadow and no marketing-card treatment.
- `helm-pulse` appears as a narrow live rail, progress bead, or subtle ring around active dots.

Dark alternate:

- Dark mode follows the user's Jadekit/DaisyUI theme selection as an alternate, not as the default visual direction.
- Keep the same semantic roles, not inverted custom colors.
- Jadekit dark theme uses `base-100 #161b22`, `base-200 #21262d`, and `base-300 #0d1117`. Because `base-300` is the deepest background rather than a simple border color, production should use Tailwind/DaisyUI semantic classes and apply border alpha or semantic border treatment carefully instead of assuming `base-300` always means a visible line.
- Increase explicit border contrast only where pane separation needs it, and reduce filled color area. Dark Helm should read like an instrument panel, not a neon dashboard.
- `helm-pulse` remains the live accent but should be limited to tiny surfaces: dot ring, stream cursor, run progress rail.
- Amber wait and red interrupted must remain distinguishable without relying only on saturation. Pair color with icon/shape/text.

Production rule: add only small `--helm-*` extensions for pulse, spacing, and component-specific density. Do not create a parallel palette.

## Font Roles

- UI prose and labels: existing app sans stack.
- IDs, model names, branch names, diff counts, token counts, timestamps: mono with tabular numerals (`font-mono tabular-nums`).
- Pane headings: compact sans, semibold, small uppercase only for operational labels.
- Session text: existing chat typography; avoid shrinking the transcript below readable size.

## Layout Wireframe

```text
+---------------------------- Helm run bar -----------------------------+
| goal, run status, progress pulse, Cmd+K Jump, Roster, stop/cancel      |
+------------ fleet -----------+------------ session -----------+--------+
| lanes: pending/running/review| activity timeline + transcript | inspect|
|                              |                                |        |
| AgentStateDot + tool/model   | status/activity events         | diff   |
| task spec + worktree branch  | fanout compare when N selected | judge  |
| diff +/- + gate marker       | composer at bottom             | gates  |
+------------------------------+--------------------------------+--------+
| composer hint: "Ask Hermes to dispatch a goal..." + roster snapshot     |
+------------------------------------------------------------------------+
```

Responsive behavior:

- Wide: three panes visible, left and right collapsible.
- Medium: inspector becomes a slide-over or bottom sheet; fleet remains visible.
- Narrow: one pane at a time with persistent top switcher and `Cmd+K`/search access. No horizontal overflow for worker metadata. Center-pane header actions must wrap or stack so the grounded primary action ("跳到 worktree") remains visible around 390px wide.

## Status Mapping

The cockpit must use the Phase 3 contract vocabulary exactly for agent status:

| Contract token | Visual | Meaning | User action |
|---|---|---|---|
| `working` | yellow/primary spinner or pulsing ring | stream activity: text, thinking, or tool use | watch, jump, optionally stop |
| `needs-attention` | amber solid dot with attention marker | `NeedsInput` or permission request; waiting for user/tool authorization | answer gate or approve/deny |
| `done` | emerald check | agent stopped normally; task failure is shown by task status, not this dot alone | inspect result, merge if retained |
| `interrupted` | red dot or stop marker | supervisor marked suspect then reap/abort terminated it | inspect failure, retry or discard |

Amber wait versus red interrupted:

- Amber means the agent is intentionally paused and must not be killed by timeout. This covers permission prompts, ask-user questions, and decision gates.
- Red means the system intervened because the agent was suspect, aborted, reaped, or otherwise interrupted. It is not a normal wait.
- Do not label `needs-attention` as stuck, crashed, dead, or failed.
- Do not use red for ordinary blocked/waiting states.
- Validate status dots on pure white. `working`/amber markers need enough outline, darker amber stroke/glyph, or ring contrast while preserving amber = waiting/attention and red = interrupted/reaped.

Related task/run states stay separate:

- Task status: `pending`, `ready`, `dispatched`, `completed`, `failed`, `blocked`.
- Additional review lane event: `awaiting-merge` from cleanup/retained worktree behavior.
- Dispatch status: `pending`, `dispatched`, `completed`, `failed`, `circuit_broken`.
- Run status: `idle`, `running`, `completed`, `failed`, `cancelled`.

## Empty, Loading, And Error Writing

Every pane needs explicit empty, loading, and error states.

Empty states:

- Explain what is missing and offer the next action.
- Cold start first screen: before any run exists, show an empty cockpit with composer-forward copy: "下达一个目标，启动舰队".
- Example fleet empty: "No agents in this run yet. Send a goal or open Roster to assign a worker."
- Example inspector empty: "Select a worker with retained changes to review diff, judge notes, and merge options."

Loading states:

- State what is being fetched or subscribed to.
- Prefer skeleton rows that preserve layout over centered spinners.
- Avoid motion-only loading indicators; pair motion with text or shape.

Error states:

- Say what happened and how to recover.
- Include safe recovery actions: retry subscription, refresh run, reopen worktree, copy diagnostic id.
- Do not expose secret values, raw tokens, or internal paths unless the path is already part of the user's selected worktree context.

## Accessibility And Interaction Notes

- Keyboard: pane toggles, lane cards, jump palette, merge/discard, gate answers, and composer must be keyboard reachable.
- Grounded detail access: worker cards are clickable to focus the agent session. In the target state, the center transcript uses the Phase 3.5 bridge and can expose rich execution details. In pre-bridge production, rows must fall back to event payload summaries that are actually emitted today. The inspector can open full diff and judge detail only when a retained review worker is selected and backend data is available. Production must provide keyboard equivalents for every click target and route all visible labels through zh/en i18n.
- Target transcript slot: the design mock should show the intended rich multi-turn agent work session with fake data: dispatch, thinking, `tool_use(Read)` + result, thinking, `tool_use(Edit)` + diff preview, text explanation, `tool_use(Bash)` + output, subagent sidechain, more rounds, and done. The visual language should reuse existing Jadekit chat patterns: `MessageList`-like turns, `ReadToolBlock`, `EditToolBlock` + `EditDiffPreview`, `BashToolBlock`, `SearchToolGroupBlock`, `ThinkingBlock`, `SubagentHistoryPanel`, and `MessageMeta`-style token/duration metadata.
- Fallback slot: the center pane still needs visible copy explaining that implementation before the bridge uses a grounded activity timeline fallback, not fake transcript data.
- Single-agent actions: "跳到 worktree" is grounded and should be active. "停止该 agent" and "重试" may appear as disabled/planned entries until Phase 3.5 single-agent abort/retry exists.
- Intervention slot: `needs-attention` rows should include an inline placeholder for permission / ask-user / gate. Phase 4f implements the live resolution flow.
- Focus: use visible focus rings with enough contrast against both pure-white and dark panels; prefer primary/strong outline plus offset or dual ring for buttons, inputs, worker cards, timeline rows, and detail links. Never remove outline without replacing it.
- Reduced motion: `working` spinner and pulse animations become static rings or gentle opacity states under `prefers-reduced-motion: reduce`.
- Screen reader labels: each `AgentStateDot` needs a status label such as "agent codex-07 needs attention, waiting for permission".
- Color independence: status must include text, icon/shape, or aria labels, not color alone.
- Narrow screens: avoid hidden-only critical actions. If panes collapse, the active pane switcher and jump palette must expose the same actions.
- Destructive operations: merge conflicts, discard, and worktree removal require preflight information and confirmation before invoking engine commands.

## Composer And Roster Grounding

- Helm composer is a one-shot goal entry to the orchestrator: the user submits a goal, Hermes plans and dispatches. It is not an ongoing one-on-one chat with a worker.
- Model choice is Roster selection: reuse `SelectorDropdown` patterns for model/mode/reasoning, `ModelIcon`, and existing roster/provider management. Selecting means "派哪些 CLI x 模型".
- Heterogeneous fanout should reuse the existing `FanoutComposer`.
- SDK missing intercept should reuse the existing grounded flow.
- Do not introduce slash commands, per-message completion, or direct worker chat turns in the cockpit mock. Existing `PromptEnhancer` and `@file` references are optional only where they are already grounded.

## Phase 3.5 Pending Connections

- `hermes_worker_transcript`: provide real worker transcript/detail data for the center transcript container; then reuse `MessageList`, `toolBlocks`, `ThinkingBlock`, `EditDiffPreview`, and `SubagentHistoryPanel`.
- Single-agent abort/retry: enable currently planned actions for stopping or retrying a specific agent.
- Detail logs beyond coarse status/activity: replace placeholder summaries with emitted structured data when the backend exposes it.

## Static Mock

The review artifact is `docs/helm-phase4-cockpit-mock.html`. It is intentionally standalone, uses fake data, and avoids network or build dependencies. It demonstrates the cockpit identity, three-pane shell, fleet kanban, `AgentStateDot` states, target rich transcript experience, grounded pre-bridge fallback note, inspector empty state, cold-start state, and composer/roster hint.
