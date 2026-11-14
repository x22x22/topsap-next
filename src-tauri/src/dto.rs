//! 跨领域 DTO：发给前端的序列化结构。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct AppInfo {
    pub version: String,
    pub platform: String,
    pub arch: String,
    pub elevated: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoginForm {
    pub gateway: String,
    #[serde(default)]
    pub port: Option<u16>,
    pub user: String,
    pub password: String,
    #[serde(default)]
    pub remember: bool,
    /// 多线路候选（host:port）；为空时使用 gateway/port
    #[serde(default)]
    pub lines: Vec<String>,
    /// 0=单线路 1=多线路优选 2=多线路备份
    #[serde(default)]
    pub connect_type: u8,
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout: u32,
}

fn default_connect_timeout() -> u32 {
    3
}

impl LoginForm {
    pub fn normalized(&self) -> Result<Self, String> {
        let gateway = self.gateway.trim().to_string();
        let user = self.user.trim().to_string();
        if gateway.is_empty() {
            return Err("请填写网关地址".to_string());
        }
        if user.is_empty() {
            return Err("请填写账号".to_string());
        }
        if self.password.is_empty() {
            return Err("请填写密码".to_string());
        }
        Ok(Self {
            gateway,
            port: Some(self.port.unwrap_or(443)),
            user,
            password: self.password.clone(),
            remember: self.remember,
            lines: self
                .lines
                .iter()
                .map(|line| line.trim().to_string())
                .filter(|line| !line.is_empty())
                .collect(),
            connect_type: self.connect_type.min(2),
            connect_timeout: self.connect_timeout.clamp(1, 60),
        })
    }

    pub fn port_or_default(&self) -> u16 {
        self.port.unwrap_or(443)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionSummary {
    pub user: String,
    pub vip: String,
    pub mask: String,
    pub dns1: String,
    pub dns2: String,
    pub routes: usize,
    pub hosts: usize,
    pub acl: usize,
    pub tunnel_type: i32,
    pub gateway: String,
    pub port: u16,
    pub gateway_version: String,
    pub login_info: LoginInfoDto,
    pub resources: Vec<ResourceDto>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct LoginInfoDto {
    pub current_ip: String,
    pub current_time: String,
    pub previous_ip: String,
    pub previous_time: String,
    pub failed_ip: String,
    pub failed_time: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ResourceDto {
    pub name: String,
    pub url: String,
    pub group: String,
    pub module: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct StatsDto {
    pub vip: String,
    pub routes: usize,
    pub duration_secs: u64,
    pub bytes_up: u64,
    pub bytes_down: u64,
    pub packets_in: u64,
    pub packets_out: u64,
    pub dropped_out: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SelfTestStepDto {
    pub name: String,
    pub status: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CaptchaDto {
    pub png_base64: String,
    pub cookie: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LineProbeDto {
    pub line: String,
    pub host: String,
    pub port: u16,
    pub latency_ms: u32,
    pub ok: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForgetPasswordResult {
    /// 接收重置信息的邮箱或手机号，空表示网关未提供
    pub target: String,
    /// 需要继续输入动态验证码并设置新密码
    pub need_sms: bool,
    /// 第二步请求必须复用的 Cookie
    pub cookie: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SelfTestDto {
    pub ok: bool,
    pub passed: usize,
    pub total: usize,
    pub steps: Vec<SelfTestStepDto>,
}

/// 与其他接口完全同前缀的现有路由，供接管确认弹窗展示
#[derive(Debug, Clone, Serialize)]
pub struct RouteConflictDto {
    pub dest: String,
    pub mask: String,
    pub prefix: u8,
    pub gateway: String,
    pub ifname: String,
    pub kind: String,
}

impl From<crate::netcfg::RouteConflict> for RouteConflictDto {
    fn from(value: crate::netcfg::RouteConflict) -> Self {
        Self {
            dest: value.dest,
            mask: value.mask,
            prefix: value.prefix,
            gateway: value.gateway,
            ifname: value.ifname,
            kind: value.kind.as_str().to_string(),
        }
    }
}

/// 引擎向界面推送的事件，统一走 `engine://event` 通道
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EngineEvent {
    State {
        state: String,
        label: String,
        detail: String,
        /// 本次状态变化是否源自异常断开（掉线、重连失败），供界面区分人为断开
        abnormal: bool,
    },
    Log {
        line: String,
    },
    Stats {
        stats: StatsDto,
    },
    Captcha {
        png_base64: String,
    },
    SmsChallenge {
        challenge: bool,
        /// 重试时给出的原因（如验证码错误），首次弹出为空
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    Error {
        message: String,
    },
    /// 网关要求先修改密码（到期、过弱、首次登录或管理员要求）
    PasswordChangeRequired {
        code: i32,
        user: String,
        host: String,
        port: u16,
        message: String,
    },
    /// 登录成功后的账号/密码到期提醒，0 表示不提醒
    ExpiryReminder {
        account_end: i64,
        password_remind: i64,
    },
    /// 检测到其他接口已占用本次要注入的网段，等待用户确认是否接管
    RouteConflict {
        total: usize,
        conflicts: Vec<RouteConflictDto>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub gateway: String,
    pub port: u16,
    pub user: String,
    pub remember: bool,
    pub auto_reconnect: bool,
    /// 断线自动重连的最大次数；0 表示不限次数
    pub auto_reconnect_max_attempts: u32,
    pub close_to_tray: bool,
    /// 启动软件后自动使用默认连接配置与已保存密码连接
    pub auto_connect_on_start: bool,
    /// system / dark / light
    pub theme: String,
    /// 是否把代理绕过条目写入系统代理设置
    pub proxy_bypass_enabled: bool,
    /// 域名/泛域名/IP 绕过列表，例如 *.sunline.cn
    pub proxy_bypass: Vec<String>,
    /// macOS：关闭窗口后同时隐藏 Dock 图标，仅保留菜单栏托盘
    pub hide_dock_when_closed: bool,
    /// 不写入系统路由表的网段，格式 `目的地址/前缀`，例如 10.0.0.0/8
    pub disabled_routes: Vec<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            gateway: String::new(),
            port: 443,
            user: String::new(),
            remember: false,
            auto_reconnect: true,
            auto_reconnect_max_attempts: 5,
            close_to_tray: true,
            auto_connect_on_start: false,
            theme: "system".to_string(),
            proxy_bypass_enabled: true,
            proxy_bypass: crate::proxy_bypass::default_entries(),
            hide_dock_when_closed: false,
            disabled_routes: Vec::new(),
        }
    }
}

/// 系统代理绕过的当前生效状态，供设置窗口展示。
#[derive(Debug, Clone, Serialize)]
pub struct ProxyBypassStatus {
    pub supported: bool,
    pub enabled: bool,
    pub entries: Vec<String>,
    pub defaults: Vec<String>,
    pub applied: bool,
    pub message: String,
}

impl AppSettings {
    /// 落盘前做一次清洗，避免把非法值写进配置文件
    pub fn sanitized(mut self) -> Self {
        self.gateway = self.gateway.trim().to_string();
        self.user = self.user.trim().to_string();
        if self.port == 0 {
            self.port = 443;
        }
        if !matches!(self.theme.as_str(), "system" | "dark" | "light") {
            self.theme = "system".to_string();
        }
        self.auto_reconnect_max_attempts = self.auto_reconnect_max_attempts.min(1000);
        self.proxy_bypass = crate::proxy_bypass::sanitize_entries(&self.proxy_bypass);
        self.disabled_routes = crate::route_policy::sanitize_disabled(&self.disabled_routes);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 登录表单_必填校验() {
        let mut form = LoginForm {
            gateway: "  ".into(),
            port: None,
            user: "u".into(),
            password: "p".into(),
            remember: false,
            lines: vec![],
            connect_type: 0,
            connect_timeout: 3,
        };
        assert!(form.normalized().is_err());

        form.gateway = " vpn.example.com ".into();
        let ok = form.normalized().unwrap();
        assert_eq!(ok.gateway, "vpn.example.com");
        assert_eq!(ok.port, Some(443));
    }

    #[test]
    fn 登录表单_密码为空报错() {
        let form = LoginForm {
            gateway: "h".into(),
            port: Some(8443),
            user: "u".into(),
            password: String::new(),
            remember: false,
            lines: vec![],
            connect_type: 0,
            connect_timeout: 3,
        };
        assert!(form.normalized().is_err());
    }

    #[test]
    fn 设置清洗_非法主题回退() {
        let s = AppSettings {
            theme: "neon".into(),
            port: 0,
            ..Default::default()
        };
        let s = s.sanitized();
        assert_eq!(s.theme, "system");
        assert_eq!(s.port, 443);
    }

    #[test]
    fn 设置缺字段时使用默认值() {
        let s: AppSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(s.port, 443);
        assert!(s.auto_reconnect);
        assert_eq!(s.auto_reconnect_max_attempts, 5);
        assert!(!s.hide_dock_when_closed);
        assert!(s.disabled_routes.is_empty());
    }

    #[test]
    fn 设置清洗_关闭路由归一化并去重() {
        let s = AppSettings {
            disabled_routes: vec![
                " 10.1.2.3/8 ".into(),
                "10.0.0.0/8".into(),
                "not-a-cidr".into(),
            ],
            ..Default::default()
        };
        let s = s.sanitized();
        assert_eq!(s.disabled_routes, vec!["10.0.0.0/8".to_string()]);
    }
}
