# 交接：GLM 全量执行 Helm × Hermes — Phase 2（Hermes 编排引擎）

> 给执行 agent（GLM）的冷启动交接。GLM 从 Task 1 推进到 Task 18，每个子阶段结尾的 GATE 必须停下自检 + 合并。本文件 + 引用文档即全部所需，逐字遵守纪律。

## 0. 执行方式（防跑偏，最重要）

**首选 `subagent-driven-development` 技能**——Phase 2 是 18 任务、并发重灾区的长计划，这套机制防漂移最强：
- **每个 task 派一个全新子 agent**，只给它该 task 的 brief（看不到其他任务），干完即弃 → 不累积上下文、不顺手改无关代码、不被前文带偏。
- **每个 task 完成后强制过两道审查**（spec-compliance + code-quality）才接收、才打勾、才 commit → 跑偏当场拦截。
- 用 `.superpowers/sdd/progress.md` 持久化检查点 → 长跑/压缩后能精确恢复到第几任务。

**GLM 在此模式下是协调者，不是执行者**：你负责按计划把 task 切成 brief、派子 agent、跑两道审查、按 progress.md 推进；**绝不自己埋头写代码**，绝不跳过审查，绝不用"未完成任务总表"代替当前任务的定向验证。

> 若 `subagent-driven-development` 不可用，降级用 `executing-plans`（单会话直跑），并更严格地守住计划里的 6 个子阶段 GATE。两者都不可用时，按计划正文 TDD 步骤手动执行。

## 0b. 环境要求

可写工作区 + 可执行 `git`/`cargo`/`npm`；可能新增 Rust 依赖（`async-trait`、`portable-pty`）需联网 `cargo fetch`。请在 workspace-write + 网络可用模式运行。codegraph 优先用 MCP，没有就用 codegraph CLI。

## 1. 角色与目标

实现者，**全量执行 Phase 2**：在 Helm 之上构建 Hermes 编排引擎（AgentRuntime 契约 + SDK/CLI 介质 + SQLite Store + Coordinator + Supervisor + Planner），并先修一个 Phase 1b 遗留 bug。这是**并发重灾区**，纪律比速度重要。

## 2. 工作环境与分支约定（重要）

- 仓库：`/Users/jiaxing/code/github/jadekit`。
- **主功能分支 = `feat/helm`**（既非 main 也非 develop）。它已含 Phase 0/1/1b 全量。
- **分支模型**：每个子阶段从 `feat/helm` 拉自己的分支，干完 + GATE 自检过 → `--no-ff` merge 回 `feat/helm`：
  ```bash
  git checkout feat/helm
  git checkout -b feat/helm-phase2a-runtime   # 各子阶段分支名见计划
  # ... 干完该子阶段 + 自检 ...
  git checkout feat/helm && git merge --no-ff feat/helm-phase2a-runtime -m "merge: Phase 2a ..."
  ```
- **不要** merge 到 main/develop。worktree 隔离时也基于 `feat/helm`。
- 子阶段 2·pre 的小修（Task 1）可直接在 `feat/helm` 上做。

## 3. 必读文档

1. **实现计划（逐步剧本，严格按它，含每个子阶段的分支与 GATE）**：
   `docs/superpowers/plans/2026-06-28-helm-hermes-phase2-engine.md`
2. **设计文档（全貌，落地依据）**：`docs/superpowers/specs/2026-06-27-helm-hermes-design.md`（§4/§6/§7/§9/§13）
3. **生产级范本（只读他山之石）**：orca `/Users/jiaxing/code/github/orca/src/main/runtime/orchestration/{types,db,coordinator,lifecycle-reconciliation}.ts`
4. **jadekit 真实代码**：`src-tauri/src/chat/{manager,pool,daemon_client,worktree}.rs`、`src-tauri/src/database/{mod,schema}.rs`（rusqlite 范式）、`src-tauri/src/commands/chat_commands.rs`
5. **项目规约**：`CLAUDE.md`（含「代码探索 codegraph」）、`AGENTS.md`

## 4. 代码探索：codegraph 优先

```bash
codegraph node ChatManager AgentPool DaemonClient WorktreeManager
codegraph explore database rusqlite schema migration connection
codegraph explore chat send abort agent_id stream done event
```
改完 `codegraph sync`。仅找纯文本时才 grep。

## 5. 执行纪律（并发重灾区，不可违反）

- **逐 task、逐 step、TDD 闭环**：先写失败测试 → 运行**亲眼确认失败** → 写**最小**实现（YAGNI） → 运行**确认通过** → 用计划给的信息 **commit** → 下一 task。
- **每个子阶段(2a–2g)结尾的 GATE 必须停下**：跑该子阶段测试 + `cargo check`，绿了才 `--no-ff` merge 回 `feat/helm`，再进下一子阶段。这是防止并发 bug 攒到最后的关卡。
- **禁止**：攒批提交；改/删测试断言凑绿；跳过任一步或任一 GATE；偏离计划新增设计（与真实代码冲突就停下记录，按最小改动对齐计划意图，不扩张范围）。
- **并发不变量必须有测试**：Store 的 promote_ready 与 update_status 同事务；熔断 3 次；判活分级 + WaitingInput 永不被杀 + tool_use 未闭合不判卡死；崩溃恢复对账。
- **失败处理**：测试/编译/行为失败时**根因优先**（systematic debugging），不猜不乱试不用 try/catch 掩盖；同一处连续 3 次修不好就停下回报。
- **加法式**：不破坏现有 chat —— 每个 GATE 都要确认 `cargo test chat` 仍全绿。

## 6. 完成定义（DoD）

- [ ] 计划 Task 1–18 全部 step 打勾、各自 commit；6 个子阶段各自过 GATE 并 `--no-ff` merge 回 `feat/helm`。
- [ ] `cargo check` 通过；`cargo test --manifest-path src-tauri/Cargo.toml`（hermes + chat 全绿）。
- [ ] `npm run build` 通过；`git diff --check` 干净。
- [ ] 整引擎在 **mock AgentRuntime** 下端到端跑通（Coordinator+Store+Supervisor+Planner 闭环）。
- [ ] `docs/helm-phase2-verification.md` 已写；真 LLM 路径手动 e2e 未跑标「待人工执行」，**不造假**。
- [ ] 做完 Task 18 即停。

## 7. 交付报告

1. DoD 逐条状态（每个子阶段 GATE 是否过、是否已 merge 回 feat/helm）。
2. 改动清单（新增 `src-tauri/src/hermes/*` 各文件职责）。
3. `git log --oneline --graph` 展示各子阶段分支与 merge。
4. 验证证据（`cargo test` / `npm run build` 末尾输出；mock 端到端结果；真 LLM 手测或「待执行」）。
5. 偏差与未决（尤其任何并发/介质相关取舍）。
6. 是否就绪进 Phase 3（完整 DAG/Gate/Message 总线增强 + LLM-judge）。

## 8. 关键约束速记

不写魔法字符串（状态/消息类型/表名/字段/事件名集中 enum/常量）；配置集中 + 注明默认值；JSON camelCase + Rust snake_case；新命令注册 `lib.rs`；DB 用 rusqlite 照 `database/` 风格(WAL/迁移/事务)；新增代码补中文注释（并发/事务/状态流转/崩溃恢复尤其）；引擎只依赖 `AgentRuntime` 契约，不绑死介质。

---

开工顺序：读文档 + codegraph 摸现有 chat/db → 在 `feat/helm` 做 Task 1（worktree 泄漏修复）→ 按子阶段 2a→2g 各拉分支、TDD 推进、GATE 自检、`--no-ff` 合回 `feat/helm` → Task 18 最终验收 → 出交付报告。记住：这是并发重灾区，每个 GATE 都要停、都要绿、都要合回主功能分支。
