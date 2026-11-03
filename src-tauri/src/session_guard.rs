//! 异常退出后的会话补偿：记录未正常注销的会话，下次启动或连接前补注销。
//!
//! 只保存网关、端口与 session_id，不保存密码与完整 Cookie；文件使用
//! 机器绑定 AES-256-GCM 加密，Unix 下权限收紧到 0600。

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::auth::AuthClient;
use crate::{paths, secret_store};

const FILE_NAME: &str = "session_guard.bin";
const MAX_AGE_SECS: u64 = 24 * 60 * 60;
const PURGE_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingSession {
    gateway: String,
    port: u16,
    session_id: String,
    #[serde(default)]
    user: String,
    created_at: u64,
}

fn file_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(paths::config_dir(app)?.join(FILE_NAME))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(0)
}

fn is_fresh(created_at: u64, now: u64) -> bool {
    created_at > 0 && now.saturating_sub(created_at) <= MAX_AGE_SECS
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) {}

/// 登录成功后记录当前会话，供异常退出后的下次启动补注销
pub fn save(
    app: &AppHandle,
    gateway: &str,
    port: u16,
    session_id: &str,
    user: &str,
) -> Result<(), String> {
    if gateway.trim().is_empty() || session_id.trim().is_empty() {
        return Ok(());
    }

    let pending = PendingSession {
        gateway: gateway.to_string(),
        port,
        session_id: session_id.to_string(),
        user: user.to_string(),
        created_at: now_secs(),
    };
    let payload = serde_json::to_vec(&pending).map_err(|e| format!("序列化会话标记失败：{e}"))?;
    let sealed = secret_store::seal_bytes(&payload)?;
    let path = file_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建会话标记目录失败：{e}"))?;
    }
    std::fs::write(&path, sealed).map_err(|e| format!("写入会话标记失败：{e}"))?;
    restrict_permissions(&path);
    Ok(())
}

/// 读取未注销会话；过期或损坏的标记会被清除
fn load(app: &AppHandle) -> Option<PendingSession> {
    let path = file_path(app).ok()?;
    let bytes = std::fs::read(&path).ok()?;
    let Some(pending) = secret_store::open_bytes(&bytes)
        .and_then(|plaintext| serde_json::from_slice::<PendingSession>(&plaintext).ok())
    else {
        clear(app);
        return None;
    };
    if pending.session_id.trim().is_empty() || !is_fresh(pending.created_at, now_secs()) {
        clear(app);
        return None;
    }
    Some(pending)
}

/// 清除会话标记；正常注销成功后调用
pub fn clear(app: &AppHandle) {
    if let Ok(path) = file_path(app) {
        let _ = std::fs::remove_file(path);
    }
}

/// 尝试注销上次异常退出留下的会话；返回是否真的执行了补注销
pub async fn purge(app: &AppHandle) -> Result<bool, String> {
    static PURGE_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    let lock = PURGE_LOCK.get_or_init(|| tokio::sync::Mutex::new(()));
    let _guard = lock.lock().await;

    let Some(pending) = load(app) else {
        return Ok(false);
    };

    let mut auth = AuthClient::new_with_timeout(
        pending.gateway,
        pending.port,
        String::new(),
        String::new(),
        PURGE_TIMEOUT,
    );
    match auth.logout(&pending.session_id).await {
        Ok(()) => {
            clear(app);
            Ok(true)
        }
        Err(err) => Err(err.user_message()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 会话标记可序列化往返() {
        let pending = PendingSession {
            gateway: "vpn.example.com".to_string(),
            port: 443,
            session_id: "session-1".to_string(),
            user: "demo".to_string(),
            created_at: 1,
        };
        let text = serde_json::to_string(&pending).unwrap();
        let decoded: PendingSession = serde_json::from_str(&text).unwrap();
        assert_eq!(decoded.gateway, pending.gateway);
        assert_eq!(decoded.session_id, pending.session_id);
    }

    #[test]
    fn 超过有效期的标记不再尝试补注销() {
        let now = 10_000_000;
        assert!(is_fresh(now - MAX_AGE_SECS, now));
        assert!(!is_fresh(now - MAX_AGE_SECS - 1, now));
        assert!(!is_fresh(0, now));
    }
}
