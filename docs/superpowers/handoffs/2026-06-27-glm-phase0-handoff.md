# 交接：GLM 执行 Helm × Hermes — Phase 0（多 Agent Daemon 池）

> 这是给执行 agent（GLM）的冷启动交接。你没有先前对话上下文，本文件 + 引用的两份文档即全部你需要的东西。**逐字遵守执行纪律,不要自由发挥。**

## 0. 你的角色与目标

你是本任务的实现者。目标：把 jadekit chat 后端从「单 daemon、`SESSION_ID="default"` 写死、全局 abort」改造成「按 `agent_id` 索引的 daemon 池」，使多个会话各自独立 daemon 进程 / 独立 cwd / 独立 session / 独立 abort，真正并行、互不踩 `process.chdir`。

**范围 = 只做 Phase 0。** 做完 Phase 0 的全部 7 个 task 并通过验收即停，**不要**开始 Phase 1/2/3。

## 1. 工作环境

- 仓库：`/Users/jiaxing/code/github/jadekit`（Tauri 2 + React 19 + TS + Rust）。
- **先建分支再动手**（不要在 main/默认分支直接改）：
  ```bash
  cd /Users/jiaxing/code/github/jadekit
  git checkout -b feat/helm-phase0-daemon-pool
  ```
- 开发命令：后端 `cargo check --manifest-path src-tauri/Cargo.toml`、`cargo test --manifest-path src-tauri/Cargo.toml`；前端 `npm run build`、`npx vitest run <file>`。

## 2. 必读文档（开工前全部读完）

1. **实现计划（你的逐步剧本）**：`docs/superpowers/plans/2026-06-27-helm-hermes-phase0-daemon-pool.md`
   —— 7 个 task，每个含精确文件、接口签名、可粘贴代码、确切命令与预期输出。**严格按它执行。**
2. **设计文档（背景与全貌）**：`docs/superpowers/specs/2026-06-27-helm-hermes-design.md`
   —— 理解 Phase 0 在整个 Helm/Hermes 里的位置；§3 解释了为什么必须进程池。
3. **项目规约**：仓库根 `CLAUDE.md`（尤其「代码探索 codegraph」「最高优先级规则」）与 `AGENTS.md`。

## 3. 代码探索：用 codegraph，别盲目 grep

仓库已建 codegraph 索引（`.codegraph/codegraph.db`）。探索代码优先：
```bash
codegraph node DaemonClient
codegraph node ChatManager
codegraph explore chat send abort daemon session permission
codegraph callers <symbol>   # 改一个符号前先看谁调用它
codegraph impact <symbol>    # 评估改动影响面
```
改完源码可 `codegraph sync` 刷新索引。仅找纯文本/字面量时才用 grep。

## 4. 执行纪律（最重要，不可违反）

**逐 task、逐 step、TDD 闭环、逐 task commit。** 对计划里的每个 task：

1. 按 Step 顺序做，**先写失败测试**。
2. **运行测试，亲眼确认它失败**（看到预期的失败信息）。不要跳过这一步。
3. 写**最小**实现让测试通过（不超纲、不顺手加计划没要求的功能 —— YAGNI）。
4. **运行测试，确认通过**。
5. **commit**（用计划里给的 Conventional Commit 信息）。
6. 进入下一个 task。

**禁止**：
- 禁止把多个 task 攒到一起再测/再提交。
- 禁止为了让测试变绿而改测试断言或删测试（除非计划明确要求改）。
- 禁止跳过「确认失败」「确认通过」「commit」任一步。
- 禁止偏离计划新增设计；如果发现计划某处与真实代码冲突，**停下来，在交付报告里记录冲突点和你的处理**，按最小改动对齐计划意图，不要擅自扩张范围。

## 5. 遇到失败怎么办（系统化调试）

测试失败 / 编译失败 / 行为异常时：
- **先定位根因再改**，不要靠猜乱试、不要加无关 try/catch 掩盖。
- 读真实报错与相关源码（codegraph）；形成假设 → 最小验证 → 修复。
- 修复后重跑该 task 的测试 + `cargo check`。
- 连续 3 次同一处修不好，**停下来**，在报告里写清现象、已试方案、卡点，交回人类。

## 6. 阶段边界与「完成定义」（DoD）

Phase 0 完成 = 同时满足：
- [ ] 计划 Task 1–7 全部 step 打勾，每个 task 已 commit。
- [ ] `cargo check --manifest-path src-tauri/Cargo.toml` 通过。
- [ ] `cargo test --manifest-path src-tauri/Cargo.toml chat` 全绿。
- [ ] `npm run build` 通过；`npx vitest run src/stores/chatEventRouting.test.ts` 通过。
- [ ] `git diff --check` 无空白错误。
- [ ] **手动 e2e（Task 7）已实跑并把结果回填**到 `docs/helm-phase0-verification.md`：
      开两个 tab、不同 cwd、同时发长任务 → 确认是各自独立 node ai-bridge 进程（`ps` 可见多个）、互不阻塞/串流；abort 一个不影响另一个；关闭 app 后所有 daemon 退出。

⚠️ 关于「测试」：本项目中**手动 e2e 是验收的一部分**，不是只跑单测就算完。但也**不要**反复重跑已经绿的单测充数。

完成后**停止**，不要开始 Phase 1。

## 7. 交付报告（最后输出给人类）

用如下结构回报：
1. **结果**：Phase 0 是否全部完成（DoD 逐条勾选状态）。
2. **改动清单**：新增/修改的文件 + 一句话职责。
3. **提交记录**：`git log --oneline` 本分支的提交。
4. **验证证据**：`cargo test` / `npm run build` 末尾输出；手动 e2e 的实际观察（进程数、并行性、abort 隔离、退出清理）。
5. **偏差与未决**：任何与计划冲突的地方、你的处理、以及需要人类决策的卡点。
6. **下一步建议**：是否就绪进入 Phase 1。

## 8. 关键约束速记（详见计划 Global Constraints）

- 不写魔法字符串（agent_id / 事件名 / env key / 默认值集中为常量）。
- JSON 字段 camelCase（`#[serde(rename)]`），Rust 内部 snake_case。
- 向后兼容：`agent_id` 缺省回退 `"default"`；旧事件字段保留，`agentId` 为附加字段。
- 新增 Tauri 命令必须在 `src-tauri/src/lib.rs` 的 `generate_handler!` 注册。
- 新增代码补中文注释（并发边界、进程生命周期、状态流转处尤其要写）。
- daemon 操作只经 `DaemonClient` / `ManagerDaemonClient`，不要绕过抽象直接操作进程或文件。

---

开工顺序：建分支 → 读三份文档 + codegraph 摸 `DaemonClient`/`ChatManager` → 从 Task 1 Step 1 开始，按 TDD 闭环逐 task 推进到 Task 7 → 跑 DoD → 出交付报告。
