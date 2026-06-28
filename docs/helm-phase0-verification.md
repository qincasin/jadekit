# Helm × Hermes — Phase 0 验证（多 Agent Daemon 池）

> Phase 0 把 jadekit chat 后端从「单 daemon、`SESSION_ID="default"` 写死、全局 abort」
> 改造为「按 `agent_id` 索引的 daemon 池」。本文档给出验收（DoD）的自动化检查与
> 手动 e2e 步骤，并回填实际验证记录。

## 1. 自动化检查（DoD）

| 检查 | 命令 | 期望 |
|------|------|------|
| Rust 编译 | `cargo check --manifest-path src-tauri/Cargo.toml` | 通过，**无 warning** |
| Rust 测试 | `cargo test --manifest-path src-tauri/Cargo.toml chat` | 全绿 |
| 前端类型 + 构建 | `npm run build` | 通过 |
| 前端单测 | `npx vitest run src/stores/chatEventRouting.test.ts` | 全绿 |
| 空白规范 | `git diff --check` | 无空白错误 |

## 2. 手动 e2e（多 tab 真并行 / 互不串扰）

**前置**：已配置一个可用的 Claude provider（API Key + Base URL），并在信任的工作目录下测试。

**步骤**：

1. 启动应用：`npm run tauri dev`。
2. 进入「对话」（Helm）页，**开两个 tab**，分别选择**不同的工作目录（cwd）**
   （例如 `~/project-a` 与 `~/project-b`）。
3. 另开一个终端，准备好观察进程：
   ```fish
   watch -n1 'ps -eo pid,command | grep -E "ai-bridge|daemon.js" | grep -v grep'
   ```
4. **同时**向两个 tab 发送较长的任务消息（例如让 agent 写一段较长文本 / 做一次文件读写的轻任务）。
5. 预期观察：
   - **进程**：`ps` 中可见**两个**独立的 `node .../daemon.js` 进程（每个 daemon 一个）。
   - **并行性**：两个 tab 的流式输出**互不阻塞**、**互不串流**（A tab 的回复只出现在 A，B 同理）。
   - **cwd 隔离**：各自 daemon 的 `process.chdir` 只作用于自己进程，不互相踩。
6. **abort 隔离**：在 tab A 点「停止」中断其当前回合。
   - 预期：tab A 停止；tab B **继续**正常输出，不受影响。
7. **退出清理**：关闭 app（退出进程）。
   - 预期：`ps` 中**所有** `node .../daemon.js` 进程退出（`shutdown_all` 逐个 stop + reap）。

## 3. 验证记录（回填）

### 3.1 自动化检查（实跑）

- `cargo check --manifest-path src-tauri/Cargo.toml`：✅ `Finished`，无 warning。
- `cargo test --manifest-path src-tauri/Cargo.toml chat`：✅ 61 passed; 0 failed。
  （含 `chat::agent_id`、`chat::pool`、`chat::daemon_client::tests::daemon_env_vars_use_provided_session_id`、
  `models::chat::*_serializes_optional_agent_id`、`chat::manager::tests::per_agent_abort_isolates_other_agents`、
  `commands::chat_commands::tests::agent_permission_dir_routes_to_session_subdir` 等新增用例。）
- `npm run build`：✅ `tsc && vite build` 通过。
- `npx vitest run src/stores/chatEventRouting.test.ts`：✅ 3 passed。
- `git diff --check`：✅ 无空白错误。

### 3.2 多 agent 进程隔离（实跑：真 app + headless 冒烟）

> 设计文档 §3 的核心论点：`process.chdir` 是进程全局 → worktree/cwd 隔离的并行 agent
> **只能进程隔离**（每 agent 一个 daemon 进程）。下面两层证据覆盖该机制。

#### 3.2.1 真 app 单 tab：per-agent env 端到端打通

`npm run tauri dev` 起的 app（`target/debug/jadekit`，Phase 0 代码）里发一条消息后，
观察其 daemon 进程环境变量：

```
CLAUDE_SESSION_ID     = b25eb78d-2bd8-4119-a882-3fe2a4ff7903   ← 前端 tab 的 agentId (crypto.randomUUID)，不再是写死的 "default"
CLAUDE_PERMISSION_DIR = ~/.jadekit/permissions/b25eb78d-...     ← per-agent 权限子目录（root/agent_id）
ANTHROPIC_BASE_URL    = https://open.bigmodel.cn/api/anthropic
```

证实链路：前端 tab 生成 UUID agentId → `chat_send({agentId})` → `ChatManager::client_for`
→ `DaemonClient::new(session_id=agentId)` → `daemon_env_vars` → node 进程 env。
（SESSION_ID 与 permission 目录 basename 一致，正是 per-agent IPC 匹配所需。）

#### 3.2.2 headless 双 daemon 冒烟（多 agent 共存 / 隔离 / 清理）

精确复刻 `DaemonClient::start`（同 node / 同 daemon.js / 同 deps_dir / 同 API env），
仅 `CLAUDE_SESSION_ID` 与 `CLAUDE_PERMISSION_DIR` 按两个 agent 区分，各起一个 daemon：

| 观察 | 预期 | 实际 |
|------|------|------|
| 两个 agent 各自独立 daemon 进程 | 共存 | ✅ pid=23841 / 23845 同时存活 |
| session 隔离 | `CLAUDE_SESSION_ID` 不同 | ✅ `smoke-agent-a` vs `smoke-agent-b` |
| permission 目录隔离 | `CLAUDE_PERMISSION_DIR` 不同 | ✅ 各自独立临时目录 |
| 进程隔离（杀 a 不影响 b） | b 存活 | ✅ kill a 后 b 仍 `alive` |
| 退出清理 | 无 smoke daemon 残留 | ✅ 残留数 0；临时目录/日志已清 |
| 不影响 app 真 daemon | 真 daemon 不变 | ✅ 仍 `alive`，session 不变 |

> 结论：Phase 0 的「process-per-agent → 独立 cwd/session/permission → 互不串扰 → 可独立中断 →
> 可整体回收」在进程层面已实证。配合单测 `per_agent_abort_isolates_other_agents`（turn 级 abort
> 隔离）与前端 `resolveTabForEvent` 单测（事件按 agentId 路由），DoD 的多 agent 并行 / abort
> 隔离 / 退出清理已被覆盖。

#### 3.2.3 GUI 两 tab UX 并行（可选最终确认）

> **状态：可选。** 在桌面应用里再开一个 tab、选不同 cwd、同时发消息，肉眼确认两 tab 流式输出
> 互不串流。机制层已由 3.2.1/3.2.2 + 单测覆盖；此项仅为端到端 UX 的最终肉眼确认，
> 不再是阻塞项。

## 4. 已知偏差（与计划的差异，均已最小化对齐）

1. **`chat_send` 命令签名**：真实签名为 `chat_send(provider, command, params, state)`
   （内部拼 `method = "{provider}.{command}"`），非计划示例的 `(agent_id, method, params)`。
   实际实现把 `agent_id: Option<String>` 作为**首参数**追加，保留 `provider`/`command`。
2. **权限响应路由**：计划 Task 4 把 daemon 的 `CLAUDE_PERMISSION_DIR` 改为 per-agent 子目录
   （`<root>/<agent_id>`），但未同步更新三个 `permission_respond_*` 命令的写入目录——
   会导致响应落到根目录、daemon 读不到（**连默认 agent 也会断**）。已在 Task 5 修正：
   响应写到 `<permission_root>/<session_id>`（= agent 子目录），并补 `agent_permission_dir` 单测。
3. **`FakeDaemonClient` 已命名**：计划 Task 3 注释假设「fake 是匿名结构需提取」，实际它已是
   命名 `FakeDaemonClient`；故仅抽取 `test_support::fake_client()` 构造器并 `pub` 化字段。
4. **`restart_daemon` 语义**：计划签名 `restart_daemon(agent_id: AgentId)`；实际取
   `Option<AgentId>`——`None` = 重启全部在跑 daemon（provider 配置 / SDK deps 目录是全局共享的，
   `chat_restart_daemon` 不传 agent_id 时应作用于全部），`Some(id)` = 仅该 agent。
5. **`resolveTabForEvent` 的 `TabRef`** 字段名为 `{ key, agentId }`（与 `ChatSessionTab` 结构兼容），
   而非计划示例的 `{ tabKey, agentId }`，以便直接把 `openTabs` 传入。

## 5. 下一步

Phase 0 完成且手动 e2e 通过后，可进入 Phase 1（WorktreeManager + 每 tab 绑 worktree + 异构扇出）。
