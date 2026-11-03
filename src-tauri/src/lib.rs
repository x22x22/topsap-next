//! TopSAP-Next 应用入口。
//!
//! `lib.rs` 只做装配：安装 TLS provider、注册状态与命令、托盘与单实例、
//! 退出前断开并清理网络配置。

pub mod auth;
pub mod commands;
pub mod dto;
#[cfg(target_os = "macos")]
pub mod elevate;
pub mod netcfg;
pub mod password_reset;
pub mod paths;
pub mod platform;
pub mod profiles;
pub mod protocol;
pub mod proxy_bypass;
pub mod secret_store;
pub mod session;
pub mod session_guard;
pub mod settings;
pub mod shutdown;
pub mod tls;
pub mod tun;
pub mod tunnel;

use std::path::PathBuf;
use std::sync::Arc;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, RunEvent, WindowEvent};

use session::Engine;

pub struct AppState {
    pub engine: Arc<Engine>,
    pub dll_path: PathBuf,
}

pub fn run() {
    #[cfg(target_os = "macos")]
    elevate::ensure_admin();

    // 全链路统一 rustls provider，必须在任何 TLS 连接建立前完成
    tls::install_default_provider();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // 二次启动不新建进程，聚焦已有窗口
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // 配置窗口 create=false，这里统一创建；Windows 可指定便携 WebView 数据目录
            ensure_user_dirs(app.handle())?;
            create_main_window(app.handle())?;
            let dll_path = resolve_dll(app.handle())?;
            let engine = Engine::new(app.handle().clone(), dll_path.clone());
            app.manage(AppState { engine, dll_path });
            {
                let engine = app.state::<AppState>().engine.clone();
                tauri::async_runtime::spawn(async move {
                    engine.purge_previous_session().await;
                });
            }
            crate::shutdown::install(app.handle());
            let settings = crate::settings::load(app.handle());
            if let Err(err) = crate::proxy_bypass::apply(app.handle(), &settings) {
                log::warn!("启动时应用代理绕过失败：{err}");
            }
            setup_tray(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                if crate::settings::load(app).close_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::app::app_info,
            commands::app::open_log_dir,
            commands::app::open_url,
            commands::conn::connect,
            commands::conn::disconnect,
            commands::conn::engine_snapshot,
            commands::conn::submit_captcha,
            commands::conn::submit_sms,
            commands::conn::resend_sms,
            commands::conn::change_password,
            commands::conn::change_password_for_login,
            commands::conn::fetch_captcha,
            commands::conn::probe_lines,
            commands::conn::forget_password,
            commands::conn::reset_password,
            commands::diag::run_self_test,
            commands::diag::check_residue,
            commands::diag::purge_residue,
            commands::settings::load_settings,
            commands::settings::save_settings,
            commands::settings::proxy_bypass_status,
            commands::settings::load_password,
            commands::profiles::list_connections,
            commands::profiles::save_connection,
            commands::profiles::delete_connection,
            commands::profiles::set_active_connection,
            commands::profiles::import_connections,
            commands::profiles::export_connections,
            commands::profiles::clear_connections,
        ]);

    let app = builder
        .build(tauri::generate_context!())
        .expect("构建 Tauri 应用失败");

    app.run(|app_handle, event| match event {
        RunEvent::ExitRequested { api, .. } => {
            let state = app_handle.state::<AppState>();
            if state.engine.needs_cleanup() {
                // 退出前先断开，确保路由、DNS 与网卡都被清理
                api.prevent_exit();
                let engine = state.engine.clone();
                let handle = app_handle.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = engine.disconnect().await;
                    handle.exit(0);
                });
            }
        }
        // macOS 的 AppKit 终止与部分异常退出不会触发 ExitRequested，这里补最后一道清理
        RunEvent::Exit => {
            let state = app_handle.state::<AppState>();
            if state.engine.needs_cleanup() {
                crate::shutdown::quick_disconnect_blocking(&state.engine);
            }
        }
        _ => {}
    });
}

/// 启动时创建配置与日志目录，保证便携目录结构完整
fn ensure_user_dirs(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    for dir in [crate::paths::config_dir(app)?, crate::paths::log_dir(app)?] {
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("创建目录 {} 失败：{e}", dir.display()))?;
    }
    Ok(())
}

/// 按 `tauri.conf.json` 的 main 窗口配置创建窗口。
/// Windows 额外把 WebView2 用户数据目录指到可执行文件旁的 `webview`。
fn create_main_window(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == "main")
        .cloned()
        .ok_or_else(|| std::io::Error::other("缺少 main 窗口配置"))?;

    let builder = tauri::WebviewWindowBuilder::from_config(app, &config)?;

    #[cfg(target_os = "windows")]
    let builder = {
        let dir = crate::paths::webview_dir().map_err(std::io::Error::other)?;
        std::fs::create_dir_all(&dir).map_err(std::io::Error::other)?;
        builder.data_directory(dir)
    };

    builder.build()?;
    Ok(())
}

#[cfg(unix)]
fn resolve_dll(_app: &tauri::AppHandle) -> Result<PathBuf, Box<dyn std::error::Error>> {
    // Unix 使用系统 TUN，不需要随包驱动
    Ok(PathBuf::new())
}

#[cfg(all(windows, target_arch = "aarch64"))]
const WINTUN_FILE: &str = "wintun-arm64.dll";
#[cfg(all(windows, not(target_arch = "aarch64")))]
const WINTUN_FILE: &str = "wintun.dll";

#[cfg(windows)]
fn resolve_dll(app: &tauri::AppHandle) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let mut candidates = Vec::new();
    if let Ok(dir) = app.path().resource_dir() {
        candidates.push(dir.join("resources").join(WINTUN_FILE));
        candidates.push(dir.join(WINTUN_FILE));
    }
    // 开发模式：直接读源码目录
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join(WINTUN_FILE),
    );

    tun::resolve_dll(&candidates).ok_or_else(|| {
        let checked = candidates
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join("；");
        format!("未找到 {WINTUN_FILE}，已检查：{checked}").into()
    })
}

fn setup_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
    let disconnect = MenuItem::with_id(app, "disconnect", "断开连接", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &disconnect, &quit])?;

    let mut builder = TrayIconBuilder::new()
        .tooltip("TopSAP Client")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "disconnect" => {
                let engine = app.state::<AppState>().engine.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = engine.disconnect().await;
                });
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                ..
            } = event
            {
                if let Some(window) = tray.app_handle().get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }
    builder.build(app)?;
    Ok(())
}
