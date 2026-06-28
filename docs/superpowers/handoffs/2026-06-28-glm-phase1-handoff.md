# 交接：GLM 执行 Helm × Hermes — Phase 1（Worktree 隔离）

> 给执行 agent（GLM）的冷启动交接。你没有先前对话上下文，本文件 + 引用文档即全部所需。**逐字遵守执行纪律,不要自由发挥。**

## 0. 前置门（未过不许开工）

⚠️ **Phase 1 依赖 Phase 0 的「多 daemon 真并行」已被人工 e2e 验证通过。** 开工前确认：
- `docs/helm-phase0-verification.md` 的「§3.2 手动 e2e」已回填且结论为通过（两个 tab 各自独立 `node daemon.js` 进程、互不串流、abort 隔离、退出全清理）。
- 若该 e2e 仍是「待人工执行」，**停止**，回报人类先跑 Phase 0 e2e，不要开始 Phase 1。

## 1. 你的角色与目标

实现者。目标：在 Phase 0 daemon 池之上，给每个 Agent 绑定一个独立 git worktree，使多 Agent 并行改同一 repo 互不踩；并先修两个 Phase 0 code review 发现的真实缺陷。**只做 Phase 1**，做完 8 个 task 并通过验收即停，不要碰 Phase 1b（扇出）/Phase 2。

## 2. 工作环境

- 仓库：`/Users/jiaxing/code/github/jadekit`。
- **从 Phase 0 分支拉新分支**：
  ```bash
  cd /Users/jiaxing/code/github/jadekit
  git checkout feat/helm-phase0-daemon-pool
  git checkout -b feat/helm-phase1-worktree
  ```
- 命令：`cargo check/test --manifest-path src-tauri/Cargo.toml`；`npm run build`；`npx vitest run <file>`。

## 3. 必读文档

1. **实现计划（逐步剧本，严格按它）**：`docs/superpowers/plans/2026-06-28-helm-hermes-phase1-worktree.md`
2. **设计文档（全貌）**：`docs/superpowers/specs/2026-06-27-helm-hermes-design.md`（§6.2 WorktreeManager、§10 UI）
3. **Phase 0 实现**（你要在其上构建）：`docs/superpowers/plans/2026-06-27-helm-hermes-phase0-daemon-pool.md` + 真实代码 `src-tauri/src/chat/{pool,manager,worktree?}.rs`
4. **项目规约**：`CLAUDE.md`（含「代码探索 codegraph」）、`AGENTS.md`

## 4. 代码探索：codegraph 优先

```bash
codegraph node AgentPool
codegraph node ChatManager
codegraph explore worktree git branch cwd chat_send permission watcher
codegraph callers <symbol>   # 改符号前先看调用方
```
改完 `codegraph sync`。仅找纯文本时才 grep。

## 5. 执行纪律（不可违反）

逐 task、逐 step、TDD 闭环、逐 task commit：
1. 先写失败测试 → 2. 运行**亲眼确认失败** → 3. 写**最小**实现（YAGNI） → 4. 运行**确认通过** → 5. **commit**（用计划给的信息） → 下一 task。

**禁止**：攒批提交；改/删测试断言凑绿；跳过任一步；偏离计划新增设计（与真实代码冲突时停下记录，按最小改动对齐计划意图，不扩张范围）。

**特别注意（Phase 1 破坏性资源）**：worktree 建/删是真实文件与 git 操作。删除前必须脏检查预检（计划 Task 2 的 `has_uncommitted_changes` + Task 6 的 force 控制）。**绝不强删用户未确认的工作树。**

## 6. 失败处理

根因优先，不猜不乱试、不用 try/catch 掩盖。读真实报错 + codegraph 形成假设→最小验证→修复→重跑该 task 测试 + `cargo check`。同一处连续 3 次修不好就停下回报。

## 7. 完成定义（DoD）

- [ ] 计划 Task 1–8 全部 step 打勾并各自 commit。
- [ ] `cargo check --manifest-path src-tauri/Cargo.toml` 通过。
- [ ] `cargo test --manifest-path src-tauri/Cargo.toml chat` 全绿（含新增 `chat::worktree::*`、`pool::concurrent_init_stops_discarded_loser`、`close_agent_*`）。
- [ ] `npm run build` 通过；新增 vitest（`chatSendCwd`、`worktreeBadge`）全绿。
- [ ] `git diff --check` 无空白错误。
- [ ] `docs/helm-phase1-verification.md` 已写；手动 e2e 若未跑标「待人工执行」，**不要造假**。

完成后停止，不要开始 Phase 1b/Phase 2。

## 8. 交付报告

1. 结果（DoD 逐条状态）。2. 改动清单（文件+职责）。3. `git log --oneline` 本分支提交。4. 验证证据（`cargo test`/`npm run build` 末尾输出；手动 e2e 观察或「待执行」）。5. 偏差与未决。6. 下一步建议（是否就绪 Phase 1b）。

## 9. 关键约束速记

不写魔法字符串（worktree 分支前缀/命令名/目录集中常量）；JSON camelCase + Rust snake_case；新命令注册到 `lib.rs` `generate_handler!`；worktree/daemon 操作经抽象；新增代码补中文注释；i18n 文案同步 `zh.json`/`en.json`；删除 worktree 必须预检。

---

开工顺序：确认 Phase 0 e2e 已过 → 从 `feat/helm-phase0-daemon-pool` 拉 `feat/helm-phase1-worktree` → 读文档 + codegraph 摸 `AgentPool`/`ChatManager` → 从 Task 1（修池竞态）开始按 TDD 推进到 Task 8 → 跑 DoD → 出交付报告。
