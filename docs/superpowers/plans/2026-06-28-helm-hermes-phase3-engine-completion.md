# Helm × Hermes — Phase 3：引擎收尾 + 驾驶舱契约（GLM 全量执行）实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development（**推荐**，防跑偏最强：每 task 全新子 agent + 两道审查 + progress.md 检查点；GLM 作协调者不自己写）。不可用时降级 superpowers:executing-plans（单会话直跑，更严守每个子阶段 GATE）。Steps 用 checkbox（`- [ ]`）跟踪。

**Goal:** 在 Phase 2 的 Hermes 引擎之上完成生产化收尾——冻结并落地「引擎↔驾驶舱」事件/命令契约（解锁 Phase 4 并行）、异构介质单实例路由（一次 run 内 SDK×CLI 混跑）、run 内取消、收敛后 worktree/分支安全清理、并发 replan 单飞，并把真 LLM e2e 步骤补全。

**Architecture:** 全程**加法式、不破坏现有 chat 与 Phase 2 引擎行为**。所有新增可选钩子默认 `None`/`Null`，不注入时 Coordinator/Supervisor 行为与 Phase 2 逐字一致（关键非回归保证）。引擎继续只依赖抽象：事件经新增 `OrchestrationEventSink` 契约（Tauri 实现放命令层，引擎不碰 `AppHandle`）；介质经 `AgentRuntime` 契约的**注册表**（`RuntimeKind → Arc<dyn AgentRuntime>`）按 `task.assignment.runtime` 路由。数据模型/循环延续设计文档 §7/§9 与 orca。

**Tech Stack:** Rust（tokio、rusqlite 0.31 bundled、async-trait）、Tauri 2、前端命令/事件桥接（最小，真正 UI 在 Phase 4）。

## 分支约定（沿用 Phase 2，固化）

- **主功能分支 = `feat/helm`**（既非 `main` 也非 `develop`）。当前 tip 含 Phase 0/1/1b/2 全量。
- **每个子阶段从 `feat/helm` 拉自己的分支**（如 `feat/helm-phase3a-contract`），干完 + GATE 自检过 → `--no-ff` merge 回 `feat/helm`：
  ```bash
  git checkout feat/helm && git checkout -b feat/helm-phase3a-contract
  # ... 干完该子阶段 + GATE 绿 ...
  git checkout feat/helm && git merge --no-ff feat/helm-phase3a-contract -m "merge: Phase 3a ..."
  ```
- **不要** merge 到 main/develop。

## 并行执行隔离（若 Phase 3/4 同时跑，强制）

两个 agent（Phase 3=GLM / Phase 4=Codex）**绝不能在同一个工作树上同时操作**——会发生 checkout 互相覆盖（本项目此前已栽过两次工作树冲突）。**并行的前提是各自独立的 git worktree**：

```bash
# 主仓库工作树保持干净，谁都不在它上面直接干活。
# GLM（Phase 3）：
git worktree add ../jadekit-phase3 feat/helm   # 在此 worktree 内拉 feat/helm-phase3a-contract 等子分支
# Codex（Phase 4）：
git worktree add ../jadekit-phase4 feat/helm   # 在此 worktree 内拉 feat/helm-phase4-* 子分支
```

- 各自子阶段分支都基于 `feat/helm`；GATE 过后 `--no-ff` merge 回 `feat/helm`（merge 操作在各自 worktree 内执行，目标都是同一个 `feat/helm` ref）。
- **合并顺序**：3a 必须先合回 `feat/helm`（Phase 4 的契约依赖）；Phase 4 在 3a 合回后 `git -C ../jadekit-phase4 merge feat/helm` 同步，再接真事件。
- 用完销毁：`git worktree remove ../jadekit-phase3`。
- **若选串行（先 3 后 4）则无需 worktree**：GLM 跑完整个 Phase 3 合回 `feat/helm` 后，Codex 再在主工作树（或单独 worktree）开 Phase 4。串行更稳、无契约漂移；并行更快、但要守住上面的隔离纪律。
- **与 Phase 4 并行的契约点**：Phase 4（Codex）只依赖 **3a 落地后的 `feat/helm`**；3a 合回后通知 Phase 4 接真事件。

## Global Constraints（逐字遵守，每个 task 隐含包含）

- 不写魔法字符串：状态名、消息类型、**事件名**、表名、字段名、scope、配置 key 必须集中为 Rust `enum`/常量（CLAUDE.md 规约）。新增事件名/状态 token 一律加到既有常量区，不散落。
- 配置集中：超时、并发上限、清理策略开关、轮询间隔进集中常量并注明默认值。
- 不绕过抽象：daemon/worktree/DB 操作经各自抽象（`AgentPool`/`WorktreeManager`/Hermes `Store`/`OrchestrationEventSink`）。
- 新增代码补中文注释，尤其并发边界、状态流转、WAL/事务、清理策略、崩溃恢复、取消信号。
- 新增能力必须补测试 + 中文文档（无测试/文档视为未完成）。
- JSON camelCase + Rust snake_case；新 Tauri 命令注册 `lib.rs` `generate_handler!`。
- 每 task 结束 commit（Conventional Commits）；提交前 `cargo check --manifest-path src-tauri/Cargo.toml`；`git diff --check` 干净。
- **非回归红线**：每个子阶段 GATE 必须确认 `cargo test chat`（69）+ Phase 2 引擎测试全绿；新增可选钩子默认关闭时，Coordinator/Supervisor 行为与 Phase 2 一致。
- **破坏性安全红线**（3d 尤其）：worktree/分支清理**绝不**删除「已完成但未合并」的工作；只在用户明确选择（merge/discard，Phase 4 UI）或确认无未提交改动时才删。

## 代码探索：codegraph 优先（索引已 sync，禁止默认 grep）

仓库已 `codegraph sync`。**探索代码一律先用 codegraph**（`node` 看符号定义/签名/成员，`explore` 做语义搜索）；只有纯文本字面量匹配（如找某条字符串常量出现处）才退回 grep。改完代码后 `codegraph sync` 重建索引。本计划所有引用的签名都已用 `codegraph node` 核对过真实代码（见各 Task 的"准确性约束"）。

```bash
codegraph node Coordinator WorkerSupervisor HermesEngine WorktreeManager Store
codegraph explore hermes runtime sink event dispatch watcher cancel reap
codegraph explore worktree merge remove has_uncommitted diff_summary list
codegraph node AgentAssignment RuntimeKind   # 确认 assignment 字段（runtime/tool/model，无 provider 字段）
```
通读：
- 设计文档 `docs/superpowers/specs/2026-06-27-helm-hermes-design.md`（**§4 AgentRuntime 契约、§6 组件、§9 工作流、§10 UI/UX、§13 路线**）。
- Phase 2 交付与未决：`docs/helm-phase2-delivery.md`（§5 八条 gap = 本计划主体）、`docs/helm-phase2-verification.md`（§D）。
- 真实代码：`src-tauri/src/hermes/{coordinator,supervisor,store,sdk_runtime,cli_runtime,runtime,types}.rs`、`src-tauri/src/commands/hermes_commands.rs`、`src-tauri/src/chat/worktree.rs`、`src-tauri/src/lib.rs`。
- 生产范本（只读）：orca `src/main/runtime/orchestration/{coordinator,db,lifecycle-reconciliation}.ts`。

> **新增文件**：`src-tauri/src/hermes/events.rs`（`OrchestrationEventSink` 契约 + `OrchestrationEvent` + `NullEventSink`）、`src-tauri/src/hermes/runtime_registry.rs`（介质注册表）、`src-tauri/src/hermes/run_lifecycle.rs`（收敛后 worktree 清理纯逻辑）。其余在既有文件内加法式扩展。

---

# 子阶段 3a：引擎↔驾驶舱契约 + 实时 task/agent 事件（KEYSTONE，解锁 Phase 4）

> 分支：`git checkout feat/helm && git checkout -b feat/helm-phase3a-contract`
> 目标：冻结并落地事件/命令契约，让 Coordinator/Supervisor 在派发、活动、完成、失败、判活时发射 `hermes://task`/`hermes://agent` 事件；补 run/agent 读命令；统一时间戳 RFC3339（D.5）。**这是 Phase 4 唯一的硬依赖，必须最先合回 `feat/helm`。**

## Task 1: `OrchestrationEventSink` 契约 + `OrchestrationEvent` + `NullEventSink`

**Files:**
- Create: `src-tauri/src/hermes/events.rs`
- Modify: `src-tauri/src/hermes/mod.rs`（声明 `mod events;` + re-export）
- Test: `events.rs`（payload 序列化 camelCase + NullEventSink no-op）

**Interfaces:**
- Consumes: 无（新基元）。
- Produces:
  ```rust
  // src-tauri/src/hermes/events.rs
  use serde::Serialize;

  /// 编排进度事件（引擎 → 前端）。三类通道统一走这一个枚举，
  /// 由 OrchestrationEventSink 的实现决定怎么落地（Tauri emit / 测试收集 / no-op）。
  #[derive(Debug, Clone, Serialize, PartialEq, Eq)]
  #[serde(tag = "kind", rename_all = "camelCase")]
  pub enum OrchestrationEvent {
      /// run 级：启动 / 完成 / 失败 / 取消。
      #[serde(rename_all = "camelCase")]
      Run { run_id: String, goal: String, status: String, error: Option<String> },
      /// task 级：ready / dispatched / completed / failed / blocked。
      #[serde(rename_all = "camelCase")]
      Task { run_id: String, task_id: String, status: String, dispatch_id: Option<String> },
      /// agent 级：worker 判活状态 + 最近活动类别（用于驾驶舱 AgentStateDot）。
      #[serde(rename_all = "camelCase")]
      Agent { run_id: String, agent_id: String, task_id: Option<String>, status: String, activity: Option<String> },
  }

  /// 事件下游契约：引擎只调 emit，不认 Tauri。生产实现在 hermes_commands.rs。
  pub trait OrchestrationEventSink: Send + Sync {
      fn emit(&self, event: OrchestrationEvent);
  }

  /// 默认无操作 sink——不注入时引擎零成本、行为与 Phase 2 一致。
  pub struct NullEventSink;
  impl OrchestrationEventSink for NullEventSink {
      fn emit(&self, _event: OrchestrationEvent) {}
  }
  ```

- [ ] **Step 1: 写失败测试**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn run_event_serializes_camel_case_with_kind_tag() {
        let ev = OrchestrationEvent::Run {
            run_id: "run_1".into(), goal: "g".into(), status: "running".into(), error: None,
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("\"kind\":\"run\""), "got {json}");
        assert!(json.contains("\"runId\":\"run_1\""), "got {json}");
    }
    #[test]
    fn agent_event_serializes_activity_and_status() {
        let ev = OrchestrationEvent::Agent {
            run_id: "run_1".into(), agent_id: "a1".into(), task_id: Some("t1".into()),
            status: "working".into(), activity: Some("tool_use".into()),
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("\"kind\":\"agent\"") && json.contains("\"agentId\":\"a1\""), "got {json}");
    }
    #[test]
    fn null_sink_is_noop() {
        NullEventSink.emit(OrchestrationEvent::Task {
            run_id: "r".into(), task_id: "t".into(), status: "ready".into(), dispatch_id: None,
        });
    }
}
```
- [ ] **Step 2: 确认失败** → `cargo test --manifest-path src-tauri/Cargo.toml events` → FAIL（模块不存在）
- [ ] **Step 3: 实现** `events.rs`（上方 Interfaces 全文）；`mod.rs` 加 `mod events; pub use events::{OrchestrationEvent, OrchestrationEventSink, NullEventSink};`
- [ ] **Step 4: 确认通过** → `cargo test --manifest-path src-tauri/Cargo.toml events` → PASS
- [ ] **Step 5: 提交** `feat(hermes): orchestration event sink contract and null sink`

## Task 2: Coordinator 注入可选 EventSink + dispatch/convergence 发射 task 事件

**Files:**
- Modify: `src-tauri/src/hermes/coordinator.rs`（`Coordinator` 增字段 `event_sink: Arc<dyn OrchestrationEventSink>`，默认 `Arc::new(NullEventSink)`；`with_event_sink` builder；`dispatch_one` 派发成功后发 `Task{Dispatched}`；`drain_inbox` 标 Completed 后发 `Task{Completed}`；`fail_dispatch_with_cascade` 内或调用点发 `Task{Failed}`）
- Test: `coordinator.rs::tests`（注入收集型 sink，断言事件序列）

**Interfaces:**
- Consumes: `OrchestrationEvent`、`OrchestrationEventSink`、`NullEventSink`（Task 1）；现有 `dispatch_one`/`drain_inbox`/`fail_dispatch_with_cascade`。
- Produces:
  ```rust
  // Coordinator 新增两个字段：
  //   event_sink: Arc<dyn OrchestrationEventSink>,   // 默认 Arc::new(NullEventSink)
  //   event_run_id: std::sync::OnceLock<String>,     // run() 入口一次性写入
  pub fn with_event_sink(mut self, sink: Arc<dyn OrchestrationEventSink>) -> Self;
  ```
  > **关键准确性约束（防跑偏）**：`tick(&self)`、`dispatch_one(&self, task: &Task)`、`drain_inbox(&self)` 的现有签名**绝不能改**——Phase 2 有大量测试直接调 `tick()`/`dispatch_one()`，改签名会全红。run_id 用**内部可变**传递：
  > - `Coordinator::new` 里 `event_sink: Arc::new(NullEventSink)`、`event_run_id: OnceLock::new()`（`OnceLock` 与 `&self` 共存，无需 `&mut`）。
  > - `run(run_id)` 进循环前：`let _ = self.event_run_id.set(run_id.to_string());`。
  > - `dispatch_one` / `drain_inbox` / watcher 内读：`let run_id = self.event_run_id.get().cloned().unwrap_or_default();`，再 `self.event_sink.emit(OrchestrationEvent::Task{ run_id, .. })`。
  > - watcher 是 `dispatch_one` 里的 `tokio::spawn`（见 coordinator.rs:573）：spawn 前先 `let sink_w = Arc::clone(&self.event_sink); let run_id_w = self.event_run_id.get().cloned().unwrap_or_default();`，move 进闭包，在 `AgentEvent::Done/Failed` 分支 emit `Task{Completed/Failed}`。
  > - 三处 emit 点：`dispatch_one` 派发成功后（在 coordinator.rs:644 `Ok(())` 前）emit `Task{Dispatched, dispatch_id}`；watcher 的 `Done{success:true}` 写完 worker_done 后 emit `Task{Completed}`；`fail_dispatch_with_cascade` 返回 `task_failed=true` 时 emit `Task{Failed}`。`drain_inbox` 标 Completed 的事件由 watcher 侧已覆盖，避免重复发射（二选一：**优先在 watcher 发**，drain_inbox 不发，防重复）。

- [ ] **Step 1: 写失败测试**（收集型 sink）
```rust
// 测试辅助：把 emit 的事件收集到 Vec，断言用。
struct CollectSink(std::sync::Mutex<Vec<OrchestrationEvent>>);
impl OrchestrationEventSink for CollectSink {
    fn emit(&self, ev: OrchestrationEvent) { self.0.lock().unwrap().push(ev); }
}
#[tokio::test]
async fn dispatch_emits_task_dispatched_event() {
    // 用现有 mock runtime + in-memory store 建一个 ready 任务，注入 CollectSink，
    // 跑一轮 tick，断言收集到 Task{status:"dispatched", task_id:..}。
    let sink = Arc::new(CollectSink(Default::default()));
    let coord = /* Coordinator::new(...).with_event_sink(sink.clone()) */;
    coord.run("run_x").await.ok(); // 或 driven tick
    let evs = sink.0.lock().unwrap();
    assert!(evs.iter().any(|e| matches!(e, OrchestrationEvent::Task{status, ..} if status=="dispatched")));
}
```
- [ ] **Step 2: 确认失败** → `cargo test --manifest-path src-tauri/Cargo.toml dispatch_emits_task` → FAIL
- [ ] **Step 3: 实现** 字段 + builder + 三处 emit（dispatched/completed/failed），run_id 沿调用链传递；`Coordinator::new` 默认 NullEventSink
- [ ] **Step 4: 确认通过 + 非回归** → `cargo test --manifest-path src-tauri/Cargo.toml hermes::coordinator` 全绿（含 Phase 2 旧用例不变）
- [ ] **Step 5: 提交** `feat(hermes): coordinator emits task lifecycle events via sink`

## Task 3: Supervisor/watcher 发射 agent 事件（活动 / 等待输入 / 判活）

**Files:**
- Modify: `src-tauri/src/hermes/coordinator.rs`（watcher 在 `supervisor.on_event` 后，按事件类别 emit `Agent{activity}`；`NeedsInput` → `Agent{status:"needs-attention"}`；reap 标 Suspect → `Agent{status:"interrupted"}`）
- Test: `coordinator.rs::tests`

**Interfaces:**
- Consumes: `OrchestrationEvent::Agent`、watcher 的 `AgentEvent` 流、`supervisor.reap` 结果。
- Produces: agent 状态词表（**与设计 §10 AgentStateDot 对齐，集中常量**）：
  ```rust
  // coordinator.rs 常量区（无魔法串）
  const AGENT_STATUS_WORKING: &str = "working";        // 有 stream 活动
  const AGENT_STATUS_NEEDS_ATTENTION: &str = "needs-attention"; // NeedsInput / 权限请求
  const AGENT_STATUS_DONE: &str = "done";              // Done
  const AGENT_STATUS_INTERRUPTED: &str = "interrupted"; // reap/ abort
  const AGENT_ACTIVITY_TOOL_USE: &str = "tool_use";
  const AGENT_ACTIVITY_TEXT: &str = "text";
  const AGENT_ACTIVITY_THINKING: &str = "thinking";
  ```
- [ ] **Step 1: 写失败测试** —— 程序化一个发 `ToolUse` 然后 `NeedsInput` 的 mock worker，断言收集到 `Agent{status:"working", activity:"tool_use"}` 与 `Agent{status:"needs-attention"}`。
- [ ] **Step 2: 确认失败** → FAIL
- [ ] **Step 3: 实现** watcher 内按 `AgentEvent` 分支 emit；reap 分支 emit `interrupted`。状态词全用常量。
- [ ] **Step 4: 确认通过 + 非回归** → `cargo test hermes::coordinator hermes::supervisor` 全绿
- [ ] **Step 5: 提交** `feat(hermes): watcher emits agent activity and liveness events`

## Task 4: 命令层 `TauriEventSink` + run/agent 读命令 + 时间戳 RFC3339（D.5）

**Files:**
- Modify: `src-tauri/src/commands/hermes_commands.rs`（实现 `TauriEventSink`：包 `AppHandle`，`emit` 把 `OrchestrationEvent` 映射到 `hermes://run`/`hermes://task`/`hermes://agent`；`start_run` 构造 sink 注入 Coordinator + 删掉本地重复 run emit，改走 sink；新增命令 `hermes_run_show`、`hermes_agent_list`）
- Modify: `src-tauri/src/hermes/store.rs`（把 `runs`/`gates`/`dispatch` 里 `datetime('now')` 列统一改为 `chrono::Utc::now().to_rfc3339()`，对齐其余列；D.5）
- Modify: `src-tauri/src/lib.rs`（`generate_handler!` 注册新命令）
- Test: `hermes_commands.rs::tests`（事件名映射纯函数 + RunShowDto 转换）

**Interfaces:**
- Consumes: `OrchestrationEvent`（Task 1）、`OrchestrationEventSink`、现有 `HermesEngine`。
- Produces:
  ```rust
  pub struct TauriEventSink { app: AppHandle, run_id: String }
  impl OrchestrationEventSink for TauriEventSink {
      fn emit(&self, ev: OrchestrationEvent) {
          let channel = match &ev {
              OrchestrationEvent::Run{..} => HERMES_EVENT_RUN,
              OrchestrationEvent::Task{..} => HERMES_EVENT_TASK,
              OrchestrationEvent::Agent{..} => HERMES_EVENT_AGENT,
          };
          let _ = self.app.emit(channel, ev); // best-effort
      }
  }
  #[derive(Serialize)] #[serde(rename_all="camelCase")]
  pub struct RunShowDto { pub id: String, pub goal: String, pub status: String,
      pub created_at: String, pub completed_at: Option<String>, pub task_count: usize, pub completed_count: usize }
  // 命令：
  // hermes_run_show(run_id) -> RunShowDto       —— 驾驶舱顶部 run 概览
  // hermes_agent_list() -> Vec<DispatchDto>     —— 当前活跃 dispatch（驾驶舱 Roster）
  ```
  > `start_run` 改造：`let sink = Arc::new(TauriEventSink{ app: app.clone(), run_id: run.id.clone() });` → `Coordinator::new(...).with_event_sink(sink)`；run 启动/终态事件改 `sink.emit(OrchestrationEvent::Run{..})`（去掉原 `app.emit(HERMES_EVENT_RUN, RunEventPayload{..})` 重复路径，保持单一发射口）。移除现已无用的 `RunEventPayload`/`TaskEventPayload` 占位（或保留但标注被 OrchestrationEvent 取代——优先删，减熵）。`#[allow(dead_code)]` 从 `HERMES_EVENT_TASK`/`HERMES_EVENT_AGENT` 上移除（现已被消费）。
- [ ] **Step 1: 写失败测试** —— `event_channel_for(&OrchestrationEvent)` 纯函数映射三类→三通道；`RunShowDto` 从 run + task 列表计数。
- [ ] **Step 2: 确认失败** → FAIL
- [ ] **Step 3: 实现** `TauriEventSink` + 两个命令 + Store 时间戳统一 + lib.rs 注册
- [ ] **Step 4: 确认通过 + build** → `cargo test hermes_commands` 全绿；`npm run build` green（前端暂未用新命令，确认编译不破）
- [ ] **Step 5: 提交** `feat(hermes): tauri event sink, run/agent read commands, rfc3339 timestamps`

## Task 5: 冻结契约文档（Phase 4 据此并行开工）

**Files:**
- Create: `docs/helm-hermes-ui-contract.md`（命令清单 + 事件清单 + payload 形状 + agent 状态词表 + 干预点占位）
- Test: 无（文档）；但必须与 Task 1–4 实际签名**逐字一致**。

**Interfaces:** 文档列出（供 Phase 4 / Codex 冷启动消费）：
- 命令：`hermes_run(goal, opts)→runId`、`hermes_task_list(filter)→TaskDto[]`、`hermes_dispatch_show(dispatchId)→DispatchDto`、`hermes_gate_resolve(gateId, resolution)`、`hermes_run_stop(runId)`、`hermes_run_show(runId)→RunShowDto`、`hermes_agent_list()→DispatchDto[]`。
- 事件：`hermes://run`/`hermes://task`/`hermes://agent`（统一 `OrchestrationEvent` 形状，含 `kind` tag）。
- agent 状态词表：`working`/`needs-attention`/`done`/`interrupted`（+ Phase 3 后续补 `idle`/`active`）。
- **Phase 3 后续子阶段会扩充的契约**（先占位声明，Phase 4 据此预留 UI）：`hermes_run_cancel`（3c）、worktree 合并/丢弃命令（3d）、人工干预命令族（Phase 4 与引擎共建）。

- [ ] **Step 1: 写文档**（对照 hermes_commands.rs / events.rs 实际签名）
- [ ] **Step 2: 自检** —— 文档每个签名能在代码里找到对应（命令名 grep 命中、payload 字段名与 DTO 一致）
- [ ] **Step 3: 提交** `docs(helm): freeze hermes ui contract for phase 4 parallel work`

### ✅ GATE 3a（必须停下自检 + 合并）
```bash
cargo test --manifest-path src-tauri/Cargo.toml hermes   # 全绿（含新增 events/coordinator/commands 用例）
cargo test --manifest-path src-tauri/Cargo.toml chat     # 69 全绿（非回归）
cargo check --manifest-path src-tauri/Cargo.toml
npm run build && git diff --check
git checkout feat/helm && git merge --no-ff feat/helm-phase3a-contract -m "merge: Phase 3a — UI contract + live task/agent events"
```
**合回后立即知会 Phase 4（Codex）：契约已冻结在 `docs/helm-hermes-ui-contract.md`，可接真事件。**

---

# 子阶段 3b：异构介质单实例路由（一次 run 内 SDK × CLI 混跑）

> 分支：`git checkout feat/helm && git checkout -b feat/helm-phase3b-registry`
> 目标：把 Coordinator/Supervisor 从「单 `Arc<dyn AgentRuntime>`」升级为「`RuntimeKind → Arc<dyn AgentRuntime>` 注册表」，按 `task.assignment.runtime` 选介质。这是"管理多 CLI × 多模型并行"的核心兑现（D.4）。

## Task 6: `RuntimeRegistry`（介质注册表 + 按 RuntimeKind 路由）

**Files:**
- Create: `src-tauri/src/hermes/runtime_registry.rs`
- Modify: `src-tauri/src/hermes/mod.rs`（声明 + re-export）
- Test: `runtime_registry.rs`

**Interfaces:**
- Consumes: `AgentRuntime`、`RuntimeKind`（types.rs）。
- Produces:
  ```rust
  /// 介质注册表：一次 run 内按 task.assignment.runtime 选具体介质。
  #[derive(Clone)]
  pub struct RuntimeRegistry {
      runtimes: std::collections::HashMap<RuntimeKind, Arc<dyn AgentRuntime>>,
  }
  impl RuntimeRegistry {
      pub fn new() -> Self;
      pub fn with(mut self, kind: RuntimeKind, rt: Arc<dyn AgentRuntime>) -> Self;
      /// 取某介质；缺失返回 Err（调用方决定 fail_dispatch）。
      pub fn get(&self, kind: RuntimeKind) -> Result<Arc<dyn AgentRuntime>, String>;
      /// 单介质便捷构造（兼容 Phase 2：所有 task 都走同一介质）。
      pub fn single(rt: Arc<dyn AgentRuntime>) -> Self; // 同时登记为默认
      /// liveness 探测用：返回任一已登记介质（reap 需要按 agent 的介质查；见 Task 8）。
      pub fn kinds(&self) -> Vec<RuntimeKind>;
  }
  ```
- [ ] **Step 1: 写失败测试** —— `with(Sdk, a).with(Cli, b)` 后 `get(Sdk)` 命中 a、`get(Cli)` 命中 b、`get(缺失)` Err；`single(rt)` 后 `get(Sdk)`==`get(Cli)`==rt。
- [ ] **Step 2: 确认失败** → FAIL
- [ ] **Step 3: 实现** `runtime_registry.rs`
- [ ] **Step 4: 确认通过** → PASS
- [ ] **Step 5: 提交** `feat(hermes): runtime registry for heterogeneous media routing`

## Task 7: Coordinator 用注册表派发（按 assignment.runtime 选介质）

**Files:**
- Modify: `src-tauri/src/hermes/coordinator.rs`（`runtime: Arc<dyn AgentRuntime>` → `registry: RuntimeRegistry`；`new(store, registry, ...)`；`dispatch_one` 读 `task.assignment.runtime`（缺省 Sdk）→ `registry.get(kind)` 选介质 start/send；watcher 持有该 task 的介质引用做 abort）
- Modify: `src-tauri/src/commands/hermes_commands.rs`（`HermesEngine` 持 `RuntimeRegistry`；`new` 改签名；`start_run` 用 registry 建 Coordinator）
- Modify: `src-tauri/src/lib.rs`（setup 构造 registry：先登记 `SdkRuntime`；CliRuntime 在 Task 9 接）
- Test: `coordinator.rs::tests`（两介质 mock，一 task 走 Sdk 一 task 走 Cli，断言各自介质被调）

**Interfaces:**
- Consumes: `RuntimeRegistry`（Task 6）、`AgentAssignment.runtime`（types.rs）。
- Produces: `Coordinator::new(store, registry: RuntimeRegistry, repo_root, worktrees_dir)`（**签名变更**——所有 Phase 2 测试构造点改为 `RuntimeRegistry::single(rt)` 包装，行为不变即非回归）。
  > **准确性约束（dispatch_one 现状，已 codegraph 核对）**：当前 `dispatch_one`（coordinator.rs:471）在 4 处用 `self.runtime`：`self.runtime.start`（:503）、`self.runtime.send`（:531）、`self.runtime.capabilities().structured_events`（:544，喂 supervisor.register）。它**故意不读** `task.assignment.runtime`，且 `start_spec.provider` 写死 `DEFAULT_PROVIDER`（:500，代码注释 :486-491 明确说"Phase 3 才支持异构选择"）。本 Task 改法：在 `dispatch_one` 开头 `let kind = task.assignment.as_ref().map(|a| a.runtime).unwrap_or(RuntimeKind::Sdk); let runtime = self.registry.get(kind)?;`，随后 4 处 `self.runtime` 改用局部 `runtime`。`start_spec.provider` 暂仍用 `DEFAULT_PROVIDER`（vendor 选择不在本子阶段——`AgentAssignment` 无 provider 字段；medium=RuntimeKind 已满足"多 CLI 并行"目标，model 已来自 `assignment.model`）。
  > 非回归策略：Phase 2 的 `Coordinator::new(store, rt, ...)` 全部改为 `Coordinator::new(store, RuntimeRegistry::single(rt), ...)`。`single` 把同一 rt 登记到所有 kind，`dispatch_one` 无论 assignment 是什么都拿到它 → 行为与 Phase 2 逐字一致。`with_supervisor` 仍接受 supervisor（Task 8 再让 supervisor 也走 registry）。
- [ ] **Step 1: 写失败测试** —— 建两个 ready task：t_sdk.assignment.runtime=Sdk、t_cli.assignment.runtime=Cli；registry 登记两个不同 mock（各记录被调 agent_id）；跑 tick；断言 SdkMock 收到 t_sdk、CliMock 收到 t_cli。
- [ ] **Step 2: 确认失败** → FAIL（当前单 runtime）
- [ ] **Step 3: 实现** registry 化 + dispatch_one 路由；逐个改 Phase 2 测试构造点为 `RuntimeRegistry::single(...)`
- [ ] **Step 4: 确认通过 + 非回归** → `cargo test hermes` 全绿（所有旧用例经 single() 包装后仍通过）
- [ ] **Step 5: 提交** `feat(hermes): coordinator routes dispatch by assignment runtime kind`

## Task 8: Supervisor reap 按 agent 介质查 liveness

**Files:**
- Modify: `src-tauri/src/hermes/supervisor.rs`（`WorkerSupervisor` 持 `RuntimeRegistry` 而非单 runtime；`register(agent_id, structured, kind)` 记下该 agent 的 `RuntimeKind`；reap 的 liveness 探针用 `registry.get(kind)`）
- Modify: `src-tauri/src/hermes/coordinator.rs`（`dispatch_one` 调 `supervisor.register(agent_id, structured, kind)` 带上介质种类）
- Test: `supervisor.rs::tests`（两介质，断言各自 liveness 探针走对介质）

**Interfaces:**
- Consumes: `RuntimeRegistry`（Task 6）。
- Produces: `WorkerSupervisor::new(registry: RuntimeRegistry)`（签名变更，Phase 2 测试用 `RuntimeRegistry::single`）；`register(&self, agent_id: &str, structured: bool, kind: RuntimeKind)`。
- [ ] **Step 1: 写失败测试** —— 注册一个 Cli agent + 一个 Sdk agent，各自介质 mock 返回不同 liveness，断言 reap 用对介质判活。
- [ ] **Step 2: 确认失败** → FAIL
- [ ] **Step 3: 实现** supervisor registry 化 + register 带 kind
- [ ] **Step 4: 确认通过 + 非回归** → `cargo test hermes::supervisor hermes::coordinator` 全绿
- [ ] **Step 5: 提交** `feat(hermes): supervisor probes liveness per-agent runtime kind`

## Task 9: setup 登记 SDK + CLI 双介质 + 验证混跑

**Files:**
- Modify: `src-tauri/src/lib.rs`（setup 构造 `RuntimeRegistry::new().with(Sdk, SdkRuntime::new(...)).with(Cli, CliRuntime::new(...))`，注入 HermesEngine + Supervisor 共享同一 registry）
- Test: `coordinator.rs::tests`（混合 run：3 task 两 Sdk 一 Cli，全 Completed）

**Interfaces:**
- Consumes: Task 6–8 全部；真实 `SdkRuntime`/`CliRuntime`。
- Produces: 无新签名；这是接线 + 端到端验证 task。
- [ ] **Step 1: 写失败测试** —— mock 两介质的混合 e2e：goal 拆 3 task（assignment 分别 Sdk/Sdk/Cli）→ 并行派发 → 各介质 mock 发 Done → 全 Completed、run Completed。
- [ ] **Step 2: 确认失败** → FAIL
- [ ] **Step 3: 实现** lib.rs 双介质登记；确认 Supervisor 与 Coordinator 共享同一 registry 实例
- [ ] **Step 4: 确认通过 + build** → `cargo test hermes` 全绿；`npm run build` green
- [ ] **Step 5: 提交** `feat(hermes): register sdk and cli runtimes for in-run heterogeneous scheduling`

### ✅ GATE 3b
```bash
cargo test --manifest-path src-tauri/Cargo.toml hermes && cargo test --manifest-path src-tauri/Cargo.toml chat
cargo check --manifest-path src-tauri/Cargo.toml && npm run build && git diff --check
git checkout feat/helm && git merge --no-ff feat/helm-phase3b-registry -m "merge: Phase 3b — heterogeneous in-run media routing"
```

---

# 子阶段 3c：run 内取消（mid-run cancel，D.2）

> 分支：`git checkout feat/helm && git checkout -b feat/helm-phase3c-cancel`
> 目标：Coordinator 每轮 tick 前检查取消信号；取消时 abort 在飞 dispatch、把 run 标终态（新增 `RunStatus::Cancelled`）、发 `Run{status:"cancelled"}` 事件。当前 `hermes_run_stop` 只在进循环前生效——本子阶段补 mid-loop。

## Task 10: `RunStatus::Cancelled` + Coordinator 取消信号

**Files:**
- Modify: `src-tauri/src/hermes/types.rs`（`RunStatus` 加 `Cancelled` + as_str/from_str `"cancelled"` + roundtrip 测试）
- Modify: `src-tauri/src/hermes/store.rs`（**关键：schema CHECK 约束**——`coordinator_runs.status` 当前是 `CHECK(status IN ('idle', 'running', 'completed', 'failed'))`（store.rs:271）。**必须**改为 `CHECK(status IN ('idle', 'running', 'completed', 'failed', 'cancelled'))`，否则 `update_run(.., Cancelled)` 运行时被 CHECK 拒绝。Hermes DB 是 Phase 2 新建、无历史数据，schema 直接改即可；若已有本地 `hermes.db` 需删库重建——在 Task 注释说明。）
- Modify: `src-tauri/src/hermes/coordinator.rs`（`with_cancel(Arc<AtomicBool>)`；`run()` 循环每轮 tick 前 `if cancel.load() { abort 在飞 + 标 Cancelled + emit + break }`；`derive_final_status` 不受影响——cancel 走独立分支直接 `update_run(Cancelled)`）
- Test: `types.rs`（roundtrip 含 Cancelled）、`store.rs::tests`（`update_run(.., Cancelled)` 成功且读回 Cancelled——证明 CHECK 已放行）、`coordinator.rs::tests`（注入 cancel，跑到一半置 true，断言 run=Cancelled 且在飞 dispatch 的介质 abort 被调）

**Interfaces:**
- Consumes: 现有 `run()` 循环、`runtime.abort`、`Store::list_active_dispatches`、`OrchestrationEvent::Run`。
- Produces:
  ```rust
  // types.rs: RunStatus::Cancelled => "cancelled"
  // coordinator.rs:
  pub fn with_cancel(mut self, cancel: Arc<std::sync::atomic::AtomicBool>) -> Self;
  // run() 每轮起始：检查 cancel → abort 所有 active dispatch 的 agent → update_run(Cancelled) → sink.emit(Run{cancelled}) → return Ok(Cancelled)
  ```
- [ ] **Step 1: 写失败测试** —— roundtrip `Cancelled`；`store.rs::tests` 里 `update_run(.., Cancelled)` 成功并读回 Cancelled；`coordinator.rs::tests` 注入 cancel=false 跑一轮、置 true 再跑一轮 → run 状态 Cancelled、active dispatch 的介质 abort 被调。
- [ ] **Step 2: 确认失败** → FAIL（CHECK 拒绝 / 无 with_cancel）
- [ ] **Step 3: 实现** RunStatus::Cancelled（types.rs）+ **schema CHECK 加 'cancelled'**（store.rs:271）+ with_cancel + 循环内检查 + abort 在飞（`list_active_dispatches` → 各 dispatch 的 assignee → `registry.get(kind).abort`）+ emit `Run{cancelled}`
- [ ] **Step 4: 确认通过 + 非回归** → `cargo test hermes::types hermes::store hermes::coordinator` 全绿
- [ ] **Step 5: 提交** `feat(hermes): mid-run cancellation with cancelled run status`

## Task 11: `hermes_run_cancel` 命令接线（复用 RunHandle.cancel）

**Files:**
- Modify: `src-tauri/src/commands/hermes_commands.rs`（`start_run` 把 `cancel` 经 `with_cancel` 注入 Coordinator；新增/重命名命令 `hermes_run_cancel(run_id)`——语义改为真 mid-run；保留 `hermes_run_stop` 作别名或废弃说明）
- Modify: `src-tauri/src/lib.rs`（注册 `hermes_run_cancel`）
- Test: `hermes_commands.rs::tests`（cancel 标志置位 + 不存在 run_id 报错——沿用现有 `engine_stop_run_*` 模式）

**Interfaces:**
- Consumes: Task 10 `with_cancel`；现有 `RunHandle{cancel}`、`stop_run`。
- Produces: `hermes_run_cancel(run_id, state)`（薄 delegate `stop_run`）；`start_run` 内 `Coordinator::new(...).with_cancel(cancel.clone())`。
- [ ] **Step 1: 写失败测试** —— 注册 RunHandle，`hermes_run_cancel` 把 cancel 置 true；空/未知 run_id 报错。
- [ ] **Step 2: 确认失败** → FAIL
- [ ] **Step 3: 实现** 注入 with_cancel + 命令 + lib.rs 注册；契约文档 `hermes_run_cancel` 占位转正
- [ ] **Step 4: 确认通过 + build** → `cargo test hermes_commands` 全绿；`npm run build`
- [ ] **Step 5: 提交** `feat(hermes): hermes_run_cancel wires cancel signal into coordinator loop`

### ✅ GATE 3c
```bash
cargo test --manifest-path src-tauri/Cargo.toml hermes && cargo test --manifest-path src-tauri/Cargo.toml chat
cargo check --manifest-path src-tauri/Cargo.toml && npm run build && git diff --check
git checkout feat/helm && git merge --no-ff feat/helm-phase3c-cancel -m "merge: Phase 3c — mid-run cancellation"
```

---

# 子阶段 3d：收敛后 worktree/分支安全清理（D.8，Phase 3 首要正确性补丁）

> 分支：`git checkout feat/helm && git checkout -b feat/helm-phase3d-cleanup`
> 目标：一次 run 收敛（Completed/Failed/Cancelled）后，对每个 task 的 worktree 执行**安全清扫**——失败/被取消/无改动的 → 清理；已完成且有改动的 → **保留并标记"待合并决策"**（绝不静默删除未合并工作）。提供 `hermes_run_cleanup` 命令。**破坏性安全红线**：见 Global Constraints。

## Task 12: `run_lifecycle` 纯逻辑——决定每个 worktree 的处置

**Files:**
- Create: `src-tauri/src/hermes/run_lifecycle.rs`
- Modify: `src-tauri/src/hermes/mod.rs`
- Test: `run_lifecycle.rs`

**Interfaces:**
- Consumes: `TaskStatus`、`DispatchContext`、worktree 是否有未提交改动（布尔，注入便于纯测）。
- Produces:
  ```rust
  /// 单个 worktree 的清理处置（纯决策，不碰 git）。
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub enum WorktreeDisposition {
      /// 安全删除（task 失败/取消，或无任何改动）。
      Remove,
      /// 保留，等用户在驾驶舱选 merge/discard（task 完成且有改动）。
      RetainForReview,
  }
  pub struct WorktreeCleanupInput {
      pub task_status: TaskStatus,
      pub has_uncommitted_changes: bool,
      pub has_commits_ahead: bool, // 相对 feat/helm 有领先提交 = 有产出
  }
  /// 决策规则：完成 + (有改动 or 领先提交) → RetainForReview；否则 Remove。
  pub fn decide_disposition(input: &WorktreeCleanupInput) -> WorktreeDisposition;
  ```
- [ ] **Step 1: 写失败测试** —— Completed+有领先提交→RetainForReview；Completed+无改动无领先→Remove；Failed→Remove；Cancelled+有未提交→**RetainForReview**（保守，不丢用户可能想要的中间产物）。
- [ ] **Step 2: 确认失败** → FAIL
- [ ] **Step 3: 实现** `decide_disposition`
- [ ] **Step 4: 确认通过** → PASS
- [ ] **Step 5: 提交** `feat(hermes): worktree cleanup disposition pure logic`

## Task 13: WorktreeManager 补 `has_commits_ahead` + end-of-run 清扫

**Files:**
- Modify: `src-tauri/src/chat/worktree.rs`（新增**关联函数** `has_commits_ahead`：`git -C <worktree> rev-list --count <base>..HEAD` > 0）
- Modify: `src-tauri/src/hermes/run_lifecycle.rs`（`sweep_run_worktrees`：枚举 run 内 task → 定位其 worktree → 查 disposition → `Remove` 调 `WorktreeManager::remove`、`RetainForReview` 发 `Task{status:"awaiting-merge"}` 事件）
- Test: `worktree.rs::tests`（用 `tempfile` 真 git repo，断言 has_commits_ahead）

**Interfaces:**
- Consumes: `decide_disposition`（Task 12）、`OrchestrationEventSink`、`TaskStatus`、`Store::list_tasks`。
- **准确性约束**：`WorktreeManager` 是**关联函数风格**（无实例方法、无 `&self`）。真实签名（worktree.rs 已存在，逐字对齐）：
  ```rust
  // 已存在（只读引用，勿改）：
  // WorktreeManager::create(repo_root: &Path, worktrees_dir: &Path, name: &str) -> Result<WorktreeInfo, String>
  // WorktreeManager::remove(repo_root: &Path, worktree_path: &Path, force: bool) -> Result<(), String>
  // WorktreeManager::list(repo_root: &Path) -> Result<Vec<WorktreeInfo>, String>
  // WorktreeManager::has_uncommitted_changes(worktree_path: &Path) -> Result<bool, String>
  // 分支前缀常量：HELM_BRANCH_PREFIX = "helm/"；每 task 的 worktree 分支 = format!("{HELM_BRANCH_PREFIX}{task_id}")
  ```
- Produces:
  ```rust
  // worktree.rs —— 关联函数（不是 &self）
  pub fn has_commits_ahead(worktree_path: &Path, base_branch: &str) -> Result<bool, String>;
  // run_lifecycle.rs —— 介质无关；用关联函数 + repo_root，不持有 WorktreeManager 实例
  pub fn sweep_run_worktrees(
      repo_root: &Path, store: &Store, base_branch: &str,
      sink: &dyn OrchestrationEventSink, run_id: &str,
  ) -> Result<SweepReport, String>; // SweepReport{ removed: usize, retained: usize }
  ```
  > 定位每个 task 的 worktree：`WorktreeManager::list(repo_root)` 枚举，按分支名 `helm/<task_id>` 匹配（`WorktreeInfo` 含 path + branch）。
  > **破坏性安全双保险**：`Remove` 分支执行 `WorktreeManager::remove(.., force=true)` 前，再查一次 `WorktreeManager::has_uncommitted_changes(path)`——若意外有未提交改动则降级为 `RetainForReview`，**绝不删未保存工作**（即使 task 标 Failed）。
- [ ] **Step 1: 写失败测试** —— tempfile repo：建 worktree、commit 一次 → `has_commits_ahead`==true；不 commit → false。
- [ ] **Step 2: 确认失败** → FAIL
- [ ] **Step 3: 实现** `has_commits_ahead` + `sweep_run_worktrees`（含删除前双保险复查）
- [ ] **Step 4: 确认通过** → `cargo test worktree` 全绿
- [ ] **Step 5: 提交** `feat(hermes): end-of-run worktree sweep with retain-for-review safety`

## Task 14: 收敛后自动清扫接线 + `hermes_run_cleanup` 命令

**Files:**
- Modify: `src-tauri/src/hermes/coordinator.rs`（`run()` 收敛/取消后、返回前调 `sweep_run_worktrees(&self.repo_root, &self.store, base_branch, self.event_sink.as_ref(), run_id)`；`base_branch` 由 Coordinator 新字段 `base_branch: String`（默认 `"feat/helm"` 常量 `HELM_BASE_BRANCH`，可 builder 覆盖）提供；常量开关 `SWEEP_ON_CONVERGE: bool = true`）
- Modify: `src-tauri/src/commands/hermes_commands.rs`（`hermes_run_cleanup(run_id)` 手动触发清扫——给 UI 兜底入口；`SweepReport` → `SweepReportDto` camelCase）
- Modify: `src-tauri/src/lib.rs`（注册 `hermes_run_cleanup`）
- Test: `run_lifecycle.rs::tests`（**真 tempfile git repo**，不 mock 关联函数）；`hermes_commands.rs::tests`（SweepReportDto 转换）

**Interfaces:**
- Consumes: Task 13 `sweep_run_worktrees`（关联函数风格）。
- Produces: `hermes_run_cleanup(run_id: String, state) -> SweepReportDto`；`SweepReportDto{ removed: usize, retained: usize }`（camelCase）。
  > **准确性约束**：`WorktreeManager` 无实例、不可 mock。sweep 的验证走**真 tempfile git repo**：在临时 repo 建两个 worktree——一个 commit 一次（领先 → RetainForReview）、一个干净（→ Remove）；Store 里对应建两个 task（一 Completed 一 Failed）；调 `sweep_run_worktrees` 断言 `SweepReport{removed:1, retained:1}` 且被删的 worktree 路径已不存在、保留的仍在。纯决策分支已由 Task 12 `decide_disposition` 单测覆盖。
- [ ] **Step 1: 写失败测试** —— `run_lifecycle.rs::tests` 用 tempfile repo 跑 sweep，断言 SweepReport 计数 + 文件系统实际状态；`SweepReportDto` 转换测试。
- [ ] **Step 2: 确认失败** → FAIL
- [ ] **Step 3: 实现** Coordinator 收敛后 sweep（带 base_branch 字段）+ 手动命令 + 注册
- [ ] **Step 4: 确认通过 + 非回归 + build** → `cargo test hermes` 全绿；`npm run build`
- [ ] **Step 5: 提交** `feat(hermes): sweep worktrees on run convergence and manual cleanup command`

### ✅ GATE 3d
```bash
cargo test --manifest-path src-tauri/Cargo.toml hermes && cargo test --manifest-path src-tauri/Cargo.toml chat
cargo check --manifest-path src-tauri/Cargo.toml && npm run build && git diff --check
git checkout feat/helm && git merge --no-ff feat/helm-phase3d-cleanup -m "merge: Phase 3d — end-of-run worktree lifecycle"
```

---

# 子阶段 3e：并发 replan 单飞（D.3）

> 分支：`git checkout feat/helm && git checkout -b feat/helm-phase3e-singleflight`
> 目标：N 个 watcher 同一波熔断时各自独立 `planner.replan`——本子阶段加 per-run 单飞，避免重复 replan/重复 Converge 写入。

## Task 15: per-run 单飞守卫 + 接入 maybe_replan_on_failure

**Files:**
- Modify: `src-tauri/src/hermes/coordinator.rs`（`replan_inflight: Arc<tokio::sync::Mutex<HashSet<String>>>` 按 `run_id` 去重；`maybe_replan_on_failure` 进入前 `if !inflight.insert(run_id) { return }`，结束 remove）
- Test: `coordinator.rs::tests`（并发触发两次同 run replan，断言 planner.replan 只被调一次）

**Interfaces:**
- Consumes: 现有 `maybe_replan_on_failure`、`Planner`。
- Produces: Coordinator 字段 `replan_inflight`（默认空集）；单飞语义注释。
  > 注意：单飞 key = `run_id`（同一 run 同时只允许一个 replan 在飞）。用 `tokio::sync::Mutex` 守 HashSet（async 安全）。不改变无 planner 注入时的行为。
- [ ] **Step 1: 写失败测试** —— mock planner 计数被调次数；两个并发 fail 触发 → 断言 replan 调用次数 == 1（单飞）。
- [ ] **Step 2: 确认失败** → FAIL（当前会调 2 次）
- [ ] **Step 3: 实现** 单飞守卫
- [ ] **Step 4: 确认通过 + 非回归** → `cargo test hermes::coordinator` 全绿
- [ ] **Step 5: 提交** `fix(hermes): single-flight replan per run to dedupe concurrent circuit-breaks`

### ✅ GATE 3e
```bash
cargo test --manifest-path src-tauri/Cargo.toml hermes && cargo test --manifest-path src-tauri/Cargo.toml chat
cargo check --manifest-path src-tauri/Cargo.toml && npm run build && git diff --check
git checkout feat/helm && git merge --no-ff feat/helm-phase3e-singleflight -m "merge: Phase 3e — single-flight replan"
```

---

# 子阶段 3f：LLM-judge 收敛/扇出评分（设计 §9）

> 分支：`git checkout feat/helm && git checkout -b feat/helm-phase3f-judge`
> 目标：给扇出/收敛加一个 LLM-judge——对多候选产物打分选优，或对"任务是否真完成"做判定。

## Task 16: Judge 纯函数 prompt + 解析 + Coordinator 钩子

**Files:**
- Modify: `src-tauri/src/hermes/planner.rs`（复用 Planner 的 LLM 驱动路径，加 `judge(candidates) -> JudgeVerdict` 纯 prompt 构造 + 容错解析）
- Test: `planner.rs::tests`（prompt 构造 + 解析边界）

**Interfaces:**
- Produces: `JudgeVerdict{ winner_index: usize, scores: Vec<f32>, reason: String }`；解析失败回落到确定性规则（取第一个/最多领先提交）。
- [ ] **Step 1: 写失败测试** —— 解析合法 judge JSON → winner_index；解析坏 JSON → 回落确定性。
- [ ] **Step 2: 确认失败** → FAIL
- [ ] **Step 3: 实现** judge prompt + 解析 + 回落
- [ ] **Step 4: 确认通过** → PASS
- [ ] **Step 5: 提交** `feat(hermes): llm-judge for fan-out/convergence scoring`

### ✅ GATE 3f
```bash
cargo test --manifest-path src-tauri/Cargo.toml hermes && cargo test --manifest-path src-tauri/Cargo.toml chat
cargo check --manifest-path src-tauri/Cargo.toml && npm run build && git diff --check
git checkout feat/helm && git merge --no-ff feat/helm-phase3f-judge -m "merge: Phase 3f — llm-judge for fan-out/convergence scoring"
```

---

# 子阶段 3g：收尾——验证文档 + 真 LLM e2e 步骤 + 交付报告

> 分支：`git checkout feat/helm && git checkout -b feat/helm-phase3g-verify`

## Task 17: Phase 3 验证文档（自动门证据 + mock e2e + 真 LLM 步骤）

**Files:**
- Create: `docs/helm-phase3-verification.md`
- Test: 无（文档），但所有"自动门"输出必须是**真跑出来的尾部证据**，不造假。

**内容：**
- A 自动门：`cargo test`（全量数字）、`cargo check`、`npm run build`、`git diff --check` 真实尾部输出。
- B mock 端到端：异构混跑（3b Task 9）、mid-run cancel（3c）、收敛清扫（3d）、单飞 replan（3e）各一段证据。
- C 真 LLM 手动 e2e（**待人工执行**，给步骤；含异构混跑：一个 task 指 Sdk/claude、一个指 Cli/codex，观察并行 + 各自 worktree + 收敛后清扫只留"待合并"产物 + mid-run cancel 能真停）。
- D 已知未决（进 Phase 4 / 之后）。

- [ ] **Step 1**: 跑全部自动门，抓真实尾部输出
- [ ] **Step 2**: 写文档（B 段引用具体测试名）
- [ ] **Step 3: 提交** `docs(hermes): phase 3 verification guide`

## Task 18: 交付报告 + Phase 4 就绪确认

**Files:**
- Create: `docs/helm-phase3-delivery.md`（DoD 逐条 + 改动清单 + git graph + 证据 + 是否就绪进 Phase 4 集成）

- [ ] **Step 1**: 写交付报告（对照本计划 DoD）
- [ ] **Step 2**: `git log --oneline --graph` 贴各子阶段分支与 merge
- [ ] **Step 3: 提交** `docs(hermes): phase 3 delivery report`

### ✅ GATE 3g（最终）
```bash
cargo test --manifest-path src-tauri/Cargo.toml          # 全量全绿
cargo check --manifest-path src-tauri/Cargo.toml && npm run build && git diff --check
git checkout feat/helm && git merge --no-ff feat/helm-phase3g-verify -m "merge: Phase 3g — verification + delivery"
```
**完成后通知 Phase 4：引擎契约全部就绪（含 cancel/cleanup/异构），可做末端集成验收。**

---

# 完成定义（DoD）

- [ ] Task 1–18 全部 step 打勾、各自 commit；7 个子阶段（3a–3g）各过 GATE 并 `--no-ff` merge 回 `feat/helm`。
- [ ] `cargo check` 通过；`cargo test`（hermes + chat 全绿）。
- [ ] `npm run build` 通过；`git diff --check` 干净。
- [ ] **契约冻结**：`docs/helm-hermes-ui-contract.md` 与代码签名逐字一致；3a 合回后 Phase 4 可并行接真事件。
- [ ] **异构混跑**：mock 下一次 run 内 SDK×CLI 各自路由、并行、收敛（3b Task 9）。
- [ ] **mid-run cancel**：mock 下跑到一半取消 → run=Cancelled + 在飞 abort（3c）。
- [ ] **收敛清扫安全**：失败/无改动 → Remove；完成有产出 → RetainForReview；绝不删未合并工作（3d）。
- [ ] **单飞 replan**：并发熔断只 replan 一次（3e）。
- [ ] `docs/helm-phase3-verification.md` + `docs/helm-phase3-delivery.md` 已写；真 LLM e2e 标「待人工执行」，**不造假**。
- [ ] 做完 Task 18 即停。

# 交付报告（GLM 收尾输出）

1. DoD 逐条状态（每子阶段 GATE 是否过、是否已 merge 回 feat/helm）。
2. 改动清单（新增 `events.rs`/`runtime_registry.rs`/`run_lifecycle.rs` 职责 + 既有文件加法式改动）。
3. `git log --oneline --graph` 各子阶段分支与 merge。
4. 验证证据（`cargo test`/`npm run build` 尾部；mock e2e：异构/cancel/cleanup/单飞各一段）。
5. 偏差与未决（尤其异构路由 / 取消 / 清理安全相关取舍）。
6. Phase 4 集成就绪确认（契约冻结 + 真事件可接）。

# Self-Review（计划作者已核）

- **Spec 覆盖**：D.1（3a Task 2/3）、D.2（3c）、D.3（3e）、D.4（3b）、D.5（3a Task 4）、D.8（3d）全部有对应 task；D.6/D.7（测试设计/PTY flake）属测试基础设施，3g 验证文档中说明，非功能 task。设计 §9 LLM-judge → 3f。§10 AgentStateDot 状态词表 → 3a Task 3 常量。
- **类型一致性**：`OrchestrationEvent`/`OrchestrationEventSink`（Task 1）贯穿 Task 2/3/4；`RuntimeRegistry`（Task 6）贯穿 7/8/9；`RunStatus::Cancelled`（Task 10）→ 11；`WorktreeDisposition`/`sweep_run_worktrees`（Task 12/13）→ 14。命令名 `hermes_run_cancel`/`hermes_run_cleanup`/`hermes_run_show`/`hermes_agent_list` 在契约文档（Task 5）与 lib.rs 注册一致。
- **非回归**：每个签名变更（Coordinator::new/WorkerSupervisor::new）都给了 `RuntimeRegistry::single` 兼容路径，Phase 2 测试经包装后行为不变；所有新钩子默认 Null/None/关闭。
- **无占位符**：每个 code step 给了真实签名/测试断言；GLM 执行时按 TDD 先写失败测试。
