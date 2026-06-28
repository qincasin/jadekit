# Helm × Hermes Phase 3 验证

> 执行者 GLM（subagent-driven-development 协调者），验证日期 2026-06-28。
> 主功能分支 `feat/helm`（tip `0cb7c9c`，Phase 3a–3f 已 `--no-ff` 合回）；本文在 `feat/helm-phase3g-verify` 上完成。
> 详细进度见 `.superpowers/sdd/progress.md`；Phase 2 交付见 `docs/helm-phase2-delivery.md`。

## 范围

Phase 3 在 Phase 2 引擎（确定性 Coordinator 循环 + SQLite DAG Store + LLM Planner + 双层
WorkerSupervisor + 可插拔 AgentRuntime）之上收尾 5 项引擎能力，并冻结驾驶舱契约。子阶段
顺序固定 3a→3b→3c→3d→3e→3f→3g，全部 `--no-ff` 合回 `feat/helm`：

| 子阶段 | 分支后缀 | 内容 |
|--------|----------|------|
| 3a — 契约 | `phase3a-contract` | OrchestrationEventSink 契约 + run/task/agent 事件 + TauriEventSink + RFC3339 时间戳 + 冻结 UI 契约 |
| 3b — 异构路由 | `phase3b-registry` | RuntimeRegistry + Coordinator 按 `assignment.runtime` 派发 + Supervisor 按介质查活 + SDK/Cli 双介质登记 + 混跑 e2e |
| 3c — 取消 | `phase3c-cancel` | `RunStatus::Cancelled` + cancel 信号 + `run()` 循环顶检查 + `hermes_run_cancel` 命令 |
| 3d — 清扫 | `phase3d-cleanup` | `decide_disposition` 纯逻辑 + `has_commits_ahead` + `sweep_run_worktrees`（删除前双保险复查 + fail-safe）+ 收敛/取消/超时三终态点自动清扫 + `hermes_run_cleanup` 命令 |
| 3e — 单飞 | `phase3e-singleflight` | per-run 单飞 replan（`tokio::Mutex<HashSet<run_id>>`）+ 不跨 await 持锁 + guard 单测 |
| 3f — 判官 | `phase3f-judge` | `JudgeVerdict`/`JudgeCandidate` + `build_judge_prompt` + `parse_judge_response`（容错 + 确定性 fallback）+ `Planner::judge` 镜像 plan |
| 3g — 验证 | `phase3g-verify` | 本文 |

---

## A. 自动门（Automated Gates）— 真实捕获的尾部输出

所有输出为本次在 `feat/helm-phase3g-verify`（基于 `feat/helm @ 0cb7c9c`）上实际运行后捕获，
逐字摘自 `.superpowers/sdd/evidence/` 下的证据文件，**非伪造**。

### A.1 `cargo test` 全量（单线程）

命令：`cargo test --manifest-path src-tauri/Cargo.toml -- --test-threads=1`

本次跑结果：**404 passed / 0 failed**（单线程权威跑）。证据尾部逐字：

```
test result: ok. 404 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 27.35s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

**如实记录（不回避）：** `hermes::cli_runtime::tests` 下的三个 PTY-echo 测试——
`happy_path_echo_then_done_success`（`cli_runtime.rs:600`）、
`multi_line_output_yields_two_text_deltas`（`:655`）、
`non_zero_exit_yields_done_failure`（`:630`）——
在全套件负载下**单线程也会偶发 flaky**（详见 D.7）。这是 Phase 2 既有的测试基建问题，
**非 Phase 3 回归**——Phase 3 全程未修改 `cli_runtime.rs`；它们隔离单跑全绿（见下）。

因此本文**不声称「永远 404/0」**。本次跑 404/0 是真实的；已知偶发 flake 如实记录。权威的
非回归信号来自两段确定性证据：

**确定性子集**（`--skip cli_runtime`，跳过 PTY-echo 测试）：**393 passed / 0 failed**，证据尾部逐字：

```
test result: ok. 393 passed; 0 failed; 0 ignored; 0 measured; 11 filtered out; finished in 6.39s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

**cli_runtime 隔离单跑**：**8 passed / 0 failed**，证据尾部逐字：

```
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 396 filtered out; finished in 0.59s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s
```

确定性子集 393/0（引擎逻辑：Coordinator / Store / Supervisor / Planner / SdkRuntime /
run_lifecycle / 命令层零回归）+ cli_runtime 隔离 8/0（PTY 适配器隔离全绿）= 本次 Phase 3
的权威非回归信号。

### A.2 `cargo check`

命令：`cargo check --manifest-path src-tauri/Cargo.toml`。结果：**clean（无 error）**，
26 个既有 warning（未用 import / Phase 2 `#[allow(dead_code)]` 透传残留，无新增 error）。
证据尾部逐字：

```
warning: `jadekit` (lib) generated 26 warnings (run `cargo fix --lib -p jadekit` to apply 13 suggestions)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.32s
```

### A.3 `npm run build`

结果：**green**（前端无回归）。证据尾部逐字：

```
- Adjust chunk size limit for this warning via build.chunkSizeWarningLimit.
✓ built in 1.69s
```

（chunk-size warning 为既有提示，非 Phase 3 引入。）

### A.4 `git diff --check`

结果：**clean（exit 0）**。证据逐字：

```
exit=0 (empty=clean)
```

---

## B. Mock 端到端（DoD 不变量，每个带真实测试名 + 一句话证明什么）

DoD 由 4 条核心 mock e2e 测试驱动，捕获的 `... ok` 证据逐字摘自
`.superpowers/sdd/evidence/mock-e2e-names.txt`：

```
test hermes::coordinator::tests::concurrent_replans_for_same_run_are_single_flight ... ok
test hermes::coordinator::tests::mid_run_cancel_aborts_inflight_and_marks_cancelled ... ok
test hermes::coordinator::tests::mixed_run_dispatches_sdk_and_cli_tasks_through_respective_runtimes ... ok
test hermes::coordinator::tests::run_sweeps_worktrees_on_convergence ... ok
```

外加各子任务落地的支撑单测（路由 / 活性 / store CHECK / 清扫安全 / 单飞 guard），均已在源码
中核对存在（位置见下）。

### B.1 异构混跑（3b / Task 9）— 异构介质按 `assignment.runtime` 路由并收敛

- **`mixed_run_dispatches_sdk_and_cli_tasks_through_respective_runtimes`**
  （`coordinator.rs:3775`，捕获 `ok`）：3 个任务（Sdk / Cli / Sdk）经各自 RuntimeKind 路由
  派发，全部 `Completed`，run 收敛。证明一个 Coordinator 持 `RuntimeRegistry` 即可同一次
  run 内混合 Sdk + Cli 介质。
- 支撑：`dispatch_routes_by_assignment_runtime_kind`（`coordinator.rs:3686`，Task 7）——
  断言 `dispatch_one` 按 `assignment.runtime` 选 RuntimeKind 的路由单元；
  `reap_probes_liveness_via_per_agent_runtime_kind`（`supervisor.rs:755`，Task 8）——
  断言 `WorkerSupervisor` 持 registry、`reap` 两阶段按 agent 介质查活（`registry.get(kind)`）。

### B.2 mid-run 取消（3c / Task 10/11）— 中断在飞派发 + 标 Cancelled + 发事件

- **`mid_run_cancel_aborts_inflight_and_marks_cancelled`**
  （`coordinator.rs:3948`，捕获 `ok`）：`HangingRuntime` 让一个 dispatch 停在 in-flight；
  置 cancel 信号 → `run()` 循环顶检查命中 → 中断在飞 + 标 `RunStatus::Cancelled` + 发
  `Run{cancelled}` 事件。
- 支撑：`update_run_accepts_cancelled_status`（`store.rs:2134`，Task 10）——证明 store 的
  `coordinator_runs.status` CHECK 放行 `'cancelled'`（schema CHECK 已扩为含 cancelled）。

### B.3 收敛清扫安全（3d / Task 13/14）— 删除前双保险复查 + fail-safe，绝不删未提交工作

- **`run_sweeps_worktrees_on_convergence`**（`coordinator.rs:2002`，Task 14，捕获 `ok`）：
  run 收敛后自动清扫，干净 worktree 在 converge 时被移除。
- `sweep_run_worktrees_removes_clean_and_retains_with_commits`（`run_lifecycle.rs:333`，
  Task 13）——干净 Failed → `Remove`；有领先提交的 Completed → `RetainForReview`。
- `sweep_downgrades_failed_dirty_worktree_to_retain`（`run_lifecycle.rs:383`，Task 13）——
  dirty Failed → 降级为 `RetainForReview`（不删）。
- `sweep_retains_when_uncommitted_check_errors_fail_safe`（`run_lifecycle.rs:419`，
  Task 13 fix）——删除前双保险复查 `has_uncommitted_changes`，git 命令出错时 fail-safe
  保留（`.unwrap_or(true)`），**不确定时绝不删**。

**破坏性安全红线：** 任何 `RetainForReview` / `Remove` 决定都经过 `has_uncommitted_changes`
二次复查；git 命令失败（不确定）一律保留，永不删除未提交工作。

### B.4 单飞 replan（3e / Task 15）— 同一 run 并发熔断只 replan 一次

- **`concurrent_replans_for_same_run_are_single_flight`**
  （`coordinator.rs:2556`，Task 15，捕获 `ok`）：`YieldingMockRuntime` 强制真实交错下，对同一
  run 的两次并发熔断 → `planner.replan` 只被调用一次（第一次插入 inflight set 后释放锁→await；
  第二次命中单飞 guard 短路）。

---

## C. 真 LLM 手动 e2e — 待人工执行（未在本次自动化中跑，不造假）

> **状态：待人工执行。** 本次自动化未跑真 LLM e2e；以下为推荐手测步骤，由人工执行后填入观测。
> **未跑即不标完成。**

### C.1 前置

- Jadekit（Claude 页）配置并切换激活一个真实 Claude API token（写入 `~/.claude/settings.json`）。
- PATH 上有 `claude` CLI 二进制——生产 `CliRuntime::new(["claude"])` 的默认命令
  （Task 9 注明此默认值待真 e2e 校验/调整）。
- 一个至少有一次提交的真实 Git 仓库（Hermes 每个 dispatched task 建一个 worktree）。
- 从 `feat/helm` HEAD 构建的应用（`npm run tauri dev` 或 `cargo tauri dev`）。

### C.2 步骤

1. `npm run tauri dev`（或 `cargo tauri dev`）启动应用。
2. 在驾驶舱（或经 `invoke`）调用 `hermes_run(goal)`，goal 能拆成异构任务（一个 task
   assignment Sdk/claude、一个 Cli/codex）。例：
   ```js
   await window.__TAURI__.core.invoke("hermes_run", {
     goal: "<可拆解为异构任务的真实目标>",
     opts: { maxConcurrent: 2 },
   });
   ```
3. 观察并行派发：每个 task 按其 `assignment.runtime` 路由到对应 runtime
   （`hermes://task` + `hermes://agent` 事件），各自独立 worktree。
4. 收敛时调 `hermes_run_cleanup`：只有 `RetainForReview`（有提交的）worktree 保留，干净的
   被移除。用 `hermes_run_show` / 文件系统核对。
5. mid-run 取消：起一个长 run，中途 `hermes_run_cancel(runId)` → run=`Cancelled` + 在飞被
   中断（`hermes://run{cancelled}` 事件）。
6. （可选）跨 run 复跑：先停后起验证 worktree 隔离与清扫幂等。

### C.3 跑完记录

- runId、goal、Planner 拆出的 task 数、收敛耗时。
- 各 task 的 runtime kind（Sdk/Cli）、各自 worktree 路径。
- cleanup 后保留 / 移除的 worktree 清单。
- `hermes://run` 终态事件与 store run 行一致。

---

## D. 已知未决（延后到 Phase 4 / 后续）

以下为有意延后项，非 Phase 3 缺陷；多数在子任务 report 中已记，此处汇总。

### D.7 PTY flake 恶化

`hermes::cli_runtime::tests` 的 PTY-echo 测试现在**单线程全套件也 flaky**（Phase 2 baseline
时仅并行 flaky）。原因：hermes 子集从 ~229（3a）涨到全量 404，单线程跑 27s，PTY 子进程时序
竞争更频繁。**非 3d 回归**（3d 未碰 `cli_runtime.rs`；隔离单跑 8/0 全绿；失败集合跨 run 变化、
含最初 baseline 就 flaky 的 `happy_path_echo_then_done_success`）。GATE 应对：跑 hermes 时
`--skip cli_runtime` + cli_runtime 隔离单跑。根因修复（fake-clock / PTY mocking 测试基建）延后。

### D.6 fake-clock 测试基建

supervisor-reap-in-tick + mid-flight-cancel 的时序无法在无注入时钟下确定性单测。Task 3 的
interrupted-agent 事件路径由 code-reading + 确定性 `emit_agent_event` 单测验证；完整 tick-reap
e2e 延后到手测（§C）。

### D.5 时间戳残留（Phase 3 Task 4 已大幅收敛，非阻塞）

Task 4 已把 store.rs 中 `datetime('now')` 运行时写入统一为 RFC3339（`update_run` completed_at /
`resolve_gate` resolved_at / reconcile dispatch completed_at / `create_run` DB↔struct 一致）。
残留的 schema DEFAULT 列（`created_at`）仍为 SQLite `datetime`——前端展示前归一为 RFC3339 即
可，非引擎 bug。

### Per-task merge/discard 命令

清扫产出 `RetainForReview` + `awaiting-merge` 事件；实际的 per-task merge/discard UI 命令是
Phase 4（在已冻结契约之上）。

### Judge 接线

Task 16 落地了 `Planner::judge` 纯逻辑 + driver（`build_judge_prompt` + `parse_judge_response`
容错 + 确定性 fallback），但 Coordinator**尚未**在 fan-out/convergence 点调用 judge——该接线
为后续任务。judge 逻辑本身已单测（parse + fallback）。

### Minor findings 清单

各子任务累计约 15 条 Minor（全部 non-blocking，留终审 triage），见
`.superpowers/sdd/progress.md` 的「Minor findings」段。典型如：store.rs 测试辅助
`is_rfc3339_chrono` 手写形状检查弱于 `parse_from_rfc3339`（test-only）；
`runtime_registry::single()` 硬编码 `[Sdk, Cli]`；`hermes_run_cancel` 错误前缀带
`hermes_run_stop:`（薄 delegate，by-design）；planner.rs `KEY_CANDIDATES` 未用；
`maybe_replan_on_failure` 单飞 `remove` 非 RAII（brief 明定结构，panic 时 slot 残留——但 panic
隔离在 watcher task 内、run 仍走熔断收敛）。

### 真 LLM e2e

标「待人工执行」（见 §C），未伪造。

---

## E. 自检

- **DoD 逐条：** 异构混跑 / mid-run 取消 / 收敛清扫安全 / 单飞 replan 四条 DoD 各有捕获的
  `ok` mock e2e + 支撑单测。
- **非回归：** 确定性子集 393/0 + cli_runtime 隔离 8/0 全绿；cargo check clean；npm build
  green；git diff --check clean。
- **诚实：** 全量 404/0 为本次跑真实结果；PTY-echo 偶发 flake（D.7）如实记录，未声称
  「永远 404/0」。
- **真 LLM e2e：** 标「待人工执行」，未伪造。
- **证据可溯：** 所有 gate 输出逐字摘自 `.superpowers/sdd/evidence/`，测试名已在源码核对存在。

## F. 交接

Phase 3 引擎收尾 5 项能力全部落地、6 子阶段（3a–3f）GATE 通过并 `--no-ff` 合回
`feat/helm @ 0cb7c9c`，驾驶舱契约已冻结（Phase 4 可据此并行接线）。真 LLM e2e 待手测
（§C，不阻塞 Phase 4 设计）。Phase 4 工作项：驾驶舱 UI（消费 `hermes://run`/`task`/`agent`
事件 + `hermes_run`/`hermes_run_cancel`/`hermes_run_cleanup` 命令）+ per-task merge/discard
命令 + judge 接线。
