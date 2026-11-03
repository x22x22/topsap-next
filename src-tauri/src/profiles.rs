//! 连接配置：多网关配置的增删改查与导入导出。
//!
//! 配置文件为 JSON，保存到应用配置目录；导入时按名称合并，同名覆盖。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::auth::model::is_ipv4;

const MAX_NAME_LEN: usize = 64;
const MAX_LINES: usize = 16;
const MAX_TIMEOUT_SECS: u32 = 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionProfile {
    pub name: String,
    #[serde(default = "default_login_way")]
    pub login_way: String,
    #[serde(default)]
    pub connect_type: u8,
    #[serde(default)]
    pub connect_select: bool,
    #[serde(default = "default_timeout")]
    pub connect_timeout: u32,
    #[serde(default)]
    pub lines: Vec<String>,
}

fn default_login_way() -> String {
    "pwd".to_string()
}

fn default_timeout() -> u32 {
    3
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProfileStore {
    #[serde(default)]
    pub active: String,
    #[serde(default)]
    pub connections: Vec<ConnectionProfile>,
}

impl ConnectionProfile {
    pub fn validate(&self) -> Result<(), String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err("连接名称不能为空".to_string());
        }
        if name.chars().count() > MAX_NAME_LEN {
            return Err(format!("连接名称过长（最多 {MAX_NAME_LEN} 个字符）"));
        }
        if name.contains([',', '&', '"', '\'', '%', '<', '>']) {
            return Err("连接名称不能包含 , & \" ' % < > 等字符".to_string());
        }
        if !matches!(self.connect_type, 0..=2) {
            return Err(
                "连接模式必须是 0（单线路）、1（多线路优选）或 2（多线路备份）".to_string(),
            );
        }
        if self.connect_timeout == 0 || self.connect_timeout > MAX_TIMEOUT_SECS {
            return Err(format!("检测超时必须在 1-{MAX_TIMEOUT_SECS} 秒之间"));
        }
        if self.lines.is_empty() {
            return Err("地址列表至少需要一条记录".to_string());
        }
        if self.lines.len() > MAX_LINES {
            return Err(format!("地址列表最多 {MAX_LINES} 条"));
        }
        for line in &self.lines {
            parse_line(line)?;
        }
        Ok(())
    }

    /// 首选线路（host, port）
    pub fn first_line(&self) -> Option<(String, u16)> {
        self.lines.first().and_then(|line| parse_line(line).ok())
    }
}

/// 解析 `host:port`，IPv6 使用 `[addr]:port`
pub fn parse_line(line: &str) -> Result<(String, u16), String> {
    let text = line.trim();
    if text.is_empty() {
        return Err("地址不能为空".to_string());
    }

    let (host, port_text) = if let Some(rest) = text.strip_prefix('[') {
        let (host, tail) = rest
            .split_once(']')
            .ok_or_else(|| format!("地址「{text}」缺少 ]"))?;
        let port_text = tail
            .strip_prefix(':')
            .ok_or_else(|| format!("地址「{text}」缺少端口"))?;
        (host.to_string(), port_text.to_string())
    } else {
        let (host, port_text) = text
            .rsplit_once(':')
            .ok_or_else(|| format!("地址「{text}」缺少端口，格式应为 host:port"))?;
        (host.to_string(), port_text.to_string())
    };

    if host.is_empty() {
        return Err(format!("地址「{text}」缺少主机名"));
    }
    if !is_ipv4(&host) && !is_valid_hostname(&host) && !host.contains(':') {
        return Err(format!("地址「{text}」的主机名不合法"));
    }
    let port: u16 = port_text
        .parse()
        .map_err(|_| format!("地址「{text}」的端口不是有效数字"))?;
    if port == 0 {
        return Err(format!("地址「{text}」的端口必须在 1-65535 之间"));
    }
    Ok((host, port))
}

fn is_valid_hostname(host: &str) -> bool {
    if host.len() > 255 {
        return false;
    }
    host.split('.').all(|part| {
        !part.is_empty()
            && part.len() <= 63
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    })
}

fn store_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = crate::paths::config_dir(app)?;
    Ok(dir.join("connections.json"))
}

pub fn load(app: &AppHandle) -> ProfileStore {
    let Ok(path) = store_path(app) else {
        return ProfileStore::default();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return ProfileStore::default();
    };
    let mut store: ProfileStore = serde_json::from_str(&text).unwrap_or_default();
    store.connections.retain(|item| item.validate().is_ok());
    if !store
        .connections
        .iter()
        .any(|item| item.name == store.active)
    {
        store.active = store
            .connections
            .first()
            .map(|item| item.name.clone())
            .unwrap_or_default();
    }
    store
}

pub fn save(app: &AppHandle, store: &ProfileStore) -> Result<(), String> {
    let path = store_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建配置目录失败：{e}"))?;
    }
    let text =
        serde_json::to_string_pretty(store).map_err(|e| format!("序列化连接配置失败：{e}"))?;
    std::fs::write(&path, text).map_err(|e| format!("写入连接配置失败：{e}"))
}

pub fn upsert(app: &AppHandle, profile: ConnectionProfile) -> Result<ConnectionProfile, String> {
    profile.validate()?;
    let mut store = load(app);
    let name = profile.name.trim().to_string();
    let mut profile = profile;
    profile.name = name.clone();
    profile.login_way = if profile.login_way.trim().is_empty() {
        "pwd".to_string()
    } else {
        profile.login_way.trim().to_string()
    };

    match store.connections.iter_mut().find(|item| item.name == name) {
        Some(existing) => *existing = profile.clone(),
        None => store.connections.push(profile.clone()),
    }
    if store.active.is_empty() {
        store.active = name;
    }
    save(app, &store)?;
    Ok(profile)
}

pub fn remove(app: &AppHandle, name: &str) -> Result<(), String> {
    let mut store = load(app);
    store.connections.retain(|item| item.name != name);
    if store.active == name {
        store.active = store
            .connections
            .first()
            .map(|item| item.name.clone())
            .unwrap_or_default();
    }
    save(app, &store)
}

pub fn set_active(app: &AppHandle, name: &str) -> Result<(), String> {
    let mut store = load(app);
    if !store.connections.iter().any(|item| item.name == name) {
        return Err(format!("连接配置「{name}」不存在"));
    }
    store.active = name.to_string();
    save(app, &store)
}

pub fn clear(app: &AppHandle) -> Result<(), String> {
    save(app, &ProfileStore::default())
}

/// 从 JSON 文件导入，同名覆盖；支持 `ProfileStore` 或纯数组两种格式。
pub fn import_from(app: &AppHandle, path: &str) -> Result<usize, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("读取配置文件失败：{e}"))?;

    let imported: Vec<ConnectionProfile> = match serde_json::from_str::<ProfileStore>(&text) {
        Ok(store) if !store.connections.is_empty() => store.connections,
        _ => serde_json::from_str::<Vec<ConnectionProfile>>(&text)
            .map_err(|e| format!("配置文件格式不正确：{e}"))?,
    };

    let mut valid = Vec::new();
    for profile in imported {
        profile
            .validate()
            .map_err(|e| format!("配置「{}」不合法：{e}", profile.name))?;
        valid.push(profile);
    }
    if valid.is_empty() {
        return Err("配置文件中没有可导入的连接".to_string());
    }

    let mut store = load(app);
    for profile in &valid {
        match store
            .connections
            .iter_mut()
            .find(|item| item.name == profile.name)
        {
            Some(existing) => *existing = profile.clone(),
            None => store.connections.push(profile.clone()),
        }
    }
    if store.active.is_empty() {
        store.active = valid[0].name.clone();
    }
    save(app, &store)?;
    Ok(valid.len())
}

/// 导出全部连接配置到 JSON 文件。
pub fn export_to(app: &AppHandle, path: &str) -> Result<usize, String> {
    let store = load(app);
    if store.connections.is_empty() {
        return Err("当前没有可导出的连接配置".to_string());
    }
    let text =
        serde_json::to_string_pretty(&store).map_err(|e| format!("序列化连接配置失败：{e}"))?;
    std::fs::write(path, text).map_err(|e| format!("写入配置文件失败：{e}"))?;
    Ok(store.connections.len())
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
            connect_timeout: 3,
            lines: lines.into_iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn 解析地址_域名与端口() {
        let (host, port) = parse_line("vpn.example.com:8443").unwrap();
        assert_eq!(host, "vpn.example.com");
        assert_eq!(port, 8443);
    }

    #[test]
    fn 解析地址_ipv4() {
        let (host, port) = parse_line("192.0.2.10:443").unwrap();
        assert_eq!(host, "192.0.2.10");
        assert_eq!(port, 443);
    }

    #[test]
    fn 解析地址_ipv6() {
        let (host, port) = parse_line("[2001:db8::1]:8443").unwrap();
        assert_eq!(host, "2001:db8::1");
        assert_eq!(port, 8443);
    }

    #[test]
    fn 解析地址_缺少端口报错() {
        assert!(parse_line("vpn.example.com").is_err());
    }

    #[test]
    fn 解析地址_端口越界报错() {
        assert!(parse_line("vpn.example.com:0").is_err());
        assert!(parse_line("vpn.example.com:70000").is_err());
    }

    #[test]
    fn 校验_合法配置() {
        assert!(profile(vec!["vpn.example.com:8443"]).validate().is_ok());
    }

    #[test]
    fn 校验_名称为空或含非法字符() {
        let mut p = profile(vec!["vpn.example.com:8443"]);
        p.name = "  ".to_string();
        assert!(p.validate().is_err());
        p.name = "a,b".to_string();
        assert!(p.validate().is_err());
    }

    #[test]
    fn 校验_连接模式与超时() {
        let mut p = profile(vec!["vpn.example.com:8443"]);
        p.connect_type = 3;
        assert!(p.validate().is_err());
        p.connect_type = 1;
        p.connect_timeout = 0;
        assert!(p.validate().is_err());
        p.connect_timeout = 61;
        assert!(p.validate().is_err());
    }

    #[test]
    fn 校验_地址列表为空或超限() {
        let mut p = profile(vec![]);
        assert!(p.validate().is_err());
        let lines: Vec<String> = (0..17).map(|i| format!("vpn{i}.example.com:443")).collect();
        p.lines = lines;
        assert!(p.validate().is_err());
    }

    #[test]
    fn 首选线路() {
        let p = profile(vec!["vpn1.example.com:8443", "vpn2.example.com:8443"]);
        assert_eq!(p.first_line(), Some(("vpn1.example.com".to_string(), 8443)));
    }

    #[test]
    fn 导入格式_纯数组与完整结构() {
        let single = r#"[{"name":"A","lines":["vpn.example.com:443"]}]"#;
        assert!(serde_json::from_str::<Vec<ConnectionProfile>>(single).is_ok());

        let full = r#"{"active":"A","connections":[{"name":"A","lines":["vpn.example.com:443"]}]}"#;
        assert!(serde_json::from_str::<ProfileStore>(full).is_ok());
    }
}
