//! 系统关机与终止信号下的限时断开。
//!
//! 关机钩子只做 best-effort：最多等待 2 秒，超时后交由操作系统回收
//! TUN 网卡与路由；异常退出留下的会话由 `session_guard` 在下次启动补注销。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::session::Engine;

const LIMIT: Duration = Duration::from_secs(2);
static STARTED: AtomicBool = AtomicBool::new(false);

fn begin(engine: &Arc<Engine>) -> bool {
    if STARTED.swap(true, Ordering::SeqCst) {
        return false;
    }
    engine.report_log("收到系统终止，正在断开连接");
    true
}

/// 主线程/窗口过程调用：限时等待异步断开，超时后放行系统终止
pub fn quick_disconnect_blocking(engine: &Arc<Engine>) {
    if !begin(engine) {
        return;
    }
    let worker = engine.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    tauri::async_runtime::spawn(async move {
        let _ = worker.disconnect().await;
        let _ = tx.send(());
    });
    if rx.recv_timeout(LIMIT).is_err() {
        engine.report_log("系统终止：2 秒内未完成断开，剩余网络配置交由系统回收");
    }
}

/// 异步上下文（Unix 信号任务）调用：限时等待断开；已有清理在跑时返回 false
#[cfg(unix)]
pub async fn quick_disconnect_async(engine: &Arc<Engine>) -> bool {
    if !begin(engine) {
        return false;
    }
    if tokio::time::timeout(LIMIT, engine.disconnect())
        .await
        .is_err()
    {
        engine.report_log("系统终止：2 秒内未完成断开，剩余网络配置交由系统回收");
    }
    true
}

pub fn install(app: &AppHandle) {
    let engine = app.state::<crate::AppState>().engine.clone();
    #[cfg(windows)]
    platform::install(app, &engine);
    #[cfg(unix)]
    platform::install(app, &engine);
}

#[cfg(windows)]
mod platform {
    use std::sync::{Arc, OnceLock};

    use tauri::Manager;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
    use windows_sys::Win32::UI::WindowsAndMessaging::WM_ENDSESSION;

    use super::{quick_disconnect_blocking, AppHandle, Engine};

    const SUBCLASS_ID: usize = 0x5453_5041_5000;

    static ENGINE: OnceLock<Arc<Engine>> = OnceLock::new();

    /// 在主窗口上安装子类过程，抢在 Tao 的 `process::exit` 前补一次断开
    pub fn install(app: &AppHandle, engine: &Arc<Engine>) {
        let _ = ENGINE.set(engine.clone());

        let Some(window) = app.get_webview_window("main") else {
            engine.report_log("警告：未找到主窗口，系统关机钩子未安装");
            return;
        };
        let Ok(hwnd) = window.hwnd() else {
            engine.report_log("警告：无法获取主窗口句柄，系统关机钩子未安装");
            return;
        };
        let ok = unsafe { SetWindowSubclass(hwnd.0, Some(subclass_proc), SUBCLASS_ID, 0) };
        if ok == 0 {
            engine.report_log("警告：安装系统关机钩子失败");
        }
    }

    unsafe extern "system" fn subclass_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        _ref_data: usize,
    ) -> LRESULT {
        if msg == WM_ENDSESSION && wparam != 0 {
            if let Some(engine) = ENGINE.get() {
                quick_disconnect_blocking(engine);
            }
        }
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }
}

#[cfg(unix)]
mod platform {
    use std::sync::Arc;

    use super::{quick_disconnect_async, AppHandle, Engine};

    /// 捕获 SIGTERM/SIGINT，在系统终止前限时断开
    pub fn install(_app: &AppHandle, engine: &Arc<Engine>) {
        let engine = engine.clone();
        tauri::async_runtime::spawn(async move {
            use tokio::signal::unix::{signal, SignalKind};

            let Ok(mut terminate) = signal(SignalKind::terminate()) else {
                engine.report_log("警告：无法注册 SIGTERM 处理器");
                return;
            };
            let Ok(mut interrupt) = signal(SignalKind::interrupt()) else {
                engine.report_log("警告：无法注册 SIGINT 处理器");
                return;
            };
            tokio::select! {
                _ = terminate.recv() => {}
                _ = interrupt.recv() => {}
            }
            if quick_disconnect_async(&engine).await {
                std::process::exit(0);
            }
        });
    }
}
