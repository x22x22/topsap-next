//! 设置与凭据命令。

use tauri::AppHandle;

use crate::dto::{AppSettings, ProxyBypassStatus};
use crate::secret_store;

#[tauri::command]
pub fn load_settings(app: AppHandle) -> AppSettings {
    crate::settings::load(&app)
}

#[tauri::command]
pub fn save_settings(app: AppHandle, settings: AppSettings) -> Result<(), String> {
    let settings = settings.sanitized();
    crate::settings::save(&app, &settings)?;
    if let Err(err) = crate::proxy_bypass::apply(&app, &settings) {
        log::warn!("应用代理绕过失败：{err}");
    }
    Ok(())
}

#[tauri::command]
pub fn proxy_bypass_status(app: AppHandle) -> ProxyBypassStatus {
    crate::proxy_bypass::status(&app)
}

#[tauri::command]
pub fn load_password(app: AppHandle, user: String) -> Option<String> {
    secret_store::load_password(&app, &user)
}
