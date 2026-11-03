//! 运行目录解析。
//!
//! Windows 采用便携布局，配置、日志与 WebView2 数据都放在可执行文件旁，
//! 不写入 `%APPDATA%` / `%LOCALAPPDATA%`；其他平台继续使用系统标准目录
//! （macOS 的 `.app` 包内不可写）。

use std::path::PathBuf;

use tauri::AppHandle;
#[cfg(not(target_os = "windows"))]
use tauri::Manager;

/// Windows 便携目录布局：`<exe>/config`、`<exe>/data/logs`、`<exe>/webview`
#[cfg(target_os = "windows")]
fn layout(exe: &std::path::Path) -> Option<(PathBuf, PathBuf, PathBuf)> {
    let root = exe.parent()?;
    Some((
        root.join("config"),
        root.join("data").join("logs"),
        root.join("webview"),
    ))
}

#[cfg(target_os = "windows")]
fn exe_layout() -> Result<(PathBuf, PathBuf, PathBuf), String> {
    let exe = std::env::current_exe().map_err(|e| format!("无法定位程序目录：{e}"))?;
    layout(&exe).ok_or_else(|| "无法定位程序目录".to_string())
}

/// 配置目录：settings.json、connections.json 与凭据回退文件
pub fn config_dir(app: &AppHandle) -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    {
        let _ = app;
        exe_layout().map(|(config, _, _)| config)
    }
    #[cfg(not(target_os = "windows"))]
    {
        app.path()
            .app_config_dir()
            .map_err(|e| format!("无法定位配置目录：{e}"))
    }
}

/// 日志目录：client.log / client.old.log
pub fn log_dir(app: &AppHandle) -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    {
        let _ = app;
        exe_layout().map(|(_, logs, _)| logs)
    }
    #[cfg(not(target_os = "windows"))]
    {
        app.path()
            .app_log_dir()
            .map_err(|e| format!("无法定位日志目录：{e}"))
    }
}

/// WebView2 用户数据目录；WebView2 会在其下创建 `EBWebView`
#[cfg(target_os = "windows")]
pub fn webview_dir() -> Result<PathBuf, String> {
    exe_layout().map(|(_, _, webview)| webview)
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;

    #[test]
    fn 便携目录固定在可执行文件旁() {
        let (config, logs, webview) =
            layout(std::path::Path::new(r"C:\Apps\TopSAP\topsap-client.exe")).unwrap();
        assert_eq!(config, std::path::PathBuf::from(r"C:\Apps\TopSAP\config"));
        assert_eq!(logs, std::path::PathBuf::from(r"C:\Apps\TopSAP\data\logs"));
        assert_eq!(webview, std::path::PathBuf::from(r"C:\Apps\TopSAP\webview"));
    }
}
