//! 应用信息、版本检查与日志目录。

use tauri::AppHandle;

use crate::dto::AppInfo;
use crate::netcfg::is_elevated;

#[tauri::command]
pub async fn app_info(app: AppHandle) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        platform: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        elevated: is_elevated().await,
    }
}

/// 检查发布目录中的最新版本；区分“无更新”和“检查失败”
#[tauri::command]
pub async fn check_update() -> crate::update::UpdateCheck {
    crate::update::check_result().await
}

/// 打开日志目录，返回路径供界面展示
#[tauri::command]
pub fn open_log_dir(app: AppHandle) -> Result<String, String> {
    let dir = crate::paths::log_dir(&app)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建日志目录失败：{e}"))?;

    #[cfg(target_os = "windows")]
    let opener = "explorer";
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(target_os = "linux")]
    let opener = "xdg-open";

    std::process::Command::new(opener)
        .arg(&dir)
        .spawn()
        .map_err(|e| format!("打开日志目录失败：{e}"))?;

    Ok(dir.display().to_string())
}

/// 打开资源链接（http/https/smb），由系统默认程序处理
#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    let url = url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://") || url.starts_with("smb://")) {
        return Err("不支持的链接协议".to_string());
    }
    if url
        .chars()
        .any(|ch| ch.is_whitespace() || ch == '"' || ch == '\'' || ch.is_control())
    {
        return Err("链接包含非法字符".to_string());
    }

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("rundll32.exe");
        command.arg("url.dll,FileProtocolHandler").arg(url);
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("open");
        command.arg(url);
        command
    };
    #[cfg(target_os = "linux")]
    let mut command = {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(url);
        command
    };

    command
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("打开资源失败：{e}"))
}
