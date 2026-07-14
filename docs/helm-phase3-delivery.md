# Phase 3 Delivery Report — Hermes 引擎收尾 + 驾驶舱契约

> 执行者 GLM（subagent-driven-development 协调者），交付日期 2026-06-28。
> 主功能分支 `feat/helm`（Phase 3 tip `0cb7c9c`，未 merge 到 main/develop，等用户指示）。
> Phase 3 起点 `dce2570`（Phase 2 终审 tip）。本报告对照计划
> `docs/superpowers/plans/2026-06-28-helm-hermes-phase3-engine-completion.md` 的「完成定义 (DoD)」逐条核账。
> 详细技术证据见 `docs/helm-phase3-verification.md`（Task 17）与
> `.superpowers/sdd/evidence/`（真实捕获的自动门尾部输出）。

## 1. DoD 逐条状态

| DoD 条目 | 状态 | 证据 / 子阶段 GATE |
|---|---|---|
| Task 1–18 全部 step 打勾、各自 commit；7 个子阶段（3a–3g）各过 GATE 并 `--no-ff` merge 回 `feat/helm` | ✅ | Task 1–16 全部 step 完成、各自 commit（见 §3 git graph）；3a–3f 六子阶段 GATE 绿并 `--no-ff` 合回 `feat/helm`：**3a@229e066 / 3b@bd41c29 / 3c@9883e83 / 3d@15ef1a6 / 3e@df96163 / 3f@0cb7c9c**；3g = Task 17 验证文档 + Task 18 交付报告（本文），本提交收尾 |
| `cargo check` 通过；`cargo test`（hermes + chat 全绿） | ✅ | `cargo check` Finished（26 pre-existing warnings，无 error）；全套件单线程 **404 passed; 0 failed**；确定性子集（`--skip cli_runtime`）**393/0**；`cli_runtime` 隔离单跑 **8/0**（D.7 PTY-flake 诚实说明见 §5） |
| `npm run build` 通过；`git diff --check` 干净 | ✅ | `✓ built in 1.69s`；`git diff --check` exit=0（empty=clean） |
| **契约冻结**：`docs/helm-hermes-ui-contract.md` 与代码签名逐字一致；3a 合回后 Phase 4 可并行接真事件 | ✅ | Task 5 逐字对齐代码、0 drift；3c/3d 增量扩展（`cancelled` token / `hermes_run_cancel` / `hermes_run_cleanup` / `SweepReportDto`）已同步回契约文档 |
| **异构混跑**：mock 下一次 run 内 SDK×CLI 各自路由、并行、收敛（3b Task 9） | ✅ | `mixed_run_dispatches_sdk_and_cli_tasks_through_respective_runtimes` … ok |
| **mid-run cancel**：mock 下跑到一半取消 → run=Cancelled + 在飞 abort（3c） | ✅ | `mid_run_cancel_aborts_inflight_and_marks_cancelled` … ok |
| **收敛清扫安全**：失败/无改动 → Remove；完成有产出 → RetainForReview；绝不删未合并工作（3d） | ✅ | `run_sweeps_worktrees_on_convergence` … ok；`decide_disposition` 11/11 单测 + 删除前 `has_uncommitted_changes` 双保险复查（fail-safe） |
| **单飞 replan**：并发熔断只 replan 一次（3e） | ✅ | `concurrent_replans_for_same_run_are_single_flight` … ok |
| `docs/helm-phase3-verification.md` + `docs/helm-phase3-delivery.md` 已写；真 LLM e2e 标「待人工执行」，不造假 | ✅ | 交付报告 = 本文（Task 18，本提交）；验证文档（Task 17）+ 真实自动门证据捕获于 `.superpowers/sdd/evidence/`（见 §4）；真 LLM e2e 一律标「待人工执行」，**未伪造任何结果** |
| 做完 Task 18 即停 | ✅ | 已停，未 merge 到 main，等用户指示 |

**结论：DoD 全部满足（真 LLM e2e 按计划标待人工执行，所有自动门数字均为真实捕获）。**

## 2. 改动清单

### 2.1 新增文件

| 文件 | 职责 | 引入子阶段 |
|---|---|---|
| `src-tauri/src/hermes/events.rs` | `OrchestrationEvent`（判别联合，`kind` tag + camelCase）+ `OrchestrationEventSink` 契约 + `NullEventSink`（默认 no-op） | 3a Task 1 |
| `src-tauri/src/hermes/runtime_registry.rs` | `RuntimeRegistry`（`RuntimeKind → Arc<dyn AgentRuntime>` 注册表）；`single()` 兼容 Phase 2 单介质 | 3b Task 6 |
| `src-tauri/src/hermes/run_lifecycle.rs` | `decide_disposition` 纯决策 + `sweep_run_worktrees` + `SweepReport` / `WorktreeCleanupInput` / `WorktreeDisposition` | 3d Task 12/13 |
| `docs/helm-hermes-ui-contract.md` | 冻结的引擎↔驾驶舱契约（命令/事件/DTO/状态词表），3c/3d 增量扩展 | 3a Task 5（3c/3d 增补） |
| `docs/helm-phase3-verification.md` | Phase 3 验证文档（自动门证据 + mock e2e + 真 LLM 手动步骤 + 已知未决） | 3g Task 17 |
| `docs/helm-phase3-delivery.md` | 本文（交付报告 + Phase 4 就绪确认） | 3g Task 18 |

### 2.2 既有文件加法式扩展

| 文件 | 加法式改动 |
|---|---|
| `hermes/coordinator.rs` | 注入 `event_sink`（`OnceLock<run_id>` + 3 处 task emit 点）+ agent 事件发射（`emit_agent_event` 7 变体穷尽 + reap→interrupted）+ 注册表路由（`dispatch_one` 按 `assignment.runtime` 选介质）+ reap-per-kind liveness + mid-run cancel（`with_cancel` + tick-top 检查 + abort 在飞）+ 收敛后清扫（3 终态点 best-effort sweep + `base_branch` / `sweep_on_converge` 字段）+ 单飞 replan（`replan_inflight: Arc<tokio::sync::Mutex<HashSet<String>>>`） |
| `hermes/supervisor.rs` | `WorkerSupervisor` 持 `RuntimeRegistry` + `register(agent_id, structured, kind)` + reap 两阶段探 `registry.get(kind)` |
| `hermes/store.rs` | `coordinator_runs.status` CHECK 放行 `'cancelled'` + `update_run` 终态 CASE + 运行时时间戳统一 RFC3339（D.5，对齐 `datetime('now')` 写入点）+ `get_run` |
| `hermes/types.rs` | `RunStatus::Cancelled` + `as_str`/`from_str` `"cancelled"` + roundtrip |
| `hermes/planner.rs` | LLM-judge：`JudgeVerdict` / `JudgeCandidate` + `build_judge_prompt` + `parse_judge_response`（容错 + 确定性 fallback）+ `Planner::judge` 镜像 `plan` 驱动路径 |
| `commands/hermes_commands.rs` | `TauriEventSink`（包 `AppHandle`，emit 映射 3 通道）+ `event_channel_for` 纯函数 + `hermes_run_show` / `hermes_agent_list` / `hermes_run_cancel` / `hermes_run_cleanup` + `RunShowDto` / `SweepReportDto` + `start_run` 单一 emit 路径 |
| `lib.rs` | `generate_handler!` 注册 4 新命令 + setup 构造 `RuntimeRegistry` 双介质登记（Sdk + Cli）注入 HermesEngine/Supervisor 共享 |
| `chat/worktree.rs` | 关联函数 `has_commits_ahead`（`git rev-list --count <base>..HEAD` > 0） |

### 2.3 非回归保证（关键不变量）

**全程加法式、非回归。** 所有新增可选钩子默认关闭，不注入时 Coordinator/Supervisor 行为与 Phase 2 逐字一致：

- `Coordinator::new` 默认 `NullEventSink`（emit 立即返回、不分配）；
- `RuntimeRegistry::single(rt)` 把同一介质登记到所有 kind（Phase 2 测试经包装后行为不变）；
- `with_cancel` 默认不注入（`None`），循环按 `RUN_MAX_ITERATIONS` + 收敛判定自然退出；
- 收敛后清扫 best-effort（`sweep_on_converge=false` 可测试关闭；`Remove` 前还有 `has_uncommitted_changes` 双保险复查，git 错按「脏」处理 → 降级保留）；
- 单飞 `replan_inflight` 默认空集，无 planner 注入时不触发。

## 3. git log（子阶段分支与 merge）

下图为 `git log --oneline --graph feat/helm ^dce2570` 的**真实输出**（逐字粘贴，未删改）。3a 的 merge `229e066` 有两个父提交（`dce2570` base + `5cb5052` phase3a tip），因 `^dce2570` 排除了 base 父而在图中呈线性外观——它仍是 `--no-ff` merge。3b–3f 的 `--no-ff` 分叉/合并气泡清晰可见。

```
*   0cb7c9c merge: Phase 3f — llm-judge for fan-out/convergence scoring
|\
| * d48e9de feat(hermes): llm-judge for fan-out/convergence scoring
|/
*   df96163 merge: Phase 3e — single-flight replan
|\
| * 5f5a01a fix(hermes): single-flight replan per run to dedupe concurrent circuit-breaks
|/
*   15ef1a6 merge: Phase 3d — end-of-run worktree lifecycle
|\
| * 8e76fa5 feat(hermes): sweep worktrees on run convergence and manual cleanup command
| * 6abdbea fix(hermes): fail-safe worktree cleanup double-check on git error
| * fda3019 feat(hermes): end-of-run worktree sweep with retain-for-review safety
| * d448498 feat(hermes): worktree cleanup disposition pure logic
|/
*   9883e83 merge: Phase 3c — mid-run cancellation
|\
| * df3b59a feat(hermes): hermes_run_cancel wires cancel signal into coordinator loop
| * a6e8b0a feat(hermes): mid-run cancellation with cancelled run status
|/
*   bd41c29 merge: Phase 3b — heterogeneous in-run media routing
|\
| * 39dfa19 feat(hermes): register sdk and cli runtimes for in-run heterogeneous scheduling
| * 02a5c09 feat(hermes): supervisor probes liveness per-agent runtime kind
| * 62a674a feat(hermes): coordinator routes dispatch by assignment runtime kind
| * 8db5bfd feat(hermes): runtime registry for heterogeneous media routing
|/
* d1ceb6e chore: untrack sdd scratch reports; gitignore .superpowers/
* 229e066 merge: Phase 3a — UI contract + live task/agent events
* 5cb5052 docs(helm): freeze hermes ui contract for phase 4 parallel work
* b705f73 feat(hermes): tauri event sink, run/agent read commands, rfc3339 timestamps
* 2d5d1a3 feat(hermes): watcher emits agent activity and liveness events
* 749774e feat(hermes): coordinator emits task lifecycle events via sink
* d9b76ed feat(hermes): orchestration event sink contract and null sink
```

每个子阶段独立分支 → GATE 绿 → `--no-ff` 合回 `feat/helm`，集成记录完整保留。`d1ceb6e` 是 3a 合回后的卫生提交（误入库的 SDD scratch report untrack + `.gitignore .superpowers/`，根因修复，后续 `git add -A` 不再扫进 scratch）。

## 4. 验证证据

以下均为 `.superpowers/sdd/evidence/*.txt` 中**真实捕获**的尾部输出（逐字引用，未修饰）：

**cargo test 全套件（单线程，本次跑）** — `full-suite-results.txt`：
```
test result: ok. 404 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 27.35s
```

**确定性子集（`--skip cli_runtime`）** — `deterministic-results.txt`：
```
test result: ok. 393 passed; 0 failed; 0 ignored; 0 measured; 11 filtered out; finished in 6.39s
```

**cli_runtime 隔离单跑** — `cli-runtime-isolation.txt`：
```
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 396 filtered out; finished in 0.59s
```

**cargo check** — `cargo-check.txt`：
```
warning: `jadekit` (lib) generated 26 warnings (run `cargo fix --lib -p jadekit` to apply 13 suggestions)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.32s
```

**npm run build** — `npm-build.txt`：
```
✓ built in 1.69s
```

**git diff --check** — `git-diff-check.txt`：
```
exit=0 (empty=clean)
```

**mock 端到端**（4 条，一一对应 DoD 的异构/取消/清扫/单飞）— `mock-e2e-names.txt`：
```
test hermes::coordinator::tests::mixed_run_dispatches_sdk_and_cli_tasks_through_respective_runtimes ... ok
test hermes::coordinator::tests::mid_run_cancel_aborts_inflight_and_marks_cancelled ... ok
test hermes::coordinator::tests::run_sweeps_worktrees_on_convergence ... ok
test hermes::coordinator::tests::concurrent_replans_for_same_run_are_single_flight ... ok
```

> 真 LLM 手动 e2e（异构 Sdk/Cli 混跑 + mid-run cancel 真停 + 收敛后清扫只留 RetainForReview 产物）**待人工执行**，步骤见 `docs/helm-phase3-verification.md` §C。本文不声称已跑、未伪造任何结果。

## 5. 偏差与未决

**设计取舍（均已文档化、非阻塞）**：

- **Cancelled→Dispatched 映射（Task 12）**：`TaskStatus` 没有 `Cancelled` 变体；brief 的「Cancelled + 有未提交 → RetainForReview」在 `decide_disposition` 里映射为「Dispatched + `has_uncommitted`」。已在 `run_lifecycle.rs` 注释说明。
- **pre-loop cancel 标 Failed（非 Cancelled）**：`hermes_run_stop` / run() 启动前置位 cancel → 走 pre-loop 快路径标 `Failed`；mid-loop tick-top 命中（Task 10）→ 标 `Cancelled`。两路径均已在命令文档与契约 §5.5 注明。
- **PTY flake 恶化（D.7）**：`hermes::cli_runtime::tests` 的 PTY echo 用例现在**单线程全套件也 flaky**（Phase 2 baseline 时仅并行 flaky）。原因：hermes 子集从 ~229(3a) 涨到 ~190+，单线程跑 28-31s，PTY 子进程时序竞争更频繁。**非 3d 回归**（3d 未碰 `cli_runtime.rs`；隔离单跑 8/0 全绿）。GATE 改法：跑 hermes 时 `--skip cli_runtime`，`cli_runtime` 隔离单跑。属 pre-existing 测试基础设施，deferred。
- **CliRuntime 生产命令默认 `["claude"]`**：对 mock-e2e 可辩护；真实 CLI 介质校验留给 Task 17 手动 e2e。

**Minor findings（约 15 条，非阻塞，留终审 triage）**：完整清单见 `.superpowers/sdd/progress.md`「Minor findings」段。代表性条目：`store.rs` 测试辅助 `is_rfc3339_chrono` 弱于 `parse_from_rfc3339`（test-only）；`runtime_registry::single()` 硬编码 `[Sdk, Cli]` 变体列表（加第三种 RuntimeKind 会漏登）；`maybe_replan_on_failure` 单飞 `remove` 不在 RAII/scopeguard 里（panic 时 slot 残留，不阻塞收敛）；`planner.rs` `KEY_CANDIDATES` 常量未用（被 `#![allow(dead_code)]` 压住）；若干 stale `#[allow(dead_code)]` 标签与 test 命名可收紧。

**真 LLM e2e**：待人工执行（未跑、未伪造）。

## 6. Phase 4 集成就绪确认

**就绪。** Phase 4（驾驶舱 UI）可基于冻结契约独立并行开工：

1. **契约冻结**：`docs/helm-hermes-ui-contract.md` 与代码签名逐字一致（Task 5 review 验证 0 drift；3c/3d 增量扩展已同步），Phase 4 只读此文即可冷启动接线。
2. **真事件可接**：Coordinator 经 `TauriEventSink` 发射 `hermes://run` / `hermes://task` / `hermes://agent` 三通道（统一 `OrchestrationEvent` 判别联合，TS 侧可直接 `JSON.parse` 成 discriminated union）；Phase 4 用 `app.listen(...)` 订阅，监听失败不影响引擎循环（best-effort）。
3. **读命令就绪**：`hermes_run_show`（run 概览 + 任务计数）、`hermes_agent_list`（活跃 Roster）、`hermes_task_list`（按 status/ready 过滤）、`hermes_dispatch_show`（单条派发上下文）。
4. **干预命令就绪**：`hermes_run_cancel`（mid-run 取消，与 `hermes_run_stop` 共用 cancel 标志）、`hermes_run_cleanup`（手动触发 worktree 清扫兜底入口）、`hermes_gate_resolve`（决策门解决）。
5. **引擎能力齐备**：异构介质路由（SDK×CLI 一次 run 内混跑）+ per-kind liveness 判活 + mid-run cancel + 收敛后安全清扫（绝不删未合并工作）+ 并发 replan 单飞 + LLM-judge，全部落地并经 mock e2e 验证。
6. **非回归前提成立**：所有新钩子默认关闭时，引擎行为 byte-identical 于 Phase 2——Phase 4 可独立迭代 UI 而不破坏引擎。

**Phase 4 可立即据此构建驾驶舱 UI；真 LLM e2e 是唯一剩余的人工验证项**（不阻塞 Phase 4 设计与接线）。
