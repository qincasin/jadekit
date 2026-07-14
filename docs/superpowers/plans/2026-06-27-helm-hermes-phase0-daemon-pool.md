# Helm × Hermes — Phase 0：多 Agent Daemon 池 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 jadekit chat 后端从「单 daemon、`SESSION_ID="default"` 写死、全局 abort」改造成「按 `agent_id` 索引的 daemon 池」，使多个会话各自拥有独立 daemon 进程、独立 cwd、独立 session id 与独立 abort，从而真正并行、互不踩 `process.chdir`。

**Architecture:** 在 `ChatManager` 内用 `Mutex<HashMap<AgentId, AgentEntry>>` 取代单一 `OnceCell<client>`。每个 `AgentEntry` 持有一个 `DaemonClient`（独立进程，`CLAUDE_SESSION_ID = agent_id`，独立 permission 子目录与 watcher）。所有 `send/abort/is_running/shutdown` 改为按 `agent_id` 寻址；事件 payload 增加 `agentId` 字段；前端每个会话 tab 绑定一个 `agentId`。

**Tech Stack:** Rust (Tauri 2, tokio, serde_json)、Node ai-bridge（不改协议）、React 19 + TypeScript + Zustand。

## Global Constraints

- 不写魔法字符串：`agent_id`、事件名、env key、默认 agent id 等必须集中为常量/enum（CLAUDE.md 规约）。
- 配置集中：新增配置进 `api/config` 等已有体系或集中常量，文档说明默认值。
- 不绕过既有抽象：daemon 操作只经 `DaemonClient` / `ManagerDaemonClient` trait。
- 新增代码补中文注释，尤其状态流转、进程生命周期、并发边界。
- 新增能力必须补测试与中文文档（无测试/文档视为未完成）。
- 向后兼容：旧前端事件 `chat://stream{requestId,kind,text}` 字段保留，新增 `agentId` 为附加字段，不删旧字段。
- JSON 字段 camelCase（`#[serde(rename)]`），Rust 内部 snake_case。
- 新增 Tauri 命令必须在 `lib.rs` 的 `generate_handler!` 注册。
- 提交前置检查：`cargo check --manifest-path src-tauri/Cargo.toml` 通过；涉及前端跑 `npm run build`。
- 每个 task 结束即 commit，使用 Conventional Commits（`feat:`/`refactor:`/`test:`）。

---

## 阅前必读（执行者冷启动先做）

在动任何代码前，用 codegraph 读这些文件（仓库已建索引，见 CLAUDE.md「代码探索」）：

```bash
codegraph node DaemonClient
codegraph node ChatManager
codegraph explore chat send abort daemon session permission
```

并通读以下真实文件，后续任务的「Modify」基于它们当前内容：

- `src-tauri/src/chat/daemon_client.rs`（`DaemonClient`、`daemon_env_vars`、`SESSION_ID`）
- `src-tauri/src/chat/manager.rs`（`ChatManager`、`ManagerDaemonClient` trait、`client()`/`send()`/`abort()`）
- `src-tauri/src/chat/mod.rs`（导出）
- `src-tauri/src/commands/chat_commands.rs`（`chat_send`/`chat_abort`/`chat_is_running`/`chat_start_daemon`、`ChatState`）
- `src-tauri/src/lib.rs`（`generate_handler!` 注册、`ChatState` 注入）
- `src-tauri/src/chat/permission_watcher.rs`（`PermissionWatcher::new(dir, session_id, app)`）
- `src/stores/useChatStore.ts`（`requestTabKeys`、`pendingSendOwners`、事件监听）
- `src/services/`（前端调用 `chat_send` 的封装）

> 文件结构决策：本阶段**新建** `src-tauri/src/chat/agent_id.rs`（AgentId 与默认常量）与 `src-tauri/src/chat/pool.rs`（daemon 池）。`AgentEntry` 放 `pool.rs`。其余为就地 Modify。每个文件单一职责。

---

## Task 1: 定义 `AgentId` 与默认常量

**Files:**
- Create: `src-tauri/src/chat/agent_id.rs`
- Modify: `src-tauri/src/chat/mod.rs`（新增 `mod agent_id;` 与 `pub use`）
- Test: `src-tauri/src/chat/agent_id.rs`（`#[cfg(test)]` 内联）

**Interfaces:**
- Produces:
  - `pub type AgentId = String;`
  - `pub const DEFAULT_AGENT_ID: &str = "default";`
  - `pub fn sanitize_agent_id(raw: &str) -> AgentId`（去除路径分隔符/空白，空则回退 `DEFAULT_AGENT_ID`；保证可安全作为 `CLAUDE_SESSION_ID` 与目录名）

- [ ] **Step 1: 写失败测试**

在 `agent_id.rs` 末尾：

```rust
#[cfg(test)]
mod tests {
    use super::{sanitize_agent_id, DEFAULT_AGENT_ID};

    #[test]
    fn empty_falls_back_to_default() {
        assert_eq!(sanitize_agent_id("   "), DEFAULT_AGENT_ID);
        assert_eq!(sanitize_agent_id(""), DEFAULT_AGENT_ID);
    }

    #[test]
    fn strips_path_separators_and_trims() {
        assert_eq!(sanitize_agent_id("  a/b\\c  "), "abc");
    }

    #[test]
    fn keeps_plain_id() {
        assert_eq!(sanitize_agent_id("agent-7"), "agent-7");
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml chat::agent_id -- --nocapture`
Expected: 编译失败（`sanitize_agent_id` 未定义）。

- [ ] **Step 3: 写最小实现**

在 `agent_id.rs` 顶部：

```rust
//! Agent 标识与归一化。
//! agent_id 同时用作 daemon 的 CLAUDE_SESSION_ID 与 permission 子目录名，
//! 因此必须可安全用于文件路径。集中定义，避免魔法字符串。

/// 一个可部署 Agent 实例的标识。当前用字符串，未来可换 newtype。
pub type AgentId = String;

/// 单聊默认 Agent（兼容旧的单 daemon 行为）。
pub const DEFAULT_AGENT_ID: &str = "default";

/// 归一化前端传入的 agent_id：去空白、去路径分隔符；空则回退默认。
pub fn sanitize_agent_id(raw: &str) -> AgentId {
    let cleaned: String = raw
        .trim()
        .chars()
        .filter(|c| !matches!(c, '/' | '\\') && !c.is_whitespace())
        .collect();
    if cleaned.is_empty() {
        DEFAULT_AGENT_ID.to_string()
    } else {
        cleaned
    }
}
```

在 `mod.rs` 顶部模块声明区加入：

```rust
mod agent_id;
```

并在 `pub use` 区加入：

```rust
pub use agent_id::{sanitize_agent_id, AgentId, DEFAULT_AGENT_ID};
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test --manifest-path src-tauri/Cargo.toml chat::agent_id`
Expected: 3 个测试 PASS。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/chat/agent_id.rs src-tauri/src/chat/mod.rs
git commit -m "feat(chat): add AgentId type and sanitize_agent_id"
```

---

## Task 2: `DaemonClient` 支持 per-agent 的 session id 与 permission 目录

当前 `daemon_env_vars` 用全局 `SESSION_ID` 常量写 `CLAUDE_SESSION_ID`，`DaemonClient::new` 接收一个 `permission_dir`。本任务让二者可按 agent 注入，**保持旧调用默认行为不变**（默认 agent 仍写 `"default"`）。

**Files:**
- Modify: `src-tauri/src/chat/daemon_client.rs`（`DaemonClient` 增加 `session_id: String` 字段；`new` 增加 `session_id` 参数；`daemon_env_vars` 用该字段替代常量）
- Test: `src-tauri/src/chat/daemon_client.rs`（扩展现有 `daemon_env_vars_*` 测试）

**Interfaces:**
- Consumes: `DEFAULT_AGENT_ID`（Task 1）
- Produces:
  - `DaemonClient::new(node_path, bridge_dir, deps_dir, permission_dir, session_id: String, api_key, base_url, debug) -> Self`
  - `pub fn session_id(&self) -> &str`
  - `daemon_env_vars(permission_dir, deps_dir, session_id: &str, provider_config) -> Vec<(&'static str, OsString)>`

- [ ] **Step 1: 写失败测试**

替换 `daemon_client.rs` 中 `daemon_env_vars_include_sdk_deps_dir_and_provider_config` 测试体里对 `daemon_env_vars` 的调用为携带 session id，并断言：

```rust
    #[test]
    fn daemon_env_vars_use_provided_session_id() {
        let deps_dir = std::path::PathBuf::from("/tmp/deps");
        let permission_dir = std::path::PathBuf::from("/tmp/perm");
        let provider_config = ProviderRuntimeConfig { api_key: None, base_url: None };

        let vars: std::collections::HashMap<_, _> =
            daemon_env_vars(&permission_dir, &deps_dir, "agent-42", &provider_config)
                .into_iter()
                .map(|(k, v)| (k, v.to_string_lossy().into_owned()))
                .collect();

        assert_eq!(vars.get("CLAUDE_SESSION_ID").map(String::as_str), Some("agent-42"));
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml chat::daemon_client::tests::daemon_env_vars_use_provided_session_id`
Expected: 编译失败（`daemon_env_vars` 参数数量不匹配）。

- [ ] **Step 3: 写最小实现**

改 `daemon_env_vars` 签名与体：

```rust
fn daemon_env_vars(
    permission_dir: &Path,
    deps_dir: &Path,
    session_id: &str,
    provider_config: &ProviderRuntimeConfig,
) -> Vec<(&'static str, OsString)> {
    let mut vars = vec![
        ("AI_BRIDGE_DEPS_DIR", deps_dir.as_os_str().to_owned()),
        ("CLAUDE_PERMISSION_DIR", permission_dir.as_os_str().to_owned()),
        ("CLAUDE_SESSION_ID", OsString::from(session_id)),
    ];
    if let Some(ref key) = provider_config.api_key {
        vars.push(("ANTHROPIC_AUTH_TOKEN", OsString::from(key)));
    }
    if let Some(ref url) = provider_config.base_url {
        vars.push(("ANTHROPIC_BASE_URL", OsString::from(url)));
    }
    vars
}
```

`DaemonClient` 结构体新增字段 `session_id: String,`；`new` 增加 `session_id: String` 参数并存入；`start()` 内调用处改为：

```rust
        for (key, value) in
            daemon_env_vars(&self.permission_dir, &self.deps_dir, &self.session_id, &provider_config)
        {
            cmd.env(key, value);
        }
```

新增访问器：

```rust
    /// 该 daemon 绑定的 session id（= agent_id），用于 permission IPC 匹配。
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
```

保留旧常量但标注用途（默认 agent）：`pub const SESSION_ID: &str = "default";`（=`DEFAULT_AGENT_ID`，permission_watcher 旧路径仍可用）。

- [ ] **Step 4: 修正既有调用点 + 既有测试**

`manager.rs` 里 `DaemonClient::new(node, bridge, deps, perm_dir, api_key, base_url, debug)` 临时改为传入默认 session：`DaemonClient::new(node, bridge, deps, perm_dir, DEFAULT_AGENT_ID.to_string(), api_key, base_url, debug)`（Task 3 会替换）。同时把旧测试 `daemon_env_vars_include_...` 调用补上 `"default"` 参数，并保留对 `CLAUDE_SESSION_ID == "default"` 的断言。

- [ ] **Step 5: 运行确认通过**

Run: `cargo test --manifest-path src-tauri/Cargo.toml chat::daemon_client`
Expected: 全部 PASS。

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/chat/daemon_client.rs src-tauri/src/chat/manager.rs
git commit -m "refactor(chat): make DaemonClient session_id injectable per agent"
```

---

## Task 3: 新建 daemon 池 `AgentPool`

把「按需创建 + 缓存 + 寻址 + 关闭」从 `ChatManager` 抽到独立 `pool.rs`，键为 `AgentId`。`ChatManager` 改为持有 `AgentPool`。

**Files:**
- Create: `src-tauri/src/chat/pool.rs`
- Modify: `src-tauri/src/chat/mod.rs`（`mod pool;`）
- Test: `src-tauri/src/chat/pool.rs`（`#[cfg(test)]` 用 fake client）

**Interfaces:**
- Consumes: `ManagerDaemonClient`（manager.rs 现有 trait；本任务将其 `pub(crate)` 化以便 pool 引用）、`AgentId`、`DEFAULT_AGENT_ID`
- Produces:
  - `pub struct AgentPool { /* Mutex<HashMap<AgentId, Arc<dyn ManagerDaemonClient>>> */ }`
  - `AgentPool::new() -> Self`
  - `async fn get_or_init<F, Fut>(&self, id: &AgentId, init: F) -> Result<Arc<dyn ManagerDaemonClient>, String>` where `F: FnOnce() -> Fut`, `Fut: Future<Output = Result<Arc<dyn ManagerDaemonClient>, String>>`（已存在则返回缓存，否则调用 `init` 并缓存）
  - `async fn get(&self, id: &AgentId) -> Option<Arc<dyn ManagerDaemonClient>>`
  - `async fn ids(&self) -> Vec<AgentId>`
  - `async fn remove(&self, id: &AgentId) -> Option<Arc<dyn ManagerDaemonClient>>`

- [ ] **Step 1: 把 trait 暴露给 pool**

在 `manager.rs` 把 `trait ManagerDaemonClient` 改为 `pub(crate) trait ManagerDaemonClient`，并在 `mod.rs` 不导出（仅 crate 内）。

- [ ] **Step 2: 写失败测试**

`pool.rs` 末尾：

```rust
#[cfg(test)]
mod tests {
    use super::AgentPool;
    use crate::chat::manager::ManagerDaemonClient;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    // 复用一个最小 fake：仅计数 init 次数。
    fn fake() -> Arc<dyn ManagerDaemonClient> {
        crate::chat::manager::test_support::fake_client()
    }

    #[tokio::test]
    async fn get_or_init_caches_per_id() {
        let pool = AgentPool::new();
        let calls = Arc::new(AtomicUsize::new(0));

        for _ in 0..2 {
            let calls = calls.clone();
            pool.get_or_init(&"a".to_string(), || async move {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(fake())
            })
            .await
            .unwrap();
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1, "同一 id 只初始化一次");
        assert_eq!(pool.ids().await, vec!["a".to_string()]);
    }

    #[tokio::test]
    async fn remove_drops_entry() {
        let pool = AgentPool::new();
        pool.get_or_init(&"a".to_string(), || async { Ok(fake()) }).await.unwrap();
        assert!(pool.remove(&"a".to_string()).await.is_some());
        assert!(pool.get(&"a".to_string()).await.is_none());
    }
}
```

> 依赖：在 `manager.rs` 新增 `#[cfg(test)] pub(crate) mod test_support { ... fake_client() ... }`，复用文件末尾已有的 fake `send_streaming` 实现（manager.rs:637 附近的 fake impl）抽成可复用构造器。

- [ ] **Step 3: 运行确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml chat::pool`
Expected: 编译失败（`AgentPool` / `test_support` 未定义）。

- [ ] **Step 4: 写最小实现**

`pool.rs`：

```rust
//! 按 agent_id 索引的 daemon 池。
//! 每个 agent 一个独立 DaemonClient（独立进程/cwd/session）。
//! 取代原先 ChatManager 的单例 OnceCell。

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use tokio::sync::Mutex;

use super::agent_id::AgentId;
use super::manager::ManagerDaemonClient;

pub struct AgentPool {
    clients: Mutex<HashMap<AgentId, Arc<dyn ManagerDaemonClient>>>,
}

impl AgentPool {
    pub fn new() -> Self {
        Self { clients: Mutex::new(HashMap::new()) }
    }

    /// 取缓存；不存在则用 `init` 创建并缓存。init 失败不写入。
    pub async fn get_or_init<F, Fut>(
        &self,
        id: &AgentId,
        init: F,
    ) -> Result<Arc<dyn ManagerDaemonClient>, String>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Arc<dyn ManagerDaemonClient>, String>>,
    {
        if let Some(c) = self.clients.lock().await.get(id) {
            return Ok(c.clone());
        }
        let client = init().await?;
        let mut guard = self.clients.lock().await;
        // 双检：并发下若他人已写入，复用之，丢弃本次。
        if let Some(existing) = guard.get(id) {
            return Ok(existing.clone());
        }
        guard.insert(id.clone(), client.clone());
        Ok(client)
    }

    pub async fn get(&self, id: &AgentId) -> Option<Arc<dyn ManagerDaemonClient>> {
        self.clients.lock().await.get(id).cloned()
    }

    pub async fn ids(&self) -> Vec<AgentId> {
        self.clients.lock().await.keys().cloned().collect()
    }

    pub async fn remove(&self, id: &AgentId) -> Option<Arc<dyn ManagerDaemonClient>> {
        self.clients.lock().await.remove(id)
    }
}
```

`mod.rs` 加 `mod pool;`（crate 内用，不必 `pub use`）。在 `manager.rs` 增补 `test_support`：

```rust
#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    /// 构造一个最小 fake daemon client，供 pool/manager 测试复用。
    pub fn fake_client() -> std::sync::Arc<dyn ManagerDaemonClient> {
        // 复用本文件已有的 Fake 实现（见文件末尾 fake send_streaming）。
        std::sync::Arc::new(FakeDaemonClient::default())
    }
}
```

（若文件末尾 fake 是匿名结构，先将其提取为命名 `FakeDaemonClient` 并 `Default` 化，再供 `test_support` 使用。）

- [ ] **Step 5: 运行确认通过**

Run: `cargo test --manifest-path src-tauri/Cargo.toml chat::pool`
Expected: 2 个测试 PASS。

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/chat/pool.rs src-tauri/src/chat/mod.rs src-tauri/src/chat/manager.rs
git commit -m "feat(chat): add AgentPool keyed by agent_id"
```

---

## Task 4: `ChatManager` 改用 `AgentPool`，`send/abort/is_running/shutdown` 按 agent 寻址

**Files:**
- Modify: `src-tauri/src/chat/manager.rs`（`ChatManager` 字段：`client: OnceCell` → `pool: AgentPool`；`permission_watchers: Mutex<HashMap<AgentId, PermissionWatcher>>`；改写 `client()`→`client_for(agent_id)`、`running_client`→`running_client_for(agent_id)`、`send`、`abort`、`is_running`、`shutdown`、`restart_daemon`、`warm_up`）
- Test: `src-tauri/src/chat/manager.rs`（新增针对寻址的单测，用 `test_support`）

**Interfaces:**
- Consumes: `AgentPool`、`AgentId`、`DEFAULT_AGENT_ID`、`DaemonClient::new(..., session_id, ...)`（Task 2）
- Produces（公有方法签名变更）:
  - `pub async fn send(&self, agent_id: AgentId, method: String, params: Value) -> Result<String, String>`
  - `pub async fn abort(&self, agent_id: AgentId) -> Result<(), String>`
  - `pub async fn is_running(&self, agent_id: &AgentId) -> bool`
  - `pub async fn warm_up(&self, agent_id: AgentId) -> Result<(), String>`
  - `pub async fn shutdown_all(&self)`
  - `pub async fn restart_daemon(&self, agent_id: AgentId) -> Result<(), String>`

- [ ] **Step 1: 写失败测试（寻址隔离）**

```rust
#[tokio::test]
async fn send_routes_to_per_agent_client() {
    // 用 test_support 注入两个 fake，验证 send(agent) 命中对应 client、
    // abort(agent_a) 不影响 agent_b。具体断言基于 FakeDaemonClient 的计数字段。
    // （FakeDaemonClient 增加 AtomicUsize: send_calls / abort_calls 以便断言。）
}
```

> 先给 `FakeDaemonClient` 增加 `send_calls`/`abort_calls` 计数字段并在对应 trait 方法里自增，使断言可写实。

- [ ] **Step 2: 运行确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml chat::manager::tests::send_routes_to_per_agent_client`
Expected: 编译失败（`send` 签名不含 `agent_id`）。

- [ ] **Step 3: 写实现**

要点（基于现有 `manager.rs` 改）：

1. 字段：
```rust
pub struct ChatManager {
    app: AppHandle,
    pool: AgentPool,
    permission_watchers: Mutex<HashMap<AgentId, PermissionWatcher<tauri::Wry>>>,
}
```
`new` 初始化 `pool: AgentPool::new()`、`permission_watchers: Mutex::new(HashMap::new())`。

2. `client_for(agent_id)`：把原 `client()` 体搬进来，外层包 `self.pool.get_or_init(&agent_id, || async { ... })`。其中 `DaemonClient::new(...)` 传入 `agent_id.clone()` 作为 session_id；permission 子目录用 `resources::permission_dir(&self.app)?.join(&agent_id)`（每 agent 隔离），并 `create_dir_all`。事件 sink 里 emit 的 payload 追加 `"agentId": agent_id`。

3. permission watcher：每 agent 一个，watch 该 agent 的 permission 子目录、用 `agent_id` 作为 session：
```rust
let mut watchers = self.permission_watchers.lock().await;
if !watchers.contains_key(&agent_id) {
    let sub = resources::permission_dir(&self.app)?.join(&agent_id);
    std::fs::create_dir_all(&sub).ok();
    let w = PermissionWatcher::new(sub, agent_id.clone(), self.app.clone());
    w.start();
    watchers.insert(agent_id.clone(), w);
}
```

4. `send`：签名加 `agent_id`，内部 `let client = self.running_client_for(agent_id.clone()).await?;`，转发事件时 payload 追加 `"agentId": agent_id`（`chat://stream`/`chat://done`/`chat://message`/`chat://subagent-message` 都加）。

5. `abort(agent_id)`：`if let Some(c) = self.pool.get(&agent_id).await { c.abort().await } else { Ok(()) }`。

6. `is_running(&agent_id)`：`self.pool.get(agent_id).await.map(|c| c.is_running()).unwrap_or(false)`。

7. `shutdown_all`：遍历 `pool.ids()`，逐个 `stop()`。

8. `restart_daemon(agent_id)` / `warm_up(agent_id)`：同理按 id 取 client。

> SDK 安装/卸载相关方法（`install_sdk`/`uninstall_sdk`/`restart_daemon` 用于 SDK 刷新）：改为对**所有** pool 内 client 操作，或仅对默认 agent。本阶段：对所有 client 执行 restart（遍历 `pool.ids()`）。

- [ ] **Step 4: 运行确认通过**

Run: `cargo test --manifest-path src-tauri/Cargo.toml chat::manager`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/chat/manager.rs
git commit -m "refactor(chat): route ChatManager send/abort by agent_id via AgentPool"
```

---

## Task 5: Tauri 命令增加 `agent_id` 参数

**Files:**
- Modify: `src-tauri/src/commands/chat_commands.rs`（`chat_send`、`chat_abort`、`chat_is_running`、`chat_start_daemon`/warm_up、`chat_restart_daemon`）
- Modify: `src-tauri/src/lib.rs`（如有命令签名变更不需改注册，但确认 `generate_handler!` 仍包含它们）
- Test: 复用 manager 测试；命令层做编译级验证

**Interfaces:**
- Consumes: `ChatManager::send(agent_id, method, params)` 等（Task 4）
- Produces（命令签名，参数 camelCase）:
  - `chat_send(agent_id: Option<String>, method: String, params: Value, state) -> Result<String,String>`
  - `chat_abort(agent_id: Option<String>, state) -> Result<(),String>`
  - `chat_is_running(agent_id: Option<String>, state) -> Result<bool,String>`
  - `chat_start_daemon(agent_id: Option<String>, state) -> Result<(),String>`

> `Option<String>` + `sanitize_agent_id(unwrap_or(DEFAULT))` 保证旧前端不传也能工作（回退默认 agent）。

- [ ] **Step 1: 改命令签名与体**

对每个命令：
```rust
#[tauri::command]
pub async fn chat_send(
    agent_id: Option<String>,
    method: String,
    params: Value,
    state: State<'_, ChatState>,
) -> Result<String, String> {
    let agent = crate::chat::sanitize_agent_id(agent_id.as_deref().unwrap_or(crate::chat::DEFAULT_AGENT_ID));
    state.manager.send(agent, method, params).await
}
```
其余命令同构（`abort`/`is_running`/`start_daemon`）。

- [ ] **Step 2: 编译检查**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 通过（注意 `lib.rs` 里 `app.manager.shutdown()` 改为 `shutdown_all()`）。

- [ ] **Step 3: 全量后端测试**

Run: `cargo test --manifest-path src-tauri/Cargo.toml chat`
Expected: PASS。

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/commands/chat_commands.rs src-tauri/src/lib.rs
git commit -m "feat(chat): add agent_id param to chat commands (defaults to 'default')"
```

---

## Task 6: 前端——每个会话 tab 绑定 `agentId`，事件按 agentId 路由

**Files:**
- Modify: `src/types/chat.ts`（事件类型加 `agentId`）
- Modify: `src/services/`（封装 `chat_send` 的服务：传 `agentId`）
- Modify: `src/stores/useChatStore.ts`（每个 tab 生成稳定 `agentId`；监听 `chat://stream|done|message|subagent-message` 时优先按 `agentId` 命中 tab，回退现有 `requestId→tabKey` 映射）
- Test: `src/stores/useChatStore.test.ts`（或就近新建）

**Interfaces:**
- Consumes: 后端事件 payload 新增字段 `agentId`
- Produces: store 内 `tab.agentId`；`sendMessage` 调用 `chat_send({ agentId, method, params })`

- [ ] **Step 1: 写失败测试**

```ts
import { describe, it, expect } from 'vitest';
import { resolveTabForEvent } from './chatEventRouting'; // 新建纯函数

describe('resolveTabForEvent', () => {
  it('prefers agentId match over requestId map', () => {
    const tabs = [{ tabKey: 't1', agentId: 'a1' }, { tabKey: 't2', agentId: 'a2' }];
    const got = resolveTabForEvent(tabs, { agentId: 'a2', requestId: 'r9' }, new Map());
    expect(got).toBe('t2');
  });
  it('falls back to requestId map when agentId absent (旧事件)', () => {
    const tabs = [{ tabKey: 't1', agentId: 'a1' }];
    const map = new Map([['r9', 't1']]);
    const got = resolveTabForEvent(tabs, { requestId: 'r9' }, map);
    expect(got).toBe('t1');
  });
});
```

- [ ] **Step 2: 运行确认失败**

Run: `npx vitest run src/stores/chatEventRouting.test.ts`
Expected: FAIL（`resolveTabForEvent` / 文件不存在）。

> 若仓库未配置 vitest：本任务先加最小 vitest 配置（`npm i -D vitest`，`package.json` 加 `"test": "vitest"`），这是「新增能力补测试」的必要基础设施，折入本任务。

- [ ] **Step 3: 实现纯函数 + 接线**

`src/stores/chatEventRouting.ts`：
```ts
// 事件→tab 路由：优先 agentId（新），回退 requestId 映射（旧，向后兼容）。
export interface TabRef { tabKey: string; agentId: string; }
export interface IncomingEvent { agentId?: string; requestId?: string; }

export function resolveTabForEvent(
  tabs: TabRef[],
  ev: IncomingEvent,
  requestTabKeys: Map<string, string>,
): string | undefined {
  if (ev.agentId) {
    const hit = tabs.find((t) => t.agentId === ev.agentId);
    if (hit) return hit.tabKey;
  }
  if (ev.requestId) return requestTabKeys.get(ev.requestId);
  return undefined;
}
```
在 `useChatStore.ts`：新建 tab 时生成稳定 `agentId`（如 `crypto.randomUUID()`，存入 tab 状态）；`sendMessage` 把 `agentId` 传给 `chat_send`；所有 `chat://*` 监听用 `resolveTabForEvent(...)` 定位 tab。

- [ ] **Step 4: 运行确认通过 + 构建**

Run: `npx vitest run src/stores/chatEventRouting.test.ts`
Expected: PASS。
Run: `npm run build`
Expected: 类型检查 + 构建通过。

- [ ] **Step 5: 提交**

```bash
git add src/types/chat.ts src/services/ src/stores/useChatStore.ts src/stores/chatEventRouting.ts src/stores/chatEventRouting.test.ts package.json
git commit -m "feat(chat): bind agentId per tab and route events by agentId"
```

---

## Task 7: 端到端手测脚本 + 中文文档

**Files:**
- Modify: `docs/`（新增 `docs/helm-phase0-verification.md`）
- Modify: `CLAUDE.md` 或 `AGENTS.md`（如需补「多 agent daemon 池」运行说明，可选）

**Interfaces:** 无新代码接口；这是验证 + 文档任务。

- [ ] **Step 1: 写验证文档**

`docs/helm-phase0-verification.md` 写明手动 e2e 步骤：
1. `npm run tauri dev` 启动。
2. 在 Helm（对话）开 **两个 tab**，分别选不同工作目录（cwd）。
3. 同时向两个 tab 发长任务消息。
4. 预期：两个 tab 各自独立 daemon 进程（`ps` 可见多个 node ai-bridge），互不阻塞、互不串流；abort 其中一个不影响另一个。
5. 关闭 app：所有 daemon 进程退出（`shutdown_all`）。

- [ ] **Step 2: 执行手测并记录结果**

按文档跑一遍，把实际观察（进程数、是否并行、abort 隔离、退出清理）回填到文档「验证记录」小节。

> 注意（CLAUDE.md 记忆）：用户问「怎么测试」通常指手动 e2e，不是再跑一遍单测；本任务即手动 e2e。

- [ ] **Step 3: 提交**

```bash
git add docs/helm-phase0-verification.md
git commit -m "docs(chat): add Phase 0 multi-agent daemon pool verification guide"
```

---

## Self-Review（已执行，结论）

- **Spec 覆盖**：本计划覆盖设计文档 §6.1 RuntimePool/DaemonPool（Task 3-4）、§3 约束破除（per-agent cwd/session：Task 2-4）、§6 事件加 agentId（Task 4、6）。§6.2 WorktreeManager、§6.3 Supervisor、§6.4-6.5 Hermes 引擎、§9 多模型工作流、§10 UI 看板/扇出 = **Phase 1+ 范畴，本计划不含**，留待后续计划（见下）。
- **占位符扫描**：无 TBD/TODO；唯一「描述性」步骤为 Task 4 Step 3 与 Task 7（重构接线与手测），已给出具体字段/方法/payload 改动点。
- **类型一致性**：`AgentId`/`sanitize_agent_id`/`DEFAULT_AGENT_ID`（Task 1）→ `DaemonClient::new(session_id)`（Task 2）→ `AgentPool`（Task 3）→ `ChatManager::send(agent_id,…)`（Task 4）→ 命令 `agent_id: Option<String>`（Task 5）→ 前端 `agentId`（Task 6），命名贯穿一致。

## 后续计划（不在本文件）

- **Phase 1 计划**：`WorktreeManager`（`git worktree add/remove`）+ 每 tab 绑 worktree + 异构扇出对比视图。建议另起 `docs/superpowers/plans/<date>-helm-phase1-worktree-fanout.md`。
- **Phase 2 计划**：`AgentRuntime` 契约 + `CliRuntime` + Hermes 引擎（SQLite/Coordinator/Planner）+ WorkerSupervisor 判活。移植 orca 数据模型与 coordinator 循环。
- **Phase 3 计划**：完整 DAG + Gate + Message 总线 + LLM-judge。

详见设计文档 `docs/superpowers/specs/2026-06-27-helm-hermes-design.md` §13。
