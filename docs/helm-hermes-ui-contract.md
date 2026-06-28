# Helm × Hermes 引擎↔驾驶舱契约（Phase 4 据此并行开工）

> **状态：冻结（Phase 3a，Task 5）**
> 权威源码：`src-tauri/src/hermes/events.rs`、`src-tauri/src/commands/hermes_commands.rs`、
> `src-tauri/src/hermes/coordinator.rs`、`src-tauri/src/hermes/types.rs`、`src-tauri/src/lib.rs`。
> 本文每一处签名均与上述源码逐字对齐；任何不一致都以源码为准。

## 1. 目的与状态

本文冻结 Phase 3a（Tasks 1–4）落地的「Hermes 编排引擎 ↔ Tauri 前端驾驶舱」契约面：
**命令（驾驶舱 → 引擎）+ 事件（引擎 → 驾驶舱）+ 状态词表**。Phase 4（驾驶舱 UI，由独立
agent Codex 实现）只读本文即可冷启动接线，无需再读引擎源码。

后续 Phase 3 子阶段（3c/3d）会扩展契约（新增命令、扩展状态空间），届时增量更新本文；本
文 §6 列出已知占位项。**非回归不变量**：默认 `NullEventSink`、无 cancel、
`RuntimeRegistry::single`（Phase 3b 引入），引擎行为与 Phase 2 逐字一致——驾驶舱
不监听事件 / 不发命令时，引擎 byte-identical 于 Phase 2（见 §7）。

## 2. 命令（驾驶舱 → 引擎）

权威清单 = `src-tauri/src/lib.rs::generate_handler!` 中注册的 7 个 `#[tauri::command]`
函数（位置：`src-tauri/src/lib.rs:1058–1064`）。**只有这 7 个命令可被 `invoke()` 调用。**

| 命令 | 参数（名: TS 类型） | 返回类型 | 用途 | 状态 |
|------|---------------------|----------|------|------|
| `hermes_run` | `goal: string`, `opts?: HermesRunOpts` | `Promise<string>` (runId) | 启动一次编排 run，立即返回 run_id；进展通过 `hermes://run` / `hermes://task` 事件流推送 | 冻结 |
| `hermes_task_list` | `filter?: TaskListFilterDto` | `Promise<TaskDto[]>` | 列出任务（可按 status / ready 过滤） | 冻结 |
| `hermes_dispatch_show` | `dispatchId: string` | `Promise<DispatchDto>` | 取一条派发上下文（按 dispatch_id 查） | 冻结 |
| `hermes_gate_resolve` | `gateId: string`, `resolution: string` | `Promise<void>` | 解决一个决策门（resolution 写入 Store，status → Resolved） | 冻结 |
| `hermes_run_stop` | `runId: string` | `Promise<void>` | 取消指定 run（置 cancel 标志，spawned 循环下一轮 tick 前退出并把 run 标 Failed）。Phase 3c 会**新增** `hermes_run_cancel`（真正的 mid-run cancel，把 run 置 Cancelled 而非 Failed）——见 §6 | 冻结（语义将在 3c 增补旁路） |
| `hermes_run_show` | `runId: string` | `Promise<RunShowDto>` | 取一条 run 概览 + 任务计数（驾驶舱顶部用） | 冻结（Phase 3a 新增） |
| `hermes_agent_list` | （无） | `Promise<DispatchDto[]>` | 列出当前活跃派发上下文（`status = Dispatched`），驾驶舱 Roster 用 | 冻结（Phase 3a 新增） |

**Tauri State / AppHandle 参数不在 TS 签名中**：`State<'_, HermesEngine>` 与
`AppHandle` 由 Tauri 运行时注入，前端 `invoke` 只传上表所列业务参数。

## 3. DTO 形状

所有 DTO 在 Rust 端均带 `#[serde(rename_all = "camelCase")]`，因此 **JSON 键为 camelCase**、
Rust 字段名为 snake_case。下表「JSON 键」是前端实际看到的字段名。

### 3.1 `TaskDto`（`TaskDto` 来源：`Task → TaskDto::from`）

```rust
// src-tauri/src/commands/hermes_commands.rs:120–146
#[serde(rename_all = "camelCase")]
pub struct TaskDto {
    pub id: String,
    pub parent_id: Option<String>,
    pub spec: String,
    pub status: String,           // 取 TaskStatus::as_str() 的 token，见 §5
    pub deps: Vec<String>,
    pub result: Option<String>,
    pub created_at: String,       // ISO-8601
    pub completed_at: Option<String>,
}
```

| Rust 字段 | JSON 键 | 类型 | 含义 |
|-----------|---------|------|------|
| `id` | `id` | string | 任务 id |
| `parent_id` | `parentId` | string \| null | 父任务 id（子任务用） |
| `spec` | `spec` | string | 任务规约（派发给 agent 的 prompt 主体） |
| `status` | `status` | string | 任务状态 token（§5 Task status） |
| `deps` | `deps` | string[] | 依赖任务 id 列表 |
| `result` | `result` | string \| null | 完成时 agent 回写的结果文本 |
| `created_at` | `createdAt` | string (ISO-8601) | 创建时间 |
| `completed_at` | `completedAt` | string \| null | 完成时间 |

### 3.2 `DispatchDto`（来源：`DispatchContext → DispatchDto::from`）

```rust
// src-tauri/src/commands/hermes_commands.rs:148–178
#[serde(rename_all = "camelCase")]
pub struct DispatchDto {
    pub id: String,
    pub task_id: String,
    pub assignee: Option<String>,
    pub status: String,            // 取 DispatchStatus::as_str() 的 token，见 §5
    pub failure_count: u32,
    pub last_heartbeat_at: Option<String>,
    pub last_failure: Option<String>,
    pub dispatched_at: Option<String>,
    pub completed_at: Option<String>,
    pub created_at: String,
}
```

| Rust 字段 | JSON 键 | 类型 | 含义 |
|-----------|---------|------|------|
| `id` | `id` | string | dispatch id（形如 `disp_<taskId>_<nanos_hex>`） |
| `task_id` | `taskId` | string | 关联任务 id |
| `assignee` | `assignee` | string \| null | 被派发的 agent handle（未发出前为 null） |
| `status` | `status` | string | dispatch 状态 token（§5 Dispatch status） |
| `failure_count` | `failureCount` | number (u32) | 累计失败次数；达 3 触发熔断 `circuit_broken` |
| `last_heartbeat_at` | `lastHeartbeatAt` | string \| null | 最近心跳时间 |
| `last_failure` | `lastFailure` | string \| null | 最近失败原因 |
| `dispatched_at` | `dispatchedAt` | string \| null | 派发时间 |
| `completed_at` | `completedAt` | string \| null | 完成时间 |
| `created_at` | `createdAt` | string (ISO-8601) | 创建时间 |

### 3.3 `RunShowDto`（Phase 3a 新增）

```rust
// src-tauri/src/commands/hermes_commands.rs:261–271
#[serde(rename_all = "camelCase")]
pub struct RunShowDto {
    pub id: String,
    pub goal: String,
    pub status: String,            // 取 RunStatus::as_str() 的 token，见 §5
    pub created_at: String,
    pub completed_at: Option<String>,
    pub task_count: usize,
    pub completed_count: usize,    // 仅数 TaskStatus::Completed
}
```

| Rust 字段 | JSON 键 | 类型 | 含义 |
|-----------|---------|------|------|
| `id` | `id` | string | run id（形如 `run_<nanos_hex>`） |
| `goal` | `goal` | string | 本次 run 的目标（`hermes_run` 入参） |
| `status` | `status` | string | run 状态 token（§5 Run status） |
| `created_at` | `createdAt` | string (ISO-8601) | 创建时间 |
| `completed_at` | `completedAt` | string \| null | 完成时间 |
| `task_count` | `taskCount` | number | 任务总数 |
| `completed_count` | `completedCount` | number | 已 `Completed` 任务数（不含 Failed/Blocked） |

### 3.4 `HermesRunOpts`（`hermes_run` 的可选第二参数）

```rust
// src-tauri/src/commands/hermes_commands.rs:54–63
#[serde(rename_all = "camelCase")]
pub struct HermesRunOpts {
    pub max_concurrent: Option<usize>,
    pub poll_interval_ms: Option<u64>,
    pub repo_root: Option<String>,
}
```

| Rust 字段 | JSON 键 | 类型 | 缺省值 | 含义 |
|-----------|---------|------|--------|------|
| `max_concurrent` | `maxConcurrent` | number \| null | `DEFAULT_MAX_CONCURRENT = 4` | 并发上限；必须 > 0，否则命令返回 Err |
| `poll_interval_ms` | `pollIntervalMs` | number \| null | `DEFAULT_POLL_INTERVAL_MS = 2000` | Coordinator poll 间隔（毫秒）；必须 > 0 |
| `repo_root` | `repoRoot` | string \| null | `HermesEngine.repo_root`（setup 注入，缺省 cwd） | 本次 run 工作目录；空/纯空白串视为 null |

### 3.5 `TaskListFilterDto`（`hermes_task_list` 的可选参数）

```rust
// src-tauri/src/commands/hermes_commands.rs:181–188
#[serde(rename_all = "camelCase")]
pub struct TaskListFilterDto {
    pub status: Option<String>,
    pub ready: Option<bool>,
}
```

| Rust 字段 | JSON 键 | 类型 | 含义 |
|-----------|---------|------|------|
| `status` | `status` | string \| null | 按 Task status token 过滤（与 `ready=true` 互斥；`ready=true` 优先） |
| `ready` | `ready` | boolean \| null | 仅返回 Ready 任务（Coordinator 派发循环用）；缺省 false |

`status` 必须是 §5 中合法的 Task status token，否则命令返回 Err
（`TaskStatus::from_str` 拒绝未知 token）。

## 4. 事件（引擎 → 驾驶舱）

### 4.1 三个通道（常量）

```rust
// src-tauri/src/commands/hermes_commands.rs:32–40
pub const HERMES_EVENT_RUN: &str   = "hermes://run";
pub const HERMES_EVENT_TASK: &str  = "hermes://task";
pub const HERMES_EVENT_AGENT: &str = "hermes://agent";
```

前端用 `app.listen("hermes://run", ...)` 等订阅；监听失败 / 无监听者不影响引擎循环
（`TauriEventSink` 是 best-effort）。

### 4.2 统一 payload：`OrchestrationEvent`（判别联合）

`OrchestrationEvent` 由 `#[serde(tag = "kind", rename_all = "camelCase")]` 标注，
**所有变体共享 `kind` 判别字段**，TS 侧可直接 `JSON.parse` 成判别联合
（discriminated union）。变体内部字段为 camelCase。

```rust
// src-tauri/src/hermes/events.rs:21–49
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OrchestrationEvent {
    #[serde(rename_all = "camelCase")]
    Run   { run_id: String, goal: String, status: String, error: Option<String> },
    #[serde(rename_all = "camelCase")]
    Task  { run_id: String, task_id: String, status: String, dispatch_id: Option<String> },
    #[serde(rename_all = "camelCase")]
    Agent { run_id: String, agent_id: String, task_id: Option<String>,
            status: String, activity: Option<String> },
}
```

| 变体 | `kind` 值 | 字段（JSON 键） |
|------|-----------|-----------------|
| `Run` | `"run"` | `runId: string`、`goal: string`、`status: string`（Run status token，§5）、`error: string \| null` |
| `Task` | `"task"` | `runId: string`、`taskId: string`、`status: string`（Task status token，§5）、`dispatchId: string \| null` |
| `Agent` | `"agent"` | `runId: string`、`agentId: string`、`taskId: string \| null`、`status: string`（Agent status token，§5）、`activity: string \| null`（Agent activity token，§5；仅 `status="working"` 时非空） |

**TypeScript 判别联合示例**（供 Phase 4 直接复制）：

```ts
type OrchestrationEvent =
  | { kind: "run";   runId: string; goal: string; status: string; error: string | null }
  | { kind: "task";  runId: string; taskId: string; status: string; dispatchId: string | null }
  | { kind: "agent"; runId: string; agentId: string; taskId: string | null;
      status: string; activity: string | null };
```

### 4.3 通道映射规则（`event_channel_for`）

`event_channel_for(&ev)` 是纯函数，**只看枚举变体（不看 payload）**映射到通道
（`src-tauri/src/commands/hermes_commands.rs:237–243`）：

| `OrchestrationEvent` 变体 | 通道 |
|---------------------------|------|
| `Run { .. }`   | `hermes://run`   |
| `Task { .. }`  | `hermes://task`  |
| `Agent { .. }` | `hermes://agent` |

### 4.4 发射时机（驾驶舱据此安排 UI 刷新）

| 变体 / 字段值 | 何时发射 | 发射点 |
|---------------|----------|--------|
| `Run { status: "running" }` | run 启动（`start_run` 写完 Store、spawn 循环前） | `hermes_commands.rs::HermesEngine::start_run`（命令层 sink） |
| `Run { status: "failed", error: Some(_) }` | spawned 循环中 `coordinator.run` 返回 Err | spawned task 内（`start_run` 的 closure） |
| `Run { status: <final> }` | spawned 循环退出前（`final_status` = `Completed` / `Failed`） | spawned task 内 |
| `Task { status: "dispatched" }` | 派发成功（`dispatch_one` 走完 `runtime.send`） | `Coordinator::dispatch_one` |
| `Task { status: "completed" }` | watcher 收到 `AgentEvent::Done{success:true}` → 写 `worker_done` | watcher（`dispatch_one` 内 spawn） |
| `Task { status: "failed" }` | dispatch 熔断（3 次累计 → `CircuitBroken`，task 标 Failed） | `fail_dispatch_with_cascade`（watcher / stale-reap / supervisor-reap 共用） |
| `Agent { status: "working", activity: "tool_use" | "text" | "thinking" }` | watcher 每收到一个 `TextDelta` / `Thinking` / `ToolUse` 事件 | `emit_agent_event`（watcher 内） |
| `Agent { status: "needs-attention" }` | watcher 收到 `NeedsInput`（等待用户 / 工具权限） | `emit_agent_event` |
| `Agent { status: "done" }` | watcher 收到 `Done` / `Failed`（agent 已停；失败语义由 `Task{failed}` 承载） | `emit_agent_event` |
| `Agent { status: "interrupted" }` | supervisor 标 Suspect → `runtime.abort` + `fail_dispatch` | `Coordinator::reap_silent_workers` |

> 注：`Agent{status:"done"}` 不重复表达失败——失败语义统一由 `Task{failed}` 承载，
> `Agent{done}` 只表示「agent 不再工作」（双通道分离语义，避免 UI 重复标红）。

## 5. 状态词表

所有 token 均来自源码常量 / `as_str()`，**禁止散落魔法串**。

### 5.1 Agent status（事件 `Agent.status` 字段）

来源：`src-tauri/src/hermes/coordinator.rs:64–72`。

| 常量名 | token | 含义 |
|--------|-------|------|
| `AGENT_STATUS_WORKING` | `"working"` | 有 stream 活动（TextDelta / Thinking / ToolUse）；`activity` 字段非空 |
| `AGENT_STATUS_NEEDS_ATTENTION` | `"needs-attention"` | `NeedsInput` / 权限请求：等待用户或工具授权，需人工介入 |
| `AGENT_STATUS_DONE` | `"done"` | `Done` / `Failed`：agent 已停（失败语义由 `Task{failed}` 承载） |
| `AGENT_STATUS_INTERRUPTED` | `"interrupted"` | reap / abort 强杀（supervisor 标 Suspect 后被 Coordinator 终止） |

### 5.2 Agent activity（事件 `Agent.activity` 字段；仅 `status="working"` 时非空）

来源：`src-tauri/src/hermes/coordinator.rs:74–76`。

| 常量名 | token | 触发事件 |
|--------|-------|----------|
| `AGENT_ACTIVITY_TOOL_USE` | `"tool_use"` | `AgentEvent::ToolUse { .. }` |
| `AGENT_ACTIVITY_TEXT` | `"text"` | `AgentEvent::TextDelta(_)` |
| `AGENT_ACTIVITY_THINKING` | `"thinking"` | `AgentEvent::Thinking(_)` |

> `AgentEvent::ToolResult` / `NeedsInput` / `Done` / `Failed` 不映射到 activity：
> `ToolResult` 不发事件（闭合事件，非新活动）；后三者直接决定 status 字段。

### 5.3 Task status（DTO `TaskDto.status` / 事件 `Task.status` 字段）

来源：`src-tauri/src/hermes/types.rs:84–116` `TaskStatus::as_str()`。

| 枚举变体 | token (`as_str()`) | 含义 |
|----------|-------------------|------|
| `Pending` | `"pending"` | 尚有未完成的依赖 |
| `Ready` | `"ready"` | 依赖全部 `Completed`，等待派发 |
| `Dispatched` | `"dispatched"` | 已派发给 Agent |
| `Completed` | `"completed"` | 成功完成（触发下游 `Pending → Ready`） |
| `Failed` | `"failed"` | 失败 / 熔断 |
| `Blocked` | `"blocked"` | 被决策门阻塞，等待外部回答 |

### 5.4 Dispatch status（DTO `DispatchDto.status` 字段）

来源：`src-tauri/src/hermes/types.rs:127–156` `DispatchStatus::as_str()`。

| 枚举变体 | token (`as_str()`) | 含义 |
|----------|-------------------|------|
| `Pending` | `"pending"` | 已建上下文但尚未发出 |
| `Dispatched` | `"dispatched"` | Agent 正在工作（`hermes_agent_list` 仅返回此状态） |
| `Completed` | `"completed"` | 完成（任务侧同步 `Completed`） |
| `Failed` | `"failed"` | 本次失败（任务侧退回 `Ready` 等待重派） |
| `CircuitBroken` | `"circuit_broken"` | 熔断——累计 3 次失败，任务侧直接 `Failed` |

### 5.5 Run status（DTO `RunShowDto.status` / 事件 `Run.status` 字段）

来源：`src-tauri/src/hermes/types.rs:195–222` `RunStatus::as_str()`。

| 枚举变体 | token (`as_str()`) | 含义 |
|----------|-------------------|------|
| `Idle` | `"idle"` | 尚未启动（建表默认，理论上 run 一旦 `start_run` 就进入 `Running`） |
| `Running` | `"running"` | 调度循环中 |
| `Completed` | `"completed"` | 达成目标、正常结束 |
| `Failed` | `"failed"` | 异常终止（含被 `hermes_run_stop` 取消） |

> **Phase 3c 预留**：`cancelled` token 尚未实现（当前 `hermes_run_stop` 把 run 置
> `Failed`）。Phase 3c 引入 `hermes_run_cancel` 时会在 `RunStatus` 增补 `Cancelled → "cancelled"`，
> 前端 UI 应**预留**该取值的展示分支。

## 6. Phase 3 扩展占位（Phase 4 据 UI 预留）

下列命令 / token 在 Phase 3a **尚未实现**，Phase 4 可据此预渲染占位 UI。

| 占位项 | 引入子阶段 | 说明 |
|--------|-----------|------|
| `hermes_run_cancel(runId)` | Phase 3c | 真正的 mid-run cancel：把 run 置 `RunStatus::Cancelled`（token `"cancelled"`，新增枚举变体）。与现有 `hermes_run_stop`（置 Failed）共存或替换——以 3c 实现为准。UI 预留"已取消"态。 |
| worktree 合并/丢弃命令（`hermes_run_cleanup`） | Phase 3d | 清理 run 产出的 worktrees。**注**：当前每任务一个独立 git worktree（分支前缀 `helm/<task_id>`），失败任务在 dispatch 前 `cleanup_prior_worktree` 重派，成功产物的 merge / discard **per-task 决策**留到 Phase 4 UI（基于 `RetainForReview` disposition）。 |
| 人工干预命令族 | Phase 4（与引擎共建） | 围绕 `DecisionGate`（`gate_id` / `question` / `options` / `resolution` / `GateStatus`）的查询 + 解决命令；`hermes_gate_resolve` 已落地（冻结），列表 / 详情命令在 Phase 4 与引擎协同补齐。 |
| Agent status `idle` / `active` | Phase 3 后续 | 当前 4 个 status token（§5.1）覆盖驾驶舱 AgentStateDot 主路径；后续若引入排队 / 后台区分，会增补 `idle`（待派发）/ `active`（运行中，working 的超集）token。 |

## 7. 非回归保证（关键不变量）

Phase 3a 的所有改动均以「**不注入即零成本**」为设计原则，引擎行为与 Phase 2 逐字一致：

1. **默认 `NullEventSink`**：`Coordinator::new` 不调 `with_event_sink` 时，
   `event_sink` 字段是 `Arc::new(NullEventSink)`，`emit` 立即返回、不分配、不持有状态
   （`src-tauri/src/hermes/events.rs:73–77`、`coordinator.rs:174`）。
2. **无 cancel**：`hermes_run_stop` 仅在用户显式 `invoke` 时触发；不调用时 spawned 循环
   按 `RUN_MAX_ITERATIONS`（=1000）安全阀 + 收敛判定自然退出。
3. **`RuntimeRegistry::single`（Phase 3b 将引入）**：当前 Coordinator 持有单个
   `Arc<dyn AgentRuntime>`；Phase 3b 升级为 `RuntimeKind → Arc<dyn AgentRuntime>` 注册表后，
   `single` 路径等价于 Phase 2 的单介质行为（关键非回归保证）。
4. **命令层薄**：`hermes_commands.rs` 只做参数归一化（纯函数 `normalize_run_opts` /
   `normalize_run_goal` / `parse_task_list_filter`，均有单测覆盖）+ 薄 delegate 到
   `HermesEngine`，业务逻辑全部在 `crate::hermes` 引擎层。

Phase 2 已通过的全部引擎测试（coordinator / planner / supervisor / events / commands）
在不改一行代码的前提下应继续保持 byte-identical 行为——这是 Phase 4 可独立迭代的前提。
