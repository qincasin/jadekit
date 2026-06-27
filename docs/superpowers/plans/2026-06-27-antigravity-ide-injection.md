# Antigravity IDE state.vscdb 凭据注入 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 对齐上游 Antigravity-Manager 4.2.7,给 Antigravity IDE 实现真正的 state.vscdb protobuf 凭据注入,修复 IDE 切换账号后登录失败(IDE 读 state.vscdb 不读 keychain)。

**Architecture:** 移植上游手写的 `utils/protobuf.rs`(varint 编解码 + OAuth/userStatus 消息构造)到 jadekit `services/ag_protobuf.rs`;移植 `modules/db.rs` 的 `inject_token`/`inject_new_format` 到 jadekit `services/ag_db_inject.rs`。`execute_local_switch` 加 `is_ide` 短路:Antigravity IDE 走 state.vscdb protobuf 注入,原生 Antigravity(>=2.0.0)走 keychain + storage.json(已完成)。

**Tech Stack:** Rust 2021 + rusqlite(已有)+ base64(已有)+ 纯手写 protobuf(无新依赖)。测试用真实 state.vscdb 备份 + tempfile 隔离。

## Global Constraints

- 对齐目标:上游 Antigravity-Manager **v4.2.7**(`acc633c1`)。
- protobuf 完全手写(varint 编解码),**不引入 prost/protobuf 库**。
- 上游 `crate::modules::logger::log_info/log_warn` → jadekit 换成 `tracing::info!/warn!`。
- IDE 注入写入的 ItemTable keys(对齐上游 `inject_new_format`):
  - `antigravityUnifiedStateSync.oauthToken` = base64(protobuf Topic{oauthTokenInfoSentinelKey → OAuthTokenInfo})
  - `antigravityUnifiedStateSync.userStatus` = base64(protobuf Topic{userStatusSentinelKey → email})
  - `antigravityUnifiedStateSync.enterprisePreferences` = base64(protobuf Topic{enterpriseGcpProjectId → project_id})(project_id 为空则 DELETE 该 key)
  - `antigravityOnboarding` = "true"
  - DELETE `jetskiStateSync.agentManagerInitState`(清旧 UserID)
- `is_ide` 短路(对齐上游 integration.rs:70-73):`target_ide == Some("ide")` → **不探测版本,直接走 state.vscdb 注入**(`use_keyring = false`)。
- `SwitchAccountData` 需补字段:`project_id: Option<String>`、`id_token: Option<String>`、`oauth_client_key: Option<String>`(inject_token 需要)。
- **保留不动**:已完成的 device 指纹 + storage.json 写入(原生 Antigravity 用)、真相文件 + 对账 + 原子切换。
- 每个 Task `git add` **显式列文件**,禁止 `git add .`;不碰 `StatusPanel.tsx` / `.codegraph/`。
- 测试命令:`cd /Users/jiaxing/code/github/jadekit/src-tauri && cargo test <filter> -- --nocapture`。
- 沿用线程本地覆盖模式做测试隔离,不用 env var。

---

## File Structure

| 文件 | 责任 | 动作 |
|---|---|---|
| `src-tauri/src/services/mod.rs` | 注册 `ag_protobuf`、`ag_db_inject` | **Modify** |
| `src-tauri/src/services/ag_protobuf.rs` | 手写 protobuf varint + OAuth/userStatus/stringValue 消息构造 + Topic 编解码 | **Create** |
| `src-tauri/src/services/ag_db_inject.rs` | state.vscdb 注入:inject_token / inject_new_format + user_status + enterprise | **Create** |
| `src-tauri/src/services/ag_integration.rs` | `SwitchAccountData` 补字段;`execute_local_switch` 加 `is_ide` 短路 + IDE 走 inject_token | **Modify** |
| `src-tauri/src/services/antigravity_service.rs` | switch_data 构造补 project_id/id_token/oauth_client_key | **Modify** |

---

### Task 1: protobuf 模块(varint + 消息构造 + Topic 编解码)

**Files:**
- Modify: `src-tauri/src/services/mod.rs`(注册 `ag_protobuf`)
- Create: `src-tauri/src/services/ag_protobuf.rs`

**Interfaces:**
- Produces(后续 Task 依赖,签名必须完全一致):
  - `pub fn create_oauth_info(access_token: &str, refresh_token: &str, expiry: i64, is_gcp_tos: bool, id_token: Option<&str>, email: Option<&str>) -> Vec<u8>`
  - `pub fn create_minimal_user_status_payload(email: &str) -> Vec<u8>`
  - `pub fn create_string_value_payload(value: &str) -> Vec<u8>`
  - `pub fn create_unified_topic_entry(sentinel_key: &str, payload: &[u8]) -> Vec<u8>`
  - `pub fn remove_unified_topic_entry(data: &[u8], target_key: &str) -> Result<Vec<u8>, String>`
  - `pub fn find_field(data: &[u8], target_field: u32) -> Result<Option<Vec<u8>>, String>`
  - (内部) `encode_varint` / `read_varint` / `skip_field` / `encode_string_field` / `encode_len_delim_field` / `encode_varint_field` / `unified_topic_entry_key`

- [ ] **Step 1: 注册模块**

在 `src-tauri/src/services/mod.rs`(`pub mod ag_device;` 之后)加:

```rust
pub mod ag_db_inject;
pub mod ag_protobuf;
```

- [ ] **Step 2: 创建 ag_protobuf.rs(移植上游 utils/protobuf.rs)**

创建 `src-tauri/src/services/ag_protobuf.rs`,内容为上游 `utils/protobuf.rs` 全文,但:
- 删除 `use crate::modules::logger` 相关调用(`create_oauth_info` 里的 `log_info` 换成 `tracing::info!`)。
- 保留所有公开函数签名不变。

完整内容:

```rust
//! 手写 Protobuf varint 编解码 + Antigravity 凭据消息构造。
//!
//! 对齐上游 Antigravity-Manager 4.2.7 的 utils/protobuf.rs。
//! 不依赖 prost/protobuf 库,纯手写 wire format。

use base64::{engine::general_purpose, Engine as _};

/// Protobuf Varint Encoding
pub fn encode_varint(mut value: u64) -> Vec<u8> {
    let mut buf = Vec::new();
    while value >= 0x80 {
        buf.push((value & 0x7F | 0x80) as u8);
        value >>= 7;
    }
    buf.push(value as u8);
    buf
}

/// Read Protobuf Varint
pub fn read_varint(data: &[u8], offset: usize) -> Result<(u64, usize), String> {
    let mut result = 0u64;
    let mut shift = 0;
    let mut pos = offset;

    loop {
        if pos >= data.len() {
            return Err("incomplete_data".to_string());
        }
        let byte = data[pos];
        result |= ((byte & 0x7F) as u64) << shift;
        pos += 1;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }

    Ok((result, pos))
}

/// Skip Protobuf Field
pub fn skip_field(data: &[u8], offset: usize, wire_type: u8) -> Result<usize, String> {
    match wire_type {
        0 => {
            let (_, new_offset) = read_varint(data, offset)?;
            Ok(new_offset)
        }
        1 => Ok(offset + 8),
        2 => {
            let (length, content_offset) = read_varint(data, offset)?;
            Ok(content_offset + length as usize)
        }
        5 => Ok(offset + 4),
        _ => Err(format!("unknown_wire_type: {}", wire_type)),
    }
}

/// Find specified Protobuf field content (Length-Delimited only)
pub fn find_field(data: &[u8], target_field: u32) -> Result<Option<Vec<u8>>, String> {
    let mut offset = 0;

    while offset < data.len() {
        let (tag, new_offset) = match read_varint(data, offset) {
            Ok(v) => v,
            Err(_) => break,
        };

        let wire_type = (tag & 7) as u8;
        let field_num = (tag >> 3) as u32;

        if field_num == target_field && wire_type == 2 {
            let (length, content_offset) = read_varint(data, new_offset)?;
            return Ok(Some(data[content_offset..content_offset + length as usize].to_vec()));
        }

        offset = skip_field(data, new_offset, wire_type)?;
    }

    Ok(None)
}

/// 编码长度分隔字段 (wire_type = 2)
pub fn encode_len_delim_field(field_num: u32, data: &[u8]) -> Vec<u8> {
    let tag = (field_num << 3) | 2;
    let mut f = encode_varint(tag as u64);
    f.extend(encode_varint(data.len() as u64));
    f.extend_from_slice(data);
    f
}

/// 编码字符串字段 (wire_type = 2)
pub fn encode_string_field(field_num: u32, value: &str) -> Vec<u8> {
    encode_len_delim_field(field_num, value.as_bytes())
}

/// 编码 varint 字段 (wire_type = 0)
pub fn encode_varint_field(field_num: u32, value: u64) -> Vec<u8> {
    let tag = (field_num << 3) | 0;
    let mut f = encode_varint(tag as u64);
    f.extend(encode_varint(value));
    f
}

/// 创建 OAuthTokenInfo 消息(对齐上游 create_oauth_info)
pub fn create_oauth_info(
    access_token: &str,
    refresh_token: &str,
    expiry: i64,
    mut is_gcp_tos: bool,
    id_token: Option<&str>,
    email: Option<&str>,
) -> Vec<u8> {
    if let Some(email_str) = email {
        let lower = email_str.to_lowercase();
        let is_personal = lower.ends_with("@gmail.com")
            || lower.ends_with("@outlook.com")
            || lower.ends_with("@hotmail.com")
            || lower.ends_with("@qq.com")
            || lower.ends_with("@163.com");

        if is_personal && is_gcp_tos {
            tracing::info!(
                "[Protobuf] 自动纠正个人账号 ({}) 的 GCP 标志位以确保 IDE 刷新兼容性。",
                email_str
            );
            is_gcp_tos = false;
        }
    }

    let field1 = encode_string_field(1, access_token);
    let field2 = encode_string_field(2, "Bearer");
    let field3 = encode_string_field(3, refresh_token);

    let seconds_tag = (1 << 3) | 0;
    let mut timestamp_msg = encode_varint(seconds_tag);
    timestamp_msg.extend(encode_varint(expiry as u64));
    let nanos_tag = (2 << 3) | 0;
    timestamp_msg.extend(encode_varint(nanos_tag));
    timestamp_msg.extend(encode_varint(0));
    let field4 = encode_len_delim_field(4, &timestamp_msg);

    let field5 = id_token.map(|it| encode_string_field(5, it));
    let field6 = is_gcp_tos.then(|| encode_varint_field(6, 1));

    let mut oauth_info = Vec::new();
    oauth_info.extend(field1);
    oauth_info.extend(field2);
    oauth_info.extend(field3);
    oauth_info.extend(field4);
    if let Some(f) = field5 {
        oauth_info.extend(f);
    }
    if let Some(f) = field6 {
        oauth_info.extend(f);
    }
    oauth_info
}

/// 创建 unified-state stringValue payload
pub fn create_string_value_payload(value: &str) -> Vec<u8> {
    encode_string_field(3, value)
}

/// 创建最小可用的 UserStatus payload。
pub fn create_minimal_user_status_payload(email: &str) -> Vec<u8> {
    [encode_string_field(3, email), encode_string_field(7, email)].concat()
}

/// 创建 unified-state Topic.data entry。
pub fn create_unified_topic_entry(sentinel_key: &str, payload: &[u8]) -> Vec<u8> {
    let row = encode_string_field(1, &general_purpose::STANDARD.encode(payload));
    let entry = [
        encode_string_field(1, sentinel_key),
        encode_len_delim_field(2, &row),
    ]
    .concat();
    encode_len_delim_field(1, &entry)
}

/// 从 Topic.data 中移除指定 map entry,保留同 topic 下其他 sentinel row。
pub fn remove_unified_topic_entry(data: &[u8], target_key: &str) -> Result<Vec<u8>, String> {
    let mut result = Vec::new();
    let mut offset = 0;

    while offset < data.len() {
        let start_offset = offset;
        let (tag, new_offset) = read_varint(data, offset)?;
        let wire_type = (tag & 7) as u8;
        let field_num = (tag >> 3) as u32;
        let next_offset = skip_field(data, new_offset, wire_type)?;

        let should_remove = if field_num == 1 && wire_type == 2 {
            let (length, content_offset) = read_varint(data, new_offset)?;
            let length = length as usize;
            if content_offset + length > data.len() {
                return Err("Topic.data entry 数据不完整".to_string());
            }
            let entry = &data[content_offset..content_offset + length];
            unified_topic_entry_key(entry) == Some(target_key)
        } else {
            false
        };

        if !should_remove {
            result.extend_from_slice(&data[start_offset..next_offset]);
        }
        offset = next_offset;
    }

    Ok(result)
}

fn unified_topic_entry_key(data: &[u8]) -> Option<&str> {
    let mut offset = 0;
    while offset < data.len() {
        let (tag, new_offset) = read_varint(data, offset).ok()?;
        let wire_type = (tag & 7) as u8;
        let field_num = (tag >> 3) as u32;

        if field_num == 1 && wire_type == 2 {
            let (length, content_offset) = read_varint(data, new_offset).ok()?;
            let length = length as usize;
            if content_offset + length > data.len() {
                return None;
            }
            return std::str::from_utf8(&data[content_offset..content_offset + length]).ok();
        }

        offset = skip_field(data, new_offset, wire_type).ok()?;
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_varint_roundtrip() {
        for v in [0u64, 1, 127, 128, 16384, 0xFFFF_FFFF, u64::MAX] {
            let encoded = encode_varint(v);
            let (decoded, _) = read_varint(&encoded, 0).unwrap();
            assert_eq!(decoded, v, "varint roundtrip failed for {}", v);
        }
    }

    #[test]
    fn create_oauth_info_contains_all_fields() {
        let info = create_oauth_info("at", "rt", 1700_000_000, false, None, Some("a@gmail.com"));
        // Field 1 access_token
        let f1 = find_field(&info, 1).unwrap().unwrap();
        assert_eq!(String::from_utf8(f1).unwrap(), "at");
        // Field 2 token_type
        let f2 = find_field(&info, 2).unwrap().unwrap();
        assert_eq!(String::from_utf8(f2).unwrap(), "Bearer");
        // Field 3 refresh_token
        let f3 = find_field(&info, 3).unwrap().unwrap();
        assert_eq!(String::from_utf8(f3).unwrap(), "rt");
        // 个人账号 is_gcp_tos 应被强制关 → field 6 不存在
        assert!(find_field(&info, 6).unwrap().is_none());
    }

    #[test]
    fn unified_topic_entry_removes_and_adds() {
        let payload_a = create_oauth_info("at_a", "rt", 1, false, None, None);
        let topic = create_unified_topic_entry("oauthTokenInfoSentinelKey", &payload_a);
        // 再加一个不同 key 的 entry
        let payload_b = create_oauth_info("at_b", "rt", 2, false, None, None);
        let mut topic = topic;
        topic.extend(create_unified_topic_entry("otherKey", &payload_b));

        // 移除 oauthTokenInfoSentinelKey,保留 otherKey
        let cleaned = remove_unified_topic_entry(&topic, "oauthTokenInfoSentinelKey").unwrap();
        // cleaned 里不应再有 at_a(被移除),但应保留 at_b
        let cleaned_str = general_purpose::STANDARD.encode(&cleaned);
        assert!(!cleaned_str.is_empty());
        // 简单验证:cleaned 长度 < topic 长度(移除了一个 entry)
        assert!(cleaned.len() < topic.len());
    }
}
```

- [ ] **Step 3: 跑测试**

Run: `cd /Users/jiaxing/code/github/jadekit/src-tauri && cargo test ag_protobuf -- --nocapture`
Expected: 3 tests PASS。

- [ ] **Step 4: 提交(本次按约定不提交,跳过;若需提交见下)**

```bash
git -C /Users/jiaxing/code/github/jadekit add src-tauri/src/services/mod.rs src-tauri/src/services/ag_protobuf.rs
# git commit ... (按用户约定:本次只实现+测试,不 commit)
```

---

### Task 2: state.vscdb 注入模块(inject_token + inject_new_format)

**Files:**
- Modify: `src-tauri/src/services/mod.rs`(Task 1 已注册 `ag_db_inject`)
- Create: `src-tauri/src/services/ag_db_inject.rs`

**Interfaces:**
- Consumes: `crate::services::ag_protobuf::*`、`rusqlite`、`base64`、`std::path::Path`。
- Produces:
  - `pub fn inject_token(db_path: &std::path::Path, access_token: &str, refresh_token: &str, expiry: i64, email: &str, is_gcp_tos: bool, project_id: Option<&str>, id_token: Option<&str>, oauth_client_key: Option<&str>) -> Result<(), String>`
  - `pub fn write_service_machine_id(db_path: &std::path::Path, service_machine_id: &str) -> Result<(), String>`(可选,指纹的 mac_machine_id 写入 ItemTable,对齐上游)

- [ ] **Step 1: 创建 ag_db_inject.rs**

创建 `src-tauri/src/services/ag_db_inject.rs`:

```rust
//! Antigravity IDE 的 state.vscdb 凭据注入。
//!
//! 对齐上游 Antigravity-Manager 4.2.7 的 modules/db.rs。
//! IDE(定制版)读 state.vscdb 的 ItemTable,不读 keychain;
//! 因此切换 IDE 账号必须把凭据以 protobuf 编码注入 state.vscdb。

use crate::services::ag_protobuf;
use base64::{engine::general_purpose, Engine as _};
use rusqlite::Connection;

/// 主注入入口(对齐上游 inject_token)。
///
/// `oauth_client_key == Some("antigravity_enterprise")` 时强制关 GCP TOS
/// 以使用标准 client 刷新。
pub fn inject_token(
    db_path: &std::path::Path,
    access_token: &str,
    refresh_token: &str,
    expiry: i64,
    email: &str,
    mut is_gcp_tos: bool,
    project_id: Option<&str>,
    id_token: Option<&str>,
    oauth_client_key: Option<&str>,
) -> Result<(), String> {
    tracing::info!("Starting Token injection into state.vscdb...");

    if let Some(key) = oauth_client_key {
        if key == "antigravity_enterprise" && is_gcp_tos {
            tracing::info!("[DB] Built-in client detected, forcing Standard mode for injection.");
            is_gcp_tos = false;
        }
    }

    inject_new_format(
        db_path, access_token, refresh_token, expiry, email, is_gcp_tos, project_id, id_token,
    )
}

/// 新格式注入(对齐上游 inject_new_format,>= 1.16.5)。
fn inject_new_format(
    db_path: &std::path::Path,
    access_token: &str,
    refresh_token: &str,
    expiry: i64,
    email: &str,
    is_gcp_tos: bool,
    project_id: Option<&str>,
    id_token: Option<&str>,
) -> Result<(), String> {
    let conn =
        Connection::open(db_path).map_err(|e| format!("Failed to open database: {}", e))?;

    let oauth_info = ag_protobuf::create_oauth_info(
        access_token,
        refresh_token,
        expiry,
        is_gcp_tos,
        id_token,
        Some(email),
    );

    // 读现有 oauthToken topic,移除旧 oauthTokenInfoSentinelKey,加入新条目
    let current_topic: Vec<u8> = conn
        .query_row(
            "SELECT value FROM ItemTable WHERE key = ?",
            ["antigravityUnifiedStateSync.oauthToken"],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| format!("Failed to read oauthToken: {}", e))?
        .and_then(|val| general_purpose::STANDARD.decode(val).ok())
        .unwrap_or_default();

    let mut topic =
        ag_protobuf::remove_unified_topic_entry(&current_topic, "oauthTokenInfoSentinelKey")?;
    topic.extend(ag_protobuf::create_unified_topic_entry(
        "oauthTokenInfoSentinelKey",
        &oauth_info,
    ));

    let topic_b64 = general_purpose::STANDARD.encode(&topic);

    conn.execute(
        "INSERT OR REPLACE INTO ItemTable (key, value) VALUES (?, ?)",
        rusqlite::params!["antigravityUnifiedStateSync.oauthToken", &topic_b64],
    )
    .map_err(|e| format!("Failed to write new format: {}", e))?;

    inject_user_status(&conn, email)?;

    if let Some(pid) = project_id.map(str::trim).filter(|p| !p.is_empty()) {
        inject_enterprise_project_preference(&conn, pid)?;
    } else {
        clear_enterprise_project_preference(&conn)?;
    }

    conn.execute(
        "INSERT OR REPLACE INTO ItemTable (key, value) VALUES (?, ?)",
        rusqlite::params!["antigravityOnboarding", "true"],
    )
    .map_err(|e| format!("Failed to write onboarding flag: {}", e))?;

    // 清旧 UserID,避免历史拉取失败
    let _ = conn.execute(
        "DELETE FROM ItemTable WHERE key = ?",
        rusqlite::params!["jetskiStateSync.agentManagerInitState"],
    );

    tracing::info!("Token injection successful (new format)");
    Ok(())
}

fn inject_user_status(conn: &Connection, email: &str) -> Result<(), String> {
    let payload = ag_protobuf::create_minimal_user_status_payload(email);
    let entry = ag_protobuf::create_unified_topic_entry("userStatusSentinelKey", &payload);
    let entry_b64 = general_purpose::STANDARD.encode(&entry);

    conn.execute(
        "INSERT OR REPLACE INTO ItemTable (key, value) VALUES (?, ?)",
        rusqlite::params!["antigravityUnifiedStateSync.userStatus", &entry_b64],
    )
    .map_err(|e| format!("Failed to write user status: {}", e))?;
    Ok(())
}

fn inject_enterprise_project_preference(conn: &Connection, project_id: &str) -> Result<(), String> {
    let payload = ag_protobuf::create_string_value_payload(project_id);
    let entry = ag_protobuf::create_unified_topic_entry("enterpriseGcpProjectId", &payload);
    let entry_b64 = general_purpose::STANDARD.encode(&entry);

    conn.execute(
        "INSERT OR REPLACE INTO ItemTable (key, value) VALUES (?, ?)",
        rusqlite::params!["antigravityUnifiedStateSync.enterprisePreferences", &entry_b64],
    )
    .map_err(|e| format!("Failed to write enterprise preferences: {}", e))?;
    Ok(())
}

fn clear_enterprise_project_preference(conn: &Connection) -> Result<(), String> {
    conn.execute(
        "DELETE FROM ItemTable WHERE key = ?",
        rusqlite::params!["antigravityUnifiedStateSync.enterprisePreferences"],
    )
    .map_err(|e| format!("Failed to clear enterprise preferences: {}", e))?;
    Ok(())
}

/// 注入 serviceMachineId(对齐上游 write_service_machine_id),
/// 解决 VS Code 缓存指纹不匹配导致 token 失效。
pub fn write_service_machine_id(
    db_path: &std::path::Path,
    service_machine_id: &str,
) -> Result<(), String> {
    let conn =
        Connection::open(db_path).map_err(|e| format!("Failed to open database: {}", e))?;
    conn.execute(
        "INSERT OR REPLACE INTO ItemTable (key, value) VALUES (?, ?)",
        rusqlite::params!["telemetry.serviceMachineId", service_machine_id],
    )
    .map_err(|e| format!("Failed to write serviceMachineId: {}", e))?;
    tracing::info!("Successfully injected serviceMachineId");
    Ok(())
}

//rusqlite::OptionalExtension 用于 query_row().optional()
use rusqlite::OptionalExtension;

#[cfg(test)]
mod tests {
    use super::*;

    /// 建一个带 ItemTable 的内存 SQLite,返回 conn。
    fn mem_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT)",
            [],
        )
        .unwrap();
        conn
    }

    // inject_new_format 是私有的,这里通过完整 inject_token 间接测,需可访问 conn。
    // 为可测,我们直接测 inject_token 对一个临时文件 db(带 ItemTable schema)的写入。
    fn file_db_with_schema(dir: &tempfile::TempDir) -> std::path::PathBuf {
        let path = dir.path().join("state.vscdb");
        let conn = Connection::open(&path).unwrap();
        conn.execute(
            "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT)",
            [],
        )
        .unwrap();
        path
    }

    #[test]
    fn inject_token_writes_all_keys() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = file_db_with_schema(&dir);

        inject_token(
            &db_path,
            "access-tok",
            "refresh-tok",
            1_700_000_000,
            "user@gmail.com",
            false,
            None,
            None,
            None,
        )
        .unwrap();

        let conn = Connection::open(&db_path).unwrap();
        let oauth: String = conn
            .query_row(
                "SELECT value FROM ItemTable WHERE key = 'antigravityUnifiedStateSync.oauthToken'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(!oauth.is_empty(), "oauthToken key written");
        assert!(general_purpose::STANDARD.decode(&oauth).is_ok(), "valid base64");

        let onboarding: String = conn
            .query_row(
                "SELECT value FROM ItemTable WHERE key = 'antigravityOnboarding'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(onboarding, "true");

        // enterprisePreferences(project_id=None)应被删除 → 查不到
        let ent: Option<String> = conn
            .query_row(
                "SELECT value FROM ItemTable WHERE key = 'antigravityUnifiedStateSync.enterprisePreferences'",
                [],
                |r| r.get(0),
            )
            .optional()
            .ok()
            .flatten();
        assert!(ent.is_none(), "enterprisePreferences cleared when no project_id");
    }
}
```

> 注意:`use rusqlite::OptionalExtension;` 放在模块底部是为可读性;实际可上移到文件顶部 import 区。实现时统一放到顶部 `use` 区。

- [ ] **Step 2: 跑测试**

Run: `cd /Users/jiaxing/code/github/jadekit/src-tauri && cargo test ag_db_inject -- --nocapture`
Expected: 1 test PASS。

- [ ] **Step 3: 跳过提交(按约定)**

---

### Task 3: execute_local_switch 加 is_ide 短路 + IDE 走 inject_token

**Files:**
- Modify: `src-tauri/src/services/ag_integration.rs`(`SwitchAccountData` 补字段;`execute_local_switch` 改 use_keyring 决策 + IDE 分支调 inject_token)
- Modify: `src-tauri/src/services/antigravity_service.rs`(switch_data 构造补字段)

**Interfaces:**
- Consumes: `crate::services::ag_db_inject::inject_token`、`crate::services::ag_db_inject::write_service_machine_id`。
- Produces: Antigravity IDE 切换走 state.vscdb 注入;原生 Antigravity(>=2.0.0)走 keychain+storage.json(不变)。

- [ ] **Step 1: SwitchAccountData 补字段**

在 `ag_integration.rs` 的 `SwitchAccountData` 加三个字段:

```rust
pub struct SwitchAccountData {
    pub email: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expiry_timestamp: i64,
    pub device_profile: Option<crate::models::antigravity::AntigravityDeviceProfile>,
    pub project_id: Option<String>,
    pub id_token: Option<String>,
    pub oauth_client_key: Option<String>,
}
```

- [ ] **Step 2: switch_account 构造点补字段**

在 `antigravity_service.rs` switch_data 构造:

```rust
    let switch_data = crate::services::ag_integration::SwitchAccountData {
        email: account.email.clone(),
        access_token: account.access_token.clone(),
        refresh_token: account.refresh_token.clone(),
        expiry_timestamp: account.expiry_timestamp,
        device_profile: account.device_profile.clone(),
        project_id: account.project_id.clone(),
        id_token: None, // jadekit 未存 id_token,留 None
        oauth_client_key: account.oauth_client_key.clone(),
    };
```

- [ ] **Step 3: execute_local_switch 加 is_ide 短路**

在 `ag_integration.rs` 把现有的版本决策段:

```rust
    // 2. Determine injection method based on version
    let use_keyring = match get_antigravity_version(target_ide) {
        Some(ver) => {
            tracing::info!("Detected Antigravity version: {}", ver);
            compare_version(&ver, "2.0.0") != std::cmp::Ordering::Less
        }
        None => {
            tracing::warn!(
                "Could not detect Antigravity version, defaulting to Keychain injection"
            );
            true
        }
    };
```

改为(对齐上游 integration.rs:70-102,is_ide 短路):

```rust
    // 2. Determine injection method:
    //    - Antigravity IDE(定制版)→ 永远走 state.vscdb 注入(不读 keychain)
    //    - 原生 Antigravity → 探测版本,>= 2.0.0 用 keychain,否则 legacy DB
    let is_ide = target_ide == Some("ide");
    let use_keyring = if is_ide {
        tracing::info!("Target is Antigravity IDE, using state.vscdb injection");
        false
    } else {
        match get_antigravity_version(target_ide) {
            Some(ver) => {
                tracing::info!("Detected Antigravity version: {}", ver);
                compare_version(&ver, "2.0.0") != std::cmp::Ordering::Less
            }
            None => {
                tracing::warn!(
                    "Could not detect Antigravity version, defaulting to Keychain injection"
                );
                true
            }
        }
    };
```

- [ ] **Step 4: legacy 分支(else)改为真注入**

把 `execute_local_switch` 的 else 分支(当前调 `inject_db_simple` 空壳)改为调真 `inject_token`。找到这段:

```rust
    } else {
        // Legacy path: inject into SQLite DB
        if let Some(db_path) = get_db_path(target_ide) {
            tracing::info!("Using legacy DB injection at {:?}", db_path);
            // Backup first
            let backup_path = db_path.with_extension("vscdb.backup");
            let _ = std::fs::copy(&db_path, &backup_path);
            inject_db_simple(
                &db_path,
                &account.access_token,
                &account.refresh_token,
                &account.email,
            )?;
        } else {
            tracing::warn!("No DB path found for legacy injection, trying keyring fallback");
            write_to_system_keyring(
                &account.access_token,
                &account.refresh_token,
                account.expiry_timestamp,
            )?;
        }
    }
```

改为:

```rust
    } else {
        // Antigravity IDE 或老版本 Antigravity:state.vscdb protobuf 注入
        match get_db_path(target_ide) {
            Some(db_path) => {
                tracing::info!("Using state.vscdb injection at {:?}", db_path);
                // 备份
                let backup_path = db_path.with_extension("vscdb.backup");
                let _ = std::fs::copy(&db_path, &backup_path);

                crate::services::ag_db_inject::inject_token(
                    &db_path,
                    &account.access_token,
                    &account.refresh_token,
                    account.expiry_timestamp,
                    &account.email,
                    false, // is_gcp_tos(inject_token 内部按 oauth_client_key 纠正)
                    account.project_id.as_deref(),
                    account.id_token.as_deref(),
                    account.oauth_client_key.as_deref(),
                )?;

                // 注入 serviceMachineId(指纹的 mac_machine_id),解决缓存指纹不匹配
                if let Some(ref profile) = account.device_profile {
                    let _ = crate::services::ag_db_inject::write_service_machine_id(
                        &db_path,
                        &profile.mac_machine_id,
                    );
                }
            }
            None => {
                tracing::warn!("No state.vscdb found for {:?}, falling back to keychain", target_ide);
                write_to_system_keyring(
                    &account.access_token,
                    &account.refresh_token,
                    account.expiry_timestamp,
                )?;
            }
        }
    }
```

> `inject_db_simple` 现在无调用方。删掉它(grep 确认无引用后删除),或加 `#[allow(dead_code)]`。建议删除。

- [ ] **Step 5: 编译 + 删 inject_db_simple + 跑全部测试**

Run: `cd /Users/jiaxing/code/github/jadekit/src-tauri && cargo build 2>&1 | tail -10`
Expected: 编译通过。

确认无残留:`grep -rn "inject_db_simple" src/`(应为空,或仅剩待删的定义)。

Run: `cd /Users/jiaxing/code/github/jadekit/src-tauri && cargo test --lib -- ag_device active_account ag_protobuf ag_db_inject 2>&1 | grep -E "test result|FAILED"`
Expected: 全部 PASS。

- [ ] **Step 6: 跳过提交(按约定)**

---

### Task 4: 端到端手动验证

**Files:** 无代码改动。

- [ ] **Step 1: 重启 dev(改了 Rust 必须重启)**

停掉 `npm run tauri dev`,重新 `npm run tauri dev`。

- [ ] **Step 2: 切换 Antigravity IDE 验证登录成功**

在 jadekit 切换到 `qincasin2`(**走 IDE 切换,target_ide=ide**):
- Antigravity IDE 自动重启。
- 重启后 ide 登录态 = `qincasin2`(关键:之前登不进,现在应该能进)。
- 验证 state.vscdb 已注入:
  ```bash
  sqlite3 "$HOME/Library/Application Support/Antigravity IDE/User/globalStorage/state.vscdb" "SELECT key FROM ItemTable WHERE key LIKE 'antigravityUnifiedStateSync%';"
  ```
  应有 `oauthToken`、`userStatus` 两行。

- [ ] **Step 3: 反向切换验证**

切回 `qincasin` → IDE 重启 → 登录态 = `qincasin`。

- [ ] **Step 4: 原生 Antigravity 回归(若有装)**

切原生 Antigravity(target_ide=None)→ 仍走 keychain + storage.json(已实现),登录正常。

- [ ] **Step 5: 无代码改动,任务完成。**
