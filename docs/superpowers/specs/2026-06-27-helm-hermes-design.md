# Helm × Hermes 设计文档：jadekit 多 Agent 编排驾驶舱

- 日期：2026-06-27
- 状态：设计草案（待评审）
- 命名：
  - **Helm** —— 面向用户的多 Agent 驾驶舱（cockpit），由现有「对话」页升级而来。
  - **Hermes** —— 后端编排引擎 codename（Rust `hermes/` 模块、SQLite、coordinator、状态机）。
- 借鉴对象：[orca](https://github.com/stablyai/orca)（生产级 AI 编排器），但执行介质由「刮终端跑外部 CLI」改为「可插拔 Runtime（SDK / CLI / 任意 agent loop）」。

---

## 1. 概述与定位

一句话：**Helm 是你指挥一支异构 AI 舰队（多 CLI × 多模型）的驾驶舱；Hermes 是替你拆解目标、按需选兵、并行调度、收敛交付的指挥官 AI。执行介质（SDK/CLI/任意 agent loop）可插拔，模型/CLI 路由是 AI 的一等决策。**

jadekit 的本命是「管理多个 AI CLI 与多个模型/provider」。Helm/Hermes 是把这些被管理的 CLI 和模型**真正派出去并行干活**的指挥层：jadekit 现有的 provider/模型 = 军火库（Roster），Hermes 从军火库调兵，在隔离的 git worktree 里并行执行，并把结果收敛回来。

### 与 orca 的对齐与差异

- **对齐**：worktree 隔离、进程化 agent、agent 生命周期、确定性 coordinator 循环、Kanban 状态看板、三栏 cockpit 布局、熔断/崩溃恢复等生产级机制。
- **差异**：orca 驱动外部 CLI 黑盒，被迫刮终端 + 等 `tui-idle` + 让 worker 自报 `worker_done`；Helm 走**可插拔 Runtime**，SDK 实现可直接拿结构化事件流，判活/完成是确定性信号，无需 worker 配合协议。CLI 实现保留（用于无好 SDK 的工具），但作为介质之一而非唯一。

---

## 2. 命名与信息架构（IA）

### 一个入口，三栏面板（对齐 orca，非路由分页）

orca 顶层是**单窗口、面板式** cockpit（`App.tsx` + `Sidebar.tsx` + `right-sidebar` + `terminal-pane`），靠 `PanelLeft/PanelRight` 开关面板，无页面跳转。Helm 同构，由 jadekit 现有 chat 三栏骨架就地长成：

```
┌──────────────── Tab bar（打开的 Agent）─────────────────┐
├──────────┬─────────────────────────────┬───────────────┤
│ 左栏      │  中间主区                    │ 右栏          │
│ 舰队看板  │  选中 Agent 的会话           │ worktree diff │
│ Kanban    │  （或 异构扇出：N 栏并排）    │ 合并/丢弃     │
│ 泳道+状态 │                              │ 评判结果      │
└──────────┴─────────────────────────────┴───────────────┘
   Composer（下达目标）        ⌘K Jump        Roster 面板
```

- **左栏 = 舰队看板**：按状态泳道（待派/执行中/待评审/完成）排布 Agent 卡，每卡显示 `CLI 图标 + 模型徽章 + AgentStateDot + worktree`。拖拽改状态 = 人工干预。
- **中区 = 选中 Agent 会话**；并行时变 N 栏异构扇出对比。
- **右栏 = worktree diff + 合并/丢弃 + 删除预检 + 评判结果**。
- **Composer = 下达目标**：可选「自动选兵（Hermes 路由）」或「手动指定 CLI+模型」，可选「单跑 / 扇出 / 编排」。
- **Roster 面板**：可部署的 CLI×模型清单，来自 jadekit 现有 provider 管理。

### 「对话」→「Helm」的关系

**一个入口，不是两个。** 导航里「对话」改名升级为「Helm」：

- 进 Helm 默认看到的就是现在的单 Agent 对话（零学习成本）。
- 需要多 Agent 时在 Helm 内部切到看板/扇出（面板开合，非路由）。
- **单聊 = 这个 cockpit 里左栏只选了一个 Agent 的特例**，与 orca「只开一个 worktree」完全同构。
- 代码：路由 `/chat` → `/helm`；`ChatPage` 成为 Helm 默认子视图；i18n 标签 `chat: "对话"` → `helm: "Helm"`（文案与 codename 解耦）。

---

## 3. 背景与关键约束（现状探查结论）

对 jadekit 现有 chat 后端（`chat_commands → ChatManager → DaemonClient → ai-bridge daemon → persistent-query-service`）的实探结论：

| 事实 | 位置 | 对设计的含义 |
|---|---|---|
| `pending: HashMap<id, tx>` 按 request-id 多路分发 | `daemon_client.rs:378` | 协议层已能多路复用，非瓶颈 |
| `SESSION_ID="default"` 写进进程 env | `daemon_client.rs:23,69` | 单 daemon = 单默认会话身份，需改为 per-agent |
| daemon 用 promise 队列 `.then(()=>processRequest)` **串行**处理 | `daemon.js:~595` | 单进程内 turn 串行，真并行做不到 |
| 每个 turn `process.chdir(cwd)` 切**进程级 cwd** | `persistent-query-service.js:~188` | **致命**：一个进程无法承载多 worktree 并发 → 必须进程池 |
| `abortCurrentTurn()` 操作单一全局 runtime 指针 | `persistent-query-service.js:575` | abort 需改为按 agent 精确中断 |
| 会话注册表 `activeQueryResults: Map<sessionId,…>` + 容量淘汰 | `message-session-registry.js:9` | 多会话状态管理骨架，可复用 |
| 已支持 `resume`、per-request `cwd`、codex provider、子代理 sidechain | `persistent-query-service.js:146` / `daemon.js:448` | resume / 多 provider / 子代理已具备 |

**核心结论**：`process.chdir` 是进程全局 + 请求串行队列 → worktree 隔离的并行 agent **只能进程隔离**（每 worker 一个 daemon 进程）。这与 orca 的 process-per-agent 模型一致。

---

## 4. 核心抽象：Agent = Runtime × Tool × Model

Helm 管理的最小单位不是「会话」，而是 **Agent（可部署的干活单元）**：

```
Agent = Runtime(怎么跑) × Tool(用哪个 CLI/家族) × Model(用哪个模型)

  Runtime:  SdkRuntime(ai-bridge daemon, 结构化流, 首选)
            CliRuntime(起真 CLI 进程, PTY, 用于无好 SDK 的工具)
            任意 agent loop(第三方框架 / 远程 HTTP agent / 自研 loop)
  Tool:     Claude Code / Codex / Gemini CLI / Droid / …
  Model:    Opus / Sonnet / GPT-5-codex / Gemini-Pro / GLM / …（jadekit 现有 provider）
```

### AgentRuntime 契约（介质可插拔的核心）

Hermes 只依赖一个统一契约，任何「会干活的东西」满足它即可接入，Hermes/Coordinator/看板/判活逻辑**一行不改**：

```rust
/// 可插拔的 Agent 执行介质。SDK / CLI / 任意 agent loop 都是它的具体实现。
/// Hermes 编排层只认这个契约，不关心底下是谁。
#[async_trait]
pub trait AgentRuntime: Send + Sync {
    /// 该实现的能力标记，决定 Supervisor 用哪档判活策略。
    fn capabilities(&self) -> RuntimeCapabilities;

    /// 起一个干活实例（绑定 cwd=worktree、model、工具配置）。
    async fn start(&self, spec: RuntimeStartSpec) -> Result<AgentHandle, RuntimeError>;

    /// 派活：把目标/提示发给实例。返回归一化事件流。
    async fn send(&self, handle: &AgentHandle, prompt: String)
        -> Result<UnboundedReceiver<AgentEvent>, RuntimeError>;

    /// 精确中断该实例当前 turn。
    async fn abort(&self, handle: &AgentHandle) -> Result<(), RuntimeError>;

    /// 进程级存活探测（兜底判活）。
    async fn liveness(&self, handle: &AgentHandle) -> Liveness;

    /// 回收实例。
    async fn stop(&self, handle: &AgentHandle) -> Result<(), RuntimeError>;
}

/// 归一化事件：所有 Runtime 都吐这套，Coordinator 不区分介质/provider。
pub enum AgentEvent {
    TextDelta(String),
    Thinking(String),
    ToolUse { id: String, name: String, input: serde_json::Value },
    ToolResult { tool_use_id: String, is_error: bool, content: String },
    Usage(TokenUsage),
    NeedsInput(PermissionRequest), // 工具权限 / ask-user
    Done { success: bool, result: Option<String>, files_modified: Vec<String> },
    Failed { error: String },
}

pub struct RuntimeCapabilities {
    /// true=能给结构化 tool_use/tool_result（判活精准）；
    /// false=只有文本输出 + 进程存活（判活降级）。
    pub structured_events: bool,
    pub supports_resume: bool,
    pub supports_permission_prompt: bool,
}
```

### 内置实现

| 实现 | 是什么 | 状态 | structured_events |
|---|---|---|---|
| `ClaudeSdkRuntime` | ai-bridge → Claude Agent SDK | 已有，包 adapter | true |
| `CodexSdkRuntime` | ai-bridge → Codex SDK | 已有，包 adapter | true |
| `CliRuntime` | 起真 CLI 进程（Gemini CLI / Claude Code CLI / Droid…），PTY | 新增 | false（降级判活） |
| `*` 任意 loop | 第三方框架 / 远程 agent | 加 adapter 即可 | 由实现声明 |

**好处**：① 不被任何厂商/SDK 绑死，换介质 = 写 adapter；② 异构舰队天然成立——同一 DAG 里不同节点可用不同 Runtime，统一调度、统一判活、统一看板展示。

---

## 5. 总体架构

```
前端  Helm Cockpit（三栏面板 + 看板 + 扇出 + Composer + Roster）
        │ Tauri commands / events
─────────────────────────────────────────────────────────
Rust  Hermes Engine（确定性核心）
   ├── Coordinator        单类轮询循环（派发/熔断/收敛）—— 移植 orca coordinator.ts
   ├── Planner（LLM 钩子） 目标→任务 DAG拆解 + 模型/CLI 选兵 + 失败 replan
   ├── Store (SQLite)     Task / Dispatch / Message / Gate / Run（~/.jadekit/hermes.db）
   ├── WorkerSupervisor   每 Agent 判活状态机（lastActivity / tool_use 闭合 / WaitingInput / 进程存活）
   ├── RuntimePool        Map<AgentId, Box<dyn AgentRuntime>>（介质实例池）
   └── WorktreeManager    git worktree add/remove
        │ AgentRuntime 契约（归一化事件）
─────────────────────────────────────────────────────────
介质  SdkRuntime(N×ai-bridge daemon) │ CliRuntime(N×PTY) │ 任意 loop adapter
─────────────────────────────────────────────────────────
军火库 Roster = jadekit 现有 provider/模型管理
```

---

## 6. 核心组件（职责 / 接口 / 依赖）

### 6.1 RuntimePool（含 DaemonPool）
- 职责：管理所有在跑的 Agent 实例，按 `AgentId` 索引。SDK 介质下即 `Map<AgentId, DaemonClient>`（把现有 `manager.rs` 的 `OnceCell` 单例升级为多实例），每个 daemon 独立 cwd + provider + model；去掉 `SESSION_ID` 写死，改 per-agent。
- 接口：`spawn(runtime_kind, start_spec) -> AgentId`、`get(AgentId)`、`abort(AgentId)`、`reap(AgentId)`。
- 依赖：`AgentRuntime` 实现、`DaemonClient`（几乎不改）。
- 为什么必须：见 §3 的 `process.chdir` 进程全局约束。

### 6.2 WorktreeManager
- 职责：`create(repo, name) -> Worktree{path, branch}`、`remove(path)`、`list`、diff 摘要。
- 依赖：复用现有 `chat_git_create_and_checkout_branch`，升级为 `git worktree add`。

### 6.3 WorkerSupervisor（判活，分级降级）
- 职责：每 Agent 维护 `last_activity_at`（任意 `AgentEvent` 刷新）、`open_tool_uses`（已发 ToolUse 未收 ToolResult 的集合）、`status`。reaper 定时扫描。
- 状态：`Running / WaitingInput / Done / Failed / Suspect`。
- 判定（结构化介质，`structured_events=true`）：
  - `Done{success}` → Done；`Done{fail}` 或 `Failed` 或进程退出 → Failed（熔断 +1）。
  - `NeedsInput` → WaitingInput（**永不被超时杀**）。
  - timeout 内有任意 event → Running。
  - timeout 内无 event 且非 WaitingInput 且 `open_tool_uses` 为空且进程仍在 → Suspect → 探活后 abort 重试。
  - **关键坑**：已发 ToolUse 未收 ToolResult = 正常「工具执行中」，不算卡死。
- 判定（裸 CLI 介质，`structured_events=false`，**降级**）：有输出=活着 + 进程存活=没崩 + 每实例硬上限 `max_turn_ms` 兜底。
- 依赖：`AgentEvent` 流、`AgentRuntime::liveness`、现有 `permission_watcher`（SDK 权限信号来源）。

### 6.4 Hermes Store（SQLite，移植 orca 数据模型）
- 职责：编排状态持久化，崩溃可恢复。
- 健壮性对齐 orca：`journal_mode=WAL`、`synchronous=NORMAL`、`busy_timeout`、显式迁移事务、全索引（见 §15 证据）。
- 表见 §7。状态名/消息类型全部 Rust enum 集中定义（遵循 CLAUDE.md「不写魔法字符串」「配置集中」规约）。

### 6.5 Coordinator（确定性循环）+ Planner（LLM 钩子）
- **Coordinator（确定性，移植 orca `coordinator.ts`，单类避免 split-brain）**，每 tick：
  1. 回收 stale dispatch（心跳超时）。
  2. 处理入站消息（worker_done → 标记完成、解锁依赖；escalation / merge_ready）。
  3. 解决 gates。
  4. 派发 ready 任务到空闲 Agent（≤ `maxConcurrent`，不够则新建 Agent）。
  5. 熔断：同任务连续 3 次失败 → `circuit_broken`。
  6. 收敛判定 → 结束。
- **Planner（LLM，仅两个决策点介入）**：
  - `plan(goal, roster) -> Vec<Task>`：拆解目标为任务 DAG，**并为每个任务选兵**（指定 Runtime+Tool+Model，见 §9 路由）。
  - `replan(run, failed_task, result) -> Decision`：失败/完成后决定重试/换兵/上报/收敛。
  - 实现：独立的 Claude SDK 会话（也是一个 Agent）。其余全确定性，LLM 不碰循环/计时/熔断（吸取 orca「168-commit harm」教训）。

### 6.6 Tauri 命令 / 事件（动词对齐 orca CLI，便于未来出 `jadekit` CLI / deep link）
- 命令：`hermes_run(goal, opts)` / `hermes_task_list` / `hermes_dispatch_show` / `hermes_gate_resolve` / `hermes_run_stop` / `helm_fanout(prompt, agents[])` / `helm_merge_winner(agentId)`。
- 事件：`hermes://run`、`hermes://task`、`hermes://agent`（状态）；每个 Agent 会话复用 `chat://stream|done`（带 agentId）。

---

## 7. 数据模型（移植 orca，Rust 化）

```rust
pub enum MessageType { Status, Dispatch, WorkerDone, MergeReady, Escalation, Handoff, DecisionGate, Heartbeat }
pub enum TaskStatus { Pending, Ready, Dispatched, Completed, Failed, Blocked }
pub enum DispatchStatus { Pending, Dispatched, Completed, Failed, CircuitBroken }
pub enum GateStatus { Pending, Resolved, Timeout }
pub enum RunStatus { Idle, Running, Completed, Failed }

pub struct Task {
    pub id: String, pub parent_id: Option<String>,
    pub spec: String, pub status: TaskStatus,
    pub deps: Vec<String>, pub result: Option<String>,
    /// 选兵结果：该任务该派给什么样的 Agent。
    pub assignment: Option<AgentAssignment>,
    pub created_at: String, pub completed_at: Option<String>,
}
pub struct AgentAssignment { pub runtime: RuntimeKind, pub tool: String, pub model: String }

pub struct DispatchContext {
    pub id: String, pub task_id: String, pub assignee: Option<AgentId>,
    pub status: DispatchStatus, pub failure_count: u32,
    pub last_heartbeat_at: Option<String>, /* … */
}
pub struct Message { /* from/to/type/priority/thread_id/payload/sequence/read/created_at */ }
pub struct DecisionGate { pub id: String, pub task_id: String, pub question: String, pub options: Vec<String>, pub resolution: Option<String>, pub status: GateStatus }
pub struct CoordinatorRun { pub id: String, pub goal: String, pub status: RunStatus, pub poll_interval_ms: u64 /* … */ }
```

SQLite 表：`tasks` / `dispatch_contexts` / `messages` / `decision_gates` / `coordinator_runs`，索引对齐 orca（status / parent / task_id / to_handle+read / thread_id）。

---

## 8. 端到端数据流（一次编排）

1. 用户在 Composer 下达目标 → `hermes_run(goal)`。
2. Coordinator 调 Planner：`plan(goal, roster)` → 任务 DAG + 每任务选兵（Runtime+Tool+Model）。
3. 循环取 ready 任务 → WorktreeManager 建 worktree → RuntimePool 按 assignment 起 Agent 实例（SDK 或 CLI，指定 cwd/model）。
4. `AgentRuntime::send(goal+preamble)` → WorkerSupervisor 订阅 `AgentEvent` 流判活。
5. 收到 `Done` → 标记 Task 完成、解锁依赖任务；失败走熔断 → Planner `replan`。
6. 全部收敛 → 汇总（可选 LLM-judge 评判扇出产出）→ 用户在右栏选 worktree 合并/丢弃。

---

## 9. 多模型 / 多 CLI 工作流（Helm 招牌能力）

| 工作流 | 说明 |
|---|---|
| **异构扇出** | 同一任务同时派给 Claude / Codex / Gemini 等不同 Runtime×Model，并排跑 |
| **AI 评判选赢家** | 扇出后用一个 LLM-judge 比对产出 + diff，推荐赢家，人确认合并 |
| **模型分级路由** | Planner 给简单任务选便宜模型、难任务选强模型，自动省钱 |
| **CLI 混编流水线** | DAG 不同节点用不同 CLI（如 Gemini 调研 → Claude 设计 → Codex 实现） |

选兵（路由）是 Planner 的一等 AI 决策，输入 = 任务特征 + Roster（可用 CLI×模型 + 成本/上下文窗口/能力标签），输出 = `AgentAssignment`。用户可覆盖（手动指定）。

---

## 10. UI / UX 设计

### 借鉴 orca 的组件与 UX 原则
- **AgentStateDot 状态词汇**（对齐 orca `AgentStateDot.tsx`）：`working`(转圈) / `idle`(灰) / `active`(翠绿) / `done`(独立色) / `needs-attention`(琥珀，合并 blocked+waiting+permission) / `interrupted`(红)。映射 Supervisor：Running→working、WaitingInput→needs-attention、Done→done、Failed→interrupted、Suspect→闪烁琥珀。
- **Kanban 状态看板**（对齐 `WorkspaceKanban*`）：泳道（待派/执行中/待评审/完成），跨泳道拖拽 = 人工干预，看板状态与 Task 状态**双向同步**。
- **Worktree 卡片 + 动作**（对齐 `WorktreeCardMetadataControls` / `DeleteWorktreeWarningPanels`）：diff 概要（+/-）、状态徽章、备注、合并/丢弃，删除走**预检确认对话框**。
- **Composer + Jump Palette**（对齐 `NewWorkspaceComposerModal` / `WorktreeJumpPalette`）：下达目标起编排；⌘K 快速切 Agent。

### UX 原则
- 状态用颜色点一眼可辨；**严格区分「等待输入」(琥珀) 与「卡死」(红)**（呼应 §6.3 判活）。
- 破坏性操作（删 worktree）必须预检确认。
- 看板状态 = 编排状态的可视镜像，拖拽即干预。
- 每个 Agent 卡显示 `CLI 图标 + 模型徽章`，让异构舰队「谁用什么模型干什么」一目了然。

---

## 11. 错误处理 / 熔断 / 崩溃恢复
- Agent 失败 → `DispatchContext.failure_count += 1`，3 次 `circuit_broken`，Task 标 failed，Coordinator 交 Planner replan 或上报。
- Runtime 实例崩溃 → `liveness` 探测发现 → Failed 走熔断。
- WaitingInput 永不被超时杀；权限请求冒泡到前端（复用 `permission_watcher` / codex 的 approval 检测）。
- 崩溃恢复：对齐 orca `lifecycle-reconciliation`，重启后对账未完成 dispatch / 心跳。

---

## 12. 测试策略
- **Rust**：
  - Coordinator 循环用 mock `AgentRuntime`（复用现有 fake `send_streaming`）做表驱动测试：派发 / 熔断 / 收敛 / 依赖解锁 / 崩溃恢复对账。
  - Store CRUD + 迁移 + 事务不变量。
  - WorkerSupervisor 状态机（含 tool_use 闭合追踪、WaitingInput 区分、CLI 降级判活）。
- **Node（ai-bridge）**：channel 归一化已有测试维持。
- 每条路径覆盖 happy path + 失败/超时/权限三类边。
- 遵循 CLAUDE.md：新增能力必须补测试与中文文档。

---

## 13. 分阶段路线（全量目标，标注 MVP 与灰度）

- **Phase 0 — RuntimePool/DaemonPool（地基，必做，小）**：单例 daemon → 按 agentId 多 daemon，per-agent cwd/abort。✅ 完成后多 tab 已真并行、互不踩 cwd。
- **Phase 1 — Worktree 隔离 + 异构扇出（中）**：worktree per Agent；一 prompt 扇出 N Agent（含跨 CLI×模型）并排对比，选赢家合并。
- **Phase 2 — 编排 MVP（大）← MVP 终点**：`AgentRuntime` 契约 + `CliRuntime` + Task/Dispatch/Coordinator + Planner（拆解 + 选兵，**扁平任务深度 ≤ 2**）+ maxConcurrent + 熔断 + Supervisor 判活。**建议产品层 opt-in / 灰度**（对齐 orca 把编排做成实验开关）。
- **Phase 3 — 全量（大）**：完整 DAG（deps）+ DecisionGate + Message 总线 + escalation/replan + LLM-judge 评判。

每阶段独立可交付。writing-plans 阶段先针对 **Phase 0 + Phase 1** 出实现计划，Phase 2/3 后续单独立计划。

---

## 14. 改动量评估

| 模块 | jadekit 现状 | 改动量 |
|---|---|---|
| SDK 介质（Claude/Codex Runtime） | ✅ ai-bridge 已有 | 包成 adapter，小 |
| 内容渲染 / 权限弹窗 / 三栏骨架 / tabs | ✅ 已有 | 小幅扩展 |
| 多 provider/模型（Roster） | ✅ 本命已有 | 接线 |
| DaemonPool（单例→多进程） | ❌ | 中（Phase 0） |
| AgentRuntime 契约 + CliRuntime | ❌ 无 PTY | 中大（PTY 新写） |
| WorktreeManager | 半（有 branch） | 小中 |
| WorkerSupervisor（判活） | ❌ | 中 |
| Hermes 引擎（SQLite+coordinator） | ❌ | 大（可移植 orca，省很多） |
| Planner（拆解/选兵/replan） | ❌ | 中 |
| UI（看板/状态点/扇出/Roster/Composer） | 半 | 大 |

整体多周工程；Phase 0 小且立刻见效；最大杠杆是 Hermes 引擎照搬 orca 生产级实现。

---

## 15. 风险与权衡
- **内存**：每 SDK Agent = 一个 node + SDK 常驻 → `maxConcurrent` 限流 + 空闲淘汰（复用 registry 淘汰雏形）。
- **Planner 拆解/选兵质量**：给结构化输出约束 + 人工确认 gate；选兵可被用户覆盖。
- **裸 CLI 判活更粗**：靠 `structured_events=false` 降级策略 + 硬超时兜底，不与 SDK 判活混用。
- **并发 worktree 磁盘占用**：限并发 + 完成后清理（删除预检）。

### YAGNI（明确不做）
- 移动端 / relay 远程；orca 的终端 link provider；让 worker 自报 `worker_done` 协议（改由 Supervisor 从事件流推断）；Phase 3 之前不做完整 DAG。

---

## 16. 附：orca 生产级证据 / 借鉴清单

| 生产信号 | 证据（orca 源码） |
|---|---|
| 崩溃恢复 | 独立 `src/main/runtime/orchestration/lifecycle-reconciliation.ts`（#6226 抽出） |
| 数据库健壮 | `db.ts`：WAL + `synchronous=NORMAL` + `busy_timeout=5000` + 显式迁移事务 + 全索引 |
| 并发正确性 | 注释记录「promoteReadyTasks 与 updateTaskStatus 同一 writer 保证事务不变量」 |
| 熔断/止损 | 3 次失败 `circuit_broken`；注释提及「168-commit harm」教训 |
| 测试 | `coordinator.test.ts`(878) + `db.test.ts`(853) ≈ 1700 行 |
| 活跃维护 | git log：dispatch 锁修复(#6317)、Windows payload 修复、Droid/MiMo provider 路由 |

借鉴清单：数据模型 + coordinator 循环 + 崩溃恢复（直接移植）；worktree/进程化 agent/生命周期（GA，照抄最稳）；Kanban/AgentStateDot/Worktree 卡片/Composer/Jump（UI/UX 对齐）。

执行介质改为可插拔 Runtime（本设计核心差异）。
