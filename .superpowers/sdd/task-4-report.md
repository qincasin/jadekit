# Task 4 报告 — TauriEventSink + run/agent 读命令 + RFC3339 时间戳（D.5）

## 实现摘要

Task 4 把 Hermes 引擎的事件出口从命令层直接 `app.emit(RunEventPayload)` 升级为：
- **生产 sink**（`TauriEventSink`）—— 引擎只依赖 `OrchestrationEventSink` trait，
  本实现把 `OrchestrationEvent` 按枚举变体映射到 `hermes://run` / `hermes://task` /
  `hermes://agent` 通道，best-effort 发射。
- **两条新读命令**：`hermes_run_show`（驾驶舱顶部 run 概览）、`hermes_agent_list`
  （驾驶舱 Roster 活跃 dispatch）。
- **SQLite `datetime('now')` → chrono RFC3339**：runs / gates / dispatch 表中所有
  runtime 写入的时间戳列改为绑定 chrono RFC3339 字符串，与 `create_dispatch` /
  `update_task_status` 等已有路径格式统一（D.5）。

## 改动文件

| 文件 | 性质 | 关键改动 |
|------|------|----------|
| `src-tauri/src/commands/hermes_commands.rs` | 改 | 新增 `TauriEventSink` + `event_channel_for` + `RunShowDto` + `build_run_show` + `HermesEngine::show_run` / `list_active_agents` + 两条 `#[tauri::command]`；`start_run` 重构为经 sink 单一发射 run 事件 + `.with_event_sink(...)` 注入 Coordinator；删除 `RunEventPayload` / `TaskEventPayload`；去掉 `HERMES_EVENT_TASK` / `HERMES_EVENT_AGENT` 上的 `#[allow(dead_code)]`。 |
| `src-tauri/src/hermes/store.rs` | 改 | 新增 `Store::get_run(run_id)`（任意状态，按 id 精确取）；`update_run` / `resolve_gate` / `reconcile_in_tx` 的 dispatch-completed 分支 / `create_run` 全部改为绑定 `chrono::Utc::now().to_rfc3339()`；`create_run` 同时把 `created_at` 加入 INSERT 列并复用同一 `now`（此前 DB 行走 schema DEFAULT、struct 走 chrono——格式不一致，现统一）。 |
| `src-tauri/src/lib.rs` | 改 | `generate_handler!` 注册 `hermes_commands::hermes_run_show` / `hermes_commands::hermes_agent_list`。 |

## TDD 流程（RED → GREEN）

### RED
先写 4 类失败测试（`event_channel_for` / `build_run_show` / `get_run` / RFC3339 校验），
`cargo check --tests` 之前函数不存在 → 编译失败（RED）。

### GREEN
按 brief 实现：
1. `event_channel_for(&ev)` —— 三类枚举变体映射到三个常量。
2. `build_run_show(run, tasks)` —— `task_count = tasks.len()`，
   `completed_count = tasks.iter().filter(|t| t.status == Completed).count()`。
3. `HermesEngine::show_run` —— `store.get_run()?` + `store.list_tasks(default)?` + `build_run_show`。
4. `HermesEngine::list_active_agents` —— 薄 delegate 到 `store.list_active_dispatches()`。
5. `Store::get_run(run_id)` —— 镜像 `get_active_run` 的 SQL，改 `WHERE id = ?1`。
6. 4 处 `datetime('now')` 替换为 chrono RFC3339 绑定。

### 测试结果
```
$ cargo test --manifest-path src-tauri/Cargo.toml --lib -- hermes --test-threads=1
test result: ok. 160 passed; 0 failed; 0 ignored; 0 measured; 201 filtered out; finished in 24.14s
```

新增 9 个用例全部通过（其余 151 个既有用例无回归）：

- `commands::hermes_commands::tests::event_channel_for_maps_each_kind`
- `commands::hermes_commands::tests::build_run_show_counts_tasks`
- `commands::hermes_commands::tests::build_run_show_handles_empty_task_list`
- `commands::hermes_commands::tests::engine_show_run_returns_dto_with_counts`
- `commands::hermes_commands::tests::engine_show_run_rejects_missing_and_empty`
- `commands::hermes_commands::tests::engine_list_active_agents_empty_when_no_dispatches`
- `hermes::store::tests::get_run_returns_any_status`
- `hermes::store::tests::create_run_struct_and_db_row_agree_on_created_at`
- `hermes::store::tests::resolve_gate_writes_rfc3339_resolved_at`

### 其它校验
- `cargo check`（非 test）：clean（只有仓库既存的 dead_code 警告）。
- `cargo check --tests`：clean（只新增代码无警告）。
- `npm run build`：green（`✓ built in 1.69s`；chunk-size 警告是仓库既存，与本次改动无关）。
- `git diff --check`：clean（无尾空白 / 冲突标记）。

## 设计要点 & 自评

### `start_run` 的 sink 所有权
brief 担心 spawned closure 拿不到 sink——实际模式很直接：
- 在 spawn **之前** `let sink = Arc::new(TauriEventSink::new(app.clone(), run.id.clone()));`
- 同步 emit run-start（`sink.emit(...)`，无需 move）。
- spawn 时 `let sink_for_task = sink.clone();` 后 move 进 closure。
- closure 内 `.with_event_sink(sink_for_task.clone() as Arc<dyn OrchestrationEventSink>)`
  注入 Coordinator；错误路径 / 终态 emit 都用 `sink_for_task.emit(...)`。

`Arc<TauriEventSink>` 调 `emit` 需要 `OrchestrationEventSink` trait 在 scope ——
`use crate::hermes::{... OrchestrationEventSink ...}` 解决（初次编译报 E0599 后修复）。

### `RunEventPayload` / `TaskEventPayload` 删除安全性
全仓库 grep 确认无外部引用；前端 Phase 4 才会消费这些事件，DTO 形状
`OrchestrationEvent::Run` 的字段（`runId/goal/status/error`）是 `RunEventPayload`
字段的超集（camelCase 经 serde 自动），消费侧迁移零成本。

### RFC3339 格式校准
`chrono::Utc::now().to_rfc3339()` 实际产出形如 `2026-06-28T08:22:05.632476+00:00`
（含 `+00:00` 而非 `Z`）。第一版测试用 `ends_with('Z')` 断言 → RED。修复为
`is_rfc3339_chrono(s)` 同时接受 `Z` 与 `+HH:MM` 时区后缀。生产代码行为本身正确，
只是测试断言过严。

### 未触及的边界（遵循 brief 约束）
- 没改 `coordinator.rs` / `events.rs` / `supervisor.rs`（Task 1–3 的契约保持不变）。
- 没改 `start_run` 的 cancel→Failed 语义（Task 11 才引入 Cancelled）。
- schema `DEFAULT (datetime('now'))` 子句保留（无法用 Rust 值替换列默认；且现在所有
  runtime 写入都显式绑定 RFC3339，DEFAULT 仅作 fallback）。
- 没改 messages / tasks 的 `created_at`（调用方已传 RFC3339）。

## 遗留关注点

无功能性遗留。两点轻微说明：
1. `start_run` 内的 `app` 参数在重构后只用于构造 sink（`app.clone()`），其它路径
   都走 sink —— 这正是解耦目的。若后续 Phase 4 需要在命令层直接用 `app`（如打开
   新窗口），可保留参数不变。
2. `TauriEventSink::run_id()` 标了 `#[allow(dead_code)]` —— 字段当前只用于构造
   诊断，未被生产代码读取；保留它是为将来日志 / tracing 接入留接口。

## 提交

```
feat(hermes): tauri event sink, run/agent read commands, rfc3339 timestamps
```
