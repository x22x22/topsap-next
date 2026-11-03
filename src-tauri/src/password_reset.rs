//! 记录「忘记密码重置成功」的短暂标记。
//!
//! 原网关存在一个问题：通过找回密码改密后，登录仍可能返回 -40022（密码已到期）。
//! 这里只记录重置事实与时间，不保存密码；登录时据此尝试一次无会话的同密码改密，
//! 让网关重新计算有效期。标记在成功登录或正常改密后清除。

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::paths;

/// 标记有效期：超过该时间不再尝试自动刷新，避免掩盖真实的密码到期。
const WINDOW_SECS: u64 = 7 * 24 * 60 * 60;
/// 文件内旧记录保留时间，避免无限增长。
const RETAIN_SECS: u64 = 30 * 24 * 60 * 60;
const FILE_NAME: &str = "password_reset.json";

#[derive(Debug, Default, Serialize, Deserialize)]
struct Store {
    #[serde(default)]
    entries: HashMap<String, u64>,
}

fn key(host: &str, port: u16, user: &str) -> String {
    format!(
        "{}:{}:{}",
        host.trim().to_ascii_lowercase(),
        port,
        user.trim()
    )
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(0)
}

fn is_recent(timestamp: u64, now: u64) -> bool {
    timestamp > 0 && now.saturating_sub(timestamp) <= WINDOW_SECS
}

fn file_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(paths::config_dir(app)?.join(FILE_NAME))
}

fn load(app: &AppHandle) -> Store {
    let Ok(path) = file_path(app) else {
        return Store::default();
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save(app: &AppHandle, store: &Store) -> Result<(), String> {
    let path = file_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建密码标记目录失败：{e}"))?;
    }
    let text =
        serde_json::to_string_pretty(store).map_err(|e| format!("序列化密码标记失败：{e}"))?;
    std::fs::write(path, text).map_err(|e| format!("写入密码标记失败：{e}"))
}

/// 忘记密码重置成功后调用。
pub fn mark(app: &AppHandle, host: &str, port: u16, user: &str) -> Result<(), String> {
    let mut store = load(app);
    let now = now_secs();
    store
        .entries
        .retain(|_, timestamp| now.saturating_sub(*timestamp) <= RETAIN_SECS);
    store.entries.insert(key(host, port, user), now);
    save(app, &store)
}

/// 是否存在仍在窗口内的重置标记。
pub fn recent(app: &AppHandle, host: &str, port: u16, user: &str) -> bool {
    let store = load(app);
    store
        .entries
        .get(&key(host, port, user))
        .copied()
        .is_some_and(|timestamp| is_recent(timestamp, now_secs()))
}

/// 成功登录或正常改密后清除标记。
pub fn clear(app: &AppHandle, host: &str, port: u16, user: &str) {
    let mut store = load(app);
    if store.entries.remove(&key(host, port, user)).is_some() {
        let _ = save(app, &store);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 标记键_主机不区分大小写() {
        assert_eq!(
            key("VPN.Example.com", 443, " Alice "),
            "vpn.example.com:443:Alice"
        );
    }

    #[test]
    fn 重置窗口判断() {
        assert!(is_recent(100, 100));
        assert!(is_recent(100, 100 + WINDOW_SECS));
        assert!(!is_recent(100, 101 + WINDOW_SECS));
        assert!(!is_recent(0, 100));
    }
}
