# 交接：执行 Helm × Hermes — Phase 1b（异构扇出）

> 给执行 agent（Codex / GLM 均可）的冷启动交接。本文件 + 引用文档即全部所需。逐字遵守纪律,不要自由发挥。

## 0. 环境要求（Codex 注意）

本任务需要：可写工作区、可执行 `git`/`cargo`/`npm` 命令。**不新增 Rust/npm 依赖**（`tempfile`、`vitest` Phase 1 已装），故网络非必需。请在 workspace-write + 可执行命令的模式下运行。若 superpowers 技能未安装，忽略计划头部 SUB-SKILL 行，按正文 TDD 步骤执行。

## 1. 角色与目标

实现者。目标：在 Phase 1 worktree 隔离之上实现**异构扇出**——一个 prompt 扇出到 N 个不同 CLI×模型的 Agent（各自独立 worktree）并行跑 → 并排对比 + diff → 选赢家合并 / 丢弃其余。这是 Helm 第一个"看得见的驾驶舱"能力。**只做 Phase 1b**，做完即停，不要碰 Phase 2。

## 2. 工作环境

- 仓库：`/Users/jiaxing/code/github/jadekit`。
- **从 Phase 1 分支拉新分支**：
  ```bash
  cd /Users/jiaxing/code/github/jadekit
  git checkout feat/helm-phase1-worktree
  git checkout -b feat/helm-phase1b-fanout
  ```
- 命令：`cargo check/test --manifest-path src-tauri/Cargo.toml`；`npm run build`；`npx vitest run <file>`。

## 3. 必读文档

1. **实现计划（逐步剧本，严格按它）**：`docs/superpowers/plans/2026-06-28-helm-hermes-phase1b-fanout.md`
2. **设计文档（全貌）**：`docs/superpowers/specs/2026-06-27-helm-hermes-design.md`（§9 多模型工作流、§10 UI）
3. **Phase 1 实现（你在其上构建）**：真实代码 `src-tauri/src/chat/worktree.rs`、`commands/chat_commands.rs`、`src/services/worktreeService.ts`、`src/stores/useChatStore.ts`
4. **项目规约**：`CLAUDE.md`（含「代码探索 codegraph」）、`AGENTS.md`

## 4. 代码探索：codegraph 优先

```bash
codegraph node WorktreeManager
codegraph explore worktree merge fanout provider model chat_send close_agent
codegraph node Provider   # 确认 chatProvider 推断字段真实名（appType/app_type/category）
```
改完 `codegraph sync`。仅找纯文本时才 grep。

## 5. 执行纪律（不可违反）

逐 task、逐 step、TDD 闭环：先写失败测试 → 运行**亲眼确认失败** → 写**最小**实现（YAGNI） → 运行**确认通过** → 用计划给的信息 **commit** → 下一 task。

**禁止**：攒批提交；改/删测试断言凑绿；跳过任一步；偏离计划新增设计（与真实代码冲突就停下记录，按最小改动对齐计划意图，不扩张范围）。

**破坏性资源（强化）**：
- 合并冲突必须 `git merge --abort` 回滚，绝不把主仓库留在冲突态（计划 Task 2 已实现并实测）。
- 丢弃 worktree / force 删除：前端必须**二次确认**后才执行。
- 真实字段以 codegraph 查到的为准（如 `Provider` 的 CLI 类型字段名），测试同步真实字段，别照搬计划里的占位字段名。

## 6. 失败处理

根因优先，不猜不乱试不掩盖；同一处连续 3 次修不好就停下回报。

## 7. 完成定义（DoD）

- [ ] 计划 Task 1–7 + Task 9 全部 step 打勾并各自 commit（Task 8 LLM-judge 为可选 stretch，可跳过；跳过就在报告里说明）。
- [ ] `cargo check` 通过；`cargo test --manifest-path src-tauri/Cargo.toml chat` 全绿（含新增 `merge_*`、`diff_summary_includes_untracked_new_file`）。
- [ ] `npm run build` 通过；新增 vitest（`fanoutPlan`、`fanoutGroup`、`roster`、`compare`）全绿。
- [ ] `git diff --check` 干净。
- [ ] `docs/helm-phase1b-verification.md` 已写；手动 e2e 未跑标「待人工执行」，**不要造假**。
- [ ] 做完即停，不要碰 Phase 2。

## 8. 交付报告

1. DoD 逐条状态（Task 8 做没做要说明）。2. 改动清单（文件+职责）。3. `git log --oneline` 本分支提交。4. 验证证据（`cargo test`/`npm run build` 末尾输出 + 手动 e2e 观察或「待执行」）。5. 偏差与未决。6. 是否就绪进 Phase 2。

## 9. 关键约束速记

不写魔法字符串（fanout 状态/命令名/分支前缀集中常量）；JSON camelCase + Rust snake_case；新命令注册 `lib.rs` `generate_handler!`；worktree/git 操作经抽象；新增代码补中文注释；i18n 同步 `zh.json`/`en.json`；合并冲突 abort 回滚；删除/force 前二次确认。

---

开工顺序：开分支 → 读文档 + codegraph 摸 WorktreeManager/Provider → Task 1（修 diff 口径）→ Task 2-3（merge 能力）→ Task 4-7（前端扇出/对比/选赢家）→ Task 9（验证）→（可选 Task 8）→ 出交付报告。
