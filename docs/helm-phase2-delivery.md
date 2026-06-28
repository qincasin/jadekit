# Phase 2 Delivery Report — Hermes 编排引擎

> 执行者 GLM（subagent-driven-development 协调者），交付日期 2026-06-28。
> 主功能分支 `feat/helm`（tip `7f9a061`），未 merge 到 main/develop（等用户指示）。
> 详细技术证据见 `docs/helm-phase2-verification.md`。

## 1. DoD 逐条状态

| DoD 条目 | 状态 | 证据 |
|---|---|---|
| Task 1–18 全部 step 完成、各自 commit | ✅ | 31 commits（a505aad..7f9a061） |
| 6 子阶段 GATE 自检过 + `--no-ff` merge 回 feat/helm | ✅ | 7 个 merge commit（2a ea3359b / 2b 3d14f99 / 2c 865dec3 / 2d b3d2a13 / 2e 2ca7f45 / 2f 19e8603 / 2g d320d9b） |
| `cargo check` 通过 | ✅ | clean（仅 dead-code 警告，Phase 3 wiring 会消费） |
| `cargo test`（hermes + chat 全绿） | ✅ | **345 passed; 0 failed**（单线程权威跑，含 hermes 144 + chat 69） |
| `npm run build` 通过 | ✅ | green（chunk 哈希 index-CMgpqd6T.js / ChatPage-BW28QPaK.js） |
| `git diff --check` 干净 | ✅ | `a505aad..HEAD` exit 0（末尾空行已修，7f9a061） |
| mock AgentRuntime 下端到端跑通（Coordinator+Store+Supervisor+Planner 闭环） | ✅ | `task18_full_mock_e2e_planner_supervisor_store_run` 驱动四件套收敛到 Completed |
| `docs/helm-phase2-verification.md` 已写 | ✅ | 392 行，含真证据 + 手动 e2e 标「待人工执行」 |
| 真 LLM 路径手动 e2e | ⏳ 待人工执行 | 不造假，文档 §C 给出手测步骤 |
| 做完 Task 18 即停 | ✅ | 已停，未 merge 到 main |

**结论：DoD 全部满足（真 LLM e2e 按计划标待人工执行）。**

## 2. 改动清单（新增 `src-tauri/src/hermes/` + 命令层）

| 文件 | 行数 | 职责 |
|---|---|---|
| `hermes/mod.rs` | 31 | 模块入口 + 公共 re-export |
| `hermes/runtime.rs` | 124 | `AgentRuntime` 可插拔契约 + `AgentEvent`/`Liveness`/`RuntimeCapabilities` |
| `hermes/types.rs` | 472 | 编排数据模型（5 状态 enum + RuntimeKind + Task/Dispatch/Message/Gate/Run，as_str/from_str） |
| `hermes/store.rs` | 2019 | SQLite Store（WAL）：schema + Task/Dispatch CRUD + 同事务 promote + 熔断 + Message/Gate/Run + 崩溃恢复对账 |
| `hermes/coordinator.rs` | 2607 | 确定性单类循环 tick（回收 stale/消息/解 gate/派发 ready ≤max_concurrent/熔断/收敛）+ Planner/Supervisor 可选钩子 + run() |
| `hermes/supervisor.rs` | 721 | WorkerSupervisor 判活状态机：结构化档 + 降级档（structured_events 分级，WaitingInput 永不被杀，tool_use 未闭合不判卡死） |
| `hermes/planner.rs` | 1278 | LLM 拆解/选兵/replan：纯函数 prompt+容错解析 + 经 AgentRuntime 驱动临时 planner agent |
| `hermes/sdk_runtime.rs` | 730 | SdkRuntime：把 ChatManager send 路径包成 AgentRuntime（structured_events=true，标签流→AgentEvent） |
| `hermes/cli_runtime.rs` | 882 | CliRuntime：PTY 起裸 CLI（structured_events=false），line→TextDelta、exit→Done、进程组 SIGKILL、echo off、Windows 可移植 |
| `commands/hermes_commands.rs` | 716 | Tauri 命令层：HermesEngine + hermes_run/task_list/dispatch_show/gate_resolve/run_stop + hermes:// 事件 |
| 改动既有 | — | `chat/manager.rs`(+send_raw_stream 加法式)、`chat/mod.rs`(+re-export)、`lib.rs`(+mod hermes + generate_handler! + setup manage HermesEngine)、`Cargo.toml`(+async-trait, portable-pty, libc)、前端 `src/stores/fanoutRollback.ts`(Task 1) |

合计：22 文件，+10408/-27 行，9580 行 hermes 模块。

## 3. git log（子阶段分支与 merge）

```
* 7f9a061 fix(hermes): remove trailing blank line; document worktree-leak Phase-3 gap
*   d320d9b merge: Phase 2g — Tauri wiring + final gate (engine closed end-to-end)
|\
| * bac5318 docs(hermes): Phase 2 verification guide
| * 76e32b0 feat(hermes): supervisor-in-loop closure and full mock end-to-end
| * 35887aa feat(hermes): tauri commands and events for orchestration runs
|/
*   19e8603 merge: Phase 2f — CliRuntime PTY adapter
|\
| * 33a1bc1 feat(hermes): GATE F — heterogeneous medium unified scheduling
| * 850dbad fix(hermes): CliRuntime windows portability and non-locking abort/stop
| * 63d8324 fix(hermes): CliRuntime non-blocking wait, forceful kill, echo off
| * e1f73c5 feat(hermes): CliRuntime PTY adapter for bare CLI agents
|/
*   2ca7f45 merge: Phase 2e — Planner (LLM decompose/roster/replan)
*   b3d2a13 merge: Phase 2d — WorkerSupervisor two-tier liveness
*   865dec3 merge: Phase 2c — Coordinator deterministic loop
*   3d14f99 merge: Phase 2b — Hermes Store (SQLite state machine)
*   ea3359b merge: Phase 2a — AgentRuntime contract + SdkRuntime adapter
* 5d3fa6f fix(chat): roll back created worktrees when fan-out launch fails midway  ← Task 1
```

每个子阶段独立分支 → GATE 绿 → `--no-ff` 合回 feat/helm，集成记录完整保留。

## 4. 验证证据

- **`cargo test`（单线程，权威）**：`345 passed; 0 failed; 0 ignored`（hermes 144 + chat 69 + 其余）。多线程下 `cli_runtime` PTY 用例偶有 line-buffering 竞态（D.7，引擎逻辑零 flaky），单线程稳定全绿。
- **`cargo check`**：clean（32 dead-code 警告，Phase 3 wiring 会消费，非错误）。
- **`npm run build`**：green。
- **`git diff --check a505aad..HEAD`**：exit 0。
- **mock 端到端**：`task18_full_mock_e2e_planner_supervisor_store_run` —— 给一个 goal → Planner(mock) 拆成 3 任务 → Coordinator 并行派发 → mock worker 发 Done → 全 Completed、run Completed。另 `task18_supervisor_reap_silent_worker_aborts_and_fails_dispatch` 证明 Supervisor 在环（静默 worker→Suspect→abort+fail）。
- **真 LLM 手动 e2e**：**待人工执行**（`docs/helm-phase2-verification.md` §C 给步骤），不造假。

## 5. 偏差与未决（详见 verification §D）

**并发/介质相关取舍（均文档化、非阻塞）**：
- D.1 `hermes://task`/`hermes://agent` 事件常量已留位，尚未从 tick 发射（仅 `hermes://run` 发）——Phase 3 接 Coordinator tick 钩子。
- D.2 `hermes_run_stop` 仅 run 前 check（无 mid-loop cancel）——Phase 3。
- D.3 并发熔断触发并发 replan（best-effort，无 single-flight）——Phase 3。
- D.4 单 Coordinator 实例异构 Sdk+Cli = Phase 3（GATE F 已证同代码可驱动两种介质）。
- D.5 个别 `completed_at` 用 SQLite `datetime('now')`（非 RFC3339）——Phase 4 前端接入前统一。
- D.6 Supervisor-reap 分支单测间接覆盖（需 fake clock 才能直测，YAGNI），手测 e2e 兜底。
- D.7 `cli_runtime` PTY 并行负载下偶 flake（隔离稳定）。
- **D.8（新，终审发现）**：converged run 不清扫已建 worktree/分支 —— Phase 3 首项加 end-of-run 清理。

**计划偏差（加法式，已记录）**：Task 3 因 `ChatManager::send` 吞流，加法式新增 public `send_raw_stream`（现有 send 逐字节不变）。

## 6. 是否就绪进 Phase 3

**就绪。** Phase 2 引擎在 mock 介质下端到端闭环、并发不变量引擎级成立、类型契约跨子阶段一致、6 子阶段全 GATE 通过并合回 feat/helm。真 LLM 路径待手测（不阻塞 Phase 3 设计）。建议 Phase 3 优先级：① end-of-run worktree 清理（D.8）；② 完整 DAG/Gate/Message 总线增强 + `hermes://task`/`agent` 事件 + mid-run cancel；③ 真 LLM 手测 e2e；④ 异构单实例 + LLM-judge。
