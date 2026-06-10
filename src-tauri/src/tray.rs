use crate::models::app_type::AppType;
use crate::services::provider_service;
use tauri::{
    image::Image,
    menu::MenuBuilder,
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager,
};

const NAVIGATE_EVENT: &str = "navigate-to-route";

/// AppType 显示名称
fn display_name(app_type: &AppType) -> &'static str {
    match app_type {
        AppType::Claude => "Claude",
        AppType::Codex => "Codex",
        AppType::Gemini => "Gemini",
        AppType::OpenCode => "OpenCode",
        AppType::OpenClaw => "OpenClaw",
    }
}

/// 显示并聚焦主窗口
fn show_main_window(app_handle: &tauri::AppHandle) {
    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.unminimize();
    }
}

/// 显示窗口并跳转到指定前端路由。
fn show_route(app_handle: &tauri::AppHandle, route: &str) {
    show_main_window(app_handle);
    let _ = app_handle.emit(NAVIGATE_EVENT, route);
}

/// 隐藏主窗口。
fn hide_main_window(app_handle: &tauri::AppHandle) {
    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = window.hide();
    }
}

/// 初始化系统托盘
pub fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let menu = build_tray_menu(app.handle())?;

    // macOS menu bar icons should use a monochrome template image instead of the full app icon.
    let icon = Image::from_bytes(include_bytes!("../icons/tray-icon.png"))?;

    let _tray = TrayIconBuilder::new()
        .tooltip("JadeKit")
        .icon(icon)
        .icon_as_template(true)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app_handle, event| {
            let id = event.id().as_ref();
            match id {
                "show" => {
                    show_main_window(app_handle);
                }
                "settings" => {
                    show_route(app_handle, "/settings");
                }
                "hide" => {
                    hide_main_window(app_handle);
                }
                "quit" => {
                    app_handle.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

/// 构建托盘菜单
fn build_tray_menu(
    handle: &tauri::AppHandle,
) -> Result<tauri::menu::Menu<tauri::Wry>, Box<dyn std::error::Error>> {
    let mut builder = MenuBuilder::new(handle);

    // 标题
    builder = builder.text("title", "JadeKit");
    builder = builder.separator();

    // 每个应用显示当前活跃的 Provider
    for app_type in AppType::all() {
        let providers = provider_service::list_providers(*app_type).unwrap_or_default();
        let active = providers.iter().find(|p| p.is_active);
        let label = match active {
            Some(p) => format!("{}: {}", display_name(app_type), p.name),
            None => format!("{}: (none)", display_name(app_type)),
        };
        builder = builder.text(format!("app_{}", app_type.as_str()), &label);
    }

    builder = builder.separator();

    // Window actions
    builder = builder.text("show", "显示 JadeKit");
    builder = builder.text("settings", "设置...");
    builder = builder.text("hide", "隐藏 JadeKit");
    builder = builder.separator();

    // 退出
    builder = builder.text("quit", "退出");

    let menu = builder.build()?;
    Ok(menu)
}

/// 重新构建托盘菜单（供外部调用刷新状态）
#[allow(dead_code)]
pub fn rebuild_tray_menu(app_handle: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let menu = build_tray_menu(app_handle)?;
    if let Some(tray) = app_handle.tray_by_id("main-tray") {
        tray.set_menu(Some(menu))?;
    }
    Ok(())
}
