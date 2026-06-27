# Antigravity 设备指纹与切换登录修复 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 对齐上游 Antigravity-Manager 4.2.7 的设备指纹机制,修复 jadekit 切换账号后 ide 登录失败的 bug(token 写对了但 storage.json 设备指纹未写入)。

**Architecture:** 移植上游 `device` 模块到 jadekit:为每个账号生成并绑定一套 `DeviceProfile`(machineId/macMachineId/devDeviceId/sqmId),切换账号时在写 keychain token 之后,把该账号的指纹写入 ide 的 `storage.json` 的 `telemetry` 字段(嵌套 + 扁平两种格式都写)。Antigravity / Antigravity IDE 两个 app 各自的 storage.json 路径都覆盖。复用 jadekit 已有的 `device_profile` DB 字段(已存在但恒为 None)。

**Tech Stack:** Rust 2021 + serde_json + uuid(已有)+ rand(新增)+ tempfile(dev,已有)。测试用 `tempfile::tempdir()` 隔离 storage.json。

## Global Constraints

- 对齐目标:上游 Antigravity-Manager **v4.2.7**(`acc633c1`),路径 `/Users/jiaxing/code/github/Antigravity-Manager`。
- 设备指纹四字段(与上游 `DeviceProfile` 完全一致,且与 jadekit 现有 `AntigravityDeviceProfile` 字段一致):`machine_id`、`mac_machine_id`、`dev_device_id`、`sqm_id`。
- `storage.json` 路径由 `target_ide` 区分:`Some("ide")` → `Antigravity IDE`,其他 → `Antigravity`。macOS 标准路径 `~/Library/Application Support/<App>/User/globalStorage/storage.json`。
- 写 `telemetry` 时**同时写嵌套**(`telemetry.machineId`)和**扁平**(`telemetry.machineId` 顶层 key)两种格式(对齐上游 `write_profile`,兼容新旧 ide)。
- 指纹生成规则(对齐上游 `generate_profile`):
  - `machine_id` = `"auth0|user_" + random_hex(32)`(32 位小写字母数字)
  - `mac_machine_id` = `new_standard_machine_id()`(UUID v4 变体格式 `xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx`)
  - `dev_device_id` = `Uuid::new_v4()`
  - `sqm_id` = `"{大写UUID}"`
- 账号↔指纹绑定:**首次切换某账号时生成并存 DB**,之后每次切换**复用同一套指纹**(稳定,不每次重生成)。
- **保留不动**:已完成的真相文件 `~/.jadekit/antigravity/current-account.json` + `list_accounts` 惰性对账 + `switch_account` 原子写回(它们解决内部一致性,正确)。
- 每个 Task 结尾提交;commit message 末尾加 `Co-Authored-By: Claude <noreply@anthropic.com>`。
- `StatusPanel.tsx`(用户在别处改的)和 `.codegraph/` **不要碰、不要提交**。每个 Task 的 `git add` 必须**显式列出文件**,禁止 `git add .` / `git add -A`。
- 测试命令:`cd /Users/jiaxing/code/github/jadekit/src-tauri && cargo test <filter> -- --nocapture`。
- 并行测试安全:沿用 `ag_current_account::set_test_override` 的线程本地覆盖模式,**不要用 `std::env::set_var`**(并行 race)。

---

## File Structure

| 文件 | 责任 | 动作 |
|---|---|---|
| `src-tauri/Cargo.toml` | 加 `rand` 依赖 | **Modify** |
| `src-tauri/src/services/mod.rs` | 注册 `ag_device` 模块 | **Modify** |
| `src-tauri/src/services/ag_device.rs` | 设备指纹:生成 / 读 / 写 / storage.json 路径 | **Create** |
| `src-tauri/src/services/antigravity_service.rs` | `switch_account` 加指纹生成绑定;调用 `execute_local_switch` 前确保指纹 | **Modify** |
| `src-tauri/src/services/ag_integration.rs` | `execute_local_switch` 写 keychain 后写 `storage.json` 指纹 | **Modify** |

**不改**:前端、`models/antigravity.rs`(`AntigravityDeviceProfile` 字段已对齐)、DAO(`device_profile_json` 列已存在)、`database/schema.rs`。

---

### Task 1: device 模块 —— 指纹生成 + storage.json 路径 + 写入 + rand 依赖

**Files:**
- Modify: `src-tauri/Cargo.toml`(加 `rand`)
- Modify: `src-tauri/src/services/mod.rs`(注册 `ag_device`)
- Create: `src-tauri/src/services/ag_device.rs`

**Interfaces:**
- Consumes: `crate::models::antigravity::AntigravityDeviceProfile`、`uuid`、`rand`、`serde_json`、`dirs`。
- Produces(后续 Task 依赖,签名必须完全一致):
  - `pub fn generate_profile() -> AntigravityDeviceProfile` — 生成一套全新随机指纹。
  - `pub fn get_storage_path(target_ide: Option<&str>) -> Result<std::path::PathBuf, String>` — 返回 ide 的 `storage.json` 路径(区分 Antigravity / Antigravity IDE),不存在则 Err。**测试用线程本地覆盖** `set_test_storage_override`。
  - `pub fn write_profile(storage_path: &std::path::Path, profile: &AntigravityDeviceProfile) -> Result<(), String>` — 把指纹写入 storage.json 的 telemetry(嵌套+扁平)。
  - `pub fn set_test_storage_override(path: Option<std::path::PathBuf>)` — 测试钩子(线程本地)。

- [ ] **Step 1: 加 rand 依赖**

在 `src-tauri/Cargo.toml` 的 `[dependencies]` 段末尾(`auto-launch = "0.5"` 之后)加:

```toml
rand = "0.8"
```

> 用 0.8(与上游 `rand::thread_rng()` / `gen_range` API 一致)。0.9 的 API 变了。

- [ ] **Step 2: 注册模块**

在 `src-tauri/src/services/mod.rs`(`pub mod ag_current_account;` 之后)加:

```rust
pub mod ag_device;
```

- [ ] **Step 3: 写模块(含失败测试)**

创建 `src-tauri/src/services/ag_device.rs`:

```rust
//! Antigravity 设备指纹:生成、storage.json 路径、读写。
//!
//! 对齐上游 Antigravity-Manager 4.2.7 的 modules/device.rs。
//! 每个账号绑定一套 DeviceProfile,切换时写入 ide 的 storage.json,
//! 使多账号呈现为不同设备身份。

use crate::models::antigravity::AntigravityDeviceProfile;
use rand::distributions::Alphanumeric;
use rand::Rng;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// 测试用的线程本地 storage.json 路径覆盖(避免并行测试抢 env var)。
#[cfg(test)]
thread_local! {
    static TEST_STORAGE_OVERRIDE: std::cell::RefCell<Option<PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

/// 测试钩子:设置/清除线程本地的 storage.json 路径覆盖。
#[cfg(test)]
pub fn set_test_storage_override(path: Option<PathBuf>) {
    TEST_STORAGE_OVERRIDE.with(|c| *c.borrow_mut() = path);
}

/// 生成一套全新的设备指纹(对齐上游 generate_profile)。
pub fn generate_profile() -> AntigravityDeviceProfile {
    AntigravityDeviceProfile {
        machine_id: format!("auth0|user_{}", random_hex(32)),
        mac_machine_id: new_standard_machine_id(),
        dev_device_id: uuid::Uuid::new_v4().to_string(),
        sqm_id: format!("{{{}}}", uuid::Uuid::new_v4().to_string().to_uppercase()),
    }
}

fn random_hex(length: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(length)
        .map(char::from)
        .collect::<String>()
        .to_lowercase()
}

/// 生成 UUID v4 变体格式:xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx(y in 8..b)。
fn new_standard_machine_id() -> String {
    let mut rng = rand::thread_rng();
    let mut id = String::with_capacity(36);
    for ch in "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx".chars() {
        if ch == '-' || ch == '4' {
            id.push(ch);
        } else if ch == 'x' {
            id.push_str(&format!("{:x}", rng.gen_range(0..16)));
        } else if ch == 'y' {
            id.push_str(&format!("{:x}", rng.gen_range(8..12)));
        }
    }
    id
}

/// 返回 ide 的 storage.json 路径(区分 Antigravity / Antigravity IDE)。
/// 不存在则 Err。测试时优先用线程本地覆盖。
pub fn get_storage_path(target_ide: Option<&str>) -> Result<PathBuf, String> {
    #[cfg(test)]
    if let Some(p) = TEST_STORAGE_OVERRIDE.with(|c| c.borrow().clone()) {
        return Ok(p);
    }

    let folder_name = if target_ide == Some("ide") {
        "Antigravity IDE"
    } else {
        "Antigravity"
    };

    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir().ok_or("failed_to_get_home_dir")?;
        let path = home.join(format!(
            "Library/Application Support/{}/User/globalStorage/storage.json",
            folder_name
        ));
        if path.exists() {
            return Ok(path);
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            let path = PathBuf::from(appdata)
                .join(folder_name)
                .join("User\\globalStorage\\storage.json");
            if path.exists() {
                return Ok(path);
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let home = dirs::home_dir().ok_or("failed_to_get_home_dir")?;
        let path =
            home.join(format!(".config/{}/User/globalStorage/storage.json", folder_name));
        if path.exists() {
            return Ok(path);
        }
    }

    Err(format!("storage_json_not_found for {:?}", target_ide))
}

/// 把指纹写入 storage.json 的 telemetry 字段(嵌套 + 扁平两种格式都写)。
/// storage.json 不存在则 Err。
pub fn write_profile(storage_path: &Path, profile: &AntigravityDeviceProfile) -> Result<(), String> {
    if !storage_path.exists() {
        return Err(format!("storage_json_missing: {:?}", storage_path));
    }

    let content =
        std::fs::read_to_string(storage_path).map_err(|e| format!("read_failed: {}", e))?;
    let mut json: Value =
        serde_json::from_str(&content).map_err(|e| format!("parse_failed: {}", e))?;

    // 确保 telemetry 是 object
    if !json.get("telemetry").map_or(false, |v| v.is_object()) {
        if json.as_object_mut().is_some() {
            json["telemetry"] = serde_json::json!({});
        } else {
            return Err("json_top_level_not_object".to_string());
        }
    }

    if let Some(telemetry) = json.get_mut("telemetry").and_then(|v| v.as_object_mut()) {
        telemetry.insert(
            "machineId".to_string(),
            Value::String(profile.machine_id.clone()),
        );
        telemetry.insert(
            "macMachineId".to_string(),
            Value::String(profile.mac_machine_id.clone()),
        );
        telemetry.insert(
            "devDeviceId".to_string(),
            Value::String(profile.dev_device_id.clone()),
        );
        telemetry.insert(
            "sqmId".to_string(),
            Value::String(profile.sqm_id.clone()),
        );
    } else {
        return Err("telemetry_not_object".to_string());
    }

    // 同时写扁平 key,兼容旧格式
    if let Some(map) = json.as_object_mut() {
        map.insert(
            "telemetry.machineId".to_string(),
            Value::String(profile.machine_id.clone()),
        );
        map.insert(
            "telemetry.macMachineId".to_string(),
            Value::String(profile.mac_machine_id.clone()),
        );
        map.insert(
            "telemetry.devDeviceId".to_string(),
            Value::String(profile.dev_device_id.clone()),
        );
        map.insert(
            "telemetry.sqmId".to_string(),
            Value::String(profile.sqm_id.clone()),
        );
    }

    write_atomic(storage_path, &json)
}

fn write_atomic(path: &Path, json: &Value) -> Result<(), String> {
    let tmp = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(json)
        .map_err(|e| format!("serialize_failed: {}", e))?;
    std::fs::write(&tmp, &bytes).map_err(|e| format!("write_tmp_failed: {}", e))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("rename_failed: {}", e)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 建一个 tempdir + 带基础 telemetry 的 storage.json,设置线程本地覆盖。
    fn with_storage() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("storage.json");
        std::fs::write(
            &path,
            r#"{"telemetry":{"machineId":"old","macMachineId":"old"},"someOther":"x"}"#,
        )
        .unwrap();
        set_test_storage_override(Some(path.clone()));
        (dir, path)
    }

    #[test]
    fn generate_profile_produces_valid_fields() {
        let p = generate_profile();
        assert!(p.machine_id.starts_with("auth0|user_"));
        assert_eq!(p.machine_id.len(), "auth0|user_".len() + 32);
        // mac_machine_id: 8-4-4-4-12 = 36 chars
        assert_eq!(p.mac_machine_id.len(), 36);
        assert_eq!(p.mac_machine_id.chars().nth(8), Some('-'));
        assert_eq!(p.mac_machine_id.chars().nth(13), Some('-'));
        assert_eq!(p.mac_machine_id.chars().nth(18), Some('-'));
        assert_eq!(p.mac_machine_id.chars().nth(23), Some('-'));
        // dev_device_id 是标准 UUID
        assert_eq!(p.dev_device_id.len(), 36);
        // sqm_id 是 {大写UUID}
        assert!(p.sqm_id.starts_with('{'));
        assert!(p.sqm_id.ends_with('}'));
        assert_eq!(p.sqm_id.len(), 38); // { + 36 + }
    }

    #[test]
    fn generate_profile_is_unique() {
        let a = generate_profile();
        let b = generate_profile();
        assert_ne!(a.machine_id, b.machine_id, "two profiles must differ");
        assert_ne!(a.dev_device_id, b.dev_device_id);
    }

    #[test]
    fn write_profile_updates_nested_and_flat() {
        let (_dir, path) = with_storage();
        let profile = generate_profile();
        write_profile(&path, &profile).unwrap();

        let written: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        // 嵌套
        assert_eq!(
            written["telemetry"]["machineId"].as_str(),
            Some(profile.machine_id.as_str())
        );
        assert_eq!(
            written["telemetry"]["sqmId"].as_str(),
            Some(profile.sqm_id.as_str())
        );
        // 扁平
        assert_eq!(
            written["telemetry.machineId"].as_str(),
            Some(profile.machine_id.as_str())
        );
        // 其他字段保留
        assert_eq!(written["someOther"].as_str(), Some("x"));
    }

    #[test]
    fn write_profile_errors_when_storage_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nope.json"); // 不存在
        let profile = generate_profile();
        let res = write_profile(&path, &profile);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("storage_json_missing"));
    }

    #[test]
    fn get_storage_path_uses_override_in_test() {
        let (_dir, path) = with_storage();
        // override 已设置 → 返回该路径,不受 target_ide 影响
        assert_eq!(get_storage_path(Some("ide")).unwrap(), path);
        assert_eq!(get_storage_path(None).unwrap(), path);
    }
}
```

- [ ] **Step 4: 跑测试,验证通过**

Run: `cd /Users/jiaxing/code/github/jadekit/src-tauri && cargo test ag_device -- --nocapture`
Expected: 5 tests PASS。

- [ ] **Step 5: 提交**

```bash
git -C /Users/jiaxing/code/github/jadekit add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/services/mod.rs src-tauri/src/services/ag_device.rs
git -C /Users/jiaxing/code/github/jadekit commit -m "$(cat <<'EOF'
feat(ag): 移植上游 device 模块 —— 指纹生成 + storage.json 读写

对齐 Antigravity-Manager 4.2.7:generate_profile 生成四字段指纹,
get_storage_path 按 target_ide 区分 Antigravity / Antigravity IDE,
write_profile 同时写 telemetry 嵌套与扁平格式。为后续切换登录修复铺路。

Co-Authored-By: Claude <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: switch_account 生成并绑定指纹(无则生成、存 DB、有则复用)

**Files:**
- Modify: `src-tauri/src/services/antigravity_service.rs`(switch_account,约 `:672-725` 区间)

**Interfaces:**
- Consumes: `crate::services::ag_device::generate_profile`、`crate::database::Database::upsert_antigravity_account`。
- Produces: `switch_account` 调用 `execute_local_switch` 时,`switch_data` 携带的账号已确保 `device_profile` 为 `Some`(并在 DB 持久化)。

- [ ] **Step 1: 写失败测试**

在 `src-tauri/src/services/antigravity_service.rs` 的 `active_account_tests` 模块追加:

```rust
    #[test]
    fn switch_ensures_device_profile_generated_and_persisted() {
        let (db, _a, b, _dir) = setup_two_accounts();
        // b 初始无指纹(base_account 没设 device_profile)
        let before = db.get_antigravity_account(&b.id).unwrap().unwrap();
        assert!(before.device_profile.is_none(), "b should start without profile");

        // 调用 switch 的前置步骤:确保指纹(不实际跑进程切换,只测指纹生成绑定)
        let mut account = b.clone();
        ensure_device_profile(&db, &mut account).unwrap();

        // 内存中 account 现在有指纹
        assert!(account.device_profile.is_some(), "account now has profile");
        // DB 也持久化了
        let after = db.get_antigravity_account(&b.id).unwrap().unwrap();
        assert!(after.device_profile.is_some(), "DB persisted the profile");

        // 再次 ensure 不重新生成(复用)
        let first = account.device_profile.clone().unwrap();
        let mut again = after.clone();
        ensure_device_profile(&db, &mut again).unwrap();
        assert_eq!(
            again.device_profile.as_ref().unwrap().machine_id,
            first.machine_id,
            "second ensure must reuse existing profile, not regenerate"
        );
    }
```

- [ ] **Step 2: 跑测试,验证失败**

Run: `cd /Users/jiaxing/code/github/jadekit/src-tauri && cargo test switch_ensures_device_profile -- --nocapture`
Expected: 编译失败 —— `ensure_device_profile` 未定义。

- [ ] **Step 3: 实现 `ensure_device_profile` 并接入 switch_account**

在 `antigravity_service.rs` 的 `switch_account` 函数**之前**加辅助函数:

```rust
/// 确保账号有设备指纹:无则生成、绑定并存 DB;有则复用。
/// 对齐上游 account.rs:1082-1095。
fn ensure_device_profile(
    db: &Arc<Database>,
    account: &mut AntigravityAccount,
) -> Result<(), String> {
    if account.device_profile.is_none() {
        tracing::info!(
            "Account {} has no bound fingerprint, generating new one for isolation...",
            account.email
        );
        account.device_profile = Some(crate::services::ag_device::generate_profile());
        db.upsert_antigravity_account(account)?;
    }
    Ok(())
}
```

然后在 `switch_account` 中,**在构造 `switch_data` 之前**(即 token 刷新之后、进程切换之前)插入调用。找到 `switch_account` 里这段:

```rust
    // 2. Execute local app switch: close → inject credentials → restart
    //    进程操作在前,失败直接返回 —— 真相文件与 DB 都不动(切换没成功不该改激活态)。
    let switch_data = crate::services::ag_integration::SwitchAccountData {
```

在它**之前**(`// 2.` 注释之前)插入:

```rust
    // 1.5 确保账号有设备指纹(无则生成 + 存 DB,有则复用)。进程切换前必须就绪,
    //     因为 execute_local_switch 会把指纹写入 ide 的 storage.json。
    ensure_device_profile(db, &mut account)?;
```

> 注意 `account` 此时已是 `mut`(switch_account 开头 `let mut account`),且若上面 token 刷新分支执行过 `account = refresh_account_token(...)` 也仍是 `mut` 绑定,可直接 `&mut account`。

- [ ] **Step 4: 跑测试,验证通过**

Run: `cd /Users/jiaxing/code/github/jadekit/src-tauri && cargo test switch_ensures_device_profile -- --nocapture`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git -C /Users/jiaxing/code/github/jadekit add src-tauri/src/services/antigravity_service.rs
git -C /Users/jiaxing/code/github/jadekit commit -m "$(cat <<'EOF'
feat(ag): switch_account 确保账号有设备指纹

首次切换某账号时生成指纹并持久化到 DB,后续切换复用同一套指纹。
对齐上游 account.rs:1082-1095,为切换时写入 storage.json 做准备。

Co-Authored-By: Claude <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: execute_local_switch 写 storage.json 指纹(核心修复)

**Files:**
- Modify: `src-tauri/src/services/ag_integration.rs`(`execute_local_switch`,`SwitchAccountData` 结构体)

**Interfaces:**
- Consumes: `crate::services::ag_device::{get_storage_path, write_profile}`。
- Produces: `execute_local_switch` 写 keychain 后,把指纹写入 `storage.json`(Antigravity / Antigravity IDE 两个 app 路径都覆盖)。

- [ ] **Step 1: 给 SwitchAccountData 加 device_profile 字段**

在 `ag_integration.rs` 找到 `pub struct SwitchAccountData`(grep 定位),当前形如:

```rust
pub struct SwitchAccountData {
    pub email: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expiry_timestamp: i64,
}
```

改为:

```rust
pub struct SwitchAccountData {
    pub email: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expiry_timestamp: i64,
    pub device_profile: Option<crate::models::antigravity::AntigravityDeviceProfile>,
}
```

- [ ] **Step 2: 修复 switch_account 构造点**

回到 `antigravity_service.rs` 的 `switch_account`,更新 `switch_data` 构造(Task 2 已在该处插入 `ensure_device_profile`):

```rust
    let switch_data = crate::services::ag_integration::SwitchAccountData {
        email: account.email.clone(),
        access_token: account.access_token.clone(),
        refresh_token: account.refresh_token.clone(),
        expiry_timestamp: account.expiry_timestamp,
        device_profile: account.device_profile.clone(),
    };
```

- [ ] **Step 3: 在 execute_local_switch 写 keychain 后写 storage.json**

在 `ag_integration.rs` 的 `execute_local_switch` 中,找到 keyring 分支(形如):

```rust
    if use_keyring {
        // Modern path: write to system Keychain/Credential Manager
        write_to_system_keyring(
            &account.access_token,
            &account.refresh_token,
            account.expiry_timestamp,
        )?;
    } else {
```

改为(keychain 写后追加 storage.json 写入):

```rust
    if use_keyring {
        // Modern path: write to system Keychain/Credential Manager
        write_to_system_keyring(
            &account.access_token,
            &account.refresh_token,
            account.expiry_timestamp,
        )?;

        // 写入设备指纹到 storage.json(对齐上游 integration.rs:109-114)。
        // 缺指纹或 storage.json 不存在都不阻断切换(降级,仅 keychain)。
        if let Some(ref profile) = account.device_profile {
            match crate::services::ag_device::get_storage_path(target_ide) {
                Ok(storage_path) => {
                    if let Err(e) = crate::services::ag_device::write_profile(&storage_path, profile)
                    {
                        tracing::warn!(
                            "Failed to write device profile to {:?}: {}",
                            storage_path,
                            e
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!("storage.json not found for {:?}: {}", target_ide, e);
                }
            }
        }
    } else {
```

> 同时检查 `execute_local_switch` 的 legacy 分支(`< 2.0.0`,写 SQLite)。该分支已有 `inject_db_simple`,可不加指纹写入(老版本不需要)。保持现状。

- [ ] **Step 4: 编译 + 确认无残留旧构造**

Run: `cd /Users/jiaxing/code/github/jadekit/src-tauri && cargo build 2>&1 | tail -10`
Expected: 编译通过。若有 "missing field `device_profile`" 错误,grep 所有 `SwitchAccountData {` 构造点补上字段:
`grep -rn "SwitchAccountData {" src-tauri/src/`

- [ ] **Step 5: 跑全部 ag 测试回归**

Run: `cd /Users/jiaxing/code/github/jadekit/src-tauri && cargo test --lib -- ag_device active_account 2>&1 | grep -E "test result|FAILED"`
Expected: `test result: ok. N passed`,无 FAILED。

- [ ] **Step 6: 提交**

```bash
git -C /Users/jiaxing/code/github/jadekit add src-tauri/src/services/ag_integration.rs src-tauri/src/services/antigravity_service.rs
git -C /Users/jiaxing/code/github/jadekit commit -m "$(cat <<'EOF'
fix(ag): 切换时写入 storage.json 设备指纹,修复 ide 登录失败

execute_local_switch 写 keychain token 后,把账号绑定的设备指纹写入
ide 的 storage.json telemetry(Antigravity / Antigravity IDE 路径都覆盖)。
对齐上游 integration.rs:107-114。修复 token 有效但 ide 登录失败的 bug
(根因:仅写 keychain 未写 storage.json,设备指纹与账号不匹配)。

Co-Authored-By: Claude <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: 端到端手动验证

**Files:** 无代码改动,仅运行验证。

- [ ] **Step 1: 重启 dev(改了 Rust 必须重启)**

停掉现有 `npm run tauri dev`,重新 `npm run tauri dev`。

- [ ] **Step 2: 验证切换登录成功**

在 jadekit Antigravity 页面切到 `qincasin2@gmail.com`:
- 切换完成后,**Antigravity IDE 自动重启**。
- 重启后 ide 里登录态应为 `qincasin2`(不再是旧的 qincasin)。
- 检查 storage.json 指纹已写入:
  ```bash
  python3 -c "import json; d=json.load(open('$HOME/Library/Application Support/Antigravity IDE/User/globalStorage/storage.json')); t=d['telemetry']; print('machineId:', t['machineId'][:30]); print('sqmId:', t['sqmId'])"
  ```
  应显示新指纹(非旧的 `auth0|user_ttty...`)。

- [ ] **Step 3: 验证指纹绑定稳定(切换两次复用)**

切到 `qincasin`,再切回 `qincasin2`:
- `qincasin2` 第二次切换用的指纹与第一次**相同**(DB 持久化,不重生成)。
  ```bash
  sqlite3 ~/.jadekit/jadekit.db "SELECT email, json_extract(device_profile_json,'$.machineId') FROM antigravity_accounts WHERE email='qincasin2@gmail.com';"
  ```

- [ ] **Step 4: 验证两个 app 一致性**

若同时装了 Antigravity(原生)和 Antigravity IDE,分别切换(`target_ide=None` 和 `target_ide=Some("ide")`):
- 两个 app 的 storage.json 都被写入对应指纹。
- 两个 app 重启后登录态都与切换目标一致。

- [ ] **Step 5: 验证已有功能不回归**

- `list_accounts` 对账仍正常(真相文件 + DB 一致)。
- 删除当前账号 → 真相文件回退(Task 4 of 上一份 plan 的逻辑)。
- 首次添加新账号 → 真相文件指向它。

- [ ] **Step 6: 无代码改动,跳过提交。任务完成。**
