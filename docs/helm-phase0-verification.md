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

### 3.2 手动 e2e（多 tab 真并行）

> **状态：待人工执行。** 该 e2e 需要启动 Tauri 桌面应用并在 GUI 中操作两个 tab，
> 无法由后端单测覆盖。请按「§2 手动 e2e」步骤实跑，并把观察回填到下表。

| 观察 | 预期 | 实际 |
|------|------|------|
| 两个 tab 各自独立 daemon 进程 | `ps` 见 ≥2 个 `node .../daemon.js` | _待回填_ |
| 并行不阻塞 / 不串流 | A/B 输出互不混入 | _待回填_ |
| abort(tab A) 不影响 tab B | B 继续输出 | _待回填_ |
| 关闭 app 后全部 daemon 退出 | `ps` 中无残留 daemon | _待回填_ |

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
