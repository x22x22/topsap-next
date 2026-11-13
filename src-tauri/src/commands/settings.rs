//! 设置与凭据命令。

use tauri::AppHandle;
use tauri::Manager;

use crate::dto::{AppSettings, ProxyBypassStatus};
use crate::route_policy::RoutePolicySnapshot;
use crate::{route_policy, secret_store};

#[tauri::command]
pub fn load_settings(app: AppHandle) -> AppSettings {
    crate::settings::load(&app)
}

#[tauri::command]
pub fn save_settings(app: AppHandle, settings: AppSettings) -> Result<(), String> {
    let settings = settings.sanitized();
    let previous = crate::settings::load(&app);
    crate::settings::save(&app, &settings)?;
    // 只有代理绕过配置变化时才重写系统代理，避免调整路由勾选也触发一次系统调用
    if previous.proxy_bypass_enabled != settings.proxy_bypass_enabled
        || previous.proxy_bypass != settings.proxy_bypass
    {
        if let Err(err) = crate::proxy_bypass::apply(&app, &settings) {
            log::warn!("应用代理绕过失败：{err}");
        }
    }
    Ok(())
}

#[tauri::command]
pub fn proxy_bypass_status(app: AppHandle) -> ProxyBypassStatus {
    crate::proxy_bypass::status(&app)
}

/// 本次（或上次）登录下发的隧道路由与各自的生效状态，供设置页勾选
#[tauri::command]
pub fn route_policy(app: AppHandle) -> RoutePolicySnapshot {
    let (catalog, online) = app.state::<crate::AppState>().engine.route_catalog();
    route_policy::snapshot(&app, &catalog, online)
}

#[tauri::command]
pub fn load_password(app: AppHandle, user: String) -> Option<String> {
    secret_store::load_password(&app, &user)
}

/// 只回答「是否已保存密码」，供设置页判断能否开启启动自动连接，不向前端返回明文
#[tauri::command]
pub fn has_saved_password(app: AppHandle, user: String) -> bool {
    let user = user.trim();
    if user.is_empty() {
        return false;
    }
    secret_store::load_password(&app, user).is_some_and(|password| !password.is_empty())
}
