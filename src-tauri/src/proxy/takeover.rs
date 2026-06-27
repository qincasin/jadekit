use crate::services::{app_paths, storage::json_store};
use serde_json::{Map, Value};
use std::fs;
use std::io;
use std::path::PathBuf;

pub const PROXY_TAKEOVER_BACKUP_FILE: &str = "proxy_live_backup.json";
pub const PROXY_TAKEOVER_ENV_KEYS: &[&str] = &[
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "ANTHROPIC_REASONING_MODEL",
];

fn backup_path() -> io::Result<PathBuf> {
    app_paths::data_file(PROXY_TAKEOVER_BACKUP_FILE)
}

fn ensure_env_object(settings: &mut Value) -> &mut Map<String, Value> {
    if !settings.is_object() {
        *settings = Value::Object(Map::new());
    }
    let root = settings.as_object_mut().expect("settings object ensured");
    let env = root
        .entry("env")
        .or_insert_with(|| Value::Object(Map::new()));
    if !env.is_object() {
        *env = Value::Object(Map::new());
    }
    env.as_object_mut().expect("env object ensured")
}

/// 将 Claude settings.json 接管到本地代理，并返回原始 env 子集备份。
pub fn apply_takeover_to_settings(settings: &mut Value, proxy_base_url: &str) -> Value {
    let env = ensure_env_object(settings);
    let mut backup_env = Map::new();

    for key in PROXY_TAKEOVER_ENV_KEYS {
        if let Some(value) = env.get(*key) {
            backup_env.insert((*key).to_string(), value.clone());
        }
    }

    // 安全边界：只改写 Claude 代理热更新所需字段，不触碰 token 和用户自定义 env。
    env.insert(
        "ANTHROPIC_BASE_URL".to_string(),
        Value::String(proxy_base_url.to_string()),
    );
    for key in PROXY_TAKEOVER_ENV_KEYS
        .iter()
        .filter(|key| **key != "ANTHROPIC_BASE_URL")
    {
        env.remove(*key);
    }

    let mut backup = Map::new();
    backup.insert("env".to_string(), Value::Object(backup_env));
    Value::Object(backup)
}

/// 根据 takeover 备份恢复 Claude settings.json，backup 未记录的 takeover 字段保持移除。
pub fn restore_takeover_to_settings(settings: &mut Value, backup: &Value) {
    let env = ensure_env_object(settings);

    // 状态流转：先移除本次接管写入的字段，再写回备份中真实存在的旧值。
    for key in PROXY_TAKEOVER_ENV_KEYS {
        env.remove(*key);
    }

    if let Some(backup_env) = backup.get("env").and_then(|value| value.as_object()) {
        for (key, value) in backup_env {
            env.insert(key.clone(), value.clone());
        }
    }
}

pub fn save_backup(backup: &Value) -> Result<(), io::Error> {
    let path = backup_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    json_store::write_json(&path, backup)
}

pub fn load_backup() -> Option<Value> {
    let path = backup_path().ok()?;
    json_store::read_json(&path).ok()
}

pub fn clear_backup() {
    if let Ok(path) = backup_path() {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_apply_and_restore_takeover_roundtrip() {
        let mut settings = json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.zhipu.com",
                "ANTHROPIC_DEFAULT_SONNET_MODEL": "glm-5.2[1M]",
                "ANTHROPIC_AUTH_TOKEN": "sk-x",
                "KEEP_ME": "yes"
            }
        });

        let backup = apply_takeover_to_settings(&mut settings, "http://127.0.0.1:8080");

        assert_eq!(
            settings["env"]["ANTHROPIC_BASE_URL"],
            json!("http://127.0.0.1:8080")
        );
        assert!(settings["env"]
            .get("ANTHROPIC_DEFAULT_SONNET_MODEL")
            .is_none());
        assert_eq!(settings["env"]["ANTHROPIC_AUTH_TOKEN"], json!("sk-x"));
        assert_eq!(settings["env"]["KEEP_ME"], json!("yes"));

        restore_takeover_to_settings(&mut settings, &backup);

        assert_eq!(
            settings["env"]["ANTHROPIC_BASE_URL"],
            json!("https://api.zhipu.com")
        );
        assert_eq!(
            settings["env"]["ANTHROPIC_DEFAULT_SONNET_MODEL"],
            json!("glm-5.2[1M]")
        );
    }
}
