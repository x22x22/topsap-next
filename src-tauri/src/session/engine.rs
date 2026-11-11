//! 连接生命周期编排：登录 → 建卡 → 隧道 → 路由 → 数据泵 → 断开/重连。
//!
//! 锁纪律：`std::sync::Mutex` 只在同步块内短持有，绝不跨 `.await`。

use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine as _;
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::auth::error::password_change_message;
use crate::auth::{AuthClient, AuthError, AuthParam, LoginSession};
use crate::dto::{
    EngineEvent, LineProbeDto, LoginForm, LoginInfoDto, ResourceDto, RouteConflictDto,
    SessionSummary, StatsDto,
};
use crate::netcfg::NetConfig;
use crate::password_reset;
use crate::session_guard;
use crate::tls;
use crate::tun::{TunDevice, ADAPTER_NAME};
use crate::tunnel::{self, TunnelEvent, TunnelHandle};

use super::health::{HealthIssue, HealthSample, HealthTracker};
use super::state::Phase;

const CAPTCHA_TIMEOUT: Duration = Duration::from_secs(120);
const SMS_TIMEOUT: Duration = Duration::from_secs(300);
const ROUTE_CONFLICT_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_RECONNECT: u32 = 5;
const GATEWAY_RESOLVE_TIMEOUT: Duration = Duration::from_secs(3);
const HEALTH_RECONNECT_MAX: u32 = 3;
const HEALTH_VERIFY_TIMEOUT: Duration = Duration::from_secs(10);
const HEALTH_RECOVERY_COOLDOWN: Duration = Duration::from_secs(90);
const HEALTH_RECOVERY_WINDOW: Duration = Duration::from_secs(600);
const HEALTH_RECOVERY_LIMIT: u32 = 3;
const HEALTH_FAIL_MESSAGE: &str = "网关未恢复数据转发，请重新登录";

struct Inner {
    phase: Phase,
    detail: String,
    session: Option<LoginSession>,
    tunnel: Option<TunnelHandle>,
    tunnel_generation: u64,
    tun: Option<Arc<TunDevice>>,
    net: Option<NetConfig>,
    started_at: Option<Instant>,
    user_disconnect: bool,
    cancel: CancellationToken,
    stats_task: Option<JoinHandle<()>>,
    health_task: Option<JoinHandle<()>>,
    uplink_task: Option<JoinHandle<()>>,
    tunnel_task: Option<JoinHandle<()>>,
    reader_stop: Option<Arc<AtomicBool>>,
    reader_handle: Option<std::thread::JoinHandle<()>>,
    pending_sms: Option<PendingSms>,
    /// 登录响应下发的完整 Cookie，修改密码等会话操作复用
    session_cookie: Option<String>,
    /// 本次登录使用的账号，用于修改密码
    login_user: String,
}

#[derive(Debug, Clone)]
struct PendingSms {
    host: String,
    port: u16,
    user: String,
    /// 登录时使用的图形验证码，重发短信时作为 Gid
    captcha: String,
    /// 动态验证码有效期（秒），提交短信登录时作为 SMS_TIME
    sms_time: u32,
    /// 与本次登录绑定的 cgid/session cookie，重发验证码时必须复用
    cookie: Option<String>,
}

impl Inner {
    fn new() -> Self {
        Self {
            phase: Phase::Idle,
            detail: String::new(),
            session: None,
            tunnel: None,
            tunnel_generation: 0,
            tun: None,
            net: None,
            started_at: None,
            user_disconnect: false,
            cancel: CancellationToken::new(),
            stats_task: None,
            health_task: None,
            uplink_task: None,
            tunnel_task: None,
            reader_stop: None,
            reader_handle: None,
            pending_sms: None,
            session_cookie: None,
            login_user: String::new(),
        }
    }
}

pub struct Engine {
    app: AppHandle,
    dll_path: PathBuf,
    inner: Mutex<Inner>,
    captcha: Mutex<Option<oneshot::Sender<String>>>,
    sms: Mutex<Option<oneshot::Sender<String>>>,
    route_conflict: Mutex<Option<oneshot::Sender<bool>>>,
    /// 网关下发的登录失败次数，跨连接保留以复现 auto 验证码策略
    login_err_num: Mutex<std::collections::HashMap<String, u32>>,
    /// 防止用户连点或事件延迟导致并发连接
    connecting: AtomicBool,
    /// 防止多个 Closed 事件并发启动重连循环
    reconnecting: AtomicBool,
    /// 最近一次已关闭的隧道代际
    closed_generation: AtomicU64,
    /// 上行报文发送失败计数，用于限频日志
    uplink_drops: AtomicU64,
    /// 本轮连接内进入上行泵的 TCP / ICMP 包数，用于判断是否应当出现下行响应
    health_tcp_in: AtomicU64,
    health_icmp_in: AtomicU64,
    /// 健康恢复的冷却与次数限制，避免网关持续不转发时无限重连
    health_recovery: Mutex<HealthRecoveryState>,
    /// 自动重连关闭时的提示限频
    health_notice_at: Mutex<Option<Instant>>,
}

#[derive(Default)]
struct HealthRecoveryState {
    last_at: Option<Instant>,
    window_start: Option<Instant>,
    count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecoveryGate {
    Allowed,
    Cooling,
    Exhausted,
}

/// 离开作用域时自动清除「连接中」标记，异常路径也不会卡死
struct BusyGuard<'a>(&'a AtomicBool);

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// 离开作用域时自动清除「重连中」标记，异常路径也不会留下并发循环
struct ReconnectGuard<'a>(&'a AtomicBool);

impl Drop for ReconnectGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl Engine {
    pub fn new(app: AppHandle, dll_path: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            app,
            dll_path,
            inner: Mutex::new(Inner::new()),
            captcha: Mutex::new(None),
            sms: Mutex::new(None),
            route_conflict: Mutex::new(None),
            login_err_num: Mutex::new(std::collections::HashMap::new()),
            connecting: AtomicBool::new(false),
            reconnecting: AtomicBool::new(false),
            closed_generation: AtomicU64::new(0),
            uplink_drops: AtomicU64::new(0),
            health_tcp_in: AtomicU64::new(0),
            health_icmp_in: AtomicU64::new(0),
            health_recovery: Mutex::new(HealthRecoveryState::default()),
            health_notice_at: Mutex::new(None),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn captcha_lock(&self) -> std::sync::MutexGuard<'_, Option<oneshot::Sender<String>>> {
        self.captcha.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn sms_lock(&self) -> std::sync::MutexGuard<'_, Option<oneshot::Sender<String>>> {
        self.sms.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn route_conflict_lock(&self) -> std::sync::MutexGuard<'_, Option<oneshot::Sender<bool>>> {
        self.route_conflict
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    fn remembered_login_err_num(&self, host: &str, port: u16, user: &str) -> u32 {
        let key = login_err_key(host, port, user);
        self.login_err_num
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key)
            .copied()
            .unwrap_or(0)
    }

    fn remember_login_err_num(&self, host: &str, port: u16, user: &str, count: u32) {
        let key = login_err_key(host, port, user);
        let mut map = self.login_err_num.lock().unwrap_or_else(|e| e.into_inner());
        if count == 0 {
            map.remove(&key);
        } else {
            map.insert(key, count);
        }
    }

    fn emit(&self, event: EngineEvent) {
        let _ = self.app.emit("engine://event", event);
    }

    fn set_phase(&self, phase: Phase, detail: impl Into<String>) {
        let detail = detail.into();
        {
            let mut inner = self.lock();
            inner.phase = phase;
            inner.detail = detail.clone();
        }
        self.emit(EngineEvent::State {
            state: phase.key().to_string(),
            label: phase.label().to_string(),
            detail,
        });
    }

    fn log(&self, message: impl Into<String>) {
        let line = format!(
            "[{}] {}",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            message.into()
        );
        self.emit(EngineEvent::Log { line: line.clone() });
        self.write_log_file(&line);
    }

    /// 供命令层记录与连接流程相关的附加信息
    pub fn report_log(&self, message: impl Into<String>) {
        self.log(message);
    }

    fn write_log_file(&self, line: &str) {
        use std::io::Write;

        let Ok(dir) = crate::paths::log_dir(&self.app) else {
            return;
        };
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("client.log");

        // 2 MB 轮转一次，保留上一份，避免无限增长
        if let Ok(meta) = std::fs::metadata(&path) {
            if meta.len() > 2 * 1024 * 1024 {
                let _ = std::fs::rename(&path, dir.join("client.old.log"));
            }
        }
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            let _ = writeln!(file, "{line}");
        }
    }

    pub fn phase(&self) -> (String, String) {
        let inner = self.lock();
        (inner.phase.key().to_string(), inner.detail.clone())
    }

    pub fn is_connected(&self) -> bool {
        self.lock().phase.connected()
    }

    fn note_uplink_drop(&self) {
        if !self.is_connected() {
            return;
        }
        let count = self.uplink_drops.fetch_add(1, Ordering::Relaxed) + 1;
        if count == 1 || count % 1000 == 0 {
            self.log(format!(
                "上行报文发送失败，已丢弃 {count} 个（隧道未就绪或发送队列已满）"
            ));
        }
    }

    /// 当前连接使用的 TUN 接口名；未创建网卡时为空
    pub fn tun_name(&self) -> Option<String> {
        self.lock()
            .tun
            .as_ref()
            .map(|tun| tun.interface_name().to_string())
    }

    /// 是否处于连接流程中（含重连），用于拒绝并发的建卡操作
    pub fn is_busy(&self) -> bool {
        let inner = self.lock();
        inner.phase.busy() || inner.phase == Phase::Reconnecting
    }

    /// 是否存在需要清理的系统资源（网卡 / 隧道 / 路由账本）
    pub fn needs_cleanup(&self) -> bool {
        let inner = self.lock();
        inner.tun.is_some() || inner.tunnel.is_some() || inner.net.is_some()
    }

    /// 启动或连接前补注销上次异常退出留下的会话
    pub async fn purge_previous_session(&self) {
        self.cleanup_pending_tun().await;
        match session_guard::purge(&self.app).await {
            Ok(true) => self.log("检测到上次会话未正常注销，已补注销"),
            Ok(false) => {}
            Err(err) => self.log(format!("上次会话补注销失败：{err}")),
        }
    }

    /// 清理上次异常退出留下的、属于本客户端的 utun；地址不一致时不触碰
    async fn cleanup_pending_tun(&self) {
        let Some((name, vip)) = session_guard::pending_utun(&self.app) else {
            return;
        };
        if !crate::netcfg::is_utun_name(&name) {
            return;
        }
        let Some(found) = crate::netcfg::utun_interfaces()
            .await
            .into_iter()
            .find(|item| item.name == name)
        else {
            return;
        };
        if found.address != vip {
            self.log(format!(
                "上次会话网卡 {name} 当前地址为 {}，与记录的 {vip} 不一致，跳过清理",
                found.address
            ));
            return;
        }
        match crate::netcfg::cleanup_utun(&name).await {
            Ok(result) => self.log(format!("清理上次会话残留网卡：{}", result.message)),
            Err(err) => self.log(format!("清理上次会话残留网卡失败：{err}")),
        }
    }

    pub fn snapshot(&self) -> StatsDto {
        let inner = self.lock();
        let stats = inner.tunnel.as_ref().map(|t| t.stats()).unwrap_or_default();
        let (packets_in, packets_out, dropped_out) = inner
            .tun
            .as_ref()
            .map(|t| (t.packets_in(), t.packets_out(), t.dropped_out()))
            .unwrap_or_default();

        StatsDto {
            vip: inner
                .session
                .as_ref()
                .map(|s| s.vip.clone())
                .unwrap_or_default(),
            routes: inner.net.as_ref().map(|n| n.route_count()).unwrap_or(0),
            duration_secs: inner.started_at.map(|t| t.elapsed().as_secs()).unwrap_or(0),
            bytes_up: stats.bytes_up,
            bytes_down: stats.bytes_down,
            packets_in,
            packets_out,
            dropped_out,
        }
    }

    fn note_uplink_packet(&self, packet: &[u8]) {
        match ipv4_protocol(packet) {
            Some(6) => {
                self.health_tcp_in.fetch_add(1, Ordering::Relaxed);
            }
            Some(1) => {
                self.health_icmp_in.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
    }

    fn health_sample(&self) -> Option<HealthSample> {
        let inner = self.lock();
        let tunnel = inner.tunnel.as_ref()?;
        let tun = inner.tun.as_ref()?;
        let stats = tunnel.stats();
        Some(HealthSample {
            generation: inner.tunnel_generation,
            packets_in: tun.packets_in(),
            packets_up: stats.packets_up,
            packets_down: stats.packets_down,
            packets_out: tun.packets_out(),
            dropped_out: tun.dropped_out(),
            uplink_drops: self.uplink_drops.load(Ordering::Relaxed),
            tcp_in: self.health_tcp_in.load(Ordering::Relaxed),
            icmp_in: self.health_icmp_in.load(Ordering::Relaxed),
        })
    }

    fn begin_health_recovery(&self) -> RecoveryGate {
        let mut state = self
            .health_recovery
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        if let Some(window_start) = state.window_start {
            if now.duration_since(window_start) > HEALTH_RECOVERY_WINDOW {
                state.window_start = Some(now);
                state.count = 0;
            }
        } else {
            state.window_start = Some(now);
        }
        if let Some(last_at) = state.last_at {
            if now.duration_since(last_at) < HEALTH_RECOVERY_COOLDOWN {
                return RecoveryGate::Cooling;
            }
        }
        if state.count >= HEALTH_RECOVERY_LIMIT {
            return RecoveryGate::Exhausted;
        }
        state.count += 1;
        state.last_at = Some(now);
        RecoveryGate::Allowed
    }

    fn health_notice_due(&self) -> bool {
        let mut last = self
            .health_notice_at
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        if let Some(at) = *last {
            if now.duration_since(at) < HEALTH_RECOVERY_COOLDOWN {
                return false;
            }
        }
        *last = Some(now);
        true
    }

    fn reset_health_state(&self) {
        let mut state = self
            .health_recovery
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *state = HealthRecoveryState::default();
        *self
            .health_notice_at
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
        self.health_tcp_in.store(0, Ordering::Relaxed);
        self.health_icmp_in.store(0, Ordering::Relaxed);
    }

    fn on_health_issue(self: &Arc<Self>, issue: HealthIssue) {
        if self.reconnecting.swap(true, Ordering::SeqCst) {
            return;
        }
        let engine = self.clone();
        tokio::spawn(async move {
            let _guard = ReconnectGuard(&engine.reconnecting);
            engine.recover_data_plane(issue).await;
        });
    }

    /// 用户提交验证码，唤醒等待中的登录流程
    pub fn submit_captcha(&self, code: String) -> bool {
        let code = code.trim().to_string();
        if code.is_empty() {
            return false;
        }
        match self.captcha_lock().take() {
            Some(tx) => tx.send(code).is_ok(),
            None => false,
        }
    }

    pub fn submit_sms(&self, code: String) -> bool {
        let code = code.trim().to_string();
        if code.is_empty() {
            return false;
        }
        match self.sms_lock().take() {
            Some(tx) => tx.send(code).is_ok(),
            None => false,
        }
    }

    /// 用户确认是否接管路由冲突，唤醒等待中的连接流程
    pub fn submit_route_conflict(&self, accept: bool) -> bool {
        match self.route_conflict_lock().take() {
            Some(tx) => tx.send(accept).is_ok(),
            None => false,
        }
    }

    /// 重新发送动态验证码，返回网关给出的有效期（秒）
    pub async fn resend_sms(&self) -> Result<u32, String> {
        let pending = { self.lock().pending_sms.clone() };
        let Some(pending) = pending else {
            return Err("当前没有等待中的动态验证码".to_string());
        };

        let mut auth = AuthClient::new(
            pending.host.clone(),
            pending.port,
            pending.user.clone(),
            String::new(),
        );
        if let Some(cookie) = &pending.cookie {
            auth.set_cookie(cookie.clone());
        }
        let result = auth
            .send_sms(&pending.user, &pending.captcha)
            .await
            .map_err(|e| e.user_message())?;
        if let Some(current) = self.lock().pending_sms.as_mut() {
            current.sms_time = result.sms_time;
            // 重发响应可能刷新 topsecsvkkk/topsecsveee，后续提交必须复用新 Cookie
            current.cookie = auth.cookie().map(str::to_string);
        }
        self.log(format!(
            "动态验证码已重新发送，有效期 {} 秒",
            result.valid_time
        ));
        Ok(result.valid_time)
    }

    /// 已连接状态下修改密码，复用登录会话 Cookie。
    /// 原客户端改密成功后注销会话；同时清掉本地记住的旧密码，避免下次登录失败。
    pub async fn change_password(
        self: &Arc<Self>,
        old_password: String,
        new_password: String,
    ) -> Result<(), String> {
        if old_password.is_empty() {
            return Err("请输入原密码".to_string());
        }
        if new_password.is_empty() {
            return Err("请输入新密码".to_string());
        }

        let (gateway, port, user, session_id, cookie) = {
            let inner = self.lock();
            let session = inner
                .session
                .as_ref()
                .ok_or_else(|| "请先连接网关后再修改密码".to_string())?;
            let user = if inner.login_user.is_empty() {
                session.user_name.clone()
            } else {
                inner.login_user.clone()
            };
            (
                session.gateway.clone(),
                session.port,
                user,
                session.session_id.clone(),
                inner.session_cookie.clone(),
            )
        };
        if user.is_empty() {
            return Err("无法确定当前登录账号".to_string());
        }
        let Some(cookie) = cookie else {
            return Err("登录会话已失效，请重新连接后再修改密码".to_string());
        };

        let mut auth = AuthClient::new(gateway.clone(), port, String::new(), String::new());
        auth.set_cookie(cookie);
        auth.change_password(&user, &old_password, &new_password, &session_id)
            .await
            .map_err(|e| e.user_message())?;
        password_reset::clear(&self.app, &gateway, port, &user);
        crate::secret_store::delete_password(&self.app, &user);
        self.log("密码修改成功，登录会话将注销");
        // 与原生客户端一致：改密后会话失效，主动注销并回到未连接状态。
        self.disconnect().await
    }

    /// 用当前密码做一次无会话改密，刷新网关侧密码有效期。
    async fn refresh_expired_password(
        &self,
        host: &str,
        port: u16,
        user: &str,
        password: &str,
    ) -> bool {
        let mut auth = AuthClient::new(host, port, String::new(), String::new());
        match auth.change_password(user, password, password, "").await {
            Ok(()) => true,
            Err(err) => {
                self.log(format!("密码有效期自动刷新失败：{}", err.user_message()));
                false
            }
        }
    }

    async fn wait_sms(&self) -> Result<String, String> {
        let (tx, rx) = oneshot::channel();
        *self.sms_lock() = Some(tx);
        let cancel = { self.lock().cancel.clone() };

        tokio::select! {
            _ = cancel.cancelled() => {
                let _ = self.sms_lock().take();
                Err("连接已取消".to_string())
            }
            result = tokio::time::timeout(SMS_TIMEOUT, rx) => {
                match result {
                    Ok(Ok(code)) => Ok(code),
                    Ok(Err(_)) => Err("动态验证码输入已取消".to_string()),
                    Err(_) => {
                        let _ = self.sms_lock().take();
                        Err(format!(
                            "动态验证码输入超时（{} 秒）",
                            SMS_TIMEOUT.as_secs()
                        ))
                    }
                }
            }
        }
    }

    /// 密码校验通过后，等待并提交短信/企业微信动态验证码，最多重试 3 次
    async fn complete_sms_login(
        self: &Arc<Self>,
        auth: &mut AuthClient,
        form: &LoginForm,
        captcha: &str,
        sms_time: u32,
        mut challenge: bool,
    ) -> Result<LoginSession, String> {
        {
            let cookie = auth.cookie().map(|value| {
                let value = value.to_string();
                if value.contains("sv_un=") {
                    value
                } else {
                    format!("{value}; sv_un={}", form.user)
                }
            });
            let mut inner = self.lock();
            inner.pending_sms = Some(PendingSms {
                host: form.gateway.clone(),
                port: form.port_or_default(),
                user: form.user.clone(),
                captcha: captcha.to_string(),
                sms_time,
                cookie,
            });
        }

        let mut attempt = 0u32;
        let mut kick_attempted = false;
        // 首次弹出无提示；验证码被网关拒绝后重新弹出时带上原因
        let mut notice: Option<String> = None;
        loop {
            attempt += 1;
            self.set_phase(
                Phase::SmsPending,
                notice.clone().unwrap_or_else(|| {
                    if challenge {
                        "请输入动态口令".to_string()
                    } else {
                        "请输入动态验证码".to_string()
                    }
                }),
            );
            self.emit(EngineEvent::SmsChallenge {
                challenge,
                message: notice.clone(),
            });

            let code = match self.wait_sms().await {
                Ok(code) => code,
                Err(err) => {
                    self.lock().pending_sms = None;
                    return Err(err);
                }
            };

            let cookie = {
                let inner = self.lock();
                inner
                    .pending_sms
                    .as_ref()
                    .and_then(|pending| pending.cookie.clone())
            };
            if let Some(cookie) = cookie {
                auth.set_cookie(cookie);
            }
            match auth.login_by_sms(&form.user, &code).await {
                Ok(session) => {
                    self.lock().pending_sms = None;
                    return Ok(session);
                }
                Err(AuthError::Rejected { code: -40045, .. }) if attempt < 3 => {
                    self.log("动态验证码错误或已过期，请重新输入");
                    notice = Some("验证码错误或已过期，请重新输入".to_string());
                    continue;
                }
                Err(AuthError::NeedSms { .. }) if attempt < 3 => {
                    self.log("动态验证码错误，请重新输入");
                    notice = Some("验证码错误或已过期，请重新输入".to_string());
                    continue;
                }
                Err(AuthError::Rejected { code: -40077, .. }) if !kick_attempted => {
                    kick_attempted = true;
                    self.log("账号已在别处登录，正在踢出旧会话并继续登录");
                    match auth.kick_user().await {
                        Ok(session) => {
                            self.lock().pending_sms = None;
                            return Ok(session);
                        }
                        Err(AuthError::NeedSms {
                            challenge: next_challenge,
                            sms_time,
                        }) => {
                            challenge = next_challenge;
                            let cookie = auth.cookie().map(str::to_string);
                            if let Some(pending) = self.lock().pending_sms.as_mut() {
                                pending.sms_time = sms_time;
                                if cookie.is_some() {
                                    pending.cookie = cookie;
                                }
                            }
                            notice = Some("验证码已失效，请重新输入".to_string());
                            continue;
                        }
                        Err(AuthError::PasswordChangeRequired { code, user }) => {
                            let message = password_change_message(code);
                            let user = if user.trim().is_empty() {
                                form.user.clone()
                            } else {
                                user
                            };
                            self.emit(EngineEvent::PasswordChangeRequired {
                                code,
                                user,
                                host: form.gateway.clone(),
                                port: form.port_or_default(),
                                message: message.clone(),
                            });
                            self.lock().pending_sms = None;
                            return Err(message);
                        }
                        Err(err) => {
                            self.lock().pending_sms = None;
                            return Err(err.user_message());
                        }
                    }
                }
                Err(err) => {
                    self.lock().pending_sms = None;
                    return Err(err.user_message());
                }
            }
        }
    }

    /// 等待用户确认是否删除冲突路由；取消连接或超时都会结束等待
    async fn wait_route_conflict(&self) -> Result<bool, String> {
        let (tx, rx) = oneshot::channel();
        *self.route_conflict_lock() = Some(tx);
        let cancel = { self.lock().cancel.clone() };

        tokio::select! {
            _ = cancel.cancelled() => {
                let _ = self.route_conflict_lock().take();
                Err("连接已取消".to_string())
            }
            result = tokio::time::timeout(ROUTE_CONFLICT_TIMEOUT, rx) => {
                match result {
                    Ok(Ok(accept)) => Ok(accept),
                    Ok(Err(_)) => Err("路由冲突确认已取消".to_string()),
                    Err(_) => {
                        let _ = self.route_conflict_lock().take();
                        Err(format!(
                            "路由冲突确认超时（{} 秒）",
                            ROUTE_CONFLICT_TIMEOUT.as_secs()
                        ))
                    }
                }
            }
        }
    }

    async fn wait_captcha(&self) -> Result<String, String> {
        let (tx, rx) = oneshot::channel();
        *self.captcha_lock() = Some(tx);
        let cancel = { self.lock().cancel.clone() };

        tokio::select! {
            // 用户取消连接时立刻结束等待，避免流程卡到超时
            _ = cancel.cancelled() => {
                let _ = self.captcha_lock().take();
                Err("连接已取消".to_string())
            }
            result = tokio::time::timeout(CAPTCHA_TIMEOUT, rx) => {
                match result {
                    Ok(Ok(code)) => Ok(code),
                    Ok(Err(_)) => Err("验证码输入已取消".to_string()),
                    Err(_) => {
                        // 超时后撤掉通道，避免后续提交误唤醒
                        let _ = self.captcha_lock().take();
                        Err(format!(
                            "验证码输入超时（{} 秒）",
                            CAPTCHA_TIMEOUT.as_secs()
                        ))
                    }
                }
            }
        }
    }

    pub async fn connect(self: &Arc<Self>, form: LoginForm) -> Result<SessionSummary, String> {
        let form = form.normalized()?;

        if self.connecting.swap(true, Ordering::SeqCst) {
            return Err("已有连接操作正在进行，请稍候".to_string());
        }
        let _busy = BusyGuard(&self.connecting);

        if self.is_connected() {
            self.disconnect().await?;
        }

        self.set_phase(Phase::Probing, "正在检查上次会话");
        self.purge_previous_session().await;
        self.set_phase(Phase::Probing, "正在探测网关");

        let cancel = {
            let mut inner = self.lock();
            inner.cancel = CancellationToken::new();
            inner.user_disconnect = false;
            inner.cancel.clone()
        };
        let candidates = build_candidates(&form);
        let (gateway, port, param, auth_cookie) =
            match select_line(&candidates, form.connect_type, form.connect_timeout).await {
                Ok(result) => result,
                Err(err) => {
                    self.log(format!("网关探测失败：{err}"));
                    self.set_phase(Phase::Failed, err.clone());
                    return Err(err);
                }
            };
        if candidates.len() > 1 {
            self.log(format!(
                "已选择线路 {gateway}:{port}（共 {} 条候选，模式 {}）",
                candidates.len(),
                form.connect_type
            ));
        }
        // 忘记密码重置后，网关可能仍按旧有效期返回 -40022。
        // 在登录前尝试一次同密码改密，让网关重新计算有效期；失败则继续正常登录流程。
        let has_reset_marker = password_reset::recent(&self.app, &gateway, port, &form.user)
            || candidates.iter().any(|(host, candidate_port)| {
                password_reset::recent(&self.app, host, *candidate_port, &form.user)
            });
        if !form.password.is_empty()
            && has_reset_marker
            && self
                .refresh_expired_password(&gateway, port, &form.user, &form.password)
                .await
        {
            password_reset::clear(&self.app, &gateway, port, &form.user);
            for (host, candidate_port) in &candidates {
                password_reset::clear(&self.app, host, *candidate_port, &form.user);
            }
            self.log("检测到近期重置密码，已刷新密码有效期");
        }

        let mut auth = AuthClient::new(
            gateway.clone(),
            port,
            form.user.clone(),
            form.password.clone(),
        );
        if !auth_cookie.is_empty() {
            auth.set_cookie(auth_cookie);
        }
        let remembered_err_num = self.remembered_login_err_num(&gateway, port, &form.user);
        if remembered_err_num > 0 {
            auth.set_login_err_num(remembered_err_num);
        }
        let mut captcha_required = param.captcha_required(remembered_err_num);
        self.log(format!(
            "网关能力探测完成（{}，验证码{}）",
            if param.vpn_version.is_empty() {
                "版本未知"
            } else {
                &param.vpn_version
            },
            if captcha_required {
                "必需"
            } else if param.captcha_auto() {
                "按失败次数"
            } else {
                "非必需"
            }
        ));

        let mut captcha_prompts = 0u32;
        let mut session = loop {
            let code = if captcha_required {
                self.set_phase(
                    Phase::CaptchaPending,
                    if captcha_prompts == 0 {
                        "正在获取验证码"
                    } else {
                        "正在刷新验证码"
                    },
                );
                let png = match auth.captcha().await {
                    Ok(png) => png,
                    Err(err) => {
                        let message = err.user_message();
                        self.log(format!("获取验证码失败：{message}"));
                        self.set_phase(Phase::Failed, message.clone());
                        return Err(message);
                    }
                };
                self.emit(EngineEvent::Captcha {
                    png_base64: base64::engine::general_purpose::STANDARD.encode(&png),
                });
                self.set_phase(Phase::CaptchaPending, "等待输入验证码");
                self.wait_captcha().await?
            } else {
                String::new()
            };

            if cancel.is_cancelled() {
                return Err("连接已取消".to_string());
            }

            self.set_phase(Phase::Authenticating, "正在校验账号");
            match auth.submit(&code).await {
                Ok(session) => break session,
                Err(AuthError::Rejected { code: ret, .. })
                    if captcha_required
                        && matches!(ret, -40135 | -40018)
                        && captcha_prompts < 4 =>
                {
                    captcha_prompts += 1;
                    self.log(format!("验证码校验失败（Ret={ret}），已自动刷新验证码"));
                    continue;
                }
                Err(AuthError::Rejected { .. })
                    if !captcha_required
                        && param.captcha_auto()
                        && auth.login_err_num() >= crate::auth::model::AUTO_CAPTCHA_THRESHOLD
                        && captcha_prompts < 3 =>
                {
                    captcha_prompts += 1;
                    captcha_required = true;
                    self.remember_login_err_num(&gateway, port, &form.user, auth.login_err_num());
                    self.log("登录失败次数已达到网关上限，接下来需要输入图形验证码");
                    continue;
                }
                Err(AuthError::Rejected { code: -40077, .. }) => {
                    self.log("账号已在别处登录，正在踢出旧会话并继续登录");
                    match auth.kick_user().await {
                        Ok(session) => break session,
                        Err(AuthError::NeedSms {
                            challenge,
                            sms_time,
                        }) => {
                            self.log("踢出旧会话后需要重新输入动态验证码");
                            match self
                                .complete_sms_login(&mut auth, &form, &code, sms_time, challenge)
                                .await
                            {
                                Ok(session) => break session,
                                Err(message) => {
                                    self.log(format!("动态验证码登录失败：{message}"));
                                    self.set_phase(Phase::Failed, message.clone());
                                    return Err(message);
                                }
                            }
                        }
                        Err(AuthError::PasswordChangeRequired { code, user }) => {
                            let message = password_change_message(code);
                            let user = if user.trim().is_empty() {
                                form.user.clone()
                            } else {
                                user
                            };
                            self.emit(EngineEvent::PasswordChangeRequired {
                                code,
                                user,
                                host: gateway.clone(),
                                port,
                                message: message.clone(),
                            });
                            self.log(format!("登录失败：{message}"));
                            self.set_phase(Phase::Failed, message.clone());
                            return Err(message);
                        }
                        Err(err) => {
                            let message = err.user_message();
                            self.log(format!("踢出旧会话失败：{message}"));
                            self.set_phase(Phase::Failed, message.clone());
                            return Err(message);
                        }
                    }
                }
                Err(AuthError::NeedSms {
                    challenge,
                    sms_time,
                }) => {
                    self.log(if challenge {
                        "网关要求输入动态口令"
                    } else {
                        "网关要求输入动态验证码（已发送至绑定的手机或企业微信）"
                    });
                    match self
                        .complete_sms_login(&mut auth, &form, &code, sms_time, challenge)
                        .await
                    {
                        Ok(session) => break session,
                        Err(message) => {
                            self.log(format!("动态验证码登录失败：{message}"));
                            self.set_phase(Phase::Failed, message.clone());
                            return Err(message);
                        }
                    }
                }
                Err(AuthError::PasswordChangeRequired { code, user }) => {
                    let message = password_change_message(code);
                    let user = if user.trim().is_empty() {
                        form.user.clone()
                    } else {
                        user
                    };
                    self.emit(EngineEvent::PasswordChangeRequired {
                        code,
                        user,
                        host: gateway.clone(),
                        port,
                        message: message.clone(),
                    });
                    self.log(format!("登录失败：{message}"));
                    self.set_phase(Phase::Failed, message.clone());
                    return Err(message);
                }
                Err(err) => {
                    let message = err.user_message();
                    self.remember_login_err_num(&gateway, port, &form.user, auth.login_err_num());
                    self.log(format!("登录失败：{message}"));
                    self.set_phase(Phase::Failed, message.clone());
                    return Err(message);
                }
            }
        };
        self.remember_login_err_num(&gateway, port, &form.user, 0);
        let session_cookie = auth.cookie().map(str::to_string);
        password_reset::clear(&self.app, &gateway, port, &form.user);
        if session.gateway_version.is_empty() {
            session.gateway_version = param.vpn_version.clone();
        }
        session.gateway = gateway.clone();
        session.port = port;
        if let Err(err) = session_guard::save(
            &self.app,
            &session.gateway,
            session.port,
            &session.session_id,
            &form.user,
        ) {
            self.log(format!("警告：无法记录会话保护标记：{err}"));
        }
        if let Ok(ip) = tls::resolve_ip(&gateway).await {
            session.gateway_ip = Some(ip.to_string());
        }

        let display_user = if session.user_name.is_empty() {
            form.user.clone()
        } else {
            session.user_name.clone()
        };
        self.log(format!(
            "登录成功：{display_user}，虚拟 IP {}，下发路由 {} 条",
            session.vip,
            session.routes.len()
        ));

        {
            let mut inner = self.lock();
            inner.session = Some(session.clone());
            inner.session_cookie = session_cookie;
            inner.login_user = form.user.clone();
        }

        if let Err(err) = self.establish(&session, &cancel).await {
            let mut cleanup = AuthClient::new_with_timeout(
                session.gateway.clone(),
                session.port,
                String::new(),
                String::new(),
                Duration::from_secs(2),
            );
            match cleanup.logout(&session.session_id).await {
                Ok(()) => session_guard::clear(&self.app),
                Err(_) => self.log("连接失败后的会话注销未完成，将在下次启动时重试"),
            }
            // 建卡/隧道/路由任一步失败都要清空会话，避免界面残留虚拟 IP
            {
                let mut inner = self.lock();
                inner.session = None;
                inner.session_cookie = None;
                inner.login_user.clear();
                inner.tun = None;
                inner.net = None;
                inner.tunnel = None;
                inner.started_at = None;
            }
            self.log(format!("连接失败：{err}"));
            self.set_phase(Phase::Failed, err.clone());
            return Err(err);
        }

        Ok(summary(&session))
    }

    async fn establish(
        self: &Arc<Self>,
        session: &LoginSession,
        cancel: &CancellationToken,
    ) -> Result<(), String> {
        self.reset_health_state();
        self.set_phase(Phase::Configuring, "正在检查路由冲突");
        let conflicts = crate::netcfg::route_conflicts(&session.routes).await;
        let takeover = if conflicts.is_empty() {
            false
        } else {
            let total = conflicts.len();
            self.log(format!(
                "检测到 {total} 条路由冲突：{}",
                summarize_conflicts(&conflicts)
            ));
            self.emit(EngineEvent::RouteConflict {
                total,
                conflicts: conflicts.into_iter().map(RouteConflictDto::from).collect(),
            });
            let accepted = self.wait_route_conflict().await?;
            if !accepted {
                return Err("已取消连接：检测到路由冲突".to_string());
            }
            self.log("用户选择接管路由冲突，将删除冲突路由后继续");
            true
        };

        self.set_phase(Phase::CreatingTun, "正在创建虚拟网卡");
        let dll = self.dll_path.clone();
        let vip = session.vip.clone();
        let mask = session.vip_mask.clone();
        let device = tokio::task::spawn_blocking(move || {
            TunDevice::open_for(&dll, ADAPTER_NAME, &vip, &mask)
        })
        .await
        .map_err(|e| format!("网卡任务异常：{e}"))?
        .map_err(|e| e.to_string())?;
        let interface_name = device.interface_name().to_string();
        if let Err(err) = session_guard::record_tun(&self.app, &interface_name, &session.vip) {
            self.log(format!("警告：无法记录虚拟网卡信息：{err}"));
        }
        let tun = Arc::new(device);
        self.log(format!("虚拟网卡已就绪：{interface_name}"));

        self.set_phase(Phase::Configuring, "正在配置网卡地址");
        let mut net = NetConfig::new(interface_name);
        if let Err(err) = net
            .configure_interface(&session.vip, &session.vip_mask)
            .await
        {
            for note in net.take_notes() {
                self.log(note);
            }
            self.log(format!("配置网卡失败：{err}"));
            net.cleanup().await;
            for note in net.take_notes() {
                self.log(note);
            }
            return Err(err.to_string());
        }
        for note in net.take_notes() {
            self.log(note);
        }
        if let Some(gateway_ip) = &session.gateway_ip {
            if !net.bypass_gateway(gateway_ip).await {
                self.log("警告：未能为网关地址添加旁路路由，隧道可能绕行");
            }
        }
        self.log(format!(
            "网卡地址 {}/{}，MTU 1400",
            session.vip, session.vip_mask
        ));

        self.set_phase(Phase::Handshaking, "正在建立隧道");
        let (event_tx, event_rx) = mpsc::channel::<TunnelEvent>(tunnel::EVENT_QUEUE);
        let tunnel = match tunnel::open(session, event_tx, cancel.clone()).await {
            Ok(tunnel) => tunnel,
            Err(err) => {
                net.cleanup().await;
                for note in net.take_notes() {
                    self.log(note);
                }
                return Err(err.to_string());
            }
        };
        self.log("隧道 TLS 已建立");

        self.set_phase(Phase::Configuring, "正在注入路由");
        let report = net.add_routes(&session.routes, takeover).await;
        if !report.all_ok() {
            self.log(format!(
                "警告：{} 条路由未生效（{}）",
                report.failed.len(),
                report.failed.join("、")
            ));
        }
        for note in net.take_notes() {
            self.log(note);
        }
        if !session.routes.is_empty() && report.ok == 0 {
            net.cleanup().await;
            for note in net.take_notes() {
                self.log(note);
            }
            return Err("路由注入全部失败，内网流量无法进入隧道".to_string());
        }

        let dns_ok = net.set_dns(&session.dns1, &session.dns2).await;
        for note in net.take_notes() {
            self.log(note);
        }
        // 原客户端不修改系统 hosts；dns_info 仅解析保留，不写系统文件
        let dns_servers = if session.dns2.is_empty() {
            session.dns1.clone()
        } else {
            format!("{} / {}", session.dns1, session.dns2)
        };
        let dns_note = if dns_ok {
            ""
        } else if cfg!(target_os = "linux") {
            "（Linux 不修改系统 DNS）"
        } else {
            "（网卡 DNS 设置失败）"
        };
        self.log(format!(
            "已注入 {}/{} 条路由，DNS {}{}",
            report.ok,
            session.routes.len(),
            dns_servers,
            dns_note
        ));

        let reader_stop = Arc::new(AtomicBool::new(true));
        let (up_tx, up_rx) = mpsc::channel::<Vec<u8>>(1024);
        let reader_device = tun.clone();
        let stop = reader_stop.clone();
        let reader = std::thread::spawn(move || {
            while stop.load(Ordering::Relaxed) {
                if !reader_device.wait_readable(200) {
                    continue;
                }
                while let Some(packet) = reader_device.recv_packet() {
                    if up_tx.blocking_send(packet).is_err() {
                        return;
                    }
                }
            }
        });

        let uplink_task = spawn_uplink(up_rx, Arc::downgrade(self), cancel.clone());

        let generation = {
            let mut inner = self.lock();
            inner.tunnel_generation = inner.tunnel_generation.wrapping_add(1);
            inner.tunnel_generation
        };
        let tunnel_task =
            spawn_tunnel_events(event_rx, tun.clone(), Arc::downgrade(self), generation);

        // 统计推送
        let stats_task = spawn_stats(Arc::downgrade(self), cancel.clone());
        let health_task = spawn_health(Arc::downgrade(self), cancel.clone());

        {
            let mut inner = self.lock();
            inner.tun = Some(tun);
            inner.net = Some(net);
            inner.tunnel = Some(tunnel);
            inner.reader_stop = Some(reader_stop);
            inner.reader_handle = Some(reader);
            inner.uplink_task = Some(uplink_task);
            inner.tunnel_task = Some(tunnel_task);
            inner.stats_task = Some(stats_task);
            inner.health_task = Some(health_task);
            inner.started_at = Some(Instant::now());
        }

        self.set_phase(Phase::Connected, "");
        self.uplink_drops.store(0, Ordering::Relaxed);
        if self.closed_generation.load(Ordering::SeqCst) == generation {
            self.on_tunnel_closed("连接建立后立即断开".to_string(), generation);
        }
        if session.account_end_days > 0 || session.password_remind_days > 0 {
            self.emit(EngineEvent::ExpiryReminder {
                account_end: session.account_end_days,
                password_remind: session.password_remind_days,
            });
        }
        Ok(())
    }

    pub async fn disconnect(self: &Arc<Self>) -> Result<(), String> {
        let active = {
            let inner = self.lock();
            inner.phase != Phase::Idle
                || inner.tunnel.is_some()
                || inner.tun.is_some()
                || inner.net.is_some()
        };
        if !active {
            return Ok(());
        }

        self.set_phase(Phase::Disconnecting, "正在断开");
        let cleanup_ok = self.teardown().await;
        self.set_phase(Phase::Idle, "");
        if cleanup_ok {
            self.log("已断开并恢复网络配置");
        } else {
            self.log("已断开（部分网络配置未恢复，详见上方日志）");
        }
        Ok(())
    }

    async fn teardown(self: &Arc<Self>) -> bool {
        let (
            tunnel,
            net,
            reader_stop,
            reader,
            uplink_task,
            tunnel_task,
            stats_task,
            health_task,
            session,
        ) = {
            let mut inner = self.lock();
            inner.user_disconnect = true;
            inner.cancel.cancel();
            inner.pending_sms = None;
            (
                inner.tunnel.take(),
                inner.net.take(),
                inner.reader_stop.take(),
                inner.reader_handle.take(),
                inner.uplink_task.take(),
                inner.tunnel_task.take(),
                inner.stats_task.take(),
                inner.health_task.take(),
                inner.session.clone(),
            )
        };

        let _ = self.sms_lock().take();

        if let Some(tunnel) = &tunnel {
            tunnel.close();
        }
        if let Some(task) = tunnel_task {
            task.abort();
            let _ = task.await;
        }
        if let Some(task) = stats_task {
            task.abort();
            let _ = task.await;
        }
        if let Some(task) = health_task {
            task.abort();
            let _ = task.await;
        }
        if let Some(task) = uplink_task {
            task.abort();
            let _ = task.await;
        }
        if let Some(stop) = reader_stop {
            stop.store(false, Ordering::Relaxed);
        }
        if let Some(handle) = reader {
            let _ = tokio::task::spawn_blocking(move || {
                let _ = handle.join();
            })
            .await;
        }
        let logout = async {
            if let Some(session) = session {
                if session.session_id.trim().is_empty() {
                    return;
                }
                let mut auth = AuthClient::new(
                    session.gateway.clone(),
                    session.port,
                    String::new(),
                    String::new(),
                );
                match auth.logout(&session.session_id).await {
                    Ok(()) => session_guard::clear(&self.app),
                    Err(_) => self.log("注销会话失败，单点登录名额可能需要等待超时释放"),
                }
            }
        };
        let cleanup = async move {
            if let Some(mut net) = net {
                net.cleanup().await;
                net.take_notes()
            } else {
                Vec::new()
            }
        };
        let (_, cleanup_notes) = tokio::join!(logout, cleanup);
        let mut cleanup_ok = true;
        for note in cleanup_notes {
            if note.contains("清理失败") || note.contains("恢复失败") || note.contains("状态未知")
            {
                cleanup_ok = false;
            }
            self.log(note);
        }

        {
            let mut inner = self.lock();
            inner.tun = None;
            inner.started_at = None;
        }
        drop(tunnel);

        {
            let mut inner = self.lock();
            inner.session = None;
            inner.session_cookie = None;
            inner.login_user.clear();
        }
        cleanup_ok
    }

    fn on_tunnel_closed(self: &Arc<Self>, reason: String, generation: u64) {
        let engine = self.clone();
        tokio::spawn(async move { engine.handle_tunnel_closed(reason, generation).await });
    }

    async fn handle_tunnel_closed(self: Arc<Self>, reason: String, generation: u64) {
        {
            let inner = self.lock();
            if generation != inner.tunnel_generation {
                return;
            }
        }
        self.closed_generation.store(generation, Ordering::SeqCst);

        if self.reconnecting.swap(true, Ordering::SeqCst) {
            return;
        }
        let _guard = ReconnectGuard(&self.reconnecting);

        let auto_reconnect = crate::settings::load(&self.app).auto_reconnect;
        let should_reconnect = {
            let inner = self.lock();
            inner.phase == Phase::Connected && !inner.user_disconnect
        };
        if !should_reconnect {
            return;
        }

        if !auto_reconnect {
            self.log(format!("隧道已断开：{reason}，自动重连已关闭"));
            let _ = self.disconnect().await;
            return;
        }

        self.log(format!("隧道已断开：{reason}"));
        self.set_phase(Phase::Reconnecting, "正在重连");

        let cancel = { self.lock().cancel.clone() };
        for attempt in 1..=MAX_RECONNECT {
            let wait = Duration::from_secs(u64::from(attempt) * 2);
            tokio::time::sleep(wait).await;
            if cancel.is_cancelled() {
                return;
            }

            let session = { self.lock().session.clone() };
            let Some(session) = session else { return };

            self.log(format!("第 {attempt}/{MAX_RECONNECT} 次重连尝试"));

            match self.reopen_tunnel(&session, &cancel).await {
                Ok(generation) => {
                    if self
                        .verify_data_plane(generation, HEALTH_VERIFY_TIMEOUT)
                        .await
                    {
                        self.log("隧道已恢复");
                        return;
                    }
                    self.log("新隧道已建立，但数据面未恢复");
                    self.set_phase(Phase::Reconnecting, "数据面未恢复，继续重试");
                }
                Err(err) => self.log(format!("重连失败：{err}")),
            }
        }

        self.log("重连次数已用尽，已断开连接");
        let _ = self.disconnect().await;
    }

    async fn verify_data_plane(self: &Arc<Self>, generation: u64, timeout: Duration) -> bool {
        let Some(baseline) = self.health_sample() else {
            return false;
        };
        if baseline.generation != generation {
            return false;
        }
        let started = Instant::now();
        let mut saw_uplink = false;
        loop {
            if started.elapsed() >= timeout {
                return !saw_uplink;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
            if self.lock().cancel.is_cancelled() {
                return false;
            }
            let Some(sample) = self.health_sample() else {
                return false;
            };
            if sample.generation != generation {
                return false;
            }
            if sample.packets_down > baseline.packets_down {
                return true;
            }
            if sample.packets_in > baseline.packets_in {
                saw_uplink = true;
            }
        }
    }

    async fn recover_data_plane(self: &Arc<Self>, issue: HealthIssue) {
        if !self.is_connected() {
            return;
        }
        if let Some(sample) = self.health_sample() {
            self.log(format!(
                "数据面计数：TUN 收 {} / 发 {}，隧道上行 {} / 下行 {}，上行丢弃 {}，TCP/ICMP 上行 {}/{}",
                sample.packets_in,
                sample.packets_out,
                sample.packets_up,
                sample.packets_down,
                sample.uplink_drops,
                sample.tcp_in,
                sample.icmp_in
            ));
        }

        let auto_reconnect = crate::settings::load(&self.app).auto_reconnect;
        if !auto_reconnect {
            if self.health_notice_due() {
                self.log(format!(
                    "检测到数据面异常（{}），自动重连已关闭",
                    issue.describe()
                ));
                self.set_phase(Phase::Connected, "数据面异常，请断开重连");
            }
            return;
        }

        match self.begin_health_recovery() {
            RecoveryGate::Allowed => {}
            RecoveryGate::Cooling => {
                self.log("数据面异常仍在恢复冷却期，暂不重复重连");
                return;
            }
            RecoveryGate::Exhausted => {
                self.log(format!(
                    "数据面异常反复出现，已停止自动恢复：{HEALTH_FAIL_MESSAGE}"
                ));
                self.fail_data_plane().await;
                return;
            }
        }

        self.log(format!(
            "检测到数据面异常：{}，开始恢复隧道",
            issue.describe()
        ));
        self.set_phase(Phase::Reconnecting, "数据面异常，正在恢复");

        let cancel = { self.lock().cancel.clone() };
        for attempt in 1..=HEALTH_RECONNECT_MAX {
            if cancel.is_cancelled() {
                return;
            }
            let wait = Duration::from_secs(1u64 << (attempt - 1));
            tokio::time::sleep(wait).await;
            if cancel.is_cancelled() {
                return;
            }

            let session = { self.lock().session.clone() };
            let Some(session) = session else { return };

            self.log(format!(
                "数据面恢复第 {attempt}/{HEALTH_RECONNECT_MAX} 次尝试"
            ));
            match self.reopen_tunnel(&session, &cancel).await {
                Ok(generation) => {
                    if self
                        .verify_data_plane(generation, HEALTH_VERIFY_TIMEOUT)
                        .await
                    {
                        self.log("数据面已恢复");
                        self.set_phase(Phase::Connected, "");
                        return;
                    }
                    self.log("新隧道已建立，但数据面未恢复");
                    self.set_phase(Phase::Reconnecting, "数据面未恢复，继续重试");
                }
                Err(err) => self.log(format!("数据面恢复失败：{err}")),
            }
        }

        self.log(format!("数据面恢复失败：{HEALTH_FAIL_MESSAGE}"));
        self.fail_data_plane().await;
    }

    async fn fail_data_plane(self: &Arc<Self>) {
        let cleanup_ok = self.teardown().await;
        if cleanup_ok {
            self.log("已断开并恢复网络配置");
        } else {
            self.log("已断开（部分网络配置未恢复，详见上方日志）");
        }
        self.set_phase(Phase::Failed, HEALTH_FAIL_MESSAGE.to_string());
        self.emit(EngineEvent::Error {
            message: HEALTH_FAIL_MESSAGE.to_string(),
        });
    }

    async fn repair_network(
        self: &Arc<Self>,
        session: &LoginSession,
        gateway_ip: IpAddr,
    ) -> Result<(), String> {
        let Some(mut net) = ({ self.lock().net.take() }) else {
            return Err("网络配置已释放".to_string());
        };

        let mut outcome = net
            .configure_interface(&session.vip, &session.vip_mask)
            .await
            .map(|_| ())
            .map_err(|err| err.to_string());
        for note in net.take_notes() {
            self.log(note);
        }
        if outcome.is_ok() {
            if !net.bypass_gateway(&gateway_ip.to_string()).await {
                self.log("警告：重连后未能恢复网关旁路路由");
            }
            let report = net.add_routes(&session.routes, false).await;
            for note in net.take_notes() {
                self.log(note);
            }
            if !session.routes.is_empty() && report.ok == 0 {
                outcome = Err("重连后路由恢复失败，内网流量无法进入隧道".to_string());
            } else {
                if !report.all_ok() {
                    self.log(format!(
                        "警告：重连后 {} 条路由未生效（{}）",
                        report.failed.len(),
                        report.failed.join("、")
                    ));
                }
                let dns_ok = net.set_dns(&session.dns1, &session.dns2).await;
                for note in net.take_notes() {
                    self.log(note);
                }
                if !dns_ok && !cfg!(target_os = "linux") {
                    self.log("警告：重连后 DNS 恢复失败");
                }
                let dns_servers = if session.dns2.is_empty() {
                    session.dns1.clone()
                } else {
                    format!("{} / {}", session.dns1, session.dns2)
                };
                self.log(format!(
                    "重连后网络配置已复核：{}/{} 条路由，DNS {}",
                    report.ok,
                    session.routes.len(),
                    dns_servers
                ));
            }
        }

        let mut net = Some(net);
        let cancelled = {
            let mut inner = self.lock();
            if inner.cancel.is_cancelled() {
                true
            } else {
                if let Some(current) = inner.session.as_mut() {
                    current.gateway_ip = Some(gateway_ip.to_string());
                }
                inner.net = net.take();
                false
            }
        };
        if cancelled {
            if let Some(mut net) = net {
                net.cleanup().await;
            }
            return Err("连接已取消".to_string());
        }
        outcome
    }

    async fn reconnect_gateway_ip(&self, session: &LoginSession) -> Result<IpAddr, String> {
        if let Some(ip) = cached_gateway_ip(session) {
            return Ok(ip);
        }
        tls::resolve_ip_with_timeout(&session.gateway, GATEWAY_RESOLVE_TIMEOUT)
            .await
            .map_err(|err| err.user_message())
    }

    async fn switch_gateway_ip(self: &Arc<Self>, ip: IpAddr) {
        let Some(mut net) = ({ self.lock().net.take() }) else {
            return;
        };
        if !net.bypass_gateway(&ip.to_string()).await {
            self.log("警告：重连后未能为新网关地址添加旁路路由");
        }
        for note in net.take_notes() {
            self.log(note);
        }
        let mut net = Some(net);
        let cancelled = {
            let mut inner = self.lock();
            if inner.cancel.is_cancelled() {
                true
            } else {
                if let Some(current) = inner.session.as_mut() {
                    current.gateway_ip = Some(ip.to_string());
                }
                inner.net = net.take();
                false
            }
        };
        if cancelled {
            if let Some(mut net) = net {
                net.cleanup().await;
            }
        }
    }

    async fn open_tunnel_at(
        &self,
        session: &LoginSession,
        cancel: &CancellationToken,
        ip: IpAddr,
    ) -> Result<(TunnelHandle, mpsc::Receiver<TunnelEvent>), String> {
        let (event_tx, event_rx) = mpsc::channel::<TunnelEvent>(tunnel::EVENT_QUEUE);
        let tunnel = tunnel::open_at(session, event_tx, cancel.clone(), ip)
            .await
            .map_err(|err| err.to_string())?;
        Ok((tunnel, event_rx))
    }

    async fn reopen_tunnel(
        self: &Arc<Self>,
        session: &LoginSession,
        cancel: &CancellationToken,
    ) -> Result<u64, String> {
        let tun = { self.lock().tun.clone() };
        let Some(tun) = tun else {
            return Err("虚拟网卡已释放".to_string());
        };

        let gateway_ip = self.reconnect_gateway_ip(session).await?;
        self.repair_network(session, gateway_ip).await?;

        let mut opened = self.open_tunnel_at(session, cancel, gateway_ip).await;
        if opened.is_err() {
            if let Ok(resolved) =
                tls::resolve_ip_with_timeout(&session.gateway, GATEWAY_RESOLVE_TIMEOUT).await
            {
                if resolved != gateway_ip {
                    self.switch_gateway_ip(resolved).await;
                    opened = self.open_tunnel_at(session, cancel, resolved).await;
                }
            }
        }
        let (tunnel, event_rx) = opened?;

        let generation = {
            let mut inner = self.lock();
            inner.tunnel_generation = inner.tunnel_generation.wrapping_add(1);
            inner.tunnel_generation
        };
        let task = spawn_tunnel_events(event_rx, tun, Arc::downgrade(self), generation);
        let old = {
            let mut inner = self.lock();
            let old = inner.tunnel.replace(tunnel);
            inner.tunnel_task = Some(task);
            old
        };
        if let Some(old) = old {
            old.close();
        }

        tokio::time::sleep(Duration::from_millis(300)).await;
        if cancel.is_cancelled() {
            return Err("连接已取消".to_string());
        }
        if self.closed_generation.load(Ordering::SeqCst) == generation {
            return Err("新隧道建立后立即断开".to_string());
        }
        self.set_phase(Phase::Connected, "");
        self.uplink_drops.store(0, Ordering::Relaxed);
        if self.closed_generation.load(Ordering::SeqCst) == generation {
            self.set_phase(Phase::Reconnecting, "正在重连");
            return Err("新隧道建立后立即断开".to_string());
        }
        Ok(generation)
    }
}

/// 登录时缓存的网关 IP；缺失或格式非法时返回 None
fn cached_gateway_ip(session: &LoginSession) -> Option<IpAddr> {
    session
        .gateway_ip
        .as_deref()
        .and_then(|value| value.parse().ok())
}

fn spawn_tunnel_events(
    mut event_rx: mpsc::Receiver<TunnelEvent>,
    tun: Arc<TunDevice>,
    engine: std::sync::Weak<Engine>,
    generation: u64,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            match event {
                TunnelEvent::Data(packet) => {
                    let _ = tun.send_packet(&packet);
                }
                TunnelEvent::Control { msg_type, payload } => {
                    if let Some(engine) = engine.upgrade() {
                        engine.log(format!("控制帧 type={msg_type} len={}", payload.len()));
                    }
                }
                TunnelEvent::Closed { reason } => {
                    if let Some(engine) = engine.upgrade() {
                        engine.on_tunnel_closed(reason, generation);
                    }
                    return;
                }
                TunnelEvent::Ready => {}
            }
        }
    })
}

/// 上行泵：每个报文都读取当前隧道句柄，重连替换句柄后自动走新隧道
fn spawn_uplink(
    mut up_rx: mpsc::Receiver<Vec<u8>>,
    engine: std::sync::Weak<Engine>,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return,
                packet = up_rx.recv() => {
                    let Some(packet) = packet else { return };
                    let Some(engine) = engine.upgrade() else { return };
                    engine.note_uplink_packet(&packet);
                    let tunnel = { engine.lock().tunnel.clone() };
                    if let Some(tunnel) = tunnel {
                        if !tunnel.send_packet(packet) {
                            engine.note_uplink_drop();
                        }
                    }
                }
            }
        }
    })
}

fn spawn_stats(engine: std::sync::Weak<Engine>, cancel: CancellationToken) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return,
                _ = tokio::time::sleep(Duration::from_secs(1)) => {
                    let Some(engine) = engine.upgrade() else { return };
                    let stats = engine.snapshot();
                    engine.emit(EngineEvent::Stats { stats });
                }
            }
        }
    })
}

fn spawn_health(engine: std::sync::Weak<Engine>, cancel: CancellationToken) -> JoinHandle<()> {
    tokio::spawn(async move {
        let started = Instant::now();
        let mut tracker = HealthTracker::new();
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return,
                _ = tokio::time::sleep(Duration::from_secs(1)) => {
                    let Some(engine) = engine.upgrade() else { return };
                    if engine.lock().phase != Phase::Connected {
                        continue;
                    }
                    let Some(sample) = engine.health_sample() else { continue };
                    if let Some(issue) = tracker.observe(started.elapsed(), sample) {
                        engine.on_health_issue(issue);
                    }
                }
            }
        }
    })
}

fn summary(session: &LoginSession) -> SessionSummary {
    SessionSummary {
        user: session.user_name.clone(),
        vip: session.vip.clone(),
        mask: session.vip_mask.clone(),
        dns1: session.dns1.clone(),
        dns2: session.dns2.clone(),
        routes: session.routes.len(),
        hosts: session.hosts.len(),
        acl: session.acl.len(),
        tunnel_type: session.tunnel_type,
        gateway: session.gateway.clone(),
        port: session.port,
        gateway_version: session.gateway_version.clone(),
        login_info: LoginInfoDto {
            current_ip: session.login_info.current_ip.clone(),
            current_time: session.login_info.current_time.clone(),
            previous_ip: session.login_info.previous_ip.clone(),
            previous_time: session.login_info.previous_time.clone(),
            failed_ip: session.login_info.failed_ip.clone(),
            failed_time: session.login_info.failed_time.clone(),
        },
        resources: session
            .resources
            .iter()
            .map(|item| ResourceDto {
                name: item.name.clone(),
                url: item.url.clone(),
                group: item.group.clone(),
                module: item.module.clone(),
            })
            .collect(),
    }
}

/// 读取 IPv4 头的协议字段；非 IPv4 或长度不足时返回 None
fn ipv4_protocol(packet: &[u8]) -> Option<u8> {
    if packet.len() < 20 || packet[0] >> 4 != 4 {
        return None;
    }
    let header_len = usize::from(packet[0] & 0x0f) * 4;
    if header_len < 20 || packet.len() < header_len {
        return None;
    }
    Some(packet[9])
}

/// 组装候选线路：优先使用 profiles 地址列表，否则回退到表单中的网关
fn build_candidates(form: &crate::dto::LoginForm) -> Vec<(String, u16)> {
    let mut out = Vec::new();
    for line in &form.lines {
        if let Ok((host, port)) = crate::profiles::parse_line(line) {
            out.push((host, port));
        }
    }
    if out.is_empty() {
        out.push((form.gateway.clone(), form.port_or_default()));
    }
    out
}

fn login_err_key(host: &str, port: u16, user: &str) -> String {
    format!(
        "{}:{}:{}",
        host.trim().to_ascii_lowercase(),
        port,
        user.trim()
    )
}

/// 按连接模式选择线路：0 单线路、1 并发优选、2 顺序备份
async fn select_line(
    candidates: &[(String, u16)],
    connect_type: u8,
    timeout_secs: u32,
) -> Result<(String, u16, AuthParam, String), String> {
    if candidates.is_empty() {
        return Err("没有可用的网关地址".to_string());
    }

    if connect_type == 0 || candidates.len() == 1 {
        let (host, port) = candidates[0].clone();
        let (param, cookie) = probe_line(&host, port, timeout_secs).await?;
        return Ok((host, port, param, cookie));
    }

    if connect_type == 2 {
        let mut last_error = String::new();
        for (host, port) in candidates {
            match probe_line(host, *port, timeout_secs).await {
                Ok((param, cookie)) => return Ok((host.clone(), *port, param, cookie)),
                Err(err) => last_error = err,
            }
        }
        return Err(format!("所有线路均不可用：{last_error}"));
    }

    let mut tasks = tokio::task::JoinSet::new();
    for (host, port) in candidates.iter().cloned() {
        let timeout = timeout_secs;
        tasks.spawn(async move {
            let result = probe_line(&host, port, timeout).await;
            (host, port, result)
        });
    }

    while let Some(joined) = tasks.join_next().await {
        if let Ok((host, port, Ok((param, cookie)))) = joined {
            tasks.abort_all();
            return Ok((host, port, param, cookie));
        }
    }
    Err("所有线路均不可用".to_string())
}

/// 拼出日志用的冲突摘要，避免一次写入几十条
fn summarize_conflicts(conflicts: &[crate::netcfg::RouteConflict]) -> String {
    let mut parts: Vec<String> = conflicts
        .iter()
        .take(5)
        .map(|item| {
            format!(
                "{}/{} → {}（{}）",
                item.dest, item.prefix, item.ifname, item.gateway
            )
        })
        .collect();
    if conflicts.len() > parts.len() {
        parts.push(format!("等 {} 条", conflicts.len()));
    }
    parts.join("、")
}

/// 并发探测候选线路，返回延迟排序结果，供手动选择线路使用
pub async fn probe_lines(lines: Vec<String>, timeout_secs: u32) -> Vec<LineProbeDto> {
    let mut tasks = tokio::task::JoinSet::new();
    for line in lines {
        let limit = timeout_secs;
        tasks.spawn(async move {
            let started = Instant::now();
            match crate::profiles::parse_line(&line) {
                Ok((host, port)) => match probe_line(&host, port, limit).await {
                    Ok(_) => LineProbeDto {
                        line,
                        host,
                        port,
                        latency_ms: started.elapsed().as_millis().min(u128::from(u32::MAX)) as u32,
                        ok: true,
                        error: None,
                    },
                    Err(error) => LineProbeDto {
                        line,
                        host,
                        port,
                        latency_ms: started.elapsed().as_millis().min(u128::from(u32::MAX)) as u32,
                        ok: false,
                        error: Some(error),
                    },
                },
                Err(error) => LineProbeDto {
                    line,
                    host: String::new(),
                    port: 0,
                    latency_ms: 0,
                    ok: false,
                    error: Some(error),
                },
            }
        });
    }

    let mut out = Vec::new();
    while let Some(joined) = tasks.join_next().await {
        if let Ok(item) = joined {
            out.push(item);
        }
    }
    out.sort_by_key(|item| (!item.ok, item.latency_ms));
    out
}

async fn probe_line(
    host: &str,
    port: u16,
    timeout_secs: u32,
) -> Result<(AuthParam, String), String> {
    let mut auth = AuthClient::new(host, port, String::new(), String::new());
    let limit = Duration::from_secs(u64::from(timeout_secs.max(1)));
    match tokio::time::timeout(limit, auth.probe()).await {
        Ok(Ok(param)) => Ok((param, auth.cookie().unwrap_or_default().to_string())),
        Ok(Err(err)) => Err(err.user_message()),
        Err(_) => Err(format!(
            "网关探测超时（{timeout_secs} 秒），请检查网络连接或端口是否正确"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 缓存网关_ip_仅接受合法地址() {
        let mut session = LoginSession::default();
        assert_eq!(cached_gateway_ip(&session), None);

        session.gateway_ip = Some("198.51.100.10".to_string());
        assert_eq!(
            cached_gateway_ip(&session),
            Some("198.51.100.10".parse::<IpAddr>().unwrap())
        );

        session.gateway_ip = Some("vpn.example.com".to_string());
        assert_eq!(cached_gateway_ip(&session), None);
    }

    #[test]
    fn 解析_ipv4_协议字段() {
        let mut tcp = vec![0u8; 20];
        tcp[0] = 0x45;
        tcp[9] = 6;
        assert_eq!(ipv4_protocol(&tcp), Some(6));

        let mut icmp = vec![0u8; 20];
        icmp[0] = 0x45;
        icmp[9] = 1;
        assert_eq!(ipv4_protocol(&icmp), Some(1));

        let mut ipv6 = vec![0u8; 40];
        ipv6[0] = 0x60;
        assert_eq!(ipv4_protocol(&ipv6), None);

        assert_eq!(ipv4_protocol(&[0x45, 0, 0]), None);
    }
}
