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

// 测试用的线程本地 storage.json 路径覆盖(避免并行测试抢 env var)。
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
        let path = home.join(format!(".config/{}/User/globalStorage/storage.json", folder_name));
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
        telemetry.insert("sqmId".to_string(), Value::String(profile.sqm_id.clone()));
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
    let bytes =
        serde_json::to_vec_pretty(json).map_err(|e| format!("serialize_failed: {}", e))?;
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

        let written: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
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
