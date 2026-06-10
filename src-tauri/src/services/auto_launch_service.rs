use auto_launch::AutoLaunchBuilder;
use serde::{Deserialize, Serialize};
use std::io;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoLaunchStatus {
    #[serde(rename = "enabled")]
    pub enabled: bool,
    #[serde(rename = "supported")]
    pub supported: bool,
}

const APP_NAME: &str = "JadeKit";

/// 获取 macOS 上的 .app bundle 路径
/// 将 `/path/to/JadeKit.app/Contents/MacOS/JadeKit` 转换为 `/path/to/JadeKit.app`
#[cfg(target_os = "macos")]
fn get_macos_app_bundle_path(exe_path: &std::path::Path) -> Option<std::path::PathBuf> {
    let path_str = exe_path.to_string_lossy();
    if let Some(app_pos) = path_str.find(".app/Contents/MacOS/") {
        let app_bundle_end = app_pos + 4; // ".app" 的结束位置
        Some(std::path::PathBuf::from(&path_str[..app_bundle_end]))
    } else {
        None
    }
}

/// 初始化 AutoLaunch 实例
fn build_auto_launch() -> Result<auto_launch::AutoLaunch, io::Error> {
    let exe_path = std::env::current_exe()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("无法获取应用路径: {e}")))?;

    // macOS 需要使用 .app bundle 路径
    #[cfg(target_os = "macos")]
    let app_path = get_macos_app_bundle_path(&exe_path).unwrap_or(exe_path);

    #[cfg(not(target_os = "macos"))]
    let app_path = exe_path;

    AutoLaunchBuilder::new()
        .set_app_name(APP_NAME)
        .set_app_path(&app_path.to_string_lossy())
        .build()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("创建 AutoLaunch 失败: {e}")))
}

/// 获取开机自启动状态（异步非阻塞）
pub async fn get_auto_launch_status() -> Result<AutoLaunchStatus, io::Error> {
    tokio::task::spawn_blocking(|| {
        match build_auto_launch() {
            Ok(auto_launch) => {
                let enabled = auto_launch.is_enabled().unwrap_or(false);
                Ok(AutoLaunchStatus {
                    enabled,
                    supported: true,
                })
            }
            Err(_) => {
                // 构建失败（如开发环境），标记为不支持
                Ok(AutoLaunchStatus {
                    enabled: false,
                    supported: false,
                })
            }
        }
    })
    .await
    .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("spawn_blocking 失败: {e}")))?
}

/// 设置或取消开机自启动（异步非阻塞）
pub async fn set_auto_launch(enabled: bool) -> Result<(), io::Error> {
    tokio::task::spawn_blocking(move || {
        let auto_launch = build_auto_launch()?;

        if enabled {
            auto_launch.enable().map_err(|e| {
                io::Error::new(io::ErrorKind::Other, format!("启用开机自启失败: {e}"))
            })?;
        } else {
            auto_launch.disable().map_err(|e| {
                io::Error::new(io::ErrorKind::Other, format!("禁用开机自启失败: {e}"))
            })?;
        }

        Ok(())
    })
    .await
    .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("spawn_blocking 失败: {e}")))?
}
