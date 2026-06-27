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

// 测试用的线程本地路径覆盖(避免并行测试抢同一个 env var)。
#[cfg(test)]
thread_local! {
    static TEST_OVERRIDE: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// 测试钩子:设置/清除线程本地的真相文件路径覆盖。
#[cfg(test)]
pub fn set_test_override(path: Option<PathBuf>) {
    TEST_OVERRIDE.with(|c| *c.borrow_mut() = path);
}

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
    // 测试:优先用线程本地覆盖(并行安全)。
    #[cfg(test)]
    if let Some(p) = TEST_OVERRIDE.with(|c| c.borrow().clone()) {
        return Ok(p);
    }
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
/// 顺序:先刷 DB(`set_active_antigravity_account` 自身是事务,id 不存在则报错且 DB 不动),
/// DB 成功后再写真相文件。这样 DB 失败时文件绝不会被污染,保持两者一致。
pub fn set_current_account_id(db: &Arc<Database>, id: &str) -> Result<(), String> {
    db.set_active_antigravity_account(id)?;
    let path = current_account_file_path()?;
    let payload = CurrentAccountFile {
        current_account_id: Some(id.to_string()),
        updated_at: chrono::Utc::now().timestamp(),
    };
    write_atomic(&path, &payload)
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

    /// 在隔离 tempdir 下设置线程本地真相文件路径,返回临时目录(测试期间存活)。
    /// 用线程本地而非 env var,避免并行测试互相抢同一个环境变量。
    fn with_isolated_file() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("current-account.json");
        TEST_OVERRIDE.with(|c| *c.borrow_mut() = Some(path));
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
