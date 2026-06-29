# Helm × Hermes — Phase 4：驾驶舱 UI（Codex 全量执行）实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development（**推荐**，防跑偏最强：每 task 全新子 agent + 两道审查 + progress.md 检查点；Codex 作协调者不自己写）。不可用时降级 superpowers:executing-plans。**另一项强制技能：动工视觉前必须 Use frontend-design 技能定基调**（§10 质量基线硬约束）。Steps 用 checkbox（`- [ ]`）跟踪。

**Goal:** 在 Phase 0/1/1b/2/3 之上构建 **Helm 驾驶舱**——一个「比 orca 更真实、更好用、非常 hermess 化（舰队/编排感强）」的三栏 cockpit：左=舰队看板（Kanban + AgentStateDot），中=选中 Agent 会话 / 异构扇出并排，右=worktree diff + 合并/丢弃 + 评判结果；含 ⌘K Jump、Roster、编排 run 可视化，**所有人工干预点交互友好**。

**Architecture:** **加法式、不破坏现有 chat。** 把现有「对话」surface（`ChatPage.tsx`）升级为 Helm 驾驶舱（**单入口、面板可开合、非路由分页**）。**重度复用**现有能力（会话侧栏/tabs、扇出对比 `FanoutCompareView`/`compare.ts`/`roster.ts`、子代理面板 `SubagentHistoryPanel`/`subagentRuns`、干预对话框 `ToolPermissionDialog`/`AskUserQuestionDialog`/`PlanApprovalDialog`、diff 面板 `ChatDiffReviewPane`），重组进 cockpit，不另起炉灶。引擎数据经 **Phase 3 冻结契约**（`docs/helm-hermes-ui-contract.md`：9 个 `hermes_*` 命令 + `hermes://run|task|agent` 事件，统一 `OrchestrationEvent` 判别联合）接入；新增 `useHermesStore` 订阅事件、维护编排状态。

**Tech Stack:** React 19 + TypeScript + Zustand + DaisyUI/Tailwind + i18next（zh/en）+ Tauri 2（`invoke` / `listen`）+ vitest（纯逻辑/选择器测试）。

## 设计方向（「舰桥 Command Bridge」——Codex 据此 + frontend-design 技能细化，先评审后建）

> 这是定向，不是像素稿。Codex 动工视觉前**必须加载 frontend-design 技能**自行做一轮设计 pass + 出可视稿自审（见 Task 1）。以下是不可偏离的基调约束：

- **Thesis**：异构 AI 舰队的**指挥舰桥**，不是通用 dashboard。hero = 实时舰队板（多 CLI×模型 agent 并行、各自 worktree、活体状态点）。
- **Signature**：`AgentStateDot` 舰队 + 三栏舰桥 + 贯穿的「编排脉搏」（run 进度实时律动）。一眼辨「谁用什么模型在干什么、活着还卡住」。
- **配色克制**：在 jadekit 现有 **DaisyUI 主题语义 token 之上扩展**（`base-*`/`primary`/`warning`/`error`/`success`），**不另起一套配色**（避免与 app 其它页冲突）；仅为「编排实时态」加 1 个专属 accent（如 `--helm-pulse`）。明暗主题都要过。
- **排版层次**：数据密度区（agent id / 模型徽章 / diff +/- / 分支名）用**等宽/表格数字 face**（`font-mono` / `tabular-nums`）；散文用现有 sans。状态点颜色对齐 §10 + 契约词表（见下）。
- **状态点词汇（与 Phase 3 契约 §5.1 逐字对齐，禁止自创）**：`working`(黄/转圈) · `needs-attention`(琥珀，权限/等待输入) · `done`(翠绿勾) · `interrupted`(红，reap/abort)。**严格区分「等待输入」(琥珀) 与「卡死/中断」(红)**。orca `AgentStateDot.tsx` 是交互细节范本（学不照抄）。

## 分支约定（沿用，固化）

- **主功能分支 = `feat/helm`**（当前含 Phase 0–3 全量）。Phase 4 是收官阶段，**串行单独执行**（Phase 3 已全部合回），在主工作树按子阶段分支推进即可，**无需 worktree 隔离**。
- 每个子阶段从 `feat/helm` 拉分支（如 `feat/helm-phase4a-shell`），GATE 过后 `--no-ff` merge 回 `feat/helm`。
- **不要** merge 到 main/develop（等用户明确指示）。

## Global Constraints（逐字遵守，每个 task 隐含包含）

- **契约即事实源**：所有命令名/事件名/payload 字段/状态 token **以 `docs/helm-hermes-ui-contract.md` + 真实 Rust 源码为准**，禁止自创。前端 TS 类型按契约 §4.2 的判别联合定义。
- **复用优先**：能复用的现有组件/纯函数（见 Architecture）必须复用 + 重组，禁止重复造轮子；确需新建先在 task 里说明为何不能复用。
- **不写魔法字符串**：状态 token、事件名、命令名、快捷键、i18n key 集中常量/枚举。
- **i18n 同步**：所有用户可见文案进 `src/locales/zh.json` + `en.json`，禁止硬编码中文/英文串。
- **可访问性地板**（§10 质感要求）：键盘可达 + 可见焦点环 + `prefers-reduced-motion` 尊重 + 响应式下探移动窄屏。
- **状态齐全**：每个面板都要有 empty / loading / error 三态文案（frontend-design 写作规范：错误说清「发生什么 + 怎么修」，空态是行动邀请）。
- **破坏性操作安全**：删 worktree / 丢弃 / 合并冲突 = 预检 + 二次确认 + 清晰回滚提示（对齐 Phase 3 引擎的 sweep 安全语义）。
- 每 task 结束 commit（Conventional Commits）；提交前 `npm run lint` + `npm run build` + 相关 `npx vitest run` 绿 + `git diff --check` 干净。
- **非回归**：不破坏现有 chat——升级 ChatPage 时，原对话能力路径保持可用（或明确迁移并保留入口）。每个 GATE 确认 `npm run build` + 现有前端测试全绿。

## 阅前必读（Codex 冷启动先做）

```bash
# 契约（Phase 4 施工图，最重要）
docs/helm-hermes-ui-contract.md
# 设计文档 §10 质量基线（验收硬标准）
docs/superpowers/specs/2026-06-27-helm-hermes-design.md  # §10 / §6.3 判活 / §11 干预
# Phase 3 交付（引擎能力 + Phase 4 待办）
docs/helm-phase3-delivery.md  # §5
```
**codegraph 优先**（jadekit + orca 均已 sync）：
```bash
# jadekit 现有可复用件
codegraph node ChatPage FanoutCompareView SubagentHistoryPanel ToolPermissionDialog AskUserQuestionDialog PlanApprovalDialog ChatDiffReviewPane
codegraph explore chat session sidebar tabs composer subagent runs store
# orca 交互范本（只读他山之石）—— 在 orca 仓库跑
#   AgentStateDot.tsx / WorkspaceKanbanCard.tsx / workspace-kanban-card-pointer-drag-dom.ts
#   useComposerState.ts / agent-row-lineage.ts（子代理血缘树）/ Jump palette
```
仅纯文本字面量匹配才退回 grep。改完 `codegraph sync`。

---

# 子阶段 4a：cockpit 三栏外壳 + 设计基调 + 事件接入

> 分支：`git checkout feat/helm && git checkout -b feat/helm-phase4a-shell`
> 目标：搭三栏可开合骨架（单入口、非路由）、定视觉基调（frontend-design pass + 可视稿评审）、接入 `hermes://*` 事件到 `useHermesStore`。

## Task 1: 设计基调 pass（frontend-design）+ 可视稿评审（人工确认点）

**Files:**
- Create: `docs/helm-phase4-design-direction.md`（token 系统：4–6 named hex + 字体角色 + 布局 ASCII 线框 + signature；明暗双主题；在 DaisyUI token 之上的扩展点 `--helm-*`）
- Test: 无（设计产物）；产出**可视稿/截图**。

**Interfaces:** Produces 一份落地 token 文档 + 至少 1 张三栏 cockpit 可视稿（hero/舰队板/状态点）。
- [ ] **Step 1: 加载 frontend-design 技能**，按它做设计 pass（基于本计划「设计方向」+ §10），产出 token 系统 + ASCII 线框 + signature 说明。
- [ ] **Step 2: 出可视稿**（静态 HTML/截图或 Storybook 片段），自审是否「orca 级、零模板感」。
- [ ] **Step 3: 暂停等用户确认**（人工确认点：基调不过不进 4b）。确认后写入 `docs/helm-phase4-design-direction.md`。
- [ ] **Step 4: 提交** `docs(helm): phase 4 cockpit design direction`

## Task 2: 前端契约类型 + `hermesService`（invoke 封装）

**Files:**
- Create: `src/services/hermesService.ts`（9 个命令的 typed `invoke` 封装）、`src/types/hermes.ts`（契约 §3/§4 的 TS 类型）
- Test: `src/services/hermes.test.ts`（纯类型/参数归一化；invoke mock）

**Interfaces:** Produces（**逐字对齐契约**）：
```ts
// src/types/hermes.ts —— 契约 §3 DTO + §4.2 事件判别联合
export interface TaskDto { id: string; parentId: string|null; spec: string; status: string;
  deps: string[]; result: string|null; createdAt: string; completedAt: string|null }
export interface DispatchDto { id: string; taskId: string; assignee: string|null; status: string;
  failureCount: number; lastHeartbeatAt: string|null; lastFailure: string|null;
  dispatchedAt: string|null; completedAt: string|null; createdAt: string }
export interface RunShowDto { id: string; goal: string; status: string; createdAt: string;
  completedAt: string|null; taskCount: number; completedCount: number }
export interface SweepReportDto { removed: number; retained: number }
export interface HermesRunOpts { maxConcurrent?: number; pollIntervalMs?: number; repoRoot?: string }
export type OrchestrationEvent =
  | { kind:"run";   runId:string; goal:string; status:string; error:string|null }
  | { kind:"task";  runId:string; taskId:string; status:string; dispatchId:string|null }
  | { kind:"agent"; runId:string; agentId:string; taskId:string|null; status:string; activity:string|null };
// 状态/活动 token 常量（对齐契约 §5，禁止散落魔法串）
export const AGENT_STATUS = { WORKING:"working", NEEDS_ATTENTION:"needs-attention", DONE:"done", INTERRUPTED:"interrupted" } as const;
// src/services/hermesService.ts
export const hermesService = {
  run(goal: string, opts?: HermesRunOpts): Promise<string>,           // hermes_run
  taskList(filter?: {status?:string; ready?:boolean}): Promise<TaskDto[]>,
  dispatchShow(dispatchId: string): Promise<DispatchDto>,
  gateResolve(gateId: string, resolution: string): Promise<void>,
  runStop(runId: string): Promise<void>,
  runCancel(runId: string): Promise<void>,
  runShow(runId: string): Promise<RunShowDto>,
  agentList(): Promise<DispatchDto[]>,
  runCleanup(runId: string): Promise<SweepReportDto>,
};
```
- [ ] **Step 1: 写失败测试** —— invoke mock 下，`hermesService.run("g",{maxConcurrent:2})` 调 `invoke("hermes_run",{goal:"g",opts:{maxConcurrent:2}})`；事件类型守卫 `isAgentEvent`。
- [ ] **Step 2: 确认失败** → `npx vitest run src/services/hermes.test.ts` FAIL
- [ ] **Step 3: 实现** types + service（命令名常量对齐契约 §2）
- [ ] **Step 4: 确认通过 + build** → vitest PASS；`npm run build`
- [ ] **Step 5: 提交** `feat(helm): hermes service and contract types`

## Task 3: `useHermesStore` —— 订阅事件 + 维护编排状态

**Files:**
- Create: `src/stores/useHermesStore.ts`（Zustand：`runs`/`tasks`/`agents` 索引 + `listen` 三通道事件 reducer）、`src/stores/hermesReducer.ts`（纯 reducer，便于测）
- Test: `src/stores/hermesReducer.test.ts`

**Interfaces:** Produces：
```ts
// 纯 reducer：把一个 OrchestrationEvent 折进状态（便于无 Tauri 单测）
export interface HermesState { runId: string|null; runStatus: string;
  tasks: Record<string, TaskDto>; agents: Record<string, AgentView> }
export interface AgentView { agentId: string; taskId: string|null; status: string; activity: string|null }
export function reduceHermesEvent(state: HermesState, ev: OrchestrationEvent): HermesState;
// store：挂载时 listen("hermes://run|task|agent")→reduceHermesEvent；卸载时 unlisten。
```
- [ ] **Step 1: 写失败测试** —— `reduceHermesEvent` 处理 `agent{working,tool_use}` 更新该 agent 视图；`task{completed}` 更新 task 状态；`run{cancelled}` 设 runStatus（**幂等**：重复 `run{cancelled}` 不报错——契约已知双发）。
- [ ] **Step 2: 确认失败** → FAIL
- [ ] **Step 3: 实现** 纯 reducer + store（listen/unlisten 生命周期）
- [ ] **Step 4: 确认通过** → vitest PASS
- [ ] **Step 5: 提交** `feat(helm): hermes store with event reducer (idempotent)`

## Task 4: 三栏 cockpit 外壳（可开合，单入口，非路由）

**Files:**
- Create: `src/components/helm/HelmCockpit.tsx`（三栏 grid：左 FleetPanel 占位 / 中 SessionPanel 占位 / 右 InspectorPanel 占位；`PanelLeft`/`PanelRight` 开合按钮 + 快捷键）、`src/components/helm/useCockpitLayout.ts`（面板开合状态，纯逻辑）
- Modify: `src/pages/ChatPage.tsx`（接入 Helm cockpit 模式入口——保留原对话路径不破坏）
- Test: `src/components/helm/cockpitLayout.test.ts`（开合状态机纯逻辑）

**Interfaces:** Produces：`HelmCockpit`（消费 `useHermesStore`）；`useCockpitLayout(): { leftOpen, rightOpen, toggleLeft, toggleRight }`。布局对齐设计方向（Task 1 token）。
- [ ] **Step 1: 写失败测试** —— `useCockpitLayout` 默认两栏开；`toggleLeft` 翻转；持久化 key 存在。
- [ ] **Step 2: 确认失败** → FAIL
- [ ] **Step 3: 实现** 三栏骨架 + 开合 + 接入 ChatPage（按 Task 1 基调；空态文案）
- [ ] **Step 4: 确认通过 + build + lint** → vitest PASS；`npm run build`；`npm run lint`
- [ ] **Step 5: 提交** `feat(helm): three-pane cockpit shell with collapsible panels`

### ✅ GATE 4a
```bash
npx vitest run src/services src/stores/hermesReducer.test.ts src/components/helm
npm run build && npm run lint && git diff --check
git checkout feat/helm && git merge --no-ff feat/helm-phase4a-shell -m "merge: Phase 4a — cockpit shell + design direction + event wiring"
```

---

# 子阶段 4b：舰队看板（Kanban + AgentStateDot）

> 分支：`git checkout feat/helm && git checkout -b feat/helm-phase4b-fleet`
> 目标：左栏舰队看板——泳道（待派/执行中/待评审/完成）、AgentStateDot、CLI 图标 + 模型徽章、跨泳道拖拽=干预、与 Task 状态双向同步。

## Task 5: `HermesAgentStateDot`（4 态，对齐契约词表）

**Files:**
- Create: `src/components/helm/HermesAgentStateDot.tsx` + `agentStateDot.ts`（status token → 视觉映射纯函数）
- Test: `src/components/helm/agentStateDot.test.ts`

**Interfaces:** Produces：`dotVisualFor(status: string): { kind:"spinner"|"dot"|"check"; tone:"working"|"amber"|"emerald"|"red" }`——`working`→spinner/working、`needs-attention`→dot/amber、`done`→check/emerald、`interrupted`→dot/red。**严格区分 amber vs red**。
- [ ] **Step 1: 写失败测试** —— 四个 token 各自映射正确；未知 token 回落到中性 idle 灰（不抛错）。
- [ ] **Step 2: 确认失败** → FAIL
- [ ] **Step 3: 实现**（spinner 用 reduced-motion fallback：减动时不转圈、用静态色）
- [ ] **Step 4: 确认通过** → vitest PASS
- [ ] **Step 5: 提交** `feat(helm): agent state dot with 4-state contract vocabulary`

## Task 6: 看板泳道映射 + WorkerCard（CLI 图标 + 模型徽章 + 状态点 + 分支 + diff）

**Files:**
- Create: `src/components/helm/FleetKanban.tsx`、`src/components/helm/WorkerCard.tsx`、`src/components/helm/kanbanLanes.ts`（task/agent → 泳道纯映射）
- Test: `src/components/helm/kanbanLanes.test.ts`

**Interfaces:** Produces：`laneFor(task: TaskDto): "pending"|"running"|"review"|"done"`（ready/pending→待派、dispatched→执行中、awaiting-merge→待评审、completed→完成、failed→完成区带红标）。`WorkerCard` 显示 CLI 图标（取自 provider 管理）+ 模型徽章 + `HermesAgentStateDot` + 分支 `helm/<taskId>` + diff +/-（`hermes_dispatch_show`/复用 diff 概要）。
- [ ] **Step 1: 写失败测试** —— `laneFor` 各状态映射；`awaiting-merge`（事件 token）→ review 泳道。
- [ ] **Step 2: 确认失败** → FAIL
- [ ] **Step 3: 实现** 泳道 + WorkerCard（虚拟化长列表；空态「还没有 agent，下达一个目标开始」）
- [ ] **Step 4: 确认通过 + build** → vitest PASS；`npm run build`
- [ ] **Step 5: 提交** `feat(helm): fleet kanban lanes and worker card`

## Task 7: 跨泳道拖拽 = 人工干预（双向同步）

**Files:**
- Create: `src/components/helm/kanbanDrag.ts`（拖拽落点 → 干预动作纯映射，镜像 orca `resolveWorkspaceCardDropIndexFromRects` 思路）
- Modify: `FleetKanban.tsx`（接拖拽 → 调对应命令）
- Test: `src/components/helm/kanbanDrag.test.ts`

**Interfaces:** Produces：`dropActionFor(from: Lane, to: Lane, task: TaskDto): DropAction`——如拖到「待评审」→ 触发 `hermes_run_cleanup` 复查 / 拖「完成」的 worktree 到 review 等。**破坏性落点（丢弃）需二次确认**，不在纯映射里直接执行（返回 `{ kind:"confirm-discard" }` 交 UI 确认）。
- [ ] **Step 1: 写失败测试** —— 合法落点产出对应动作；危险落点产出 `confirm-*` 而非直接动作。
- [ ] **Step 2: 确认失败** → FAIL
- [ ] **Step 3: 实现** 拖拽（键盘可达替代：卡片菜单提供同等动作，不只鼠标）
- [ ] **Step 4: 确认通过 + build + lint** → 全绿
- [ ] **Step 5: 提交** `feat(helm): cross-lane drag as intervention with confirm-gated destructive drops`

### ✅ GATE 4b
```bash
npx vitest run src/components/helm && npm run build && npm run lint && git diff --check
git checkout feat/helm && git merge --no-ff feat/helm-phase4b-fleet -m "merge: Phase 4b — fleet kanban + agent state dot"
```

---

# 子阶段 4c：中栏会话 + ⌘K Jump + 子代理可达

> 分支：`git checkout feat/helm && git checkout -b feat/helm-phase4c-session`
> 目标：点 agent 卡 → 中栏其会话（及 worktree）；⌘K 在活跃 agent 间秒切；会话内 Task 块内联展开 sidechain 子代理（复用现有）。

## Task 8: ⌘K Jump Palette（活跃 agent 秒切）

**Files:**
- Create: `src/components/helm/JumpPalette.tsx`、`src/components/helm/jumpSearch.ts`（agent 模糊匹配纯函数）
- Test: `src/components/helm/jumpSearch.test.ts`

**Interfaces:** Produces：`filterAgents(agents: AgentView[], query: string): AgentView[]`（按 agentId/taskId/模型 模糊匹配 + 按状态权重排序：needs-attention 优先）。⌘K 唤出、↑↓选、Enter 跳转、Esc 关；可见焦点。
- [ ] **Step 1: 写失败测试** —— 模糊匹配命中；needs-attention 排前。
- [ ] **Step 2: 确认失败** → FAIL
- [ ] **Step 3: 实现**（键盘优先；reduced-motion）
- [ ] **Step 4: 确认通过** → vitest PASS
- [ ] **Step 5: 提交** `feat(helm): cmd-k jump palette for active agents`

## Task 9: 中栏 agent 会话（务实 grounded：先活动流 + 干预，富 transcript 待 Phase 3.5 桥点亮）

> **后端事实（务实约束，不画后端给不出的数据）**：Hermes worker 走 `SdkRuntime.send_raw_stream`，流被引擎内部消费，**前端当前只能拿到 `hermes://agent` 粗粒度事件**（status + activity：tool_use/text/thinking），**拿不到 worker 完整 transcript / 富工具块**。因此本 task **按当前后端能给的画**：活动流时间线 + 任务 spec/result + dispatch 信息 + 干预，并把容器做成 **transcript-ready** 形状。worker 完整 transcript（复用 `MessageList`/`toolBlocks`/`ThinkingBlock`/`MessageMeta`）**依赖 Phase 3.5 的 worker-transcript 桥**（`hermes_worker_transcript` 命令，复用 `session_manager::load_messages`）落地后再点亮——届时只换数据源、UI 容器不重做。

**Files:**
- Create: `src/components/helm/SessionPanel.tsx`、`src/components/helm/sessionHeaderActions.ts`（单-agent 动作纯逻辑）
- Test: `src/components/helm/sessionSelect.test.ts`、`src/components/helm/sessionHeaderActions.test.ts`

**Interfaces:**
- **当前 grounded 数据**：`hermes://agent` 活动流（store 里维护的 `AgentView`：status + activity 时间线）、`TaskDto`（spec/result/status）、`DispatchDto`（assignee/failure_count/时间）。
- **transcript-ready 容器**：SessionPanel 预留一个 transcript 区，数据源抽象成 `loadWorkerTranscript(agentId): Promise<Message[]|null>`——当前返回 null（走活动流 + 空态「完整执行记录将在引擎接通后显示」）；Phase 3.5 提供 `hermes_worker_transcript` 后此函数接真数据，**届时复用** `MessageList`/`ContentBlockRenderer`/`toolBlocks`/`ThinkingBlock`/`MessageMeta` 渲染，UI 不重写。
- **可立即复用（grounded）**：`SubagentHistoryPanel` + `subagentRuns`（若 sidechain 数据可得则内联）、`ScrollControl`、流式指示。
- Produces：
  - `SessionPanel`（点 WorkerCard → `selectedAgentId` → 渲染该 agent 活动流 + 任务/派发信息 + transcript-ready 区）。
  - **会话头单-agent 动作**：`sessionHeaderActions(agent)` → 跳到其 worktree（grounded：worktree 路径已知）/ 重试·停止（**单-agent abort 当前后端只有 run 级 cancel**，故标注：单-agent stop 依赖 Phase 3.5/后续，UI 先给入口或禁用态 + tooltip 说明）。
  - **内联干预槽**：`needs-attention` 时权限/ask-user/gate 回答 UI 内联浮现（实现归 4f，本 task 预留挂载点）。
- [ ] **Step 1: 写失败测试** —— `selectAgent(id)` 设选中 + 切换清理临时态；`sessionHeaderActions(runningAgent)` 含「跳 worktree」；`loadWorkerTranscript` 返回 null 时 SessionPanel 走活动流 + 空态（不报错）。
- [ ] **Step 2: 确认失败** → FAIL
- [ ] **Step 3: 实现**（活动流 + transcript-ready 容器 + 干预槽；空态文案；不画假 transcript）
- [ ] **Step 4: 确认通过 + build** → 全绿
- [ ] **Step 5: 提交** `feat(helm): center session with activity stream and transcript-ready container`

### ✅ GATE 4c
```bash
npx vitest run src/components/helm && npm run build && npm run lint && git diff --check
git checkout feat/helm && git merge --no-ff feat/helm-phase4c-session -m "merge: Phase 4c — session panel + jump palette + sub-agents"
```

---

# 子阶段 4d：右栏 worktree diff + 合并/丢弃 + 评判结果

> 分支：`git checkout feat/helm && git checkout -b feat/helm-phase4d-inspector`
> 目标：右栏——选中 agent 的 worktree diff（复用）、合并/丢弃（预检+二次确认）、扇出并排对比、judge 评判结果展示。

## Task 10: Inspector 面板 — diff 概要 + 合并/丢弃（复用 + 安全）

**Files:**
- Create: `src/components/helm/InspectorPanel.tsx`（复用 `ChatDiffReviewPane` + fanout `compare.ts`）、`src/components/helm/mergeDiscard.ts`（动作前置校验纯逻辑）
- Test: `src/components/helm/mergeDiscard.test.ts`

**Interfaces:** Produces：`mergePreflight(task: TaskDto): { canMerge: boolean; reason?: string }`（仅 awaiting-merge/有产出可合）；合并走现有 `helm_worktree_merge`，丢弃走二次确认 + 现有 worktree remove。合并冲突 → 清晰回滚提示（对齐 Phase 1b 的 abort 回滚）。
- [ ] **Step 1: 写失败测试** —— 无产出 task `canMerge:false`；冲突结果产出回滚提示文案 key。
- [ ] **Step 2: 确认失败** → FAIL
- [ ] **Step 3: 实现**（破坏性=预检+二次确认+回滚提示；复用 diff/compare）
- [ ] **Step 4: 确认通过 + build** → 全绿
- [ ] **Step 5: 提交** `feat(helm): inspector panel with safe merge/discard`

## Task 11: 异构扇出并排 + judge 评判结果展示（display-only，纯前端）

> **scope 决策：先做展示、后接数据**。`Planner::judge` 的**引擎接通**（接 Coordinator fan-out/convergence 输出 + `hermes_judge_show` 命令）归 **engine Phase 3.5（GLM）**，不在本前端阶段做——保持「Codex 只管前端」的干净边界。本 task 只做**展示组件 + 空态**：数据未接通时显示空态，引擎一旦接通（Phase 3.5 提供 `hermes_judge_show`）自动点亮，前端零改动。

**Files:**
- Create: `src/components/helm/JudgeVerdictCard.tsx`（winner 高亮 + 各候选分数条 + reason；无数据时空态）
- Modify: `InspectorPanel.tsx`（复用 `FanoutCompareView` 做并排；有 judge 结果则浮现 `JudgeVerdictCard`）
- Test: `src/components/helm/judgeView.test.ts`

**Interfaces:**
- Consumes：`FanoutCompareView`（复用）；judge 数据走**契约预留** `hermesService.judgeShow(runId): Promise<JudgeVerdictDto|null>`——若 engine Phase 3.5 尚未提供该命令，service 调用失败/返回 null → 卡片走空态（不报错、不阻塞 Inspector）。
- Produces：`JudgeVerdictCard`（`winnerIndex` 高亮对应候选 + `scores[]` 分数条 + `reason`）；`JudgeVerdictDto{ winnerIndex:number, scores:number[], reason:string, candidates:{index:number,agentId:string}[] }`（**类型先定义、数据后接**，与 Phase 3.5 引擎命令对齐）。
- [ ] **Step 1: 写失败测试** —— `winnerIndex` 高亮对应候选；`scores` 长度=候选数渲染分数条；无数据 → 空态文案「评判结果将在引擎接通后显示」（不报错）。
- [ ] **Step 2: 确认失败** → `npx vitest run judgeView` FAIL
- [ ] **Step 3: 实现**（复用 FanoutCompareView；judge 卡 + 空态；数据源 `judgeShow` 失败优雅降级）
- [ ] **Step 4: 确认通过 + build + lint** → 全绿
- [ ] **Step 5: 提交** `feat(helm): heterogeneous compare and judge verdict display (display-only)`

### ✅ GATE 4d
```bash
# 4d 全前端（judge 仅展示，引擎接通归 Phase 3.5）
npx vitest run src/components/helm && npm run build && npm run lint && git diff --check
git checkout feat/helm && git merge --no-ff feat/helm-phase4d-inspector -m "merge: Phase 4d — inspector: diff + merge/discard + judge display"
```

---

# 子阶段 4e：Roster + Composer（下达目标起编排）

> 分支：`git checkout feat/helm && git checkout -b feat/helm-phase4e-composer`
> 目标：Roster（可部署 CLI×模型清单，取自 provider 管理）+ Composer（下达 goal 起 run / 手动选兵 / 异构扇出，复用现有）。

## Task 12: Roster 面板（复用 provider 管理 + roster.ts）

**Files:**
- Create: `src/components/helm/RosterPanel.tsx`（复用现有 `roster.ts` + provider 清单）
- Test: `src/components/helm/roster*.test.ts`（复用/扩展现有 roster 测试）

**Interfaces:** Consumes：现有 provider 管理 + `fanout/roster.ts`。Produces：`RosterPanel`（列可部署 CLI×模型，供 Hermes 选兵 / 手动选兵共用；每项 CLI 图标 + 模型徽章）。
- [ ] **Step 1: 写失败测试**（roster 选择/去重纯逻辑） → **Step 2: FAIL** → **Step 3: 实现** → **Step 4: vitest+build** → **Step 5: 提交** `feat(helm): roster panel reusing provider roster`

## Task 13: Composer（下达目标起 run + 选兵 + 扇出）

**Files:**
- Create: `src/components/helm/HelmComposer.tsx`、`src/components/helm/launchPlan.ts`（goal+roster → 启动参数纯逻辑）
- Test: `src/components/helm/launchPlan.test.ts`

**Interfaces:**
- **复用（grounded，对齐 Hermes「下达目标 / 选兵」语境，不照搬单聊）**：
  - **模型/provider 选择 = Roster**：`SelectorDropdown`（model/mode/reasoning）+ `ModelIcon` + `roster.ts` + provider 管理——选「派哪些 CLI×模型」。这是 grounded 的核心。
  - 异构扇出：复用 Phase 1b `FanoutComposer`。
  - 安全：SDK 缺失拦截（复用 `ChatComposer` 守卫逻辑）。
  - **可选（合适才用，不强求）**：`PromptEnhancerDialog`（增强 goal 文本）、`@` 文件引用（goal 里引文件）。
  - **不适用（务实剔除）**：`/` 斜杠命令、逐条对话补全、"跟 worker 单聊"——Hermes 是**一次性下达 goal 给编排器**（Planner 选兵自动驱动 worker），不存在逐条 chat 回合，别硬塞单聊件。
- Produces：`buildLaunch(goal, opts, roster): { goal, opts }`（校验 goal 非空、maxConcurrent>0）；提交 → `hermesService.run`。键盘优先（⌘Enter 提交）。
- **「演示运行」入口（测试用）**：composer 旁加一个「演示运行」按钮 → 调 `hermesService.runMock()`（Phase 3.5 `hermes_run_mock`，脚本化 LLM、不烧 token）；命令未就绪时禁用态 + tooltip「需引擎 Phase 3.5」。让你在 UI 里点一下就能驱动整条编排看效果，不用 devtools。
- [ ] **Step 1: 写失败测试** —— 空 goal 拒绝；opts 归一化；扇出计划构造。
- [ ] **Step 2: FAIL** → **Step 3: 实现**（Roster 选兵 + 扇出 + SDK 守卫；空/错态文案；不塞不适用的单聊件）→ **Step 4: vitest+build+lint** → **Step 5: 提交** `feat(helm): composer with roster selection and run launch`

### ✅ GATE 4e
```bash
npx vitest run src/components/helm && npm run build && npm run lint && git diff --check
git checkout feat/helm && git merge --no-ff feat/helm-phase4e-composer -m "merge: Phase 4e — roster + composer"
```

---

# 子阶段 4f：人工干预点（全部做到友好）— 重头戏

> 分支：`git checkout feat/helm && git checkout -b feat/helm-phase4f-intervention`
> 目标：把**所有**人工干预点做成友好交互（用户反复强调）。复用现有三个对话框适配 Hermes 多 agent 语境；补 DecisionGate 应答 UI（+ 引擎侧 gate 列表/详情命令，契约 §6 共建）；卡住/失败友好浮现；破坏性操作统一安全模式。

## Task 14: 引擎侧 gate 列表/详情命令（契约 §6 共建，Rust 小改）

**Files:**
- Modify: `src-tauri/src/commands/hermes_commands.rs`（新增 `hermes_gate_list(filter)` / `hermes_gate_show(gateId)`——薄 delegate 到 `Store::list_gates`，已存在）、`src-tauri/src/lib.rs`（注册）
- Modify: `docs/helm-hermes-ui-contract.md`（§6 占位转正）
- Test: `hermes_commands.rs::tests`（GateDto 转换 + filter 解析）

**Interfaces:** Produces：`hermes_gate_list(filter?: {status?:string, taskId?:string}) -> GateDto[]`、`hermes_gate_show(gateId) -> GateDto`；`GateDto{ id, taskId, question, options, resolution, status }`（camelCase）。Consumes：`Store::list_gates(GateListFilter)`（store.rs:1158，已存在）、`Store::create_gate`/`resolve_gate`（已存在）。
- [ ] **Step 1: 写失败测试**（GateDto 转换 + filter）→ **Step 2: FAIL** → **Step 3: 实现**（薄 delegate）→ **Step 4: `cargo test hermes_commands` + `cargo test chat` 全绿 + build** → **Step 5: 提交** `feat(hermes): gate list/show commands for cockpit intervention`

## Task 15: 干预对话框适配 Hermes（复用三对话框 + DecisionGate 应答）

**Files:**
- Create: `src/components/helm/InterventionCenter.tsx`（统一干预入口：监听 needs-attention agent + pending gate，浮现对应对话框）、`src/components/helm/GateDialog.tsx`（DecisionGate 应答，调 `hermes_gate_resolve`）
- Reuse: `ToolPermissionDialog`/`AskUserQuestionDialog`/`PlanApprovalDialog`（适配多 agent：标明是哪个 agent/worktree 在请求）
- Test: `src/components/helm/interventionRouting.test.ts`（事件→对话框路由纯逻辑）

**Interfaces:** Produces：`interventionFor(agent: AgentView, gates: GateDto[]): InterventionKind | null`——`needs-attention` + 权限请求 → ToolPermission；ask-user → AskUserQuestion；plan → PlanApproval；pending gate → GateDialog。每个对话框**标明来源 agent + worktree**（多 agent 语境下「谁在问」必须清楚）。
- [ ] **Step 1: 写失败测试** —— 各干预类型路由到对应对话框；多个并发请求按 needs-attention 优先排队。
- [ ] **Step 2: FAIL** → **Step 3: 实现**（复用三对话框 + GateDialog；友好文案：说清哪个 agent 因何请求、选项后果）
- [ ] **Step 4: vitest + build + lint** → 全绿
- [ ] **Step 5: 提交** `feat(helm): intervention center routing reused dialogs + gate response`

## Task 16: 卡住/失败友好浮现 + 破坏性操作统一安全模式

**Files:**
- Create: `src/components/helm/AttentionToasts.tsx`（interrupted/failed agent 友好浮现：说清「发生什么 + 怎么办」+ 一键跳转/重试/查看）、`src/components/helm/destructiveConfirm.ts`（统一二次确认 + 回滚提示文案纯逻辑）
- Test: `src/components/helm/destructiveConfirm.test.ts`

**Interfaces:** Produces：`confirmCopyFor(action: "discard"|"delete-worktree"|"merge-conflict"): { title, body, confirmLabel, undoHint }`（统一破坏性文案：错误不道歉、说清后果与回滚）。interrupted/failed → 非阻塞 toast + 行动入口（跳转/查看 escalation）。
- [ ] **Step 1: 写失败测试** —— 三类破坏性动作各自文案完整（含 undoHint）；failed agent toast 带跳转动作。
- [ ] **Step 2: FAIL** → **Step 3: 实现**（reduced-motion；键盘可达；i18n）
- [ ] **Step 4: vitest + build + lint** → 全绿
- [ ] **Step 5: 提交** `feat(helm): friendly stuck/failure surfacing and unified destructive safety`

### ✅ GATE 4f
```bash
cargo test --manifest-path src-tauri/Cargo.toml hermes_commands && cargo test --manifest-path src-tauri/Cargo.toml chat
npx vitest run src/components/helm && npm run build && npm run lint && git diff --check
git checkout feat/helm && git merge --no-ff feat/helm-phase4f-intervention -m "merge: Phase 4f — friendly intervention points"
```

---

# 子阶段 4g：集成验收 + 打磨 + 交付

> 分支：`git checkout feat/helm && git checkout -b feat/helm-phase4g-verify`
> 目标：真事件端到端联调、键盘优先/实时性/可访问性审计、视觉自审返工、§10 质量基线逐条核对、交付文档。

## Task 17: 端到端集成 + 键盘/可访问性/视觉审计

**Files:**
- Modify: 按审计结果打磨各面板
- Test: 补关键交互的集成测试（vitest + testing-library 可覆盖的）
- Create: `docs/helm-phase4-verification.md`（§10 质量基线逐条核对 + 真 LLM e2e 步骤）

**审计清单（§10 硬标准逐条）：**
- [ ] **Step 1: UI 驱动的端到端（首选 mock 运行，不靠 devtools）** —— cockpit 提供「演示运行」入口（composer 旁，调 Phase 3.5 的 `hermes_run_mock`；命令未就绪时禁用态 + tooltip）。点它即在 **UI 里**驱动整条编排，三栏全联动：舰队板状态点实时刷、点卡进会话、diff/合并可用、干预浮现可应答、⌘K 可跳。**这是日常验收主通路**（不用敲 devtools 命令行）；真 LLM run 作为补充手测。
- [ ] **Step 2: 键盘优先审计** —— 切 agent/扇出/合并/丢弃/应答 gate 全有快捷键且可见焦点。
- [ ] **Step 3: 实时性 + 可访问性** —— 流式/状态点近实时无卡顿、长列表虚拟化；reduced-motion 尊重；明暗主题 + 移动窄屏过。
- [ ] **Step 4: 视觉自审（frontend-design）** —— 出截图对照「orca 级、零模板感」，不达标返工。
- [ ] **Step 5: 写验证文档 + 提交** `docs(helm): phase 4 verification (quality baseline checklist)`

## Task 18: 交付报告 + Helm 整体就绪

**Files:**
- Create: `docs/helm-phase4-delivery.md`（DoD 逐条 + §10 基线核对结果 + 改动清单 + git graph + 真 LLM e2e 待人工执行 + Helm（Phase 0–4）整体收尾状态）
- [ ] **Step 1: 写交付报告**（含 Helm 全貌：现在「下达目标 → 拆解 → 选兵 → 并行调度 → 收敛 → 驾驶舱可视可干预」闭环可用）
- [ ] **Step 2: `git log --oneline --graph`** 贴各子阶段
- [ ] **Step 3: 提交** `docs(helm): phase 4 delivery and helm overall readiness`

### ✅ GATE 4g（最终）
```bash
cargo test --manifest-path src-tauri/Cargo.toml          # 引擎非回归全绿
npx vitest run                                           # 前端全绿
npm run build && npm run lint && git diff --check
git checkout feat/helm && git merge --no-ff feat/helm-phase4g-verify -m "merge: Phase 4g — verification + delivery"
```

---

# 完成定义（DoD）= §10 Phase 4 质量基线逐条

**功能完整度**
- [ ] 三栏 cockpit（左舰队板 / 中会话·扇出 / 右 diff·合并·评判），面板可开合、单入口非路由。
- [ ] Kanban 看板（待派/执行中/待评审/完成）+ 跨泳道拖拽=干预 + 与 Task 状态双向同步。
- [ ] AgentStateDot 4 态（working/needs-attention/done/interrupted），严格区分等待输入(琥珀) vs 卡死(红)。
- [ ] ⌘K Jump Palette + 点 agent 卡跳其会话/worktree。
- [ ] 子代理可达（会话内联展开 sidechain，复用现有）。
- [ ] Roster 面板（CLI×模型，复用 provider 管理）。
- [ ] 编排 run 视图：worker 卡（CLI 图标+模型徽章+状态点+分支+diff）、gate/decision 浮现可应答。

**质感/可用性**
- [ ] 键盘优先（核心动作皆有快捷键）；实时刷新无卡顿 + 长列表虚拟化。
- [ ] 零模板感（frontend-design 基调落地，出可视稿评审过）。
- [ ] 破坏性操作安全（删/丢弃/合并冲突 = 预检+二次确认+回滚提示）。
- [ ] 可访问性地板：可见焦点 + reduced-motion + 明暗主题 + 移动窄屏。

**工程**
- [ ] 复用现有能力重组进 cockpit，不重复造；契约逐字对齐；i18n 同步 zh/en。
- [ ] `npm run build` + `npm run lint` + `npx vitest run` + 引擎 `cargo test` 全绿；`git diff --check` 干净。
- [ ] `docs/helm-phase4-verification.md` + `docs/helm-phase4-delivery.md` 已写；真 LLM e2e 标「待人工执行」，不造假。
- [ ] 做完 Task 18 即停。

# Self-Review（计划作者已核）

- **§10 覆盖**：三栏(4a Task4)、Kanban+拖拽(4b)、AgentStateDot(4b Task5)、⌘K+会话+子代理(4c)、diff/合并/judge(4d)、Roster+Composer(4e)、干预点友好+gate 命令(4f)、键盘/实时/视觉/可访问性审计(4g)。§10 每条 → 有 task。
- **复用映射**：会话渲染/SubagentHistoryPanel(4c)、FanoutCompareView/compare/roster(4d/4e)、三对话框(4f)、ChatDiffReviewPane(4d)——均复用非重写。
- **契约一致**：types/service(4a Task2) 逐字对齐契约 §3/§4/§5；新增 gate 命令(4f Task14) 同步契约 §6。
- **Phase 3 §5 待办归位（scope 决策：引擎归引擎、前端归前端）**：
  - `Planner::judge` 引擎接通（接 Coordinator + `hermes_judge_show` 命令）→ **engine Phase 3.5（GLM）**，不在本前端阶段；Phase 4 Task 11 只做 display + 空态，数据接通后自动点亮。
  - Run{cancelled} 双发 → 4a Task3 store reducer 幂等已吸收（前端侧）。
  - single-flight RAII / is_rfc3339 测试 helper 清理 → 引擎内部小修，并入 **Phase 3.5**。
  - **Phase 3.5（engine，GLM）= judge 接通 + 上述引擎小修 + 真 LLM 冒烟暴露的任何引擎 bug**；建议在 Phase 4 之前或并行（引擎/前端不同文件，并行需 worktree 隔离）执行。
- **人工确认点**：设计基调(4a Task1) 必须用户确认才进 4b（frontend-design 质量门）。
- **无占位符**：每个 code step 给真实 interface/纯函数签名 + 测试断言；TDD 先写失败测试。
