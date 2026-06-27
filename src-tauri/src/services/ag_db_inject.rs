//! Antigravity IDE 的 state.vscdb 凭据注入。
//!
//! 对齐上游 Antigravity-Manager 4.2.7 的 modules/db.rs。
//! IDE(定制版)读 state.vscdb 的 ItemTable,不读 keychain;
//! 因此切换 IDE 账号必须把凭据以 protobuf 编码注入 state.vscdb。

use crate::services::ag_protobuf;
use base64::{engine::general_purpose, Engine as _};
use rusqlite::{Connection, OptionalExtension};

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
    let mut conn = Connection::open(db_path).map_err(|e| format!("Failed to open database: {}", e))?;

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
    // 清除 IDE 残留的登出标记(用户曾手动登出时留下),否则 IDE 启动读到 signedOut
    // 会拒绝读取我们注入的凭据。上游未处理此 key,这是 jadekit 针对该残留的增强。
    topic = ag_protobuf::remove_unified_topic_entry(&topic, "authStateWithContextSentinelKey")?;
    topic.extend(ag_protobuf::create_unified_topic_entry(
        "oauthTokenInfoSentinelKey",
        &oauth_info,
    ));

    let topic_b64 = general_purpose::STANDARD.encode(&topic);

    // 所有写入包进一个事务:任一步失败则整体回滚,避免凭据半成品导致 IDE 登录异常。
    let tx = conn.transaction().map_err(|e| format!("Failed to begin tx: {}", e))?;

    tx.execute(
        "INSERT OR REPLACE INTO ItemTable (key, value) VALUES (?, ?)",
        rusqlite::params!["antigravityUnifiedStateSync.oauthToken", &topic_b64],
    )
    .map_err(|e| format!("Failed to write new format: {}", e))?;

    inject_user_status_tx(&tx, email)?;

    if let Some(pid) = project_id.map(str::trim).filter(|p| !p.is_empty()) {
        inject_enterprise_project_preference_tx(&tx, pid)?;
    } else {
        clear_enterprise_project_preference_tx(&tx)?;
    }

    tx.execute(
        "INSERT OR REPLACE INTO ItemTable (key, value) VALUES (?, ?)",
        rusqlite::params!["antigravityOnboarding", "true"],
    )
    .map_err(|e| format!("Failed to write onboarding flag: {}", e))?;

    // 清旧 UserID,避免历史拉取失败
    let _ = tx.execute(
        "DELETE FROM ItemTable WHERE key = ?",
        rusqlite::params!["jetskiStateSync.agentManagerInitState"],
    );

    tx.commit().map_err(|e| format!("Failed to commit injection tx: {}", e))?;

    tracing::info!("Token injection successful (new format)");
    Ok(())
}

fn inject_user_status_tx(tx: &rusqlite::Transaction, email: &str) -> Result<(), String> {
    let payload = ag_protobuf::create_minimal_user_status_payload(email);
    let entry = ag_protobuf::create_unified_topic_entry("userStatusSentinelKey", &payload);
    let entry_b64 = general_purpose::STANDARD.encode(&entry);

    tx.execute(
        "INSERT OR REPLACE INTO ItemTable (key, value) VALUES (?, ?)",
        rusqlite::params!["antigravityUnifiedStateSync.userStatus", &entry_b64],
    )
    .map_err(|e| format!("Failed to write user status: {}", e))?;
    Ok(())
}

fn inject_enterprise_project_preference_tx(
    tx: &rusqlite::Transaction,
    project_id: &str,
) -> Result<(), String> {
    let payload = ag_protobuf::create_string_value_payload(project_id);
    let entry = ag_protobuf::create_unified_topic_entry("enterpriseGcpProjectId", &payload);
    let entry_b64 = general_purpose::STANDARD.encode(&entry);

    tx.execute(
        "INSERT OR REPLACE INTO ItemTable (key, value) VALUES (?, ?)",
        rusqlite::params!["antigravityUnifiedStateSync.enterprisePreferences", &entry_b64],
    )
    .map_err(|e| format!("Failed to write enterprise preferences: {}", e))?;
    Ok(())
}

fn clear_enterprise_project_preference_tx(tx: &rusqlite::Transaction) -> Result<(), String> {
    tx.execute(
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
    let conn = Connection::open(db_path).map_err(|e| format!("Failed to open database: {}", e))?;
    conn.execute(
        "INSERT OR REPLACE INTO ItemTable (key, value) VALUES (?, ?)",
        rusqlite::params!["telemetry.serviceMachineId", service_machine_id],
    )
    .map_err(|e| format!("Failed to write serviceMachineId: {}", e))?;
    tracing::info!("Successfully injected serviceMachineId");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(!oauth.is_empty());
        assert!(general_purpose::STANDARD.decode(&oauth).is_ok());

        let onboarding: String = conn
            .query_row(
                "SELECT value FROM ItemTable WHERE key = 'antigravityOnboarding'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(onboarding, "true");

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
