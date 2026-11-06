//! 连接相关命令。

use base64::Engine as _;
use tauri::{AppHandle, State};

use crate::auth::AuthClient;
use crate::dto::{
    CaptchaDto, ForgetPasswordResult, LineProbeDto, LoginForm, SessionSummary, StatsDto,
};
use crate::{secret_store, settings, AppState};

#[tauri::command]
pub async fn connect(
    app: AppHandle,
    state: State<'_, AppState>,
    form: LoginForm,
) -> Result<SessionSummary, String> {
    let user = form.user.trim().to_string();
    let password = form.password.clone();
    let remember = form.remember;

    if crate::commands::diag::self_test_running() {
        return Err("网卡自检正在进行中，请稍候再连接".to_string());
    }

    let summary = state.engine.connect(form).await?;

    // 连接成功后才落盘设置与凭据
    let mut current = settings::load(&app);
    current.gateway = summary.gateway.clone();
    current.port = summary.port;
    current.user = user.clone();
    current.remember = remember;
    if let Err(err) = settings::save(&app, &current) {
        state.engine.report_log(format!("保存设置失败：{err}"));
    }

    if remember {
        if let Err(err) = secret_store::save_password(&app, &user, &password) {
            state.engine.report_log(format!("记住密码失败：{err}"));
        }
    } else {
        secret_store::delete_password(&app, &user);
    }

    Ok(summary)
}

#[tauri::command]
pub async fn disconnect(state: State<'_, AppState>) -> Result<(), String> {
    state.engine.disconnect().await
}

#[tauri::command]
pub fn engine_snapshot(state: State<'_, AppState>) -> StatsDto {
    state.engine.snapshot()
}

#[tauri::command]
pub fn submit_captcha(state: State<'_, AppState>, code: String) -> bool {
    state.engine.submit_captcha(code)
}

/// 获取验证码（登录页与忘记密码共用），返回 PNG 与绑定 Cookie
#[tauri::command]
pub async fn fetch_captcha(gateway: String, port: u16) -> Result<CaptchaDto, String> {
    let gateway = gateway.trim().to_string();
    if gateway.is_empty() {
        return Err("请先填写网关地址".to_string());
    }
    let mut auth = AuthClient::new(gateway, port, String::new(), String::new());
    let png = auth.captcha().await.map_err(|e| e.user_message())?;
    Ok(CaptchaDto {
        png_base64: base64::engine::general_purpose::STANDARD.encode(&png),
        cookie: auth.cookie().unwrap_or_default().to_string(),
    })
}

/// 并发探测连接配置中的线路，按可用性与延迟排序
#[tauri::command]
pub async fn probe_lines(lines: Vec<String>, timeout: u32) -> Vec<LineProbeDto> {
    crate::session::engine::probe_lines(lines, timeout).await
}

/// 忘记密码第一步：提交用户名与图形验证码；网关可能直接发送重置信息，或要求动态验证码
#[tauri::command]
pub async fn forget_password(
    gateway: String,
    port: u16,
    user: String,
    code: String,
    cookie: String,
) -> Result<ForgetPasswordResult, String> {
    let gateway = gateway.trim().to_string();
    if gateway.is_empty() {
        return Err("请先填写网关地址".to_string());
    }
    if user.trim().is_empty() {
        return Err("请输入用户名".to_string());
    }
    if code.trim().is_empty() {
        return Err("请输入图形验证码".to_string());
    }

    let mut auth = AuthClient::new(gateway, port, String::new(), String::new());
    if !cookie.is_empty() {
        auth.set_cookie(cookie);
    }
    let result = auth
        .forget_password(user.trim(), code.trim())
        .await
        .map_err(|e| e.user_message())?;
    Ok(ForgetPasswordResult {
        target: result.target,
        need_sms: result.need_sms,
        cookie: result.cookie,
    })
}

/// 忘记密码第二步：动态验证码 + 新密码
#[tauri::command]
pub async fn reset_password(
    app: AppHandle,
    gateway: String,
    port: u16,
    user: String,
    sms: String,
    new_password: String,
    cookie: String,
) -> Result<ForgetPasswordResult, String> {
    let gateway = gateway.trim().to_string();
    if gateway.is_empty() {
        return Err("请先填写网关地址".to_string());
    }
    if user.trim().is_empty() {
        return Err("请输入用户名".to_string());
    }
    if sms.trim().is_empty() {
        return Err("请输入动态验证码".to_string());
    }
    if new_password.is_empty() {
        return Err("请输入新密码".to_string());
    }

    let mut auth = AuthClient::new(gateway.clone(), port, String::new(), String::new());
    if !cookie.is_empty() {
        auth.set_cookie(cookie);
    }
    let result = auth
        .reset_password(user.trim(), sms.trim(), &new_password)
        .await
        .map_err(|e| e.user_message())?;
    // 原网关重置密码后不一定刷新有效期；记录重置事实，登录时做一次补偿。
    let _ = crate::password_reset::mark(&app, &gateway, port, user.trim());
    // 旧密码已经失效，删除本地记住的密码，避免自动填充后登录失败。
    crate::secret_store::delete_password(&app, user.trim());
    Ok(ForgetPasswordResult {
        target: result.target,
        need_sms: result.need_sms,
        cookie: result.cookie,
    })
}

/// 未登录状态下修改密码（密码到期/首次登录/管理员要求改密）
#[tauri::command]
pub async fn change_password_for_login(
    app: AppHandle,
    gateway: String,
    port: u16,
    user: String,
    old_password: String,
    new_password: String,
) -> Result<(), String> {
    let gateway = gateway.trim().to_string();
    let user = user.trim().to_string();
    if gateway.is_empty() {
        return Err("请先填写网关地址".to_string());
    }
    if user.is_empty() {
        return Err("请输入用户名".to_string());
    }
    if old_password.is_empty() {
        return Err("请输入原密码".to_string());
    }
    if new_password.is_empty() {
        return Err("请输入新密码".to_string());
    }
    let mut auth = AuthClient::new(gateway.clone(), port, String::new(), String::new());
    auth.change_password(&user, &old_password, &new_password, "")
        .await
        .map_err(|e| e.user_message())?;
    // 改密成功即视为密码已刷新，清掉重置补偿标记与本地旧密码。
    crate::password_reset::clear(&app, &gateway, port, &user);
    crate::secret_store::delete_password(&app, &user);
    Ok(())
}

/// 提交短信/企业微信动态验证码
#[tauri::command]
pub fn submit_sms(state: State<'_, AppState>, code: String) -> bool {
    state.engine.submit_sms(code)
}

/// 用户确认是否接管冲突路由，唤醒等待中的连接流程
#[tauri::command]
pub fn submit_route_conflict(state: State<'_, AppState>, accept: bool) -> bool {
    state.engine.submit_route_conflict(accept)
}

/// 已连接状态下修改密码
#[tauri::command]
pub async fn change_password(
    state: State<'_, AppState>,
    old_password: String,
    new_password: String,
) -> Result<(), String> {
    state
        .engine
        .change_password(old_password, new_password)
        .await
}

/// 重新发送动态验证码，返回有效期（秒）
#[tauri::command]
pub async fn resend_sms(state: State<'_, AppState>) -> Result<u32, String> {
    state.engine.resend_sms().await
}
