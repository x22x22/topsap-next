//! 登录接口封装：能力探测 → 取验证码 → 提交登录 → 注销。

use std::time::Duration;

use base64::Engine;
use rand::RngCore;
use serde_json::Value;

use super::error::{gateway_error_message, AuthError};
use super::http::{form_encode, HttpClient, HttpResponse};
use super::json::loose_parse;
use super::model::{normalize_login, AuthParam, LoginSession};

const REQUEST_TIMEOUT_SECS: u64 = 25;

pub struct AuthClient {
    host: String,
    port: u16,
    user: String,
    pass: String,
    http: HttpClient,
    /// 同一次登录的密码与动态验证码请求必须复用同一个 S_CODE
    s_code: String,
    /// 密码与动态验证码请求复用同一个 HASH
    hash: String,
}

impl AuthClient {
    pub fn new(
        host: impl Into<String>,
        port: u16,
        user: impl Into<String>,
        pass: impl Into<String>,
    ) -> Self {
        Self::new_with_timeout(
            host,
            port,
            user,
            pass,
            Duration::from_secs(REQUEST_TIMEOUT_SECS),
        )
    }

    /// 指定单次请求超时，供异常退出后的补注销等短超时场景使用
    pub fn new_with_timeout(
        host: impl Into<String>,
        port: u16,
        user: impl Into<String>,
        pass: impl Into<String>,
        timeout: Duration,
    ) -> Self {
        let host = host.into();
        let http = HttpClient::new(host.clone(), port, timeout);
        Self {
            host,
            port,
            user: user.into(),
            pass: pass.into(),
            http,
            s_code: random_s_code(),
            hash: "0".repeat(64),
        }
    }

    /// 网关能力探测。响应不可解析时按默认值继续，不阻断登录。
    pub async fn probe(&mut self) -> Result<AuthParam, AuthError> {
        let referer = format!("https://{}/", self.host);
        let response = self
            .http
            .request(
                "POST",
                "/vone/login/auth_param",
                None,
                &[("Referer", &referer)],
            )
            .await?;

        if response.status >= 500 {
            return Err(AuthError::BadResponse(format!(
                "网关能力探测返回 HTTP {}",
                response.status
            )));
        }
        let text = response.text();
        // 端口指向普通网站时直接给出明确提示，避免后续验证码流程误判
        if looks_like_html(&text) {
            return Err(AuthError::NotGateway {
                status: response.status,
            });
        }
        let value = loose_parse(&text).unwrap_or(Value::Null);
        Ok(AuthParam::from_value(&value))
    }

    /// 取图形验证码 PNG，同时网关会下发 cgid Cookie
    pub async fn captcha(&mut self) -> Result<Vec<u8>, AuthError> {
        let referer = format!("https://{}/", self.host);
        let response = self
            .http
            .request("GET", "/vone/login/gid", None, &[("Referer", &referer)])
            .await?;

        if response.status != 200 {
            return Err(AuthError::BadResponse(format!(
                "验证码接口返回 HTTP {}",
                response.status
            )));
        }
        if response.body.len() < 100 {
            return Err(AuthError::BadResponse("验证码图片内容为空".to_string()));
        }
        Ok(response.body)
    }

    /// 提交账号密码与验证码
    pub async fn submit(&mut self, captcha_code: &str) -> Result<LoginSession, AuthError> {
        // 原生层最终发出的 U/P 为明文（前端 RSA/Base64 只是混淆）
        let body = format!(
            "U={}&P={}&Gid={}&HASH={}&S_CODE={}",
            form_encode(&self.user),
            form_encode(&self.pass),
            form_encode(captcha_code),
            form_encode(&self.hash),
            form_encode(&self.s_code),
        );

        let referer = format!("https://{}/vone/login/user_pswd/", self.host);
        let response = self
            .http
            .request(
                "POST",
                "/vone/login/user_pswd",
                Some(body.as_bytes()),
                &[
                    ("Referer", &referer),
                    ("Content-Type", "application/x-www-form-urlencoded"),
                ],
            )
            .await?;

        let text = response.text();
        let value = parse_gateway_json(&response, &text)?;

        let ret = read_ret(&value).ok_or_else(|| {
            AuthError::BadJson(format!("响应缺少 Ret 字段：{}", truncate(&text, 200)))
        })?;

        if ret != 0 {
            if ret == -40039 {
                // 密码已通过，网关要求补交短信/企业微信动态验证码
                return Err(AuthError::NeedSms {
                    challenge: read_bool(&value, "sms_challenge"),
                    sms_time: self.sms_time(&value),
                });
            }
            if matches!(ret, -40022 | -40023 | -40024 | -40199) {
                let user = value
                    .get("user_name")
                    .and_then(|v| v.as_str())
                    .filter(|text| !text.trim().is_empty())
                    .unwrap_or(&self.user)
                    .to_string();
                return Err(AuthError::PasswordChangeRequired { code: ret, user });
            }
            let gateway_msg = value
                .get("ErrMsg")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            return Err(AuthError::Rejected {
                code: ret,
                message: gateway_error_message(ret, gateway_msg),
            });
        }

        normalize_login(&value, &self.host, self.port)
    }

    /// 短信 / 企业微信动态验证码登录
    pub async fn login_by_sms(&mut self, user: &str, sms: &str) -> Result<LoginSession, AuthError> {
        // 官方客户端走 user_pswd：U/SMS/SMS_TIME=0/HASH/S_CODE，并复用挑战响应 Cookie
        let body = sms_login_body(user, sms, &self.hash, &self.s_code);
        self.apply_portal_cookies();
        self.http.remove_cookie("sv_un");

        let referer = format!("https://{}/vone/login/user_pswd", self.host);
        let response = self
            .http
            .request(
                "POST",
                "/vone/login/user_pswd",
                Some(body.as_bytes()),
                &[
                    ("Referer", &referer),
                    ("Content-Type", "application/x-www-form-urlencoded"),
                ],
            )
            .await?;

        let text = response.text();
        let value = parse_gateway_json(&response, &text)?;
        let ret = read_ret(&value).ok_or_else(|| {
            AuthError::BadJson(format!("响应缺少 Ret 字段：{}", truncate(&text, 200)))
        })?;

        if ret != 0 {
            if ret == -40039 {
                return Err(AuthError::NeedSms {
                    challenge: read_bool(&value, "sms_challenge"),
                    sms_time: self.sms_time(&value),
                });
            }
            if matches!(ret, -40022 | -40023 | -40024 | -40199) {
                let user = value
                    .get("user_name")
                    .and_then(|v| v.as_str())
                    .filter(|text| !text.trim().is_empty())
                    .unwrap_or(user)
                    .to_string();
                return Err(AuthError::PasswordChangeRequired { code: ret, user });
            }
            let gateway_msg = value
                .get("ErrMsg")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            return Err(AuthError::Rejected {
                code: ret,
                message: gateway_error_message(ret, gateway_msg),
            });
        }

        normalize_login(&value, &self.host, self.port)
    }

    /// 重新发送动态验证码，返回网关给出的有效期（秒）
    pub async fn send_sms(
        &mut self,
        user: &str,
        captcha: &str,
    ) -> Result<SmsSendResult, AuthError> {
        // 原生密码分支的 P 为空（密码已在 user_pswd 阶段校验），U 为明文，
        // Gid 为登录时的图形验证码；Cookie 需带 sv_un。
        self.apply_portal_cookies();
        self.http.upsert_cookies(&[("sv_un", user)]);
        let body = if captcha.is_empty() {
            format!("U={}&P=", form_encode(user))
        } else {
            format!("U={}&P=&Gid={}", form_encode(user), form_encode(captcha))
        };

        let referer = format!("https://{}/", self.host);
        let response = self
            .http
            .request(
                "POST",
                "/vone/login/send_sms",
                Some(body.as_bytes()),
                &[
                    ("Referer", &referer),
                    ("Content-Type", "application/x-www-form-urlencoded"),
                ],
            )
            .await?;

        let text = response.text();
        let value = parse_gateway_json(&response, &text)?;
        let ret = read_ret(&value).ok_or_else(|| {
            AuthError::BadJson(format!("响应缺少 Ret 字段：{}", truncate(&text, 200)))
        })?;

        if ret != 0 {
            let gateway_msg = value
                .get("ErrMsg")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            return Err(AuthError::Rejected {
                code: ret,
                message: gateway_error_message(ret, gateway_msg),
            });
        }

        let valid_time = read_sms_valid_time(&value);
        Ok(SmsSendResult {
            // 提交短信登录时必须回传网关种下的 topsecsveee；响应 JSON 仅作兜底。
            sms_time: self
                .cookie_sms_time()
                .or_else(|| read_sms_time(&value))
                .unwrap_or(valid_time),
            valid_time,
        })
    }

    /// 注销会话，释放单点登录占位。失败不抛错，由调用方决定是否提示。
    pub async fn logout(&mut self, session_id: &str) -> Result<(), AuthError> {
        let saved = self
            .http
            .replace_cookie(Some(format!("topafasfasfassession_id={session_id}")));
        let referer = format!("https://{}/", self.host);
        let result = self
            .http
            .request("POST", "/vone/login/logout", None, &[("Referer", &referer)])
            .await;
        self.http.replace_cookie(saved);
        result.map(|_| ())
    }

    /// 当前 Cookie（含验证码种下的 cgid），用于跨命令恢复找回密码会话
    pub fn cookie(&self) -> Option<&str> {
        self.http.cookie()
    }

    pub fn set_cookie(&mut self, cookie: impl Into<String>) {
        self.http.set_cookie(cookie);
    }

    /// 网关在挑战响应用 `topsecsveee` Cookie 下发提交短信登录所需的 SMS_TIME。
    fn cookie_sms_time(&self) -> Option<u32> {
        self.http
            .cookie_value("topsecsveee")
            .and_then(|value| value.trim().parse::<u32>().ok())
            .filter(|value| *value > 0)
            .map(|value| value.min(600))
    }

    fn sms_time(&self, value: &Value) -> u32 {
        self.cookie_sms_time()
            .or_else(|| read_sms_time(value))
            .unwrap_or(60)
    }

    /// 原客户端所有认证请求都会带上的门户静态 Cookie。
    fn apply_portal_cookies(&mut self) {
        self.http.upsert_cookies(&[
            ("topsecsvportallogodir", "default"),
            ("topsecsvuilanguage", "chinese"),
            ("topsecsvportalstyle", "style1"),
            ("topsecsvportalname", "default"),
        ]);
        // 登录失败次数由网关下发；不要用固定的 0 覆盖已有计数。
        if self.http.cookie_value("login_err_num").is_none() {
            self.http.upsert_cookies(&[("login_err_num", "0")]);
        }
    }

    /// 忘记密码：提交用户名与图形验证码，网关向绑定邮箱发送重置信息。
    pub async fn forget_password(
        &mut self,
        user: &str,
        captcha_code: &str,
    ) -> Result<ForgetResult, AuthError> {
        let base64_user = encode_user(user);
        // 第一步：校验图形验证码并请求发送重置信息（动态码场景返回 -40230）
        // 原生实现中 Gid 与 S_Gid 使用同一个图形验证码值
        let body = format!(
            "U={}&Base64_U={}&Gid={}&S_Gid={}",
            form_encode(user),
            form_encode(&base64_user),
            form_encode(captcha_code),
            form_encode(captcha_code),
        );
        let value = self.post_find_pswd(&body).await?;
        let ret =
            read_ret(&value).ok_or_else(|| AuthError::BadJson("响应缺少 Ret 字段".to_string()))?;

        if ret == -40230 {
            return Ok(ForgetResult {
                need_sms: true,
                target: String::new(),
                cookie: self.cookie().unwrap_or_default().to_string(),
            });
        }
        if ret != 0 {
            return Err(forget_password_error(ret, &value));
        }

        Ok(ForgetResult {
            need_sms: false,
            target: read_target(&value),
            cookie: self.cookie().unwrap_or_default().to_string(),
        })
    }

    /// 第二步：动态验证码 + 新密码重置（P 为明文新密码）；-40230 同样表示重置成功
    pub async fn reset_password(
        &mut self,
        user: &str,
        sms: &str,
        new_password: &str,
    ) -> Result<ForgetResult, AuthError> {
        let base64_user = encode_user(user);
        // 原生层会把前端的 RSA 密文解密后再发给网关；直接请求时 P 用明文新密码。
        let body = format!(
            "U={}&Base64_U={}&P={}&SMS={}&S_Gid=",
            form_encode(user),
            form_encode(&base64_user),
            form_encode(new_password),
            form_encode(sms),
        );
        let value = self.post_find_pswd(&body).await?;
        let ret =
            read_ret(&value).ok_or_else(|| AuthError::BadJson("响应缺少 Ret 字段".to_string()))?;
        if !reset_password_success(ret) {
            return Err(reset_password_error(ret, &value));
        }

        Ok(ForgetResult {
            need_sms: false,
            target: read_target(&value),
            cookie: self.cookie().unwrap_or_default().to_string(),
        })
    }

    /// 修改密码：Cookie 的 sv_un 为明文账号，P1/P2 为明文原密码与新密码
    pub async fn change_password(
        &mut self,
        user: &str,
        old_password: &str,
        new_password: &str,
        session_id: &str,
    ) -> Result<(), AuthError> {
        // 原生层会解密前端 RSA 密文；直接请求时 P1/P2 使用明文密码。
        // Cookie 的 sv_un 为明文账号；已有登录会话时请求体只发 P1/P2。
        self.apply_portal_cookies();
        self.http
            .upsert_cookies(&[("sv_un", user), ("seccheck", "0")]);
        if !session_id.is_empty() {
            self.http
                .upsert_cookies(&[("topafasfassession_id", session_id)]);
        }
        let body = if session_id.is_empty() {
            format!(
                "U={}&P1={}&P2={}",
                form_encode(user),
                form_encode(old_password),
                form_encode(new_password),
            )
        } else {
            format!(
                "P1={}&P2={}",
                form_encode(old_password),
                form_encode(new_password),
            )
        };

        let referer = format!("https://{}/", self.host);
        let response = self
            .http
            .request(
                "POST",
                "/vone/chg_pwd",
                Some(body.as_bytes()),
                &[
                    ("Referer", &referer),
                    (
                        "Content-Type",
                        "application/x-www-form-urlencoded;charset=UTF-8",
                    ),
                ],
            )
            .await?;
        let text = response.text();
        let value = parse_gateway_json(&response, &text)?;
        let ret =
            read_ret(&value).ok_or_else(|| AuthError::BadJson("响应缺少 Ret 字段".to_string()))?;
        if ret != 0 {
            return Err(rejected_error(ret, &value));
        }
        Ok(())
    }

    async fn post_find_pswd(&mut self, body: &str) -> Result<Value, AuthError> {
        let referer = format!("https://{}/", self.host);
        let response = self
            .http
            .request(
                "POST",
                "/vone/find_pswd",
                Some(body.as_bytes()),
                &[
                    ("Referer", &referer),
                    ("Content-Type", "application/x-www-form-urlencoded"),
                ],
            )
            .await?;
        let text = response.text();
        parse_gateway_json(&response, &text)
    }
}

/// 动态验证码登录表单：官方客户端固定发送 SMS_TIME=0，并复用密码请求的 HASH/S_CODE。
fn sms_login_body(user: &str, sms: &str, hash: &str, s_code: &str) -> String {
    format!(
        "U={}&SMS={}&SMS_TIME=0&HASH={}&S_CODE={}",
        form_encode(user),
        form_encode(sms),
        form_encode(hash),
        form_encode(s_code),
    )
}

fn rejected_error(ret: i32, value: &Value) -> AuthError {
    let gateway_msg = value
        .get("ErrMsg")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    AuthError::Rejected {
        code: ret,
        message: gateway_error_message(ret, gateway_msg),
    }
}

/// 找回密码第一步：网关对无邮箱/被禁用账号统一返回“禁止找回密码”，按用户可见语义重写。
fn forget_password_error(ret: i32, value: &Value) -> AuthError {
    let err = rejected_error(ret, value);
    if let AuthError::Rejected { message, .. } = &err {
        if ret == -40075 || message.contains("禁止找回密码") {
            return AuthError::Rejected {
                code: ret,
                message: "该用户不存在或已被禁用".to_string(),
            };
        }
    }
    err
}

/// 找回密码第二步的 `-40230` 在原客户端同样按成功处理并关闭弹窗。
fn reset_password_success(ret: i32) -> bool {
    ret == 0 || ret == -40230
}

/// 找回密码第二步：网关把新旧密码相同等重置失败统一报成“找回密码失败”。
fn reset_password_error(ret: i32, value: &Value) -> AuthError {
    let err = rejected_error(ret, value);
    if let AuthError::Rejected { message, .. } = &err {
        if matches!(ret, -40052 | -40071) || message.contains("找回密码失败") {
            return AuthError::Rejected {
                code: ret,
                message: "新密码不能与旧密码一致".to_string(),
            };
        }
    }
    err
}

fn read_target(value: &Value) -> String {
    value
        .get("Mail")
        .and_then(|v| v.as_str())
        .filter(|text| !text.trim().is_empty())
        .or_else(|| value.get("Phone").and_then(|v| v.as_str()))
        .unwrap_or_default()
        .to_string()
}

/// 找回密码提交结果
#[derive(Debug, Clone)]
pub struct ForgetResult {
    /// 网关返回的接收方（邮箱或手机号），为空表示网关未提供
    pub target: String,
    /// 网关要求二次输入动态验证码并设置新密码
    pub need_sms: bool,
    /// 与本次请求绑定的 Cookie，第二步必须复用
    pub cookie: String,
}

fn read_bool(value: &Value, key: &str) -> bool {
    match value.get(key) {
        Some(Value::Bool(flag)) => *flag,
        Some(Value::String(text)) => text == "true" || text == "1",
        Some(Value::Number(number)) => number.as_i64().unwrap_or(0) != 0,
        _ => false,
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SmsSendResult {
    /// 提交短信登录时回传的 SMS_TIME
    pub sms_time: u32,
    /// 界面倒计时使用的有效期
    pub valid_time: u32,
}

/// 原客户端所有 U 字段均使用 UTF-8 账号的 Base64。
fn encode_user(user: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(user.as_bytes())
}

/// 网关下发的 SMS_TIME；缺失时返回 None，由调用方回退 Cookie 或 60 秒。
fn read_sms_time(value: &Value) -> Option<u32> {
    value
        .get("sms_time")
        .and_then(|item| match item {
            Value::Number(number) => number.as_u64(),
            Value::String(text) => text.trim().parse().ok(),
            _ => None,
        })
        .filter(|value| *value > 0)
        .map(|value| value.min(600) as u32)
}

/// 动态验证码有效期：网关未给出时按 60 秒，超过 600 秒（10 分钟）一律截断
fn read_sms_valid_time(value: &Value) -> u32 {
    value
        .get("sms_valid_time")
        .and_then(|v| v.as_u64())
        .unwrap_or(60)
        .min(600) as u32
}

/// 解析网关伪 JSON；若返回的是 HTML 页面，直接判定为端口指向了非 VPN 服务
fn parse_gateway_json(response: &HttpResponse, text: &str) -> Result<Value, AuthError> {
    match loose_parse(text) {
        Some(value) => Ok(value),
        None if looks_like_html(text) => Err(AuthError::NotGateway {
            status: response.status,
        }),
        None => Err(AuthError::BadJson(format!(
            "HTTP {}，响应片段：{}",
            response.status,
            truncate(text, 200)
        ))),
    }
}

fn looks_like_html(text: &str) -> bool {
    let head: String = text
        .trim_start()
        .chars()
        .take(80)
        .collect::<String>()
        .to_ascii_lowercase();
    head.starts_with("<!doctype") || head.starts_with("<html")
}

fn read_ret(value: &Value) -> Option<i32> {
    let raw = value.get("Ret").or_else(|| value.get("ret"))?;
    match raw {
        Value::Number(n) => n.as_i64().map(|v| v as i32),
        Value::String(s) => s.trim().parse::<i32>().ok(),
        _ => None,
    }
}

/// 网关只要求该字段存在且格式合法，服务端不校验内容
fn random_s_code() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        text.chars().take(max).collect::<String>() + "…"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn 读取_ret_数字与字符串() {
        assert_eq!(read_ret(&json!({"Ret": 0})), Some(0));
        assert_eq!(read_ret(&json!({"Ret": "-40018"})), Some(-40018));
        assert_eq!(read_ret(&json!({"ret": 0})), Some(0));
        assert_eq!(read_ret(&json!({})), None);
    }

    #[test]
    fn 随机_s_code_为_16_字节_base64() {
        let a = random_s_code();
        let b = random_s_code();
        assert_ne!(a, b, "两次生成不应相同");
        let decoded = base64::engine::general_purpose::STANDARD.decode(a).unwrap();
        assert_eq!(decoded.len(), 16);
    }

    #[test]
    fn 读取二次认证标志() {
        assert!(read_bool(&json!({"sms_challenge": true}), "sms_challenge"));
        assert!(read_bool(&json!({"sms_challenge": "1"}), "sms_challenge"));
        assert!(read_bool(&json!({"sms_challenge": 1}), "sms_challenge"));
        assert!(!read_bool(
            &json!({"sms_challenge": false}),
            "sms_challenge"
        ));
        assert!(!read_bool(&json!({}), "sms_challenge"));
    }

    #[test]
    fn 动态验证码有效期默认_60_秒且上限_600() {
        assert_eq!(read_sms_valid_time(&json!({})), 60);
        assert_eq!(read_sms_valid_time(&json!({"sms_valid_time": 60})), 60);
        assert_eq!(read_sms_valid_time(&json!({"sms_valid_time": 900})), 600);
        // 非数字按缺省处理
        assert_eq!(read_sms_valid_time(&json!({"sms_valid_time": "60"})), 60);
    }

    #[test]
    fn 门户_cookie_保留网关下发的登录失败次数() {
        let mut auth = AuthClient::new(
            "vpn.example.com".to_string(),
            443,
            String::new(),
            String::new(),
        );
        auth.apply_portal_cookies();
        assert_eq!(
            auth.http.cookie_value("login_err_num").as_deref(),
            Some("0")
        );

        auth.http.upsert_cookies(&[("login_err_num", "3")]);
        auth.apply_portal_cookies();
        assert_eq!(
            auth.http.cookie_value("login_err_num").as_deref(),
            Some("3")
        );
    }

    #[test]
    fn 识别_html_响应为错误端口() {
        assert!(looks_like_html("<!DOCTYPE html><html></html>"));
        assert!(looks_like_html("  \n<html><body>login</body></html>"));
        assert!(!looks_like_html("{\"Ret\":0}"));
        assert!(!looks_like_html(""));
    }

    #[test]
    fn html_响应返回非网关端口错误() {
        let response = HttpResponse {
            status: 400,
            headers: Vec::new(),
            body: b"<!DOCTYPE html><html><head><title>400</title></head></html>".to_vec(),
            set_cookies: Vec::new(),
        };
        let err = parse_gateway_json(&response, &response.text()).unwrap_err();
        assert!(err.user_message().contains("网页"));
        assert!(err.user_message().contains("端口"));
    }

    #[test]
    fn 截断保留_utf8_边界() {
        assert_eq!(truncate("abc", 5), "abc");
        assert_eq!(truncate("abcdef", 3), "abc…");
        // 中文按字符截断，不产生半个字节
        assert_eq!(truncate("登录失败信息", 2), "登录…");
    }

    #[test]
    fn 找回密码第一步隐藏禁止找回密码的真实原因() {
        let err = forget_password_error(-40075, &json!({"ErrMsg": "禁止找回密码"}));
        assert_eq!(err.to_string(), "该用户不存在或已被禁用");
    }

    #[test]
    fn 动态验证码登录表单与官方客户端一致() {
        let body = sms_login_body("demo", "713375", "H1", "S1");
        assert_eq!(body, "U=demo&SMS=713375&SMS_TIME=0&HASH=H1&S_CODE=S1");
    }

    #[test]
    fn 找回密码第二步_40230_也视为成功() {
        assert!(reset_password_success(0));
        assert!(reset_password_success(-40230));
        assert!(!reset_password_success(-40052));
    }

    #[test]
    fn 找回密码第二步把通用失败映射为新旧密码相同() {
        let err = reset_password_error(-40071, &json!({"ErrMsg": "找回密码失败"}));
        assert_eq!(err.to_string(), "新密码不能与旧密码一致");
        let policy = reset_password_error(-40226, &json!({"ErrMsg": "密码长度至少8位"}));
        assert!(policy.to_string().contains("密码长度至少8位"));
    }
}
