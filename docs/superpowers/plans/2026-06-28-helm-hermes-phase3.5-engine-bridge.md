# Helm × Hermes — Phase 3.5：引擎小补（GLM 执行，刻意保持小）

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development（每 task 全新子 agent + 两道审查）；不可用降级 executing-plans。Steps 用 checkbox。

**Goal:** 在 Phase 3 引擎之上做**三件小补**，把 Phase 3 遗留 + Phase 4 驾驶舱真正需要的后端数据补齐：① worker transcript 桥（让 cockpit 能显示 worker 真实会话）；② judge 接通 Coordinator（Phase 3 §5 遗留）；③ 引擎小清理 + 单-agent abort。**刻意保持小、加法式、不重构引擎**；做完即停。

**Architecture:** 全程加法式、非回归（默认不触发的钩子/新命令，旧路径 byte-identical）。复用已有能力：`session_manager::load_messages`、`Planner::judge`（Phase 3f 已实现）、`runtime.abort` + `Store::list_active_dispatches`（已存在）。

**Tech Stack:** Rust（rusqlite、tokio）、Tauri 2。

## 分支约定（沿用）

- 主功能分支 = `feat/helm`。从它拉 `feat/helm-phase3.5-bridge`，GATE 过后 `--no-ff` 合回。不入 main/develop。
- **与 Phase 4 关系**：Phase 4（Codex）已按 grounded 做（cockpit 先活动流 + transcript-ready 容器）；本 Phase 3.5 提供 `hermes_worker_transcript` / `hermes_judge_show` / `hermes_agent_abort` 后，Phase 4 对应位置**只换数据源即点亮**，无需重做 UI。**引擎/前端文件不重叠**，可串行也可（各用 worktree）并行。

## Global Constraints

- 加法式、非回归：每个新命令/钩子默认不影响 Phase 2/3 旧行为；GATE 必须 `cargo test hermes` + `cargo test chat` 全绿。
- 不写魔法字符串；新命令注册 `lib.rs`；DTO camelCase + Rust snake_case；契约同步 `docs/helm-hermes-ui-contract.md`。
- 新增代码补中文注释；新增能力补测试。
- 提交前 `cargo check` + `cargo test`（hermes+chat）+ `git diff --check`。
- **刻意小**：不重构 Coordinator/Store 结构；只加最小接缝。与真实代码冲突就停下记录、最小对齐，不扩张。

## 阅前必读（codegraph 优先）

```bash
codegraph node Planner Coordinator Store SdkRuntime DispatchContext
codegraph explore session load_messages source_path provider session_id
codegraph explore judge JudgeVerdict planner replan
```
- `src-tauri/src/session_manager/mod.rs`（`load_messages(provider, source_path)` / `load_message_window` 已存在）。
- `src-tauri/src/hermes/{sdk_runtime,planner,coordinator,store}.rs`、`src-tauri/src/commands/hermes_commands.rs`、`docs/helm-hermes-ui-contract.md`。

---

## Task 1: worker transcript 桥（cockpit 显示 worker 真实会话的数据源）

> 背景：worker 经 `SdkRuntime.send_raw_stream` 跑，流被引擎内部消费，前端只拿到粗粒度 `hermes://agent`。但 worker 经 daemon 跑会**落盘 session 文件**（SDK 行为），且 `session_manager::load_messages(provider, source_path)` 已能解析。本 task 把这条数据接出来。

**Files:**
- Modify: `src-tauri/src/hermes/sdk_runtime.rs`（**停止丢弃** worker 的 session 来源信息——现 `[SESSION_ID]` 标签被 parse 成 None 丢掉；改为在消费循环里捕获 worker 的 session 标识，回传/记录）
- Modify: `src-tauri/src/hermes/store.rs` 或 dispatch 记录（把 worker 的 `provider` + `source_path`（或 session_id + cwd）落到可查处——最小：扩 `DispatchContext` 一个可空字段 `transcript_source: Option<String>`，或用现有 messages/run 元数据）
- Modify: `src-tauri/src/commands/hermes_commands.rs`（新增 `hermes_worker_transcript(dispatchId|agentId) -> Vec<MessageDto>`：解析该 worker 的 (provider, source_path) → 复用 `session_manager::load_messages` → 返回前端可渲染的消息）+ `lib.rs` 注册
- Modify: `docs/helm-hermes-ui-contract.md`（补 `hermes_worker_transcript` 命令 + `MessageDto` 形状——对齐现有 chat 的消息结构，前端复用 MessageList/toolBlocks 渲染）
- Test: `sdk_runtime.rs::tests`（session 来源捕获）、`hermes_commands.rs::tests`（命令解析 + 空/缺失优雅降级返回空）

**Interfaces:**
- Produces：`hermes_worker_transcript(...)` → worker 完整会话消息（缺失/未落盘 → 返回空数组，不报错，前端走空态）；transcript 数据源 = 现有 `session_manager::load_messages`，**不新写解析**。
- 非回归：不改 worker 调度/事件流；只**额外**捕获 + 暴露一个读路径。
- [ ] **Step 1: 写失败测试**（捕获 session 来源 + 命令读到消息 / 缺失返回空）
- [ ] **Step 2: 确认失败** → FAIL
- [ ] **Step 3: 实现**（最小接缝：捕获来源 + 命令复用 load_messages + 契约同步）
- [ ] **Step 4: 确认通过 + 非回归** → `cargo test hermes` + `cargo test chat` 全绿
- [ ] **Step 5: 提交** `feat(hermes): expose worker transcript via session_manager bridge`

## Task 2: judge 接通 Coordinator（Phase 3 §5 遗留）

**Files:**
- Modify: `src-tauri/src/hermes/coordinator.rs`（收敛/多候选点调 `Planner::judge`，verdict 落库；judge Err → 回落 Phase 3f 确定性规则，**不阻塞收敛**——非回归 best-effort）
- Modify: `src-tauri/src/commands/hermes_commands.rs`（`hermes_judge_show(runId) -> JudgeVerdictDto|null`）+ `lib.rs` 注册 + 契约同步
- Test: `coordinator.rs::tests`（mock judge 落库 + Err 回落不卡收敛）、`hermes_commands.rs::tests`（DTO 转换）

**Interfaces:** Produces：`hermes_judge_show(runId) -> JudgeVerdictDto{ winnerIndex, scores, reason, candidates }|null`；Consumes：`Planner::judge`（已存在）。
- [ ] **Step 1: 写失败测试** → **Step 2: FAIL** → **Step 3: 实现**（best-effort + 回落）→ **Step 4: cargo test hermes+chat 全绿** → **Step 5: 提交** `feat(hermes): wire planner judge into coordinator convergence`

## Task 3: 单-agent abort + 引擎小清理

**Files:**
- Modify: `src-tauri/src/commands/hermes_commands.rs`（`hermes_agent_abort(agentId|dispatchId)`：定位其活跃 dispatch → `runtime.abort` + `fail_dispatch`，复用现有级联）+ `lib.rs` 注册 + 契约同步
- Modify: `src-tauri/src/hermes/coordinator.rs`（single-flight replan 改 RAII guard——`inflight.remove` 用 drop guard 保证 panic/早返回也释放，Phase 3 §5 小修）
- Modify: `src-tauri/src/hermes/store.rs`（测试 helper `is_rfc3339_chrono` 换成 `chrono::DateTime::parse_from_rfc3339(s).is_ok()`，Phase 3 终审 Minor）
- Test: `hermes_commands.rs::tests`（abort 命令定位 + 调用）、coordinator 单飞 guard 测试维持
- [ ] **Step 1: 写失败测试** → **Step 2: FAIL** → **Step 3: 实现**（小改，复用已有 abort/cascade）→ **Step 4: cargo test hermes+chat 全绿 + build** → **Step 5: 提交** `feat(hermes): per-agent abort command and small engine cleanups`

### ✅ GATE（最终，一次过）
```bash
cargo test --manifest-path src-tauri/Cargo.toml hermes && cargo test --manifest-path src-tauri/Cargo.toml chat
cargo check --manifest-path src-tauri/Cargo.toml && npm run build && git diff --check
git checkout feat/helm && git merge --no-ff feat/helm-phase3.5-bridge -m "merge: Phase 3.5 — worker transcript bridge + judge wiring + small fixes"
```

# DoD

- [ ] Task 1–3 完成、各自 commit、过 GATE、`--no-ff` 合回 `feat/helm`。
- [ ] `cargo test`（hermes+chat）+ `npm run build` + `git diff --check` 全绿。
- [ ] 契约 `docs/helm-hermes-ui-contract.md` 补 `hermes_worker_transcript` / `hermes_judge_show` / `hermes_agent_abort` + `MessageDto`/`JudgeVerdictDto`。
- [ ] 非回归：Phase 2/3 旧行为 byte-identical（新命令不调用即零影响）。
- [ ] 真 LLM 冒烟（可选，若做）暴露的引擎 bug 一并修；未做标「待人工执行」不造假。
- [ ] 做完即停。

# 备注

- **判断空间**：worker transcript 来源捕获若发现 session_id/source_path 不易拿（如 daemon 未回传），可退化为「按 worker worktree cwd 用 `scan_sessions_for_project` 找其 session」——两条路任选其一，以最小改动为准，在交付报告说明选了哪条。
- Phase 4 cockpit 已按 grounded 预留接口（`loadWorkerTranscript`/`judgeShow`/单-agent stop 禁用态），本 Phase 落地后前端只换数据源即点亮，无需 Phase 4 返工。
