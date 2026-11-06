//! 登录响应与网关配置的数据结构，以及归一化解析。

use std::collections::HashSet;

use serde_json::Value;

use super::error::AuthError;

#[derive(Debug, Clone, Default)]
pub struct RouteEntry {
    pub dest: String,
    pub mask: String,
    pub prefix: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostEntry {
    pub ip: String,
    pub name: String,
}

#[derive(Debug, Clone, Default)]
pub struct AclEntry {
    pub protocol: String,
    pub sport: String,
    pub dport: String,
    pub dest: String,
    pub mask: String,
    pub action: String,
    pub week: String,
    pub start: String,
    pub end: String,
}

/// 网关的图形验证码策略，对应 `auth_param.gid` 与 `login_check_verify`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptchaMode {
    /// 不需要验证码
    Off,
    /// 连续失败达到阈值后才要求验证码
    Auto,
    /// 每次登录都要求验证码
    On,
}

/// auto 模式下触发图形验证码的失败次数，与原客户端 `login_err_num >= 3` 一致
pub const AUTO_CAPTCHA_THRESHOLD: u32 = 3;

/// 网关能力探测结果
#[derive(Debug, Clone)]
pub struct AuthParam {
    pub vpn_version: String,
    pub cipher_type: i32,
    pub captcha_mode: CaptchaMode,
    /// 网关额外要求验证码（`login_check_verify=on`）
    pub verify_captcha: bool,
    pub auth_port: Option<u16>,
    pub data_port: Option<u16>,
}

impl AuthParam {
    pub fn from_value(value: &Value) -> Self {
        let gid = get_string(value, "gid");
        let legacy = get_i64(value, "captcha").unwrap_or(0) != 0;
        let captcha_mode = match gid.trim().to_ascii_lowercase().as_str() {
            "on" => CaptchaMode::On,
            "auto" => CaptchaMode::Auto,
            "off" => CaptchaMode::Off,
            // 老网关只下发数字型 captcha 字段，兼容保留
            _ if legacy => CaptchaMode::On,
            _ => CaptchaMode::Off,
        };
        let verify = get_string(value, "login_check_verify");
        let verify_captcha = verify.trim().eq_ignore_ascii_case("on")
            || verify.trim() == "1"
            || matches!(value.get("login_check_verify"), Some(Value::Bool(true)));
        Self {
            vpn_version: get_string(value, "vpn_version"),
            cipher_type: get_i64(value, "cipher_type").unwrap_or(0) as i32,
            captcha_mode,
            verify_captcha,
            auth_port: get_i64(value, "auth_port").and_then(|v| u16::try_from(v).ok()),
            data_port: get_i64(value, "data_port").and_then(|v| u16::try_from(v).ok()),
        }
    }

    /// 本次登录是否需要图形验证码；auto 模式沿用网关下发的失败次数
    pub fn captcha_required(&self, login_err_num: u32) -> bool {
        match self.captcha_mode {
            CaptchaMode::On => true,
            CaptchaMode::Auto => login_err_num >= AUTO_CAPTCHA_THRESHOLD,
            CaptchaMode::Off => self.verify_captcha,
        }
    }

    /// auto 模式在失败次数达到阈值后需要转入验证码
    pub fn captcha_auto(&self) -> bool {
        self.captcha_mode == CaptchaMode::Auto
    }
}

/// 登录成功后归一化的会话信息
#[derive(Debug, Clone, Default)]
pub struct LoginSession {
    pub session_id: String,
    pub user_name: String,
    pub vip: String,
    pub vip_mask: String,
    pub dns1: String,
    pub dns2: String,
    pub tunnel_type: i32,
    pub spa_key: String,
    pub routes: Vec<RouteEntry>,
    pub hosts: Vec<HostEntry>,
    pub acl: Vec<AclEntry>,
    pub gateway: String,
    pub port: u16,
    /// 登录时解析到的网关 IP，供旁路路由使用
    pub gateway_ip: Option<String>,
    /// 网关版本（优先 tos_version，回退 auth_param 的 vpn_version）
    pub gateway_version: String,
    /// 登录历史信息
    pub login_info: LoginInfo,
    /// 门户资源列表
    pub resources: Vec<ResourceEntry>,
    /// 账号剩余天数，0 表示不提醒
    pub account_end_days: i64,
    /// 密码剩余天数，0 表示不提醒
    pub password_remind_days: i64,
}

#[derive(Debug, Clone, Default)]
pub struct LoginInfo {
    pub current_ip: String,
    pub current_time: String,
    pub previous_ip: String,
    pub previous_time: String,
    pub failed_ip: String,
    pub failed_time: String,
}

#[derive(Debug, Clone, Default)]
pub struct ResourceEntry {
    pub name: String,
    /// 资源网段地址；部分网关只填 name，缺失时回退 name
    pub ip: String,
    pub mask: String,
    pub url: String,
    pub group: String,
    pub module: String,
    /// show=no 的资源不展示，但仍需要按 ACL/资源网段下发路由
    pub visible: bool,
}

/// 归一化登录响应，并按 ACL 目标与资源网段生成路由。
pub fn normalize_login(value: &Value, gateway: &str, port: u16) -> Result<LoginSession, AuthError> {
    let user = value
        .get("user_param")
        .ok_or_else(|| AuthError::BadJson("登录响应缺少 user_param 节点".to_string()))?;
    let net = value
        .get("net_param")
        .ok_or_else(|| AuthError::BadJson("登录响应缺少 net_param 节点".to_string()))?;

    let session_id = get_string(user, "topafasfasfassession_id");
    if session_id.is_empty() {
        return Err(AuthError::BadJson(
            "登录响应缺少 session_id（topafasfasfassession_id）".to_string(),
        ));
    }

    let all_resources = parse_resources(value.get("res_list"));
    let resources: Vec<ResourceEntry> = all_resources
        .iter()
        .filter(|item| item.visible)
        .cloned()
        .collect();
    let acl = parse_acl(value.get("acl_list"));
    let routes = merge_routes(acl_routes(&acl), resource_routes(&all_resources));
    let hosts = parse_hosts(value.get("dns_info"));
    let login_info = LoginInfo {
        current_ip: get_string(user, "cur_ok_ip"),
        current_time: get_string(user, "cur_ok_tm"),
        previous_ip: get_string(user, "pre_ok_ip"),
        previous_time: get_string(user, "pre_ok_tm"),
        failed_ip: get_string(user, "pre_no_ip"),
        failed_time: get_string(user, "pre_no_tm"),
    };
    let tos_version = get_string(user, "tos_version");
    let gateway_version = if tos_version.is_empty() {
        get_string(user, "vpn_version")
    } else {
        tos_version
    };
    // 到期提醒通常位于 user_param，旧网关也可能放在响应顶层。
    let account_end_days = read_days(user, value, "account_end");
    let password_remind_days = read_days(user, value, "pwd_remind");

    Ok(LoginSession {
        session_id,
        user_name: get_string(user, "username"),
        vip: get_string(net, "vip_v4"),
        vip_mask: get_string(net, "vip_mask"),
        dns1: get_string(net, "dns1_v4"),
        dns2: get_string(net, "dns2_v4"),
        tunnel_type: get_i64(net, "tunnel_type").unwrap_or(0) as i32,
        spa_key: get_string(user, "spa_key"),
        routes,
        hosts,
        acl,
        gateway: gateway.to_string(),
        port,
        gateway_ip: None,
        gateway_version,
        login_info,
        resources,
        account_end_days,
        password_remind_days,
    })
}

/// 读取剩余天数；缺失或负数按 0（不提醒）处理。
fn read_days(primary: &Value, fallback: &Value, key: &str) -> i64 {
    get_i64(primary, key)
        .or_else(|| get_i64(fallback, key))
        .unwrap_or(0)
        .max(0)
}

fn parse_resources(value: Option<&Value>) -> Vec<ResourceEntry> {
    let mut out = Vec::new();
    for item in collect_items(value) {
        let name = get_string(item, "name");
        if name.is_empty() || get_string(item, "module") == "va" {
            continue;
        }
        let group = get_string(item, "group_name");
        out.push(ResourceEntry {
            name,
            ip: get_string(item, "ip"),
            mask: get_string(item, "mask"),
            url: get_string(item, "url"),
            group: if group.is_empty() {
                "default".to_string()
            } else {
                group
            },
            module: get_string(item, "module"),
            visible: get_string(item, "show") != "no",
        });
    }
    out
}

/// 从 ACL 目标网段生成路由；网关可能把 IPv4 目标放在 d_ip_v6 字段。
fn acl_routes(acl: &[AclEntry]) -> Vec<RouteEntry> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    for item in acl {
        let dest = item.dest.trim();
        if dest.is_empty() || dest == "0.0.0.0" || !is_ipv4(dest) {
            continue;
        }
        let Some(prefix) = mask_to_prefix(item.mask.trim()) else {
            continue;
        };
        if prefix == 0 || prefix > 32 {
            continue;
        }
        if let Some(dest) = network_address(dest, prefix) {
            push_route(&mut out, &mut seen, dest, prefix);
        }
    }

    out
}

/// 从资源列表提取需要走隧道的网段；优先 ip/mask，缺失时回退 name。
fn resource_routes(resources: &[ResourceEntry]) -> Vec<RouteEntry> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    for item in resources {
        let parsed = resource_network(item).or_else(|| name_route(&item.name));
        if let Some((dest, prefix)) = parsed {
            push_route(&mut out, &mut seen, dest, prefix);
        }
    }

    out
}

/// ACL 与资源网段取并集，保证资源列表中的地址都能出现在系统路由表。
fn merge_routes(primary: Vec<RouteEntry>, fallback: Vec<RouteEntry>) -> Vec<RouteEntry> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for route in primary.into_iter().chain(fallback) {
        if seen.insert((route.dest.clone(), route.prefix)) {
            out.push(route);
        }
    }
    out
}

fn push_route(
    out: &mut Vec<RouteEntry>,
    seen: &mut HashSet<(String, u8)>,
    dest: String,
    prefix: u8,
) {
    if seen.insert((dest.clone(), prefix)) {
        out.push(RouteEntry {
            mask: prefix_to_mask(prefix),
            dest,
            prefix,
        });
    }
}

fn resource_network(item: &ResourceEntry) -> Option<(String, u8)> {
    let ip = item.ip.trim();
    if !is_ipv4(ip) {
        return None;
    }
    let prefix = mask_to_prefix(item.mask.trim())?;
    if prefix == 0 || prefix > 32 {
        return None;
    }
    network_address(ip, prefix).map(|dest| (dest, prefix))
}

fn name_route(text: &str) -> Option<(String, u8)> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let (ip, prefix) = match text.split_once('/') {
        Some((ip, prefix)) => (ip.trim(), prefix.trim().parse::<u8>().ok()?),
        None => (text, 32),
    };
    if !is_ipv4(ip) || ip == "0.0.0.0" || prefix == 0 || prefix > 32 {
        return None;
    }
    Some((ip.to_string(), prefix))
}

fn network_address(ip: &str, prefix: u8) -> Option<String> {
    let value = ipv4_to_u32(ip)?;
    let mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix.min(32))
    };
    let dest = value & mask;
    if dest == 0 {
        return None;
    }
    Some(u32_to_ipv4(dest))
}

fn ipv4_to_u32(text: &str) -> Option<u32> {
    let parts: Vec<&str> = text.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let mut value = 0u32;
    for part in parts {
        value = (value << 8) | u32::from(part.parse::<u8>().ok()?);
    }
    Some(value)
}

fn u32_to_ipv4(value: u32) -> String {
    format!(
        "{}.{}.{}.{}",
        (value >> 24) & 0xff,
        (value >> 16) & 0xff,
        (value >> 8) & 0xff,
        value & 0xff
    )
}

fn prefix_to_mask(prefix: u8) -> String {
    let mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix.min(32))
    };
    u32_to_ipv4(mask)
}

fn parse_hosts(value: Option<&Value>) -> Vec<HostEntry> {
    let list = value
        .and_then(|v| v.get("dns_list"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    list.lines().filter_map(parse_host_line).collect()
}

fn parse_host_line(line: &str) -> Option<HostEntry> {
    let mut parts = line.split_whitespace();
    let ip = parts.next()?;
    if !is_ipv4(ip) {
        return None;
    }
    let raw = parts.next()?;
    let without_scheme = raw
        .strip_prefix("http://")
        .or_else(|| raw.strip_prefix("https://"))
        .unwrap_or(raw);
    let name = without_scheme.split(':').next()?;
    if name.is_empty() {
        return None;
    }
    Some(HostEntry {
        ip: ip.to_string(),
        name: name.to_string(),
    })
}

/// 解析 ACL；部分网关把真实 IPv4 目标放在 d_ip_v6，d_ip_v4 固定为 0.0.0.0。
fn parse_acl(value: Option<&Value>) -> Vec<AclEntry> {
    collect_items(value)
        .into_iter()
        .map(|item| AclEntry {
            protocol: get_string(item, "protocol"),
            sport: get_string(item, "sport"),
            dport: get_string(item, "dport"),
            dest: {
                let v4 = get_string(item, "d_ip_v4");
                let v4 = v4.trim();
                if is_ipv4(v4) && v4 != "0.0.0.0" {
                    v4.to_string()
                } else {
                    get_string(item, "d_ip_v6")
                }
            },
            mask: get_string(item, "d_mask"),
            action: get_string(item, "action"),
            week: get_string(item, "week"),
            start: get_string(item, "start_tm"),
            end: get_string(item, "end_tm"),
        })
        .collect()
}

/// 把数组或「数字键对象」统一成元素列表，保持原始顺序。
fn collect_items(value: Option<&Value>) -> Vec<&Value> {
    match value {
        Some(Value::Array(items)) => items.iter().collect(),
        Some(Value::Object(map)) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by_key(|k| k.parse::<usize>().unwrap_or(usize::MAX));
            keys.into_iter().filter_map(|k| map.get(k)).collect()
        }
        _ => Vec::new(),
    }
}

fn get_string(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

fn get_i64(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(|v| match v {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    })
}

pub fn is_ipv4(text: &str) -> bool {
    let parts: Vec<&str> = text.split('.').collect();
    parts.len() == 4
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.len() <= 3 && p.parse::<u8>().is_ok())
}

/// 推导点对点隧道的对端地址：官方客户端实测为 VIP-1。
///
/// 首地址（网络地址 + 1）时回退 VIP+1；/31 取相邻地址，/32 没有可用对端。
pub fn virtual_peer(vip: &str, mask: &str) -> Option<String> {
    let value = ipv4_to_u32(vip)?;
    let prefix = mask_to_prefix(mask)?;
    if prefix == 0 || prefix == 32 {
        return None;
    }
    if prefix == 31 {
        return Some(u32_to_ipv4(value ^ 1));
    }
    let host_bits = 32 - prefix;
    let netmask = u32::MAX << host_bits;
    let network = value & netmask;
    let broadcast = network | !netmask;
    if value == network || value == broadcast {
        return None;
    }
    let candidate = value.checked_sub(1)?;
    if candidate != network && candidate != broadcast {
        return Some(u32_to_ipv4(candidate));
    }
    let candidate = value.checked_add(1)?;
    (candidate != network && candidate != broadcast && candidate != value)
        .then(|| u32_to_ipv4(candidate))
}

/// 点分十进制掩码 -> 前缀长度。非连续掩码返回 None。
pub fn mask_to_prefix(mask: &str) -> Option<u8> {
    let parts: Vec<u8> = mask
        .split('.')
        .map(|p| p.parse::<u8>().ok())
        .collect::<Option<Vec<_>>>()?;
    if parts.len() != 4 {
        return None;
    }
    let bits = parts.iter().fold(0u32, |acc, b| (acc << 8) | u32::from(*b));
    let prefix = bits.leading_ones() as u8;
    let expected = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };
    if bits != expected {
        return None;
    }
    Some(prefix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ipv4_校验() {
        assert!(is_ipv4("192.0.2.10"));
        assert!(!is_ipv4("192.0.2"));
        assert!(!is_ipv4("192.0.2.256"));
        assert!(!is_ipv4(""));
    }

    #[test]
    fn 对端地址_按_vip_减一推导() {
        assert_eq!(
            virtual_peer("172.21.8.108", "255.255.248.0").as_deref(),
            Some("172.21.8.107")
        );
        assert_eq!(
            virtual_peer("172.21.15.82", "255.255.248.0").as_deref(),
            Some("172.21.15.81")
        );
        assert_eq!(
            virtual_peer("10.99.99.2", "255.255.255.0").as_deref(),
            Some("10.99.99.1")
        );
    }

    #[test]
    fn 对端地址_边界与非法输入() {
        assert_eq!(
            virtual_peer("10.0.0.1", "255.255.255.0").as_deref(),
            Some("10.0.0.2")
        );
        assert_eq!(virtual_peer("10.0.0.0", "255.255.255.0"), None);
        assert_eq!(virtual_peer("10.0.0.255", "255.255.255.0"), None);
        assert_eq!(
            virtual_peer("10.0.0.2", "255.255.255.254").as_deref(),
            Some("10.0.0.3")
        );
        assert_eq!(virtual_peer("10.0.0.2", "255.255.255.255"), None);
        assert_eq!(virtual_peer("bad", "255.255.255.0"), None);
        assert_eq!(virtual_peer("10.0.0.2", "255.0.255.0"), None);
    }

    #[test]
    fn 主机行解析_带协议与端口() {
        let e = parse_host_line("192.0.2.10 http://intranet.example.com:3000").unwrap();
        assert_eq!(e.ip, "192.0.2.10");
        assert_eq!(e.name, "intranet.example.com");
    }

    #[test]
    fn 主机行解析_纯主机名() {
        let e = parse_host_line("192.0.2.10   nginx.internal").unwrap();
        assert_eq!(e.name, "nginx.internal");
    }

    #[test]
    fn 主机行解析_非法行忽略() {
        assert!(parse_host_line("").is_none());
        assert!(parse_host_line("# comment").is_none());
        assert!(parse_host_line("192.0.2.10").is_none());
    }

    #[test]
    fn 归一化_完整登录响应() {
        let value = json!({
            "Ret": 0,
            "user_param": {
                "topafasfasfassession_id": "100&demo0000000000",
                "username": "test-user",
                "spa_key": "abc"
            },
            "net_param": {
                "vip_v4": "10.8.0.10",
                "vip_mask": "255.255.248.0",
                "dns1_v4": "198.51.100.5",
                "dns2_v4": "198.51.100.6",
                "tunnel_type": 1
            },
            "route_list": [
                {"dest": "10.8.0.0", "mask": "255.255.248.0"},
                {"dest": "0.0.0.0", "mask": "0.0.0.0"}
            ],
            "dns_info": {"dns_list": "192.0.2.10 http://intranet.example.com:3000\r\n"},
            "acl_list": [{"protocol": "tcp", "dport": "80", "d_ip_v4": "192.0.2.0", "d_mask": "255.255.255.0"}]
        });

        let s = normalize_login(&value, "vpn.example.com", 8443).expect("归一化");
        assert_eq!(s.session_id, "100&demo0000000000");
        assert_eq!(s.user_name, "test-user");
        assert_eq!(s.vip, "10.8.0.10");
        assert_eq!(s.routes.len(), 1, "ACL 目标应生成路由");
        assert_eq!(s.routes[0].dest, "192.0.2.0");
        assert_eq!(s.routes[0].mask, "255.255.255.0");
        assert_eq!(s.hosts.len(), 1);
        assert_eq!(s.acl.len(), 1);
        assert_eq!(s.port, 8443);
    }

    #[test]
    fn 归一化_资源列表与登录信息() {
        let value = json!({
            "user_param": {
                "topafasfasfassession_id": "s1",
                "username": "demo",
                "tos_version": "v3.220094.0061_NGVONE.1",
                "cur_ok_ip": "203.0.113.24",
                "cur_ok_tm": "1762100000",
                "pre_ok_ip": "203.0.113.18",
                "pre_ok_tm": "1762010000",
                "pre_no_ip": "203.0.113.9",
                "pre_no_tm": "1759900000"
            },
            "net_param": {},
            "res_list": [
                {"name": "10.0.1.0", "group_name": "办公区", "module": "na", "show": "yes"},
                {"name": "10.0.2.0", "group_name": "default", "module": "cifs", "show": "yes"},
                {"name": "隐藏资源", "group_name": "办公区", "module": "na", "show": "no"},
                {"name": "虚拟应用", "group_name": "办公区", "module": "va", "show": "yes"}
            ]
        });
        let s = normalize_login(&value, "h", 443).expect("归一化");
        assert_eq!(s.gateway_version, "v3.220094.0061_NGVONE.1");
        assert_eq!(s.login_info.current_ip, "203.0.113.24");
        assert_eq!(s.login_info.current_time, "1762100000");
        assert_eq!(s.resources.len(), 2, "show=no 与 module=va 应跳过");
        assert_eq!(s.resources[0].group, "办公区");
        assert_eq!(s.resources[1].group, "default");
    }

    #[test]
    fn 归一化_读取密码与账号到期提醒() {
        let value = json!({
            "user_param": {
                "topafasfasfassession_id": "s1",
                "pwd_remind": "5",
                "account_end": 0
            },
            "net_param": {}
        });
        let s = normalize_login(&value, "h", 443).expect("归一化");
        assert_eq!(s.password_remind_days, 5);
        assert_eq!(s.account_end_days, 0);

        let top_level = json!({
            "user_param": {"topafasfasfassession_id": "s2"},
            "net_param": {},
            "pwd_remind": -1,
            "account_end": "12"
        });
        let s = normalize_login(&top_level, "h", 443).expect("归一化");
        assert_eq!(s.password_remind_days, 0, "负数不提醒");
        assert_eq!(s.account_end_days, 12, "兼容顶层字段");
    }

    #[test]
    fn 归一化_缺少会话_id_报错() {
        let value = json!({"user_param": {}, "net_param": {}});
        assert!(normalize_login(&value, "h", 443).is_err());
    }

    #[test]
    fn 归一化_缺少_net_param_报错() {
        let value = json!({"user_param": {"topafasfasfassession_id": "s"}});
        assert!(normalize_login(&value, "h", 443).is_err());
    }

    fn resource(name: &str, url: &str) -> ResourceEntry {
        ResourceEntry {
            name: name.to_string(),
            ip: String::new(),
            mask: String::new(),
            url: url.to_string(),
            group: "default".to_string(),
            module: "na".to_string(),
            visible: true,
        }
    }

    fn resource_net(name: &str, ip: &str, mask: &str) -> ResourceEntry {
        ResourceEntry {
            name: name.to_string(),
            ip: ip.to_string(),
            mask: mask.to_string(),
            url: String::new(),
            group: "default".to_string(),
            module: "na".to_string(),
            visible: true,
        }
    }

    #[test]
    fn 资源地址生成路由_仅使用资源名() {
        let routes = resource_routes(&[
            resource("192.0.2.188", ""),
            resource("198.51.100.0/24", ""),
            resource("198.51.100.5/24", ""),
            resource("https://203.0.113.49:8443/portal", "https://203.0.113.49"),
            resource("internal.example.com", ""),
            resource("192.0.2.188", ""),
        ]);
        assert_eq!(routes.len(), 3, "重复、URL 与主机名应跳过");
        assert_eq!(routes[0].dest, "192.0.2.188");
        assert_eq!(routes[0].mask, "255.255.255.255");
        assert_eq!(routes[1].dest, "198.51.100.0");
        assert_eq!(routes[1].mask, "255.255.255.0");
        assert_eq!(routes[2].dest, "198.51.100.5", "CIDR 不做网络地址规范化");
    }

    #[test]
    fn 资源网段优先使用_ip_与_mask() {
        let routes = resource_routes(&[
            resource_net("198.51.100.0", "198.51.100.7", "255.255.255.0"),
            resource_net("10.0.0.0", "10.1.2.3", "255.0.0.0"),
            resource_net("非法掩码", "10.0.0.1", "0.0.0.0"),
        ]);
        assert_eq!(routes.len(), 2);
        assert_eq!(routes[0].dest, "198.51.100.0");
        assert_eq!(routes[0].mask, "255.255.255.0");
        assert_eq!(routes[1].dest, "10.0.0.0");
        assert_eq!(routes[1].mask, "255.0.0.0");
    }

    #[test]
    fn acl_目标优先非零_ipv4_否则回退_ipv6_字段() {
        let value = json!({
            "user_param": {"topafasfasfassession_id": "s"},
            "net_param": {},
            "acl_list": [
                {"d_ip_v4": "0.0.0.0", "d_ip_v6": "198.51.100.0", "d_mask": "255.255.255.0", "action": 0},
                {"d_ip_v4": "203.0.113.0", "d_ip_v6": "2001:db8::1", "d_mask": "255.255.255.0", "action": 0},
                {"d_ip_v4": "0.0.0.0", "d_ip_v6": "0.0.0.0", "d_mask": "255.255.255.255", "action": 0}
            ]
        });
        let s = normalize_login(&value, "h", 443).expect("归一化");
        assert_eq!(s.routes.len(), 2);
        assert_eq!(s.routes[0].dest, "198.51.100.0");
        assert_eq!(s.routes[1].dest, "203.0.113.0");
    }

    #[test]
    fn 隐藏资源不展示但仍生成路由() {
        let value = json!({
            "user_param": {"topafasfasfassession_id": "s"},
            "net_param": {},
            "res_list": [
                {"name": "隐藏资源", "ip": "10.1.0.0", "mask": "255.255.0.0", "show": "no", "module": "na"}
            ]
        });
        let s = normalize_login(&value, "h", 443).expect("归一化");
        assert!(s.resources.is_empty(), "show=no 不进入展示列表");
        assert_eq!(s.routes.len(), 1, "隐藏资源仍需要路由");
        assert_eq!(s.routes[0].dest, "10.1.0.0");
    }

    #[test]
    fn 归一化_优先使用资源地址而不是聚合路由() {
        let value = json!({
            "user_param": {"topafasfasfassession_id": "s1"},
            "net_param": {},
            "res_list": [
                {"name": "192.0.2.188", "module": "na", "show": "yes"},
                {"name": "198.51.100.0/24", "module": "na", "show": "yes"}
            ],
            "route_list": [{"dest": "203.0.113.0", "mask": "255.255.248.0"}]
        });
        let s = normalize_login(&value, "h", 443).expect("归一化");
        assert_eq!(s.routes.len(), 2);
        assert_eq!(s.routes[0].dest, "192.0.2.188");
        assert_eq!(s.routes[1].dest, "198.51.100.0");
    }

    #[test]
    fn 能力探测_按原客户端字段解析验证码策略() {
        let on = AuthParam::from_value(&json!({"gid": "on"}));
        assert_eq!(on.captcha_mode, CaptchaMode::On);
        assert!(on.captcha_required(0));

        let auto = AuthParam::from_value(&json!({"gid": "auto"}));
        assert_eq!(auto.captcha_mode, CaptchaMode::Auto);
        assert!(auto.captcha_auto());
        assert!(!auto.captcha_required(2));
        assert!(auto.captcha_required(3));

        let off = AuthParam::from_value(&json!({"gid": "off"}));
        assert_eq!(off.captcha_mode, CaptchaMode::Off);
        assert!(!off.captcha_required(9));
    }

    #[test]
    fn 能力探测_兼容数字验证码与额外校验开关() {
        let legacy = AuthParam::from_value(&json!({"captcha": 1}));
        assert_eq!(legacy.captcha_mode, CaptchaMode::On);

        let verify = AuthParam::from_value(&json!({"gid": "off", "login_check_verify": "on"}));
        assert!(verify.captcha_required(0));

        let none = AuthParam::from_value(&json!({"vpn_version": "ngvone", "cipher_type": 0}));
        assert_eq!(none.vpn_version, "ngvone");
        assert_eq!(none.cipher_type, 0);
        assert!(!none.captcha_required(0));
    }
}
