# 交接：GLM 全量执行 Helm × Hermes — Phase 3（引擎收尾 + 驾驶舱契约）

> 给执行 agent（GLM）的**冷启动**交接。请在一个**全新会话**里开工（不要续跑 Phase 2 的旧会话）——本文件 + 引用文档即全部所需，逐字遵守纪律。GLM 从 Task 1 推进到 Task 18，每个子阶段结尾的 GATE 必须停下自检 + 合并。

## 0. 执行方式（防跑偏，最重要）

**首选 `subagent-driven-development` 技能**——Phase 3 是 18 任务、改的是 Phase 2 刚落地的并发引擎，这套机制防漂移最强：
- **每个 task 派一个全新子 agent**，只给它该 task 的 brief（看不到其他任务），干完即弃 → 不累积上下文、不顺手改无关代码、不被前文带偏。
- **每个 task 完成后强制过两道审查**（spec-compliance + code-quality）才接收、才打勾、才 commit → 跑偏当场拦截。
- 用 `.superpowers/sdd/progress.md` 持久化检查点 → 长跑/压缩后能精确恢复到第几任务。

**GLM 在此模式下是协调者，不是执行者**：你负责按计划把 task 切成 brief、派子 agent、跑两道审查、按 progress.md 推进；**绝不自己埋头写代码**，绝不跳过审查，绝不用"未完成任务总表"代替当前任务的定向验证。

> 若 `subagent-driven-development` 不可用，降级用 `executing-plans`（单会话直跑），并更严格地守住计划里每个子阶段的 GATE。两者都不可用时，按计划正文 TDD 步骤手动执行。

## 0b. 环境要求

可写工作区 + 可执行 `git`/`cargo`/`npm`/`codegraph`；本 Phase **不新增 Rust 依赖**（`async-trait`、`portable-pty` Phase 2 已装），网络非必需。请在 workspace-write + 可执行命令模式运行。

## 1. 角色与目标

实现者，**全量执行 Phase 3**：在 Phase 2 的 Hermes 引擎之上做生产化收尾——冻结并落地「引擎↔驾驶舱」事件/命令契约（解锁 Phase 4）、异构介质单实例路由（一次 run 内 SDK×CLI 混跑）、run 内取消、收敛后 worktree/分支安全清理、并发 replan 单飞，补全真 LLM e2e 步骤。**这是并发 + 破坏性资源（worktree 清理）重灾区，纪律比速度重要。只做 Phase 3，做完 Task 18 即停，不要碰 Phase 4。**

## 2. 工作环境与分支约定（重要）

- 仓库：`/Users/jiaxing/code/github/jadekit`。当前 `feat/helm` tip = `80fd647`（含 Phase 0/1/1b/2 全量）。
- **主功能分支 = `feat/helm`**（既非 main 也非 develop）。
- **分支模型**：每个子阶段从 `feat/helm` 拉自己的分支，干完 + GATE 自检过 → `--no-ff` merge 回 `feat/helm`：
  ```bash
  git checkout feat/helm
  git checkout -b feat/helm-phase3a-contract   # 各子阶段分支名见计划
  # ... 干完该子阶段 + GATE 绿 ...
  git checkout feat/helm && git merge --no-ff feat/helm-phase3a-contract -m "merge: Phase 3a ..."
  ```
- **不要** merge 到 main/develop。
- **本 Phase 串行执行**（Phase 4 在 Phase 3 全部合回后才开始），所以你在**主工作树**上按子阶段分支推进即可，**无需 git worktree 隔离**。（worktree 隔离只在 Phase 3/4 真并行时才需要，本次不并行。）
- 子阶段顺序固定：**3a → 3b → 3c → 3d → 3e → 3f → 3g**。3a 是 keystone，必须最先完成并合回。

## 3. 必读文档

1. **实现计划（逐步剧本，严格按它，含每个子阶段的分支与 GATE + 每个 Task 的"准确性约束"）**：
   `docs/superpowers/plans/2026-06-28-helm-hermes-phase3-engine-completion.md`
   > 注意：计划里每个改动点都标了用 codegraph 核对过的真实签名/行号/约束（如 `coordinator_runs.status` 的 CHECK、`dispatch_one` 的 4 处 `self.runtime`、`WorktreeManager` 是关联函数风格、run_id 用 `OnceLock` 而非改签名）。**遇到与真实代码不一致就停下记录，按最小改动对齐计划意图，不扩张范围。**
2. **设计文档（全貌）**：`docs/superpowers/specs/2026-06-27-helm-hermes-design.md`（§4 契约、§6 组件、§9 工作流、§10 UI/UX 状态词表、§13 路线）。
3. **Phase 2 未决（本计划主体来源）**：`docs/helm-phase2-delivery.md`（§5 八条 gap）、`docs/helm-phase2-verification.md`（§D）。
4. **真实代码**：`src-tauri/src/hermes/{coordinator,supervisor,store,sdk_runtime,cli_runtime,runtime,types}.rs`、`src-tauri/src/commands/hermes_commands.rs`、`src-tauri/src/chat/worktree.rs`、`src-tauri/src/lib.rs`。
5. **生产范本（只读他山之石）**：orca `src/main/runtime/orchestration/{coordinator,db,lifecycle-reconciliation}.ts`。
6. **项目规约**：`CLAUDE.md`、`AGENTS.md`。

## 4. 代码探索：codegraph 优先（索引已 sync）

仓库已 `codegraph sync`。**探索代码一律先用 codegraph**，只有纯文本字面量匹配才退回 grep。改完代码 `codegraph sync` 重建。

```bash
codegraph node Coordinator WorkerSupervisor HermesEngine WorktreeManager Store
codegraph explore hermes runtime sink event dispatch watcher cancel reap
codegraph explore worktree merge remove has_uncommitted list
codegraph node AgentAssignment RuntimeKind   # assignment 字段：runtime/tool/model（无 provider）
```

## 5. 执行纪律（并发 + 破坏性资源重灾区，不可违反）

- **逐 task、逐 step、TDD 闭环**：先写失败测试 → 运行**亲眼确认失败** → 写**最小**实现（YAGNI） → 运行**确认通过** → 用计划给的信息 **commit** → 下一 task。
- **每个子阶段结尾的 GATE 必须停下**：跑该子阶段测试 + `cargo check`，绿了才 `--no-ff` merge 回 `feat/helm`，再进下一子阶段。
- **非回归红线（关键）**：本 Phase 全程加法式。所有新增钩子默认关闭（`NullEventSink` / `RuntimeRegistry::single` / `cancel=None` / `event_run_id` 空），不注入时 Coordinator/Supervisor 行为与 Phase 2 **逐字一致**。每个 GATE 必须确认 `cargo test chat`（69）+ Phase 2 引擎测试全绿。
- **破坏性资源安全红线（3d 尤其）**：worktree/分支清理**绝不**删除「已完成但未合并」的工作；`Remove` 执行 force 删除前**再查一次** `has_uncommitted_changes`，有改动则降级 `RetainForReview`。绝不把仓库留在脏/冲突态。
- **签名稳定红线**：`tick(&self)`、`dispatch_one(&self, task)`、`drain_inbox(&self)` 现有签名**不许改**（Phase 2 大量测试直接调它们）；run_id 用 `OnceLock` 内部可变传递（见计划 Task 2）。
- **禁止**：攒批提交；改/删测试断言凑绿；跳过任一步或任一 GATE；偏离计划新增设计（与真实代码冲突就停下记录，最小改动对齐，不扩张范围）。
- **并发不变量必须有测试**：事件发射、异构路由、mid-run cancel abort 在飞、单飞 replan、收敛清扫处置——每条都要有测试。
- **失败处理**：测试/编译/行为失败时**根因优先**（加载 `systematic-debugging`），不猜不乱试不用 try/catch 掩盖；同一处连续 3 次修不好就停下回报。

## 6. 完成定义（DoD）

- [ ] 计划 Task 1–18 全部 step 打勾、各自 commit；7 个子阶段（3a–3g）各过 GATE 并 `--no-ff` merge 回 `feat/helm`。
- [ ] `cargo check` 通过；`cargo test`（hermes + chat 全绿）。
- [ ] `npm run build` 通过；`git diff --check` 干净。
- [ ] **契约冻结**：`docs/helm-hermes-ui-contract.md` 与代码签名逐字一致（3a Task 5）。
- [ ] mock 下验证：异构混跑（3b）、mid-run cancel + abort 在飞（3c）、收敛清扫安全处置（3d）、单飞 replan（3e）。
- [ ] `docs/helm-phase3-verification.md` + `docs/helm-phase3-delivery.md` 已写；真 LLM e2e 标「待人工执行」，**不造假**。
- [ ] 做完 Task 18 即停。

## 7. 交付报告（收尾输出）

1. DoD 逐条状态（每子阶段 GATE 是否过、是否已 merge 回 feat/helm）。
2. 改动清单（新增 `hermes/events.rs`/`runtime_registry.rs`/`run_lifecycle.rs` 职责 + 既有文件加法式改动）。
3. `git log --oneline --graph` 各子阶段分支与 merge。
4. 验证证据（`cargo test`/`npm run build` 尾部；mock e2e：异构/cancel/cleanup/单飞各一段）。
5. 偏差与未决（尤其异构路由 / 取消 / 清理安全相关取舍）。
6. Phase 4 集成就绪确认（契约冻结 + 真事件可接）。

## 8. 关键约束速记

不写魔法字符串（事件名/状态/agent 状态词表集中常量）；配置集中 + 注明默认值；JSON camelCase + Rust snake_case；新命令注册 `lib.rs` `generate_handler!`；DB 改动照 rusqlite 风格（注意 `coordinator_runs.status` CHECK 要放行 `'cancelled'`）；新增代码补中文注释（并发/取消信号/清理策略/崩溃恢复尤其）；引擎只依赖 `AgentRuntime` / `OrchestrationEventSink` / `RuntimeRegistry` 抽象，不绑死介质、不碰 Tauri `AppHandle`。

## 9. 速度优化（token 充裕，GLM-5.2 1M 上下文）

目标：在**不削弱防漂移内核**（TDD 闭环、每 task 两道审查、每子阶段 GATE、串行合回 `feat/helm`）的前提下尽量快。token 不是约束，**用 token 换速度和确定性**。以下是允许的加速杠杆：

1. **协调者热上下文（首推）**：开工时一次性把「整份 Phase 3 计划 + 所有必读真实源文件（coordinator/supervisor/store/worktree/hermes_commands/lib + types/runtime/sdk_runtime/cli_runtime）」读进 1M 上下文常驻。之后每个 task 的 brief 直接从内存生成，**子 agent 不再重复 codegraph/读文件探索**——省掉每 task 的冷启动往返。
2. **两道审查并行跑**：每个 task 完成后的 spec-compliance 与 code-quality 两个审查 agent **同时派发**（用 `dispatching-parallel-agents` 技能），不要串行等。两者都过才接收。这是纯延迟优化，零鸠占。
3. **执行子 agent 一次到位**：给每个执行子 agent 的 brief 里**直接附上**它要改的文件全文 + Phase 2 同类代码范例 + 计划里该 Task 的"准确性约束"全文。token 管够 → 让它一次写对，减少"写错→审查打回→重写"的轮次。
4. **独立任务并行派发（限同一子阶段内、文件不重叠时）**：少数 task 文件互不重叠、互不依赖，可并发执行子 agent：
   - 3a Task 1（`events.rs` 新文件）与文档类 Task 5 可与其它解耦；
   - 3d Task 12（`run_lifecycle.rs` 纯函数 `decide_disposition`）是纯逻辑、独立；
   - 3e Task 15 的纯逻辑部分。
   - **硬约束**：**任何改 `coordinator.rs` 的 task 一律串行**（它是所有子阶段的汇流点，并行必冲突）。并行只在 Files 列表无交集时启用；有交集就老实串行。
5. **GATE 的测试可整段并行**：子阶段 GATE 里 `cargo test hermes` + `cargo test chat` + `npm run build` 三条无依赖，可并行跑省时间。

**不允许为了快牺牲的**：跳过/合并任一 task 的失败测试步、跳过任一道审查、跳过任一子阶段 GATE、跳过 `--no-ff` 串行合回、把多个 task 攒成一个大 commit。这些是防漂移的命根子，再快也不许动。

---

开工顺序：起新会话 → 读计划 + 设计 + Phase 2 交付 → codegraph 摸 Coordinator/Store/WorktreeManager 现状 → 子阶段 3a→3g 各拉分支、TDD 推进、GATE 自检、`--no-ff` 合回 `feat/helm` → Task 18 收尾出交付报告。记住：3a 是 Phase 4 的硬依赖必须先合；这是并发 + 破坏性资源重灾区，每个 GATE 都要停、都要绿、都要合回主功能分支；做完 Phase 3 即停，不碰 Phase 4。
