//! 启动时自动连接。
//!
//! 仅在同时满足以下条件时执行，任何一项不满足都只写运行日志、不打扰用户：
//! 1. 设置中开启 `auto_connect_on_start`；
//! 2. 存在连接配置（默认配置优先，否则取第一条）；
//! 3. 已保存账号与密码（密码从安全存储读取，不经过前端）。
//!
//! 自动连接与手动连接共用 `Engine::connect`，验证码、路由冲突、失败提示等
//! 流程完全一致；并发时由引擎内部的 `connecting` 原子锁保证只有一方执行。

use std::time::Duration;

use tauri::Manager;

use crate::dto::{AppSettings, LoginForm};
use crate::profiles::{self, ConnectionProfile};

/// 等窗口首帧与启动补注销完成后再发起连接，避免启动瞬间的状态竞争
const STARTUP_DELAY: Duration = Duration::from_millis(800);

/// 满足条件时安排一次启动自动连接；不满足时写日志并直接返回。
pub fn schedule(app: &tauri::AppHandle) {
    let settings = crate::settings::load(app);
    if !settings.auto_connect_on_start {
        return;
    }
    let engine = app.state::<crate::AppState>().engine.clone();
    let form = match prepare(app, &settings) {
        Ok(form) => form,
        Err(reason) => {
            log::info!("启动自动连接已跳过：{reason}");
            engine.report_log(format!("启动自动连接已跳过：{reason}"));
            return;
        }
    };
    engine.report_log(format!(
        "启动自动连接：正在连接 {}:{}",
        form.gateway, form.user
    ));
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(STARTUP_DELAY).await;
        if let Err(err) = engine.connect(form).await {
            // connect 已把失败原因写入运行日志并更新状态，这里只补一条来源说明
            log::warn!("启动自动连接失败：{err}");
            engine.report_log(format!("启动自动连接失败：{err}"));
        }
    });
}

/// 读取密码与默认连接配置并组装登录表单；条件缺失时返回可读原因。
fn prepare(app: &tauri::AppHandle, settings: &AppSettings) -> Result<LoginForm, String> {
    let user = settings.user.trim();
    if user.is_empty() {
        return Err("未保存账号".to_string());
    }
    let Some(password) = crate::secret_store::load_password(app, user) else {
        return Err("未保存密码".to_string());
    };
    let store = profiles::load(app);
    let profile = store
        .connections
        .iter()
        .find(|item| item.name == store.active)
        .or_else(|| store.connections.first())
        .ok_or_else(|| "没有可用的连接配置".to_string())?;
    build_form(settings, profile, password).ok_or_else(|| "连接配置缺少有效线路".to_string())
}

/// 纯函数形式的表单组装，便于单测覆盖各项缺失条件。
pub(crate) fn build_form(
    settings: &AppSettings,
    profile: &ConnectionProfile,
    password: String,
) -> Option<LoginForm> {
    let user = settings.user.trim();
    if user.is_empty() || password.is_empty() || profile.lines.is_empty() {
        return None;
    }
    let Ok((host, port)) = profiles::parse_line(&profile.lines[0]) else {
        return None;
    };
    Some(LoginForm {
        gateway: host,
        port: Some(port),
        user: user.to_string(),
        password,
        remember: settings.remember,
        lines: profile.lines.clone(),
        connect_type: profile.connect_type,
        connect_timeout: profile.connect_timeout,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(lines: Vec<&str>) -> ConnectionProfile {
        ConnectionProfile {
            name: "办公室".to_string(),
            login_way: "pwd".to_string(),
            connect_type: 1,
            connect_select: true,
            connect_timeout: 5,
            lines: lines.into_iter().map(str::to_string).collect(),
        }
    }

    fn settings(user: &str) -> AppSettings {
        AppSettings {
            user: user.to_string(),
            remember: true,
            auto_connect_on_start: true,
            ..Default::default()
        }
    }

    #[test]
    fn 条件齐全时组装出默认配置的登录表单() {
        let form = build_form(
            &settings(" demo "),
            &profile(vec!["vpn.example.com:8443", "vpn2.example.com:443"]),
            "secret".to_string(),
        )
        .expect("应能组装表单");
        assert_eq!(form.gateway, "vpn.example.com");
        assert_eq!(form.port, Some(8443));
        assert_eq!(form.user, "demo");
        assert_eq!(form.password, "secret");
        assert_eq!(form.connect_type, 1);
        assert_eq!(form.connect_timeout, 5);
        assert_eq!(form.lines.len(), 2);
    }

    #[test]
    fn 缺少账号密码或线路时跳过() {
        let valid_profile = profile(vec!["vpn.example.com:443"]);
        assert!(build_form(&settings(""), &valid_profile, "secret".into()).is_none());
        assert!(build_form(&settings("demo"), &valid_profile, String::new()).is_none());
        assert!(build_form(&settings("demo"), &profile(vec![]), "secret".into()).is_none());
    }

    #[test]
    fn 线路格式非法时跳过() {
        assert!(build_form(&settings("demo"), &profile(vec![""]), "secret".into()).is_none());
    }
}
