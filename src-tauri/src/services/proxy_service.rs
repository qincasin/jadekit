use crate::database::Database;
use crate::models::proxy::ProxyConfig;
use crate::proxy::{server, takeover, usage};
use crate::proxy::types::ProxyState;
use crate::services::{app_paths, provider_service, storage::json_store};
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

const PROXY_USAGE_LOG_DIR: &str = "proxy-usage";
const LOCAL_PROXY_BASE_URL_TEMPLATE: &str = "http://127.0.0.1:{port}";

fn get_proxy_config_path() -> Result<PathBuf, io::Error> {
    app_paths::data_file("proxy_config.json")
}

/// 从数据库加载代理配置（v3+，失败时回退到 JSON）
pub fn load_proxy_config_from_db(db: &Arc<Database>) -> Result<ProxyConfig, String> {
    match db.get_app_config("proxy_server_config")? {
        Some(json) => {
            let config: ProxyConfig =
                serde_json::from_str(&json).map_err(|e| format!("Parse config failed: {e}"))?;
            Ok(config)
        }
        None => {
            // 数据库为空时，从 JSON 文件回退加载
            if let Ok(config) = load_proxy_config() {
                // 将 JSON 数据迁移到数据库
                let config_json = serde_json::to_string(&config)
                    .map_err(|e| format!("Serialize config failed: {e}"))?;
                let _ = db.set_app_config("proxy_server_config", &config_json);
                return Ok(config);
            }
            Ok(ProxyConfig::default())
        }
    }
}

/// 保存代理配置到数据库（v3+）
pub fn save_proxy_config_to_db(db: &Arc<Database>, config: &ProxyConfig) -> Result<(), String> {
    let config_json =
        serde_json::to_string(config).map_err(|e| format!("Serialize config failed: {e}"))?;
    db.set_app_config("proxy_server_config", &config_json)
}

/// 读取代理配置，文件不存在时返回默认值（兼容旧版）
pub fn load_proxy_config() -> Result<ProxyConfig, io::Error> {
    let path = get_proxy_config_path()?;
    if !path.exists() {
        return Ok(ProxyConfig::default());
    }
    let content = fs::read_to_string(&path)?;
    serde_json::from_str(&content).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// 保存代理配置到文件（保留兼容）
#[allow(dead_code)]
pub fn save_proxy_config(config: &ProxyConfig) -> Result<(), io::Error> {
    let path = get_proxy_config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(config)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(&path, content)
}

fn proxy_base_url(port: u16) -> String {
    LOCAL_PROXY_BASE_URL_TEMPLATE.replace("{port}", &port.to_string())
}

fn read_claude_settings() -> Result<(PathBuf, serde_json::Value), String> {
    let path = provider_service::get_claude_settings_path().map_err(|e| e.to_string())?;
    if !path.exists() {
        return Ok((path, serde_json::json!({})));
    }
    let settings = json_store::read_json(&path).map_err(|e| e.to_string())?;
    Ok((path, settings))
}

fn write_claude_settings(path: &PathBuf, settings: &serde_json::Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    json_store::write_json(path, settings).map_err(|e| e.to_string())
}

fn apply_claude_takeover(port: u16) -> Result<(), String> {
    let (settings_path, mut settings) = read_claude_settings()?;
    let backup = takeover::apply_takeover_to_settings(&mut settings, &proxy_base_url(port));

    // 配置写入是接管的安全边界：只在 settings 已指向本地代理且备份落盘后才认为成功。
    write_claude_settings(&settings_path, &settings)?;
    if let Err(e) = takeover::save_backup(&backup) {
        let mut restored = settings;
        takeover::restore_takeover_to_settings(&mut restored, &backup);
        let _ = write_claude_settings(&settings_path, &restored);
        return Err(e.to_string());
    }

    Ok(())
}

fn restore_claude_takeover_if_needed() -> Result<(), String> {
    if let Some(backup) = takeover::load_backup() {
        let (settings_path, mut settings) = read_claude_settings()?;
        // 状态流转：关闭代理时先恢复 Claude settings，再清掉崩溃恢复用的备份文件。
        takeover::restore_takeover_to_settings(&mut settings, &backup);
        write_claude_settings(&settings_path, &settings)?;
        takeover::clear_backup();
    }
    Ok(())
}

/// 启动代理服务器
pub async fn start_proxy(config: ProxyConfig) -> Result<ProxyState, String> {
    let state = server::start(&config.host, config.port).await?;
    let log_dir = app_paths::data_subdir(PROXY_USAGE_LOG_DIR).map_err(|e| e.to_string())?;
    usage::logger::init_logger(log_dir);

    if let Err(e) = apply_claude_takeover(state.port) {
        let _ = server::stop().await;
        return Err(e);
    }

    Ok(state)
}

/// 停止代理服务器
pub async fn stop_proxy() -> Result<(), String> {
    restore_claude_takeover_if_needed()?;
    server::stop().await
}

/// 获取代理服务器状态
pub fn get_proxy_status() -> Result<ProxyState, String> {
    Ok(server::get_state())
}
