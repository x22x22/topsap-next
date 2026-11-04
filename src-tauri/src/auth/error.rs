//! 登录相关错误与网关返回码映射。

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("网关响应超时（{0} 秒），请检查网络连接后重试")]
    Timeout(u64),
    #[error("网络错误：{0}")]
    Network(String),
    #[error("{0}")]
    Tls(#[from] crate::tls::TlsError),
    #[error("网关响应异常：{0}")]
    BadResponse(String),
    #[error("网关响应格式异常：{0}")]
    BadJson(String),
    /// 端口指向了普通网站而不是 VPN 网关接口
    #[error("该端口返回的是网页而不是 VPN 网关接口（HTTP {status}），请检查端口是否正确")]
    NotGateway { status: u16 },
    #[error("{message}")]
    Rejected { code: i32, message: String },
    /// 密码校验通过，但网关要求补交手机短信或企业微信动态验证码
    #[error("需要输入动态验证码")]
    NeedSms { challenge: bool, sms_time: u32 },
    /// 密码到期、过弱、首次登录或管理员要求改密
    #[error("需要修改密码后才能登录（Ret={code}）")]
    PasswordChangeRequired { code: i32, user: String },
    #[error("{0}")]
    Config(String),
}

impl AuthError {
    /// 面向界面的简短说明，去掉响应片段等内部细节
    pub fn user_message(&self) -> String {
        match self {
            Self::Tls(err) => err.user_message(),
            Self::BadResponse(detail) => describe_bad_response(detail),
            Self::BadJson(detail) => describe_bad_json(detail),
            Self::PasswordChangeRequired { code, .. } => password_change_message(*code),
            _ => self.to_string(),
        }
    }
}

fn describe_bad_response(detail: &str) -> String {
    match extract_status(detail) {
        Some(status) => http_status_message(status),
        None => "网关响应异常，请检查网关地址与端口".to_string(),
    }
}

fn describe_bad_json(detail: &str) -> String {
    match extract_status(detail) {
        Some(status) => http_status_message(status),
        None if detail.contains("响应缺少 Ret") => {
            "网关返回内容不完整，请确认该端口提供的是 VPN 网关服务".to_string()
        }
        None => "网关返回内容无法识别，请检查网关地址与端口".to_string(),
    }
}

/// 从 "HTTP 400，..." 这类内部描述中取出状态码
fn extract_status(detail: &str) -> Option<u16> {
    detail
        .split("HTTP ")
        .nth(1)?
        .split(|ch: char| !ch.is_ascii_digit())
        .next()?
        .parse()
        .ok()
}

fn http_status_message(status: u16) -> String {
    match status {
        400 => "网关返回 HTTP 400，请确认端口填写正确且该端口提供 VPN 网关服务".to_string(),
        401 | 403 => format!("网关拒绝访问（HTTP {status}），请检查账号权限或网关配置"),
        404 => "网关接口不存在（HTTP 404），请检查网关地址与端口".to_string(),
        500..=599 => format!("网关服务异常（HTTP {status}），请稍后重试或联系管理员"),
        _ => format!("网关返回异常状态（HTTP {status}），请检查网关地址与端口"),
    }
}

/// 网关返回码 -> 中文说明。保留原始 Ret 码便于排查。
pub fn gateway_error_message(code: i32, fallback: &str) -> String {
    // -40226 的策略文本由后台动态下发（长度、字符集、历史密码限制等），
    // 原客户端直接展示 ErrMsg；不能再用固定文案覆盖。
    if code == -40226 && !fallback.trim().is_empty() {
        return percent_decode(fallback.trim());
    }
    // 凭据错误码的固定文案会掩盖网关自己的原因（锁定、账号不存在等）
    if matches!(code, -40080 | -40081) {
        let decoded = percent_decode(fallback.trim());
        if !decoded.is_empty() && !is_generic_credential_message(&decoded) {
            return format!("{decoded}（Ret={code}）");
        }
    }
    let base = match code {
        -7 => "请求参数格式错误，请检查账号或密码是否包含特殊字符",
        -40018 => "验证码错误或已过期，请重新获取",
        -40077 => "账号已在别处登录，请确认官方客户端已完全退出后重试",
        -40081 => "账号或密码错误",
        -40050 => "账号已被锁定，请联系管理员",
        -40142 => "重置密码过于频繁，请稍后再试",
        -40039 => "需要输入动态验证码",
        -40045 => "动态验证码错误或已过期",
        -40037 | -40129 => "新密码不能与旧密码一致",
        -40052 | -40071 => "找回密码失败",
        -40075 => "禁止找回密码",
        -40123 => "用户未拥有任何资源地址，请联系管理员",
        -40078 => "原密码错误",
        -40022 => "登录用户密码有效期已到，请修改密码",
        -40023 => "密码过于简单，请修改密码",
        -40024 => "首次登录，请修改初始密码",
        -40199 => "管理员要求修改密码",
        -40135 => "认证因子校验失败，请重新发送动态验证码",
        -40226 => "新密码不符合网关密码策略",
        -40227 | -40057 => "网关版本不支持该功能，请联系管理员升级",
        -40228 => "网关参数获取失败，请稍后重试",
        _ => "",
    };
    if base.is_empty() {
        if fallback.trim().is_empty() {
            format!("登录被拒绝（Ret={code}）")
        } else {
            format!("{fallback}（Ret={code}）")
        }
    } else {
        format!("{base}（Ret={code}）")
    }
}

/// 需要先改密才能登录的返回码说明。
pub fn password_change_message(code: i32) -> String {
    match code {
        -40022 => "登录用户密码有效期已到，请修改密码",
        -40023 => "密码过于简单，请修改密码",
        -40024 => "首次登录，请修改初始密码",
        -40199 => "管理员要求修改密码",
        _ => "需要修改密码后才能登录",
    }
    .to_string()
}

/// 部分网关把「密码到期/首次登录」等状态包在 -40081 的 ErrMsg 里返回。
/// 只有文本明确指向密码状态时才转入改密流程，普通凭据错误不受影响。
pub fn password_change_code_from_hint(message: &str) -> Option<i32> {
    let text = percent_decode(message.trim());
    if text.is_empty() {
        return None;
    }
    if text.contains("有效期已到")
        || text.contains("密码已过期")
        || text.contains("密码过期")
        || text.contains("密码到期")
    {
        Some(-40022)
    } else if text.contains("首次登录") || text.contains("第一次登录") || text.contains("初始密码")
    {
        Some(-40024)
    } else if text.contains("密码过于简单") || text.contains("密码太简单") {
        Some(-40023)
    } else if text.contains("管理员要求") && text.contains("密码") {
        Some(-40199)
    } else {
        None
    }
}

fn is_generic_credential_message(message: &str) -> bool {
    message.contains("用户名或密码错误")
        || message.contains("账号或密码错误")
        || message.contains("用户名或密码不正确")
        || message.contains("账号或密码不正确")
}

/// 网关 ErrMsg 常以 URL 编码返回中文策略文本，原前端使用 `unescape`。
fn percent_decode(text: &str) -> String {
    let expanded = decode_percent_u(text);
    percent_encoding::percent_decode_str(&expanded)
        .decode_utf8_lossy()
        .into_owned()
}

/// 兼容 JavaScript `unescape` 的 `%uXXXX` 形式（中文策略文本常见）。
fn decode_percent_u(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '%'
            && index + 5 < chars.len()
            && chars[index + 1] == 'u'
            && chars[index + 2..index + 6]
                .iter()
                .all(|ch| ch.is_ascii_hexdigit())
        {
            let hex: String = chars[index + 2..index + 6].iter().collect();
            if let Ok(code) = u16::from_str_radix(&hex, 16) {
                if let Some(ch) = char::from_u32(u32::from(code)) {
                    out.push(ch);
                    index += 6;
                    continue;
                }
            }
        }
        out.push(chars[index]);
        index += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 已知识别码给出具体原因() {
        assert!(gateway_error_message(-40018, "").contains("验证码"));
        assert!(gateway_error_message(-40077, "").contains("已在别处登录"));
    }

    #[test]
    fn 未知码保留原始_ret_并带网关消息() {
        let msg = gateway_error_message(-99999, "网关自定义原因");
        assert!(msg.contains("网关自定义原因"));
        assert!(msg.contains("-99999"));
    }

    #[test]
    fn 未知码且无消息时仍需可读() {
        let msg = gateway_error_message(-12345, "");
        assert!(msg.contains("-12345"));
        assert!(!msg.is_empty());
    }

    #[test]
    fn 密码到期返回码给出可操作文案() {
        assert!(password_change_message(-40022).contains("有效期"));
        assert!(password_change_message(-40024).contains("首次登录"));
        let err = AuthError::PasswordChangeRequired {
            code: -40022,
            user: "demo".to_string(),
        };
        assert!(err.user_message().contains("有效期"));
    }

    #[test]
    fn 密码策略优先使用网关下发的动态文本() {
        let msg = gateway_error_message(
            -40226,
            "%E5%AF%86%E7%A0%81%E8%A7%84%E5%88%99%3A%E6%9C%80%E5%B0%8F%E9%95%BF%E5%BA%A6%E4%B8%BA8",
        );
        assert_eq!(msg, "密码规则:最小长度为8");
        assert!(!msg.contains("Ret="));
    }

    #[test]
    fn 密码策略缺失时回退固定文案() {
        assert!(gateway_error_message(-40226, "").contains("密码策略"));
    }

    #[test]
    fn 新旧密码相同与无资源返回码给出明确文案() {
        assert_eq!(
            gateway_error_message(-40037, "修改密码失败"),
            "新密码不能与旧密码一致（Ret=-40037）"
        );
        assert_eq!(
            gateway_error_message(-40129, ""),
            "新密码不能与旧密码一致（Ret=-40129）"
        );
        assert_eq!(
            gateway_error_message(-40123, ""),
            "用户未拥有任何资源地址，请联系管理员（Ret=-40123）"
        );
    }

    #[test]
    fn 密码策略兼容_u_转义() {
        assert_eq!(
            gateway_error_message(-40226, "%u5BC6%u7801%u89C4%u5219"),
            "密码规则"
        );
    }

    #[test]
    fn 错误端口返回的_http_400_给出可操作提示() {
        let err = AuthError::BadJson("HTTP 400，响应片段：<!DOCTYPE html>".to_string());
        let msg = err.user_message();
        assert!(msg.contains("HTTP 400"));
        assert!(msg.contains("端口"));
        assert!(!msg.contains("<!DOCTYPE"));
    }

    #[test]
    fn 网页响应识别为非网关端口() {
        let err = AuthError::NotGateway { status: 400 };
        assert!(err.user_message().contains("网页"));
        assert!(err.user_message().contains("端口"));
    }

    #[test]
    fn 缺少_ret_的响应给出格式提示() {
        let err = AuthError::BadJson("响应缺少 Ret 字段：<html>".to_string());
        assert!(err.user_message().contains("内容不完整"));
    }

    #[test]
    fn 服务器错误提示稍后重试() {
        let err = AuthError::BadResponse("网关能力探测返回 HTTP 503".to_string());
        assert!(err.user_message().contains("503"));
        assert!(err.user_message().contains("稍后重试"));
    }

    #[test]
    fn 凭据错误优先展示网关自己的原因() {
        let msg = gateway_error_message(-40081, "用户已被锁定，请联系管理员");
        assert_eq!(msg, "用户已被锁定，请联系管理员（Ret=-40081）");
        // 网关文案与固定文案等价时不重复
        assert_eq!(
            gateway_error_message(-40081, "用户名或密码错误"),
            "账号或密码错误（Ret=-40081）"
        );
    }

    #[test]
    fn 凭据错误里的密码状态提示转为改密() {
        assert_eq!(
            password_change_code_from_hint("密码已过期，请修改密码"),
            Some(-40022)
        );
        assert_eq!(
            password_change_code_from_hint("首次登录，请修改初始密码"),
            Some(-40024)
        );
        assert_eq!(
            password_change_code_from_hint("管理员要求修改密码"),
            Some(-40199)
        );
        assert_eq!(password_change_code_from_hint("用户名或密码错误"), None);
        assert_eq!(password_change_code_from_hint(""), None);
    }
}
