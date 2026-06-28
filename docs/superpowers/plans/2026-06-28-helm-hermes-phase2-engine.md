# Helm × Hermes — Phase 2：Hermes 编排引擎（GLM 全量执行）实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans（推荐，GLM 单会话全量执行）或 subagent-driven-development 逐 task 实施。Steps 用 checkbox（`- [ ]`）跟踪。

**Goal:** 在 Helm（Phase 0/1/1b：daemon 池 + worktree 隔离 + 异构扇出）之上，构建 **Hermes 编排引擎**：可插拔 `AgentRuntime` 契约 + SQLite 持久化状态机 + 确定性 Coordinator 循环 + WorkerSupervisor 判活 + LLM Planner（拆解/选兵/replan）+ CliRuntime（裸 CLI 介质）。让"给一个目标 → 自动拆解 → 选兵 → 并行调度 → 收敛"成为后端可持久化、可崩溃恢复的能力。

**Architecture:** **加法式、不破坏现有 chat**。新增 `src-tauri/src/hermes/` 模块。Hermes 引擎只依赖 `AgentRuntime` 契约；`SdkRuntime` 适配器包装现有 `ChatManager`/`AgentPool` 的 send 路径，`CliRuntime` 新写 PTY 介质。Coordinator 是确定性单类循环（移植 orca），Planner 仅在"拆解/选兵/replan"两点调 LLM。数据模型与循环移植设计文档 §7 / orca `src/main/runtime/orchestration/`。

**Tech Stack:** Rust（tokio、rusqlite 0.31 bundled、std::process / portable-pty for CLI）、Tauri 2、（前端命令/事件桥接，最小）。

## 分支约定（固化，后续所有 phase 遵守）

- **主功能分支 = `feat/helm`**（既非 `main` 也非 `develop`）。Helm 全部工作收在它下面。
- **每个子阶段从 `feat/helm` 拉自己的分支**（如 `feat/helm-phase2a-runtime`），干完 + 验收通过后 **merge 回 `feat/helm`**（用 `--no-ff` 保留集成记录）。
- 需要 worktree 隔离时，worktree 也基于 `feat/helm`。
- **暂不**把 `feat/helm` merge 到 `main`/`develop`，等用户明确指示。
- 本计划在 `feat/helm` 上执行；若按子阶段分支，见每个子阶段开头的分支提示。

## Global Constraints

- 不写魔法字符串：状态名、消息类型、事件名、表名、字段名、scope、token 类型、配置 key 必须集中为 Rust `enum`/常量（CLAUDE.md 规约）。
- 配置集中：路径、保留天数、轮询间隔、并发上限、超时等进 `api/config` 体系或集中常量，文档注明默认值。
- 不绕过抽象：daemon/worktree/DB 操作经各自抽象（`AgentPool`/`WorktreeManager`/Hermes `Store`）。
- 新增代码补中文注释，尤其并发边界、状态流转、WAL/事务、清理策略、崩溃恢复。
- 新增能力必须补测试 + 中文文档（无测试/文档视为未完成）。
- JSON camelCase + Rust snake_case；新 Tauri 命令注册 `lib.rs` `generate_handler!`。
- 每 task 结束 commit（Conventional Commits）；提交前 `cargo check --manifest-path src-tauri/Cargo.toml`；`git diff --check` 干净。
- **并发重灾区纪律**：Coordinator/Store/Supervisor 的每个并发不变量都要有测试；遇失败先根因（systematic debugging），不猜不掩盖。

## 阅前必读（GLM 冷启动先做）

```bash
codegraph node ChatManager
codegraph node AgentPool
codegraph node DaemonClient
codegraph node WorktreeManager
codegraph explore database rusqlite schema migration connection
codegraph explore chat send abort agent_id stream done event
```
通读：
- 设计文档 `docs/superpowers/specs/2026-06-27-helm-hermes-design.md`（**§4 AgentRuntime 契约、§6 组件、§7 数据模型、§9 工作流、§13 路线** —— 本计划是它的落地）。
- 现有引擎参考（orca，只读他山之石）：`/Users/jiaxing/code/github/orca/src/main/runtime/orchestration/{types,db,coordinator,lifecycle-reconciliation}.ts`（数据模型/循环/崩溃恢复的生产级范本）。
- jadekit 真实代码：`src-tauri/src/chat/{manager,pool,daemon_client,worktree,protocol}.rs`、`src-tauri/src/database/{mod,schema}.rs`（rusqlite 用法/迁移范式，Hermes Store 照此风格）、`src-tauri/src/commands/chat_commands.rs`。

> **新建模块** `src-tauri/src/hermes/`：`mod.rs`、`runtime.rs`(AgentRuntime 契约+事件)、`sdk_runtime.rs`、`cli_runtime.rs`、`store.rs`、`types.rs`(enum/结构)、`coordinator.rs`、`supervisor.rs`、`planner.rs`。前端仅最小桥接。每文件单一职责。

---

# 子阶段 2·pre：修 Phase 1b review 遗留（worktree 泄漏）

> 分支：`feat/helm`（小修，可直接在主干）。

## Task 1: `launchFanout` 部分失败回滚已建 worktree

**Files:**
- Modify: `src/stores/useChatStore.ts`（`launchFanout` 的 worktree 创建循环，`:1946-1986` 区域）
- Test: `src/stores/fanoutRollback.test.ts`（纯函数）

**Interfaces:**
- Produces: 纯函数 `worktreesToRollback(created: {path: string}[]): string[]`（返回需回滚的 path 列表）；`launchFanout` 在某个 `createWorktree` 抛错时，对**本轮已成功创建**的 worktree 逐个 `removeWorktree(path, true)` 后再 `set(error); return`。

- [ ] **Step 1: 写失败测试**

```ts
import { describe, it, expect } from 'vitest';
import { worktreesToRollback } from './fanoutRollback';
describe('worktreesToRollback', () => {
  it('returns paths of all successfully created worktrees', () => {
    expect(worktreesToRollback([{ path: '/wt/a' }, { path: '/wt/b' }])).toEqual(['/wt/a', '/wt/b']);
  });
  it('empty when nothing created', () => {
    expect(worktreesToRollback([])).toEqual([]);
  });
});
```

- [ ] **Step 2: 确认失败** → `npx vitest run src/stores/fanoutRollback.test.ts`
- [ ] **Step 3: 实现** `fanoutRollback.ts` 纯函数；改 `launchFanout`：循环里维护 `const created: WorktreeInfo[] = []`，每次 `createWorktree` 成功后 push；`catch` 分支里 `for (const p of worktreesToRollback(created)) { await removeWorktree(p, true).catch(() => {}); }` 再 `set({error}); return;`。
- [ ] **Step 4: 确认通过 + 构建** → vitest PASS；`npm run build`
- [ ] **Step 5: 提交** `fix(chat): roll back created worktrees when fan-out launch fails midway`

---

# 子阶段 2a：AgentRuntime 契约 + 事件 + SdkRuntime 适配器

> 分支：`git checkout feat/helm && git checkout -b feat/helm-phase2a-runtime`
> 目标：定义介质契约，把现有 SDK send 路径包成一个 `AgentRuntime` 实现，**不改动现有 chat 行为**。

## Task 2: `AgentRuntime` 契约 + `AgentEvent` + 能力标记

**Files:**
- Create: `src-tauri/src/hermes/mod.rs`、`src-tauri/src/hermes/runtime.rs`
- Modify: `src-tauri/src/lib.rs`（`mod hermes;`）
- Test: `runtime.rs`（事件序列化/能力默认值）

**Interfaces（落地设计 §4，Produces）:**

```rust
// src-tauri/src/hermes/runtime.rs
use async_trait::async_trait;
use tokio::sync::mpsc;

/// 介质可插拔契约：SDK / CLI / 任意 agent loop 都实现它。Hermes 引擎只认这个。
#[async_trait]
pub trait AgentRuntime: Send + Sync {
    fn capabilities(&self) -> RuntimeCapabilities;
    async fn start(&self, spec: RuntimeStartSpec) -> Result<AgentHandle, RuntimeError>;
    async fn send(&self, handle: &AgentHandle, prompt: String)
        -> Result<mpsc::UnboundedReceiver<AgentEvent>, RuntimeError>;
    async fn abort(&self, handle: &AgentHandle) -> Result<(), RuntimeError>;
    async fn liveness(&self, handle: &AgentHandle) -> Liveness;
    async fn stop(&self, handle: &AgentHandle) -> Result<(), RuntimeError>;
}

#[derive(Debug, Clone)]
pub struct RuntimeStartSpec {
    pub agent_id: String,      // = AgentId（复用 chat::AgentId 语义）
    pub cwd: std::path::PathBuf,
    pub model: String,
    pub provider: String,      // claude / codex / gemini / ...
}

#[derive(Debug, Clone)]
pub struct AgentHandle { pub agent_id: String }

#[derive(Debug, Clone, Copy)]
pub struct RuntimeCapabilities {
    /// true=有结构化 tool_use/tool_result（判活精准）；false=仅文本+进程存活（降级判活）。
    pub structured_events: bool,
    pub supports_resume: bool,
    pub supports_permission_prompt: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Liveness { Alive, Dead, Unknown }

#[derive(Debug, Clone)]
pub enum AgentEvent {
    TextDelta(String),
    Thinking(String),
    ToolUse { id: String, name: String },
    ToolResult { tool_use_id: String, is_error: bool },
    NeedsInput,                 // 工具权限 / ask-user（等待态）
    Done { success: bool, files_modified: Vec<String> },
    Failed { error: String },
}

#[derive(Debug, Clone)]
pub struct RuntimeError(pub String);
```

- [ ] Step 1: 写失败测试（`AgentEvent` 的 `Done`/`Failed` 匹配、`RuntimeCapabilities` 字段）。
- [ ] Step 2: `cargo test --manifest-path src-tauri/Cargo.toml hermes::runtime` 确认失败。
- [ ] Step 3: 实现上述契约（`async-trait` 若 Cargo 未含则加入 `[dependencies]`）。`mod.rs` 导出。`lib.rs` 加 `mod hermes;`。
- [ ] Step 4: 确认通过。
- [ ] Step 5: 提交 `feat(hermes): define AgentRuntime contract and AgentEvent`。

## Task 3: `SdkRuntime` —— 把现有 ChatManager/AgentPool send 包成 AgentRuntime

**Files:** Create `src-tauri/src/hermes/sdk_runtime.rs`；Modify `hermes/mod.rs`
**Interfaces:** `pub struct SdkRuntime { manager: Arc<ChatManager> }`；`impl AgentRuntime for SdkRuntime`，`capabilities().structured_events = true`。`send` 调用 `ChatManager::send(agent_id, "<provider>.send", params)`，并把 `chat://` 风格的 StreamLine 标签流（`[CONTENT_DELTA]`/`[MESSAGE]`(tool_use)/`[TOOL_RESULT]`/done）**解析归一**成 `AgentEvent`。

- [ ] Step 1: 写失败测试：用一个把固定标签行喂进解析器的单测，断言 `[CONTENT_DELTA] "hi"` → `TextDelta("hi")`、`[MESSAGE]{tool_use}` → `ToolUse`、done → `Done`。把"标签行→AgentEvent"做成**纯函数** `parse_stream_line(&str) -> Option<AgentEvent>` 便于测试。
- [ ] Step 2-4: 实现纯解析函数 + `SdkRuntime`（send 内 spawn 消费 `ChatManager` 的 mpsc，转 `AgentEvent` 推给返回的 channel）。`abort`→`manager.abort(agent_id)`；`liveness`→`manager.is_running` 映射；`stop`→`manager.close_agent`。
- [ ] Step 5: 提交 `feat(hermes): SdkRuntime adapter over ChatManager send path`。

## Task 4: 子阶段 2a 验收门 A + 合并

- [ ] `cargo test --manifest-path src-tauri/Cargo.toml hermes` 全绿；`cargo check` 通过；现有 `cargo test chat` 仍全绿（证明未破坏 chat）。
- [ ] **GATE A（人工/review）**：契约稳定、SdkRuntime 解析正确、未回归 chat。通过后：
  ```bash
  git checkout feat/helm && git merge --no-ff feat/helm-phase2a-runtime -m "merge: Phase 2a AgentRuntime contract"
  ```

---

# 子阶段 2b：Hermes Store（SQLite 持久化状态机）

> 分支：`git checkout feat/helm && git checkout -b feat/helm-phase2b-store`
> 移植设计 §7 / orca `db.ts`。rusqlite，照 `src-tauri/src/database/` 现有风格（WAL、迁移）。

## Task 5: 数据类型 enum/结构（`hermes/types.rs`）

**Interfaces（落地设计 §7）:** `MessageType`/`TaskStatus`/`DispatchStatus`/`GateStatus`/`RunStatus` 枚举（含 `as_str`/`from_str` 集中映射，禁止魔法串）；`Task`/`AgentAssignment`/`DispatchContext`/`Message`/`DecisionGate`/`CoordinatorRun` 结构（字段见设计 §7）。
- [ ] Step1-5: TDD 每个枚举的 `as_str`/`from_str` 往返；提交 `feat(hermes): orchestration data types and status enums`。

## Task 6: `Store` —— schema + 迁移 + Task/Dispatch CRUD + DAG ready 提升

**Files:** Create `src-tauri/src/hermes/store.rs`
**Interfaces:** `Store::open(path) -> Store`（`PRAGMA journal_mode=WAL; synchronous=NORMAL; busy_timeout=5000`；建表 + 索引，对齐设计 §7 与 orca `db.ts`）；`create_task`/`get_task`/`list_tasks(filter)`/`update_task_status`/`create_dispatch`/`update_dispatch`/`promote_ready_tasks`（deps 全 `completed` 的 `pending`→`ready`，**与 `update_task_status` 同一写事务**保证不变量——见 orca db.ts 注释）。
- [ ] Step 1: 写失败测试：建临时 db → create_task(无 deps) → promote_ready_tasks → 状态变 ready；带 deps 的任务在依赖未完成时不 ready，依赖 completed 后 ready；熔断 `failure_count` 累加到 3 → `circuit_broken`。
- [ ] Step 2-4: 实现（迁移用事务；CRUD；promote 与 update 放同一 `tx`）。
- [ ] Step 5: 提交 `feat(hermes): SQLite store with task DAG, dispatch, and ready promotion`。

## Task 7: `Store` —— Message 总线 + Gate + Run + 崩溃恢复对账

**Interfaces:** message insert/inbox/mark-read（按 to_handle、sequence 单调）；gate create/resolve/list；run create/update/get_active；`reconcile_on_startup()`（对齐 orca `lifecycle-reconciliation`：把孤儿 `dispatched` 任务按已收 `worker_done` 消息回填，或标记需重派）。
- [ ] Step1-5: TDD（消息顺序、gate 解析、重启对账把"已 dispatched 但有 worker_done"的任务标 completed）；提交 `feat(hermes): message bus, gates, runs, and startup reconciliation`。

## Task 8: 子阶段 2b 验收门 B + 合并
- [ ] `cargo test hermes::store` 全绿；事务不变量测试通过。
- [ ] **GATE B**：schema/索引/迁移/对账符合设计 §7 与 orca 范式。通过后 `--no-ff` merge 回 `feat/helm`。

---

# 子阶段 2c：Coordinator 确定性循环

> 分支：`feat/helm-phase2c-coordinator`。移植 orca `coordinator.ts`：单类循环避免 split-brain。

## Task 9: Coordinator 骨架 + 派发 ready 任务

**Files:** Create `src-tauri/src/hermes/coordinator.rs`
**Interfaces:** `Coordinator { store, runtime: Arc<dyn AgentRuntime>, worktree, max_concurrent }`；`async fn tick(&self)`：① 回收 stale dispatch（心跳超时）→ ② 处理入站消息（`worker_done`→update_task completed + promote_ready；`escalation`/`merge_ready`）→ ③ 解 gate → ④ 派发 ready 任务到空闲槽（≤ max_concurrent，不足则起新 agent：建 worktree + `runtime.start` + `runtime.send(spec+preamble)`）→ ⑤ 熔断（3 次失败 `circuit_broken`）→ ⑥ 收敛判定。`async fn run(&self, run_id)`：轮询 `tick` 直到收敛或停止。
- [ ] Step 1: 写失败测试（用 **mock `AgentRuntime`**：可编程地对某 agent 发 `Done{success}` / `Failed`）：单任务 → 派发 → 收 Done → 任务 completed；两任务带依赖 → 拓扑顺序派发；某任务连续 3 次 Failed → circuit_broken 且不再派。
- [ ] Step 2-4: 实现 tick（确定性，无 LLM）。所有时间/计数/熔断由 Rust 控制。
- [ ] Step 5: 提交 `feat(hermes): coordinator tick — dispatch, worker_done, circuit breaker`。

## Task 10: Coordinator 收敛 + 并行波次 + 验收门 C
- [ ] TDD：N 个无依赖任务 → 并行波次（≤ max_concurrent）→ 全 Done → run completed。
- [ ] **GATE C**：循环正确、熔断/收敛/并发上限有测试。`--no-ff` merge 回 `feat/helm`。

---

# 子阶段 2d：WorkerSupervisor（判活，能力分级）

> 分支：`feat/helm-phase2d-supervisor`。落地设计 §6.3 + 心跳判活讨论。

## Task 11: Supervisor 状态机（结构化介质）

**Files:** Create `src-tauri/src/hermes/supervisor.rs`
**Interfaces:** 每 agent 维护 `last_activity_at`、`open_tool_uses: HashSet<String>`、`status: WorkerStatus{Running,WaitingInput,Done,Failed,Suspect}`；`on_event(agent_id, &AgentEvent)` 更新（任意事件刷新 last_activity；`ToolUse` 入集合、`ToolResult` 出集合；`NeedsInput`→WaitingInput；`Done`→Done(熔断不+)；`Failed`→Failed）；`reap(now, timeout)`：超时无活动 且 非 WaitingInput 且 `open_tool_uses` 为空 且 进程存活 → `Suspect`。
- [ ] Step 1: 写失败测试（表驱动）：tool_use 未闭合时即使静默也**不**判 Suspect；NeedsInput 永不被 reap 杀；超时无活动+无未闭合 tool_use → Suspect。
- [ ] Step 2-4: 实现。
- [ ] Step 5: 提交 `feat(hermes): worker supervisor liveness state machine`。

## Task 12: CLI 降级判活 + 验收门 D
- [ ] TDD：`structured_events=false` 时走降级（有输出=活、进程存活=没崩、硬超时 `max_turn_ms` 兜底），不与结构化判活混用。
- [ ] **GATE D**：两档判活 + WaitingInput/Suspect 区分有测试。`--no-ff` merge 回 `feat/helm`。

---

# 子阶段 2e：Planner（LLM 拆解 / 选兵 / replan）

> 分支：`feat/helm-phase2e-planner`。LLM 只在两点介入，其余确定性。

## Task 13: Planner 提示与解析（纯函数优先）

**Files:** Create `src-tauri/src/hermes/planner.rs`
**Interfaces:** 纯函数 `build_plan_prompt(goal, roster) -> String`（要求模型输出**结构化** JSON：任务列表 + 每任务 deps + 选兵 assignment）；`parse_plan_response(&str) -> Result<Vec<Task>, String>`（容错解析，校验 deps 合法、assignment 在 roster 内）。`build_replan_prompt(run, failed_task, result)` / `parse_replan_response`。
- [ ] Step 1: 写失败测试：给定一段合法 JSON 响应 → parse 出 N 个 Task（deps/assignment 正确）；非法/越界 assignment → Err；prompt 含 goal 与 roster 每项。
- [ ] Step 2-4: 实现纯函数（不调网络，便于测试）。
- [ ] Step 5: 提交 `feat(hermes): planner prompt builders and structured response parsing`。

## Task 14: Planner 接 LLM（经 AgentRuntime 起一个临时 planner agent）+ 验收门 E
- [ ] 实现 `Planner::plan(goal, roster)`：经 `SdkRuntime` 起一个无 worktree 的临时 agent，发 `build_plan_prompt`，收敛文本后 `parse_plan_response`。`replan` 同理。Coordinator 在开局调 `plan`、失败后调 `replan`。
- [ ] TDD（用 mock runtime 回放固定 JSON）：plan → 写入 Store 的 Task DAG；replan → 产出决策。
- [ ] **GATE E**：拆解/选兵/replan 闭环；解析容错有测试。`--no-ff` merge 回 `feat/helm`。

---

# 子阶段 2f：CliRuntime（裸 CLI 介质）

> 分支：`feat/helm-phase2f-cli`。落地"介质可插拔"的第二实现。

## Task 15: CliRuntime —— PTY 起 CLI + 文本流归一

**Files:** Create `src-tauri/src/hermes/cli_runtime.rs`
**Interfaces:** `impl AgentRuntime for CliRuntime`，`capabilities().structured_events=false`。`start`：用 PTY（`portable-pty`，若 Cargo 未含则加入）在 cwd 起 CLI（如 `gemini` / `claude`），`send`：写 stdin + 把 PTY 输出按行转 `AgentEvent::TextDelta`，进程退出 → `Done{success}`（按 exit code）。`abort`/`stop`：kill 进程组。`liveness`：进程存活探测。
- [ ] Step 1: 写失败测试：用一个**可控的假命令**（如 `bash -c 'echo hi; exit 0'`）跑通 start→send→收到 TextDelta("hi")→Done{success:true}；非零退出 → Done{success:false}。
- [ ] Step 2-4: 实现（注意 PTY 在 CI/headless 可用；不可用则降级 pipe，测试用 pipe 路径）。
- [ ] Step 5: 提交 `feat(hermes): CliRuntime PTY adapter for bare CLI agents`。

## Task 16: 验收门 F + 合并
- [ ] TDD：同一 Coordinator 用 `SdkRuntime` 与 `CliRuntime` 各跑一个任务，事件流统一、判活按 capability 分级。
- [ ] **GATE F**：异构介质统一调度验证。`--no-ff` merge 回 `feat/helm`。

---

# 子阶段 2g：Tauri 命令桥接 + 最终验收

> 分支：`feat/helm-phase2g-wiring`。

## Task 17: Hermes Tauri 命令 + 事件
- [ ] 命令：`hermes_run(goal, opts)` / `hermes_task_list` / `hermes_dispatch_show` / `hermes_gate_resolve` / `hermes_run_stop`（动词对齐 orca CLI）；事件 `hermes://run`/`hermes://task`/`hermes://agent`。注册 `lib.rs`。TDD 命令层参数归一 + 编译。提交 `feat(hermes): tauri commands and events for orchestration runs`。

## Task 18: 最终验收门 + 验证文档
- [ ] 全量：`cargo check`、`cargo test`（hermes + chat 全绿）、`npm run build`、`git diff --check`。
- [ ] 写 `docs/helm-phase2-verification.md`：自动门 + 手动 e2e（给一个目标 → Hermes 拆解 → 多 agent 并行 → 收敛 → 看 Store 状态/事件）。手动 e2e 未跑标「待人工执行」，不造假。
- [ ] **GATE 最终**：整引擎在 mock runtime 下端到端跑通；真 LLM 路径手测。`--no-ff` merge 回 `feat/helm`。提交验证文档。

---

## Self-Review

- **覆盖**：设计 §4（Task 2-3）、§7（Task 5-7）、§6.5 Coordinator（Task 9-10）、§6.3 Supervisor（Task 11-12）、§6.5 Planner（Task 13-14）、§4 CliRuntime（Task 15-16）、§6.6 命令（Task 17）、Phase 1b 遗留 worktree 泄漏（Task 1）。
- **占位符**：契约/数据类型/状态机给了真实 Rust 代码；算法件（Coordinator/Planner）给了精确 TDD 测试意图 + 接口签名 + orca 范本指引，无 "TODO/类似上文"。
- **类型一致**：`AgentRuntime`/`AgentEvent`/`RuntimeStartSpec`（2a）贯穿 SdkRuntime/CliRuntime（2a/2f）、Coordinator（2c）、Supervisor（2d）、Planner（2e）；`Task`/`DispatchContext`/枚举（2b）贯穿 Store/Coordinator。
- **并发安全**：promote_ready 与 update_status 同事务（Task 6）；熔断 3 次（Task 6/9）；判活分级 + WaitingInput 不被杀（Task 11-12）；崩溃恢复对账（Task 7）。

## GLM 执行说明（全量款）

GLM 单会话从 Task 1 顺序执行到 Task 18，**每个子阶段(2a–2g)结尾的 GATE 必须停下自检**（跑该子阶段测试 + cargo check），自检过了再继续下一子阶段；**每个 GATE 处 `--no-ff` merge 回 `feat/helm`**。全程 TDD、逐 task commit、不攒批、不改测试凑绿。任一并发不变量测试失败 → systematic debugging 定位根因，不猜。做完 Task 18 出交付报告（DoD 逐条 + git log + 自动门输出 + 手动 e2e 状态 + 偏差未决）。
