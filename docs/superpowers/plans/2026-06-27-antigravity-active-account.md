# Antigravity 激活态单一真相模型 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 Antigravity 账号激活态由单一真相文件驱动,消除 SQLite `is_active` 与外部 `accounts.json` 两份真相不一致的 bug。

**Architecture:** 仿照上游 Antigravity-Manager 的「单一真相源 + 原子切换」模型。新增 jadekit 自己的真相文件 `~/.jadekit/antigravity/current-account.json`。`list_accounts` 读时惰性对账(静默修正 DB `is_active`),`switch_account`/`delete_account`/`add_account` 写时同步更新真相文件 + DB。删除对外部 `~/.antigravity_tools/accounts.json` 的双写。

**Tech Stack:** Rust 2021 + Tauri 2 + rusqlite + serde_json + chrono + tempfile(dev)。测试用 `tempfile::tempdir()` 隔离,不碰真实 `~/.jadekit/`。

## Global Constraints

- 真相文件路径固定为 `~/.jadekit/antigravity/current-account.json`(用 `dirs::home_dir()` 拼接,跨平台)。
- 真相文件 JSON 字段:`currentAccountId: String`、`updatedAt: i64`(Unix 秒)。
- 真相文件不存在 / 解析失败 → `Ok(None)`,**绝不报错、绝不阻塞列表**(惰性兜底)。
- 写真相文件必须 `temp → rename` 原子写(对齐现有 `write_antigravity_manager_accounts` 写法)。
- 不再写 `~/.antigravity_tools/accounts.json`(不兼容独立 Antigravity Manager app)。
- 不引入 keychain / userinfo 网络探测。
- 前端零改动;后端对账后 `isActive` 字段正确,5 处展示面自动回归。
- JSON 序列化 camelCase(jadekit 全局约定)。
- 复用现有 `db.set_active_antigravity_account(id)`(`antigravity_accounts.rs:96`,已是原子事务)。
- 每个 Task 结尾提交;commit message 末尾加 `Co-Authored-By: Claude <noreply@anthropic.com>`。
- `StatusPanel.tsx`(用户在别处改的)和 `.codegraph/` **不要碰、不要提交**。每个 Task 的 `git add` 必须**显式列出文件**,禁止 `git add .` / `git add -A`。

---

## File Structure

| 文件 | 责任 | 动作 |
|---|---|---|
| `src-tauri/src/services/ag_current_account.rs` | 真相文件读写:`get_current_account_id` / `set_current_account_id` / `clear_current_account_id` | **Create** |
| `src-tauri/src/services/mod.rs` | 注册新模块 | **Modify**(:1 加 `pub mod ag_current_account;`) |
| `src-tauri/src/services/antigravity_service.rs` | `list_accounts` 加对账、`switch_account` 重排写回、`delete_account`/`add_account` 同步真相文件、删外部双写函数 | **Modify** |
| `src-tauri/Cargo.toml` | 加 `tempfile` dev-dependency | **Modify** |
| `src-tauri/src/services/ag_current_account.rs` 末尾 `#[cfg(test)]` | 真相文件读写单测 | **Create**(随文件) |
| `src-tauri/src/services/antigravity_service.rs` 末尾 `#[cfg(test)]` | 对账 / 切换原子性单测 | **Create** |

**不改**:前端任何文件、`src-tauri/src/database/dao/antigravity_accounts.rs`(DAO 已够用)、`src-tauri/src/commands/antigravity_commands.rs`(入口签名不变)。

---

### Task 1: 真相文件读写模块 + tempfile 依赖

**Files:**
- Create: `src-tauri/src/services/ag_current_account.rs`
- Modify: `src-tauri/src/services/mod.rs:1`
- Modify: `src-tauri/Cargo.toml`

**Interfaces:**
- Consumes: `crate::database::Database`(只用于 `set_current_account_id` 刷 DB)、`std::sync::Arc`、`dirs`、`serde_json`、`chrono`。
- Produces(后续 Task 依赖这些签名,类型必须完全一致):
  - `pub fn current_account_file_path() -> Result<std::path::PathBuf, String>` — 返回 `~/.jadekit/antigravity/current-account.json` 路径,确保父目录存在。
  - `pub fn get_current_account_id() -> Result<Option<String>, String>` — 读真相文件,缺失/损坏返回 `Ok(None)`。
  - `pub fn set_current_account_id(db: &Arc<Database>, id: &str) -> Result<(), String>` — 原子写文件 + 刷 DB `is_active`。
  - `pub fn clear_current_account_id() -> Result<(), String>` — 把 `currentAccountId` 置 null(删除当前账号无后继时用)。

- [ ] **Step 1: 加 tempfile dev-dependency**

在 `src-tauri/Cargo.toml` 的 `[dev-dependencies]` 段(若不存在则在 `[dependencies]` 之后新建)加:

```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: 注册模块**

在 `src-tauri/src/services/mod.rs` 第 1 行(在 `pub mod ag_integration;` 之前,保持字母序)加:

```rust
pub mod ag_current_account;
```

- [ ] **Step 3: 写失败测试(整个模块文件含 test)**

创建 `src-tauri/src/services/ag_current_account.rs`,内容:

```rust
//! Antigravity 当前账号的「单一真相源」文件读写。
//!
//! 真相文件:`~/.jadekit/antigravity/current-account.json`
//! 仿照上游 Antigravity-Manager 的 `AccountIndex.current_account_id`,
//! 但用 jadekit 自己的文件,且通过环境变量 `JADEKIT_AG_CURRENT_FILE`
//! 覆盖路径以便测试隔离。

use crate::database::Database;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurrentAccountFile {
    /// 当前账号 id;None 表示无当前账号(文件存在但已清空)。
    #[serde(default)]
    current_account_id: Option<String>,
    #[serde(default)]
    updated_at: i64,
}

/// 真相文件路径。
///
/// 优先读环境变量 `JADEKIT_AG_CURRENT_FILE`(测试用);否则用
/// `~/.jadekit/antigravity/current-account.json`,并确保父目录存在。
pub fn current_account_file_path() -> Result<PathBuf, String> {
    if let Ok(custom) = std::env::var("JADEKIT_AG_CURRENT_FILE") {
        return Ok(PathBuf::from(custom));
    }
    let home = dirs::home_dir().ok_or_else(|| "Home directory not found".to_string())?;
    let dir = home.join(".jadekit").join("antigravity");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create ag dir: {}", e))?;
    Ok(dir.join("current-account.json"))
}

/// 读真相文件。文件缺失 / 解析失败 → `Ok(None)`,绝不报错。
pub fn get_current_account_id() -> Result<Option<String>, String> {
    let path = match current_account_file_path() {
        Ok(p) => p,
        Err(_) => return Ok(None),
    };
    if !path.exists() {
        return Ok(None);
    }
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Ok(None),
    };
    let parsed: CurrentAccountFile = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    Ok(parsed.current_account_id)
}

/// 原子写真相文件 + 刷 DB `is_active`。
///
/// 顺序:先写文件(temp → rename),再 `db.set_active_antigravity_account`。
/// 文件写失败 → 直接返回错误,DB 不动(切换未成功不该改激活态)。
pub fn set_current_account_id(db: &Arc<Database>, id: &str) -> Result<(), String> {
    let path = current_account_file_path()?;
    let payload = CurrentAccountFile {
        current_account_id: Some(id.to_string()),
        updated_at: chrono::Utc::now().timestamp(),
    };
    write_atomic(&path, &payload)?;
    db.set_active_antigravity_account(id)?;
    Ok(())
}

/// 把真相文件的 `currentAccountId` 置 null(删除当前账号且无后继时用)。
pub fn clear_current_account_id() -> Result<(), String> {
    let path = current_account_file_path()?;
    let payload = CurrentAccountFile {
        current_account_id: None,
        updated_at: chrono::Utc::now().timestamp(),
    };
    write_atomic(&path, &payload)
}

fn write_atomic(path: &std::path::Path, payload: &CurrentAccountFile) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create dir: {}", e))?;
    }
    let tmp = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(payload)
        .map_err(|e| format!("Failed to serialize current-account.json: {}", e))?;
    std::fs::write(&tmp, &bytes).map_err(|e| format!("Failed to write tmp file: {}", e))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("Failed to rename tmp file: {}", e)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 在隔离 tempdir 下设置真相文件路径,返回临时目录(测试期间存活)。
    fn with_isolated_file() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        // 子进程级 env var;测试串行跑无妨,用唯一路径避免串扰。
        let path = dir.path().join("current-account.json");
        std::env::set_var("JADEKIT_AG_CURRENT_FILE", &path);
        dir
    }

    #[test]
    fn get_returns_none_when_file_missing() {
        let _dir = with_isolated_file();
        assert_eq!(get_current_account_id().unwrap(), None);
    }

    #[test]
    fn get_returns_none_when_file_corrupt() {
        let dir = with_isolated_file();
        let path = dir.path().join("current-account.json");
        std::fs::write(&path, b"not json{{{").unwrap();
        assert_eq!(get_current_account_id().unwrap(), None);
    }

    #[test]
    fn write_then_read_roundtrip() {
        let _dir = with_isolated_file();
        // 直接测 write_atomic + get,绕过 DB
        let path = current_account_file_path().unwrap();
        let payload = CurrentAccountFile {
            current_account_id: Some("acc-1".to_string()),
            updated_at: 123,
        };
        write_atomic(&path, &payload).unwrap();
        assert_eq!(get_current_account_id().unwrap(), Some("acc-1".to_string()));
    }

    #[test]
    fn clear_sets_current_to_none() {
        let _dir = with_isolated_file();
        let path = current_account_file_path().unwrap();
        let payload = CurrentAccountFile {
            current_account_id: Some("acc-1".to_string()),
            updated_at: 123,
        };
        write_atomic(&path, &payload).unwrap();
        clear_current_account_id().unwrap();
        assert_eq!(get_current_account_id().unwrap(), None);
    }
}
```

- [ ] **Step 4: 跑测试,验证通过**

Run: `cd src-tauri && cargo test ag_current_account -- --nocapture`
Expected: 4 tests PASS(get_returns_none_when_file_missing / get_returns_none_when_file_corrupt / write_then_read_roundtrip / clear_sets_current_to_none)。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/services/ag_current_account.rs src-tauri/src/services/mod.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "$(cat <<'EOF'
feat(ag): 新增当前账号真相文件读写模块

仿照上游 Antigravity-Manager 的 current_account_id 单一真相源,
jadekit 自己的真相文件 ~/.jadekit/antigravity/current-account.json。
get/set/clear 三函数,文件缺失或损坏静默兜底 Ok(None)。

Co-Authored-By: Claude <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: `list_accounts` 惰性对账(修 Bug A + Bug C 展示面)

**Files:**
- Modify: `src-tauri/src/services/antigravity_service.rs:391-393`(改 `list_accounts`)

**Interfaces:**
- Consumes: `crate::services::ag_current_account::get_current_account_id`、`crate::database::Database::set_active_antigravity_account`(:96)、`crate::database::Database::list_antigravity_accounts`(:11)。
- Produces: `list_accounts` 返回的 `Vec<AntigravityAccount>` 的 `is_active` 字段已与真相文件对账(前端 5 处展示面自动正确)。

- [ ] **Step 1: 写失败测试**

在 `src-tauri/src/services/antigravity_service.rs` 文件**最末尾**追加 test 模块(若已有 `#[cfg(test)]` 则并入):

```rust
#[cfg(test)]
mod active_account_tests {
    use super::*;
    use crate::database::Database;
    use crate::models::antigravity::AntigravityAccount;

    /// 建一个内存 SQLite + 两个账号,A 活跃、B 不活跃。
    fn setup_two_accounts() -> (Arc<Database>, AntigravityAccount, AntigravityAccount, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("current-account.json");
        std::env::set_var("JADEKIT_AG_CURRENT_FILE", &path);
        let db = Arc::new(Database::new_in_memory().expect("in-memory db"));
        let mut a = base_account("a@x.com");
        a.is_active = true;
        let mut b = base_account("b@x.com");
        b.is_active = false;
        db.upsert_antigravity_account(&a).unwrap();
        db.upsert_antigravity_account(&b).unwrap();
        (db, a, b, dir)
    }

    fn base_account(email: &str) -> AntigravityAccount {
        AntigravityAccount {
            id: email.to_string(), // 测试用 email 当 id,够用
            email: email.to_string(),
            name: None,
            access_token: "tok".into(),
            refresh_token: "rt".into(),
            expires_in: 3600,
            expiry_timestamp: chrono::Utc::now().timestamp() + 3600,
            oauth_client_key: None,
            project_id: None,
            subscription_tier: None,
            custom_label: None,
            is_active: false,
            disabled: false,
            disabled_reason: None,
            quota: None,
            device_profile: None,
            created_at: 0,
            last_used: 0,
            order_index: 0,
        }
    }

    #[test]
    fn list_reconciles_when_truth_file_disagrees() {
        let (db, _a, b, _dir) = setup_two_accounts();
        // 真相文件说是 b,DB 说 a —— list_accounts 应把 b 刷成活跃、a 不活跃
        crate::services::ag_current_account::current_account_file_path()
            .map(|p| {
                let payload = serde_json::json!({"currentAccountId": b.id, "updatedAt": 1});
                std::fs::write(&p, payload.to_string()).unwrap();
            })
            .unwrap();

        let accounts = list_accounts(&db).unwrap();
        let got_b = accounts.iter().find(|a| a.id == b.id).unwrap();
        let got_a = accounts.iter().find(|a| a.id == "a@x.com").unwrap();
        assert!(got_b.is_active, "b should be active after reconcile");
        assert!(!got_a.is_active, "a should be inactive after reconcile");

        // DB 也被静默修正
        let db_accounts = db.list_antigravity_accounts().unwrap();
        let db_b = db_accounts.iter().find(|a| a.id == b.id).unwrap();
        assert!(db_b.is_active, "DB should also reflect b active");
    }

    #[test]
    fn list_falls_back_to_db_when_truth_file_missing() {
        let (db, _a, _b, _dir) = setup_two_accounts();
        // 不写真相文件 → get_current_account_id 返回 None → 返回 DB 现状
        let accounts = list_accounts(&db).unwrap();
        let got_a = accounts.iter().find(|a| a.id == "a@x.com").unwrap();
        assert!(got_a.is_active, "fall back to DB is_active, a stays active");
    }
}
```

- [ ] **Step 2: 跑测试,验证失败**

Run: `cd src-tauri && cargo test list_reconciles_when_truth_file_disagrees -- --nocapture`
Expected: 编译失败或 FAIL —— 因为 `list_accounts` 还没对账逻辑,`list_reconciles_when_truth_file_disagrees` 会断言失败(b 仍不活跃)。
同时确认 `Database::new_in_memory` 是否存在(见 Step 3 注意项)。

- [ ] **Step 3: 实现对账逻辑**

把 `antigravity_service.rs:391-393`:

```rust
pub fn list_accounts(db: &Arc<Database>) -> Result<Vec<AntigravityAccount>, String> {
    db.list_antigravity_accounts()
}
```

改为:

```rust
pub fn list_accounts(db: &Arc<Database>) -> Result<Vec<AntigravityAccount>, String> {
    let mut accounts = db.list_antigravity_accounts()?;

    // 惰性对账:以真相文件为权威,静默修正 DB is_active。
    // 真相文件缺失/损坏 → get_current_account_id 返回 None → 跳过,返回 DB 现状(兜底,不阻塞)。
    if let Ok(Some(current_id)) = crate::services::ag_current_account::get_current_account_id() {
        let needs_fix = accounts
            .iter()
            .any(|a| (a.id == current_id) != a.is_active);
        if needs_fix {
            if let Err(e) = db.set_active_antigravity_account(&current_id) {
                tracing::warn!("Failed to reconcile is_active with truth file: {}", e);
            } else {
                for a in accounts.iter_mut() {
                    a.is_active = a.id == current_id;
                }
            }
        }
    }

    Ok(accounts)
}
```

> **注意 `new_in_memory`**:测试用了 `Database::new_in_memory()`。先 `grep -n "fn new_in_memory\|fn new\b" src-tauri/src/database/mod.rs` 确认是否存在;若不存在,在本 Step 顺便在 `Database` impl 里加一个最简实现(返回 `:memory:` 连接),并把它的实现放进 Task 1 的依赖说明。若已存在则跳过。

- [ ] **Step 4: 跑测试,验证通过**

Run: `cd src-tauri && cargo test active_account_tests -- --nocapture`
Expected: 2 tests PASS。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/services/antigravity_service.rs
# 若补了 new_in_memory,也加上 database/mod.rs:
# git add src-tauri/src/database/mod.rs
git commit -m "$(cat <<'EOF'
fix(ag): list_accounts 惰性对账真相文件,修正 is_active

读 list 时以 ~/.jadekit/antigravity/current-account.json 为权威,
静默把 DB is_active 刷成一致,真相文件缺失/损坏回退 DB 现状不阻塞。
前端 5 处展示面(AccountCard/DetailsDialog/Page/Dashboard/后端)随之正确。

Co-Authored-By: Claude <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: `switch_account` 写回原子化 + 删外部双写(修 Bug B)

**Files:**
- Modify: `src-tauri/src/services/antigravity_service.rs:635-706`(`switch_account`)
- Modify: `src-tauri/src/services/antigravity_service.rs:708-792`(删 `set_antigravity_manager_current_account` / `restore_antigravity_manager_current_account` / `write_antigravity_manager_accounts`)

**Interfaces:**
- Consumes: `crate::services::ag_current_account::set_current_account_id`。
- Produces: `switch_account` 切换成功后真相文件与 DB 在同一原子调用内一致;失败则两者都不动。

- [ ] **Step 1: 写失败测试**

在 Task 2 的 `active_account_tests` 模块里追加(复用 `setup_two_accounts` / `base_account`):

```rust
    #[test]
    fn switch_updates_truth_file_and_db_atomically() {
        let (db, _a, b, _dir) = setup_two_accounts();
        // 直接验证「写真相」这一步的对外效果:set_current_account_id 后文件与 DB 一致
        crate::services::ag_current_account::set_current_account_id(&db, &b.id).unwrap();

        // 真相文件 = b
        let cid = crate::services::ag_current_account::get_current_account_id().unwrap();
        assert_eq!(cid.as_deref(), Some(b.id.as_str()));
        // DB = b 活跃、a 不活跃
        let db_accounts = db.list_antigravity_accounts().unwrap();
        assert!(db_accounts.iter().find(|a| a.id == b.id).unwrap().is_active);
        assert!(!db_accounts.iter().find(|a| a.id == "a@x.com").unwrap().is_active);
    }
```

- [ ] **Step 2: 跑测试,验证状态**

Run: `cd src-tauri && cargo test switch_updates_truth_file_and_db_atomically -- --nocapture`
Expected: 该测试应已 PASS(Task 1 的 `set_current_account_id` 已实现)。它作为回归保护,确保 Task 3 重构 `switch_account` 时真相写入语义不破坏。

- [ ] **Step 3: 重构 `switch_account` 写回段**

把 `antigravity_service.rs:635-706` 整个 `switch_account` 函数体中,**写回相关段落**替换。原结构(伪码):

```rust
// 原 :651-664 — 先写外部 accounts.json(删掉)
let previous_manager_account_id = match set_antigravity_manager_current_account(&account.email) { ... };
// 原 :666-691 — 进程操作(execute_local_switch),失败时 restore 外部文件(改成失败直接返回)
let switch_data = ...;
tokio::task::spawn_blocking(... execute_local_switch ...)...?;
// 原 :694 — db.set_active_antigravity_account(id)?;  改成 set_current_account_id
```

改成(保留 1 刷新 token / 5 剪贴板 / 6 日志这些副作用,只重排写回):

```rust
pub async fn switch_account(
    db: &Arc<Database>,
    id: &str,
    target_ide: Option<&str>,
) -> Result<(), String> {
    let mut account = get_account(db, id)?;
    if account.disabled {
        return Err(format!("Account is disabled: {}", account.email));
    }

    // 1. Ensure token is fresh before switching
    if account.is_token_expired() {
        tracing::info!("Token expired, refreshing before switch...");
        account = refresh_account_token(db, id).await?;
    }

    // 2. Execute local app switch: close → inject credentials → restart
    //    进程操作在前,失败直接返回 —— 真相文件与 DB 都不动(切换没成功不改激活态)。
    let switch_data = crate::services::ag_integration::SwitchAccountData {
        email: account.email.clone(),
        access_token: account.access_token.clone(),
        refresh_token: account.refresh_token.clone(),
        expiry_timestamp: account.expiry_timestamp,
    };
    let ide = target_ide.map(|s| s.to_string());
    tokio::task::spawn_blocking(move || {
        crate::services::ag_integration::execute_local_switch(&switch_data, ide.as_deref())
    })
    .await
    .map_err(|e| format!("Switch task panicked: {}", e))??;

    // 3. 进程切换成功后,原子写真相文件 + 刷 DB is_active(同一调用内一致)
    crate::services::ag_current_account::set_current_account_id(db, id)?;

    // 4. Copy refresh_token to system clipboard
    if let Err(e) = copy_refresh_token_to_clipboard(&account.refresh_token) {
        tracing::warn!("Failed to copy refresh token to clipboard: {}", e);
    }

    if let Err(e) = db.log_ag_operation(&account.id, &account.email, "account_switch", target_ide) {
        tracing::warn!("Failed to log account_switch operation: {}", e);
    }

    Ok(())
}
```

- [ ] **Step 4: 删除外部双写函数**

删除 `antigravity_service.rs:708-792` 的三个函数:
- `set_antigravity_manager_current_account`
- `restore_antigravity_manager_current_account`
- `write_antigravity_manager_accounts`

删除后,`grep -n "set_antigravity_manager_current_account\|restore_antigravity_manager_current_account\|write_antigravity_manager_accounts" src-tauri/src/` 应**零命中**。

- [ ] **Step 5: 编译 + 跑全部 ag 测试**

Run: `cd src-tauri && cargo build 2>&1 | tail -20`
Expected: 编译通过(无 unused / unresolved)。

Run: `cd src-tauri && cargo test ag_ -- --nocapture`
Expected: Task 1~3 全部测试 PASS。

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/services/antigravity_service.rs
git commit -m "$(cat <<'EOF'
fix(ag): switch_account 写回原子化,删外部 accounts.json 双写

进程操作在前(失败即返回不动状态),set_current_account_id 在后
原子写真相文件 + 刷 DB is_active,消除两份真相不一致窗口。
删除 set/restore/write_antigravity_manager_current_account 三个
对外部 ~/.antigravity_tools/accounts.json 的双写函数(不兼容独立 app)。

Co-Authored-By: Claude <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: `delete_account` / `add_account` 同步真相文件(修 Bug C 收尾)

**Files:**
- Modify: `src-tauri/src/services/antigravity_service.rs:471-488`(`delete_account`)
- Modify: `src-tauri/src/services/antigravity_service.rs:459-468`(`add_account` 末尾段)

**Interfaces:**
- Consumes: `crate::services::ag_current_account::{set_current_account_id, clear_current_account_id, get_current_account_id}`。
- Produces: 删当前账号时真相文件回退/清空;首个账号入库时真相文件指向它。

- [ ] **Step 1: 写失败测试**

在 `active_account_tests` 模块追加:

```rust
    #[test]
    fn delete_active_account_falls_back_and_updates_truth_file() {
        let (db, _a, b, _dir) = setup_two_accounts();
        // 让 b 成为真相文件的当前账号
        crate::services::ag_current_account::set_current_account_id(&db, &b.id).unwrap();
        assert_eq!(
            crate::services::ag_current_account::get_current_account_id().unwrap().as_deref(),
            Some(b.id.as_str())
        );

        // 删 b(当前账号)→ 真相文件应回退到剩余账号 a
        delete_account(&db, &b.id).unwrap();

        let cid = crate::services::ag_current_account::get_current_account_id().unwrap();
        assert_eq!(cid.as_deref(), Some("a@x.com"), "truth file should fall back to a");
    }

    #[test]
    fn delete_only_account_clears_truth_file() {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("JADEKIT_AG_CURRENT_FILE", dir.path().join("current-account.json"));
        let db = Arc::new(Database::new_in_memory().unwrap());
        let only = base_account("only@x.com");
        db.upsert_antigravity_account(&only).unwrap();
        crate::services::ag_current_account::set_current_account_id(&db, &only.id).unwrap();

        delete_account(&db, &only.id).unwrap();
        assert_eq!(
            crate::services::ag_current_account::get_current_account_id().unwrap(),
            None,
            "no accounts left → truth file cleared"
        );
    }
```

- [ ] **Step 2: 跑测试,验证失败**

Run: `cd src-tauri && cargo test delete_active_account_falls_back_and_updates_truth_file -- --nocapture`
Expected: FAIL —— 删 b 后真相文件仍是 b(因为 `delete_account` 还没同步真相文件)。

- [ ] **Step 3a: 给 DAO 加「只清零」函数**

无后继账号时只需把 DB `is_active` 全置 0(现有 `set_active_antigravity_account(id)` 会因 id 找不到而报错,见 `antigravity_accounts.rs:107-109`)。先确认 `grep -n "clear_antigravity_active\|set_active_antigravity_account" src-tauri/src/database/dao/antigravity_accounts.rs`,若无 `clear_antigravity_active`,在 `set_active_antigravity_account` 函数(`:96`)之后追加:

```rust
/// 把所有账号 is_active 置 0(删除当前账号且无后继时用)。
pub fn clear_antigravity_active(&self) -> Result<(), String> {
    let conn = lock_conn!(self.conn);
    conn.execute("UPDATE antigravity_accounts SET is_active = 0", [])
        .map_err(|e| e.to_string())?;
    Ok(())
}
```

- [ ] **Step 3b: 改 `delete_account`**

把 `antigravity_service.rs:471-488` 改为:

```rust
pub fn delete_account(db: &Arc<Database>, id: &str) -> Result<(), String> {
    let account = db
        .get_antigravity_account(id)?
        .ok_or_else(|| format!("Account not found: {}", id))?;

    let is_current =
        crate::services::ag_current_account::get_current_account_id()?.as_deref() == Some(id);

    if is_current || account.is_active {
        let all = db.list_antigravity_accounts()?;
        match all.iter().find(|a| a.id != id) {
            Some(next) => {
                // 回退到下一个账号:同步真相文件 + DB
                crate::services::ag_current_account::set_current_account_id(db, &next.id)?;
            }
            None => {
                // 无后继账号:删完清空真相文件 + DB is_active
                db.clear_antigravity_active()?;
                crate::services::ag_current_account::clear_current_account_id()?;
            }
        }
    }

    if let Err(e) = db.log_ag_operation(&account.id, &account.email, "account_deleted", None) {
        tracing::warn!("Failed to log account_deleted operation: {}", e);
    }
    db.delete_antigravity_account(id)?;
    Ok(())
}
```

- [ ] **Step 4: 改 `add_account` 首账号入库**

`add_account` 现有 `:459-462`:

```rust
    let existing = db.list_antigravity_accounts()?;
    if existing.is_empty() {
        account.is_active = true;
    }
```

改为(首个账号入库时,写真相文件指向它):

```rust
    let existing = db.list_antigravity_accounts()?;
    let is_first = existing.is_empty();
    if is_first {
        account.is_active = true;
    }
```

然后在 `:464 db.upsert_antigravity_account(&account)?;` **之后**追加(首账号才写真相):

```rust
    if is_first {
        if let Err(e) = crate::services::ag_current_account::set_current_account_id(db, &account.id) {
            tracing::warn!("Failed to set truth file for first account: {}", e);
        }
    }
```

> `add_account` 是 `async fn`,内部已有网络调用(refresh/userinfo);真相文件写失败只 `warn`,不阻塞入库(与现有 quota 失败处理一致,见 `:454-457`)。

- [ ] **Step 5: 跑测试,验证通过**

Run: `cd src-tauri && cargo test active_account_tests -- --nocapture`
Expected: 全部 PASS(含 Task 4 两个新测试)。

- [ ] **Step 6: 全量编译 + 测试回归**

Run: `cd src-tauri && cargo build 2>&1 | tail -20`
Expected: 编译通过。

Run: `cd src-tauri && cargo test 2>&1 | tail -30`
Expected: 全部测试 PASS,无回归。

- [ ] **Step 7: 提交**

```bash
git add src-tauri/src/services/antigravity_service.rs src-tauri/src/database/dao/antigravity_accounts.rs
git commit -m "$(cat <<'EOF'
fix(ag): delete/add_account 同步真相文件

删当前账号时真相文件回退到下一个账号,无后继则清空;
首个账号入库时真相文件指向它。与 list_accounts 对账形成闭环。

Co-Authored-By: Claude <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: 手动验证 + 收尾

**Files:** 无代码改动,仅运行验证。

- [ ] **Step 1: 启动 dev,验证正常切换**

Run: `npm run tauri dev`
手动操作(参考 spec §测试·手动验证清单):
1. Antigravity 页面切账号 X → 列表 / Dashboard / 详情弹窗都显示 X 活跃。
2. 检查 `~/.jadekit/antigravity/current-account.json` 内容 = X 的 id。
3. 重启 app → 活跃态与切换后一致。

- [ ] **Step 2: 验证进程切换失败不破坏状态**

人为制造失败(如临时重命名 ide 可执行文件路径,或在 `execute_local_switch` 前断网)→ 切换报错 → 活跃态**不变**,`current-account.json` 与 DB 一致(都保持原值)。

- [ ] **Step 3: 验证删当前账号**

删掉活跃账号 → 活跃态回退到第一个,`current-account.json` 更新为第一个的 id;不出现「无活跃」或「两个都活」。

- [ ] **Step 4: 验证文件损坏兜底**

手动把 `~/.jadekit/antigravity/current-account.json` 写成乱码 → 打开账号列表 → 不崩,回退 DB 现状。

- [ ] **Step 5: 数据展示面回归确认**

逐一确认 4 处 UI 一致:`AccountCard.tsx`(高亮/徽章)、`AccountDetailsDialog.tsx`(状态点)、`AntigravityPage.tsx`(「活跃」徽章)、`Dashboard.tsx:233`(当前账号邮箱)。

- [ ] **Step 6: 确认无遗留外部双写引用**

Run: `grep -rn "antigravity_tools" src-tauri/src`
Expected: **零命中**(确认外部 `accounts.json` 双写已彻底移除;`get_db_path` 那个 legacy state.vscdb 是另一回事,如有命中需人工确认是否本次范围)。

Run: `cd src-tauri && cargo clippy 2>&1 | tail -20`
Expected: 无 error(warn 可接受)。

- [ ] **Step 7: 最终提交(若有 clippy 修复)**

若 Step 6 产生任何修复:

```bash
git add <具体修复的文件>
git commit -m "chore(ag): clippy 修复 / 收尾

Co-Authored-By: Claude <noreply@anthropic.com>"
```

若无修复,跳过本步。任务完成。
