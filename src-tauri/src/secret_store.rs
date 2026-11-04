//! 凭据存储。
//!
//! 优先使用系统钥匙串（Windows 凭据管理器 / macOS Keychain / Linux Secret Service）；
//! 不可用时回退为「机器标识派生密钥 + AES-256-GCM」的本地加密文件。
//! 密码只在这两种形态中出现，不写日志、不进事件 payload。
//!
//! keyring 必须启用各平台原生后端：缺少对应 feature 时会退化为进程内 mock，
//! 表现为保存成功但重启后读不到，因此 Cargo.toml 同时启用 Windows / macOS / Linux 后端。

use std::path::PathBuf;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use rand::RngCore;
use sha2::{Digest, Sha256};
use tauri::AppHandle;

const SERVICE: &str = "dev.topsap.next";
const NONCE_LEN: usize = 12;

pub fn save_password(app: &AppHandle, user: &str, password: &str) -> Result<(), String> {
    if let Ok(entry) = keyring::Entry::new(SERVICE, user) {
        if entry.set_password(password).is_ok() {
            return Ok(());
        }
    }
    save_fallback(app, user, password)
}

pub fn load_password(app: &AppHandle, user: &str) -> Option<String> {
    if let Ok(entry) = keyring::Entry::new(SERVICE, user) {
        if let Ok(password) = entry.get_password() {
            return Some(password);
        }
    }
    load_fallback(app, user)
}

pub fn delete_password(app: &AppHandle, user: &str) {
    if let Ok(entry) = keyring::Entry::new(SERVICE, user) {
        let _ = entry.delete_credential();
    }
    if let Ok(path) = fallback_path(app, user) {
        let _ = std::fs::remove_file(path);
    }
}

fn fallback_path(app: &AppHandle, user: &str) -> Result<PathBuf, String> {
    let dir = crate::paths::config_dir(app)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建配置目录失败：{e}"))?;

    let mut hasher = Sha256::new();
    hasher.update(user.as_bytes());
    let tag = hex::encode(&hasher.finalize()[..8]);
    Ok(dir.join(format!("credential-{tag}.bin")))
}

fn machine_key() -> [u8; 32] {
    let uid = machine_uid::get().unwrap_or_else(|_| "unknown-machine".to_string());
    let mut hasher = Sha256::new();
    hasher.update(b"topsap-next-credential-v1");
    hasher.update(uid.as_bytes());
    hasher.finalize().into()
}

/// 使用机器绑定密钥加密任意字节，供凭据与异常退出会话标记共用
pub(crate) fn seal_bytes(plaintext: &[u8]) -> Result<Vec<u8>, String> {
    let cipher =
        Aes256Gcm::new_from_slice(&machine_key()).map_err(|e| format!("初始化加密器失败：{e}"))?;

    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext)
        .map_err(|_| "加密数据失败".to_string())?;

    let mut out = nonce_bytes.to_vec();
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// 解密 `seal_bytes` 的输出；密钥不匹配或数据损坏时返回 None
pub(crate) fn open_bytes(sealed: &[u8]) -> Option<Vec<u8>> {
    if sealed.len() <= NONCE_LEN {
        return None;
    }
    let cipher = Aes256Gcm::new_from_slice(&machine_key()).ok()?;
    let (nonce_bytes, ciphertext) = sealed.split_at(NONCE_LEN);
    cipher
        .decrypt(Nonce::from_slice(nonce_bytes), ciphertext)
        .ok()
}

fn save_fallback(app: &AppHandle, user: &str, password: &str) -> Result<(), String> {
    let path = fallback_path(app, user)?;
    let out = seal_bytes(password.as_bytes())?;
    std::fs::write(&path, out).map_err(|e| format!("写入凭据文件失败：{e}"))
}

fn load_fallback(app: &AppHandle, user: &str) -> Option<String> {
    let path = fallback_path(app, user).ok()?;
    let bytes = std::fs::read(path).ok()?;
    String::from_utf8(open_bytes(&bytes)?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 机器密钥稳定且非全零() {
        let a = machine_key();
        let b = machine_key();
        assert_eq!(a, b);
        assert!(a.iter().any(|b| *b != 0));
    }

    #[test]
    fn 通用字节加解密可往返() {
        let sealed = seal_bytes(b"session-guard").unwrap();
        assert_eq!(open_bytes(&sealed).unwrap(), b"session-guard");
    }

    #[test]
    fn 截断密文无法解密() {
        assert!(open_bytes(&[0u8; 4]).is_none());
    }

    #[test]
    fn 加密后可解密() {
        let key = machine_key();
        let cipher = Aes256Gcm::new_from_slice(&key).unwrap();
        let mut nonce_bytes = [0u8; NONCE_LEN];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ct = cipher.encrypt(nonce, b"Test-Passw0rd".as_slice()).unwrap();
        let pt = cipher
            .decrypt(Nonce::from_slice(&nonce_bytes), ct.as_slice())
            .unwrap();
        assert_eq!(String::from_utf8(pt).unwrap(), "Test-Passw0rd");
    }
}
