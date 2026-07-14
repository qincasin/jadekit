# 交接：Codex 全量执行 Helm × Hermes — Phase 4（驾驶舱 UI）

> 给执行 agent（Codex）的**冷启动**交接。请在一个**全新会话**里开工。本文件 + 引用文档即全部所需，逐字遵守纪律。Codex 从 Task 1 推进到 Task 18，每个子阶段结尾的 GATE 必须停下自检 + 合并。

## 0. 执行方式（防跑偏 + 质量门，最重要）

**两个强制技能：**
1. **`subagent-driven-development`**（防漂移最强：每 task 全新子 agent + 两道审查 + progress.md 检查点）。**Codex 是协调者，不是执行者**：切 brief、派子 agent、跑两道审查、按 progress.md 推进；**绝不自己埋头写代码**。不可用时降级 `executing-plans`。
2. **`frontend-design`**（质量门）：**4a Task 1 动任何视觉前必须加载它**做设计 pass + 出可视稿。这是 §10「零模板感」硬约束。若该技能在 Codex 环境不可用，则严格按计划「设计方向」节 + 设计文档 §10 手动执行，同样必须出可视稿自审。

> 若 superpowers 技能在 Codex 环境未安装，忽略计划头部 SUB-SKILL 行，按计划正文 TDD 步骤 + 每个子阶段 GATE 严格执行。

## 0b. 环境要求

可写工作区 + 可执行 `git`/`npm`/`cargo`/`codegraph`；**本 Phase 基本是纯前端**（仅极少引擎只读引用，不改 Rust 引擎）。需要起 `npm run tauri dev` 做联调/截图（4a Task1、4g）。请在 workspace-write + 可执行命令模式运行。

## 1. 角色与目标

实现者，**全量执行 Phase 4**：把现有「对话」surface 升级为 **Helm 驾驶舱**——三栏 cockpit（左舰队看板 / 中 Agent 会话·扇出 / 右 worktree diff·合并·评判）+ Kanban + AgentStateDot + ⌘K Jump + Roster + Composer + **所有人工干预点做到友好**。这是 Helm 的门面，**质量门槛最高**（§10 质量基线逐条验收）。**只做 Phase 4，做完 Task 18 即停。**

**明确不做（归 engine Phase 3.5 / GLM）**：不改 Rust 引擎逻辑。`Planner::judge` 的引擎接通归 Phase 3.5；本 Phase 的 judge 卡（Task 11）只做 **display + 空态**，数据接通后自动点亮。

## 2. 工作环境与分支约定（重要）

- 仓库：`/Users/jiaxing/code/github/jadekit`。`feat/helm` 当前含 Phase 0–3 全量（Phase 3 tip `2b7eb11`，本 Phase 4 计划/handoff commit 在其上）。
- **主功能分支 = `feat/helm`**（既非 main 也非 develop）。
- **每个子阶段从 `feat/helm` 拉自己的分支**（如 `feat/helm-phase4a-shell`），干完 + GATE 自检过 → `--no-ff` merge 回 `feat/helm`：
  ```bash
  git checkout feat/helm && git checkout -b feat/helm-phase4a-shell
  # ... 干完该子阶段 + GATE 绿 ...
  git checkout feat/helm && git merge --no-ff feat/helm-phase4a-shell -m "merge: Phase 4a ..."
  ```
- **不要** merge 到 main/develop。
- **本 Phase 串行单独执行**（不与别的 agent 并行）→ 在主工作树按子阶段分支推进即可，**无需 git worktree 隔离**。
- 子阶段顺序固定：**4a → 4b → 4c → 4d → 4e → 4f → 4g**。**4a Task 1 是人工确认点**（设计基调要用户点头才进 4b）。

## 3. 必读文档

1. **实现计划（逐步剧本，严格按它，含每子阶段分支与 GATE + 每 Task 准确性约束 + 复用映射）**：
   `docs/superpowers/plans/2026-06-28-helm-hermes-phase4-cockpit.md`
2. **引擎↔驾驶舱契约（Phase 4 施工图，命令/事件/DTO/状态词表的唯一事实源）**：
   `docs/helm-hermes-ui-contract.md` —— 前端 types/service 逐字对齐它，**禁止自创命令名/事件名/字段/token**。
3. **设计文档 §10 质量基线（验收硬标准，逐条 = DoD）**：
   `docs/superpowers/specs/2026-06-27-helm-hermes-design.md`（§10 / §6.3 判活区分 / §11 干预）。
4. **Phase 3 交付（引擎能力边界 + Phase 3.5 待办）**：`docs/helm-phase3-delivery.md §5`。
5. **项目规约**：`CLAUDE.md`、`AGENTS.md`。

## 4. 代码探索：codegraph 优先（jadekit + orca 均已 sync）

```bash
# jadekit 现有可复用件（必须复用、重组进 cockpit，不重写）
codegraph node ChatPage FanoutCompareView SubagentHistoryPanel ToolPermissionDialog AskUserQuestionDialog PlanApprovalDialog ChatDiffReviewPane
codegraph explore chat session sidebar tabs composer subagent runs store fanout roster compare
# orca 交互范本（只读他山之石，学交互细节非照抄像素）—— 在 orca 仓库跑
#   AgentStateDot.tsx / WorkspaceKanbanCard.tsx / workspace-kanban-card-pointer-drag-dom.ts
#   useComposerState.ts / agent-row-lineage.ts / Jump palette
```
仅纯文本字面量匹配才退回 grep。改完 `codegraph sync`。

## 5. 执行纪律（不可违反）

- **契约即事实源**：命令/事件/payload/状态 token 全部以 `docs/helm-hermes-ui-contract.md` + 真实源码为准，前端 TS 类型按契约 §4.2 判别联合定义，**禁自创**。
- **复用优先**：能复用的现有组件/纯函数（会话渲染、SubagentHistoryPanel、FanoutCompareView、compare/roster、三干预对话框、ChatDiffReviewPane）**必须复用 + 重组**，禁止重复造；确需新建先在 task 说明为何不能复用。
- **逐 task、逐 step、TDD 闭环**：UI 的纯逻辑（选择器/状态映射/泳道/拖拽落点/模糊匹配/破坏性文案）一律先写失败测试（vitest）→ 确认失败 → 最小实现 → 确认通过 → commit。视觉/交互用可视稿 + 审查覆盖。
- **每个子阶段 GATE 必须停下**：`npx vitest run` 相关 + `npm run build` + `npm run lint` + `git diff --check` 绿，才 `--no-ff` merge 回 `feat/helm`。
- **质量门（§10，不可降级）**：键盘优先（核心动作有快捷键 + 可见焦点）；实时刷新不卡 + 长列表虚拟化；零模板感（frontend-design 基调落地）；破坏性操作预检 + 二次确认 + 回滚提示；可访问性地板（reduced-motion + 明暗主题 + 移动窄屏）；每面板 empty/loading/error 三态文案。
- **i18n 同步**：所有用户可见文案进 `src/locales/zh.json` + `en.json`，禁硬编码。
- **非回归**：不破坏现有 chat——升级 ChatPage 时原对话路径保持可用；每个 GATE 确认 `npm run build` + 现有前端测试全绿。
- **失败处理**：测试/构建失败根因优先（`systematic-debugging`），不猜不掩盖；同一处连续 3 次修不好停下回报。
- **禁止**：攒批提交；改/删测试断言凑绿；跳过任一 step / GATE / 设计确认点；偏离契约自创字段；为省事重写已有可复用件。

## 6. 完成定义（DoD）= §10 质量基线逐条

见计划「完成定义（DoD）」节（功能完整度 7 条 + 质感/可用性 4 条 + 工程 3 条）。要点：三栏 cockpit、Kanban+拖拽干预、AgentStateDot 4 态（严格区分琥珀等待 vs 红卡死）、⌘K Jump、子代理可达、Roster、run 视图；键盘优先 + 实时 + 零模板 + 破坏安全 + 可访问性；复用重组 + 契约对齐 + i18n；`npm run build`/`lint`/`vitest` + 引擎 `cargo test` 全绿；验证 + 交付文档已写（真 LLM e2e 标「待人工执行」不造假）。做完 Task 18 即停。

## 7. 交付报告（收尾输出）

1. DoD 逐条状态（§10 基线逐条核对结果，每子阶段 GATE 是否过、是否已 merge 回 feat/helm）。
2. 改动清单（新增 `src/components/helm/*` 各文件职责 + 复用了哪些现有件）。
3. `git log --oneline --graph` 各子阶段分支与 merge。
4. 验证证据（`vitest`/`build`/`lint` 尾部 + 关键界面可视稿/截图；真事件联调结果；真 LLM e2e 手测或「待执行」）。
5. 偏差与未决（尤其 judge 数据待 Phase 3.5 接通、其它待 engine 项）。
6. Helm（Phase 0–4）整体就绪确认：「下达目标 → 拆解 → 选兵 → 并行调度 → 收敛 → 驾驶舱可视可干预」闭环是否可用。

## 8. 关键约束速记

不写魔法字符串（状态 token/事件名/命令名/快捷键/i18n key 集中常量）；契约逐字对齐；复用优先不重写；i18n 同步 zh/en；可访问性地板（焦点/reduced-motion/明暗/窄屏）；三态文案（empty/loading/error）；破坏性预检+二次确认+回滚提示；不改 Rust 引擎（judge 等引擎活归 Phase 3.5）。

## 9. 速度优化（不牺牲质量内核）

- **协调者热上下文**：开工把整份计划 + 契约 + §10 + 关键可复用件源码读进上下文常驻，子 agent brief 直接附「要复用的组件源码 + 该 Task 准确性约束」，减少重复探索。
- **两道审查并行派**（spec + quality 同时），纯延迟优化。
- **文件不重叠的独立纯逻辑 task 可并发**（如 `agentStateDot.ts` / `jumpSearch.ts` / `kanbanLanes.ts` / `destructiveConfirm.ts` 各自独立）；**任何改 `ChatPage.tsx` / `HelmCockpit.tsx` / `useHermesStore.ts` 等汇流文件的 task 一律串行**。
- **不许为快牺牲**：跳测试步 / 跳审查 / 跳 GATE / 跳 4a Task1 设计确认 / 攒批提交 / 重写已有可复用件——一条都不许碰。

---

开工顺序：起新会话 → 读计划 + 契约 + §10 + Phase 3 交付 → codegraph 摸 jadekit 可复用件 + orca 范本 → **4a Task1 frontend-design 出可视稿 → 暂停等用户确认基调** → 子阶段 4a→4g 各拉分支、TDD 推进、GATE 自检、`--no-ff` 合回 `feat/helm` → Task 18 收尾出交付报告。记住：契约即事实源、复用优先、每个 GATE 都要停都要绿都要合回；§10 质量基线逐条达标，达不到「orca 级真实好用」就返工；只做 Phase 4，做完即停。
