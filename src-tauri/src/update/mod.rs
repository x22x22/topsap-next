//! 应用更新：版本检查、下载地址、进度下载与多平台安装编排。

use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::Manager;

pub mod download;
pub mod install;

const VERSION_HOST: &str = "pan.sunline.cn";
const VERSION_PATH: &str = "/index.php?share/fileDownload&user=123&sid=ra6BDZXP&path=%2Fversion";
pub const DOWNLOAD_URL: &str =
    "https://pan.sunline.cn/index.php?share/folder&user=123&sid=ra6BDZXP";
const SHARE_DOWNLOAD_BASE: &str =
    "https://pan.sunline.cn/index.php?share/fileDownload&user=123&sid=ra6BDZXP&path=";
const CHECK_TIMEOUT: Duration = Duration::from_secs(6);
pub(crate) const USER_AGENT: &str = concat!("topsap-next/", env!("CARGO_PKG_VERSION"));
pub(crate) const PROGRESS_STEP_BYTES: u64 = 128 * 1024;

/// 发现更新的版本号与下载入口
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub url: String,
}

/// 检查结果：`checked=false` 表示网络或内容异常，不能据此判断是否有更新
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpdateCheck {
    pub update: Option<UpdateInfo>,
    pub checked: bool,
}

/// 自动更新进度；stage 为 downloading / verifying / disconnecting / installing / restarting
#[derive(Debug, Clone, Serialize)]
pub struct UpdateProgress {
    pub stage: String,
    pub downloaded: u64,
    pub total: u64,
}

/// 查询发布目录中的版本号；使用正常证书校验的 HTTPS
pub async fn check() -> Result<Option<UpdateInfo>, String> {
    let client = reqwest::Client::builder()
        .timeout(CHECK_TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
        .map_err(|err| format!("创建更新检查客户端失败：{err}"))?;
    let url = format!("https://{VERSION_HOST}{VERSION_PATH}");
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|err| format!("检查更新失败：{err}"))?;
    if !response.status().is_success() {
        return Err(format!("检查更新失败：HTTP {}", response.status().as_u16()));
    }
    let text = response
        .text()
        .await
        .map_err(|err| format!("读取版本信息失败：{err}"))?;
    let latest = parse_version(&text).ok_or_else(|| "版本文件内容无法识别".to_string())?;
    let current_text = env!("CARGO_PKG_VERSION");
    let current = parse_version(current_text).ok_or_else(|| "当前版本号无法解析".to_string())?;
    if is_newer(&latest, &current) {
        Ok(Some(UpdateInfo {
            current: current_text.to_string(),
            latest: latest
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join("."),
            url: DOWNLOAD_URL.to_string(),
        }))
    } else {
        Ok(None)
    }
}

/// 供前端使用：把检查失败归一为 `checked=false`，成功时携带可选更新
pub async fn check_result() -> UpdateCheck {
    match check().await {
        Ok(update) => UpdateCheck {
            update,
            checked: true,
        },
        Err(err) => {
            log::debug!("检查更新失败：{err}");
            UpdateCheck {
                update: None,
                checked: false,
            }
        }
    }
}

/// 按平台与架构返回分享目录中的安装包相对路径
pub fn asset_path_for(os: &str, arch: &str, version: &str) -> Result<String, String> {
    let path = match (os, arch) {
        ("windows", "x86_64") => {
            format!("/Windows/topsap-client_{version}_windows_amd64.zip")
        }
        ("windows", "aarch64") => {
            format!("/Windows/topsap-client_{version}_windows_arm64.zip")
        }
        ("macos", "x86_64") => format!("/Mac/TopSAP.Client_{version}_x64.dmg"),
        ("macos", "aarch64") => format!("/Mac/TopSAP.Client_{version}_aarch64.dmg"),
        _ => return Err("当前平台不支持自动更新".to_string()),
    };
    Ok(path)
}

/// 对分享路径做百分号编码，保留点号与下划线
fn encode_path(path: &str) -> String {
    const SET: &percent_encoding::AsciiSet = &percent_encoding::CONTROLS
        .add(b' ')
        .add(b'"')
        .add(b'#')
        .add(b'%')
        .add(b'/')
        .add(b'?')
        .add(b'&')
        .add(b'=')
        .add(b'+');
    percent_encoding::utf8_percent_encode(path, SET).to_string()
}

/// 当前平台的更新包下载地址
pub fn download_url_for(version: &str) -> Result<String, String> {
    let path = asset_path_for(std::env::consts::OS, std::env::consts::ARCH, version)?;
    Ok(format!("{SHARE_DOWNLOAD_BASE}{}", encode_path(&path)))
}

/// 更新临时目录（下载、解压、helper 脚本与旧版本备份）
pub fn temp_update_dir() -> PathBuf {
    std::env::temp_dir().join("topsap-update")
}

/// 启动时清理上一轮更新残留（幂等、尽力而为）
pub fn cleanup_temp() {
    let _ = std::fs::remove_dir_all(temp_update_dir());
    install::cleanup_old_backup();
}

/// 下载、校验结构、断开连接、安装并退出当前进程
pub async fn start_update(
    app: tauri::AppHandle,
    on_progress: Channel<UpdateProgress>,
) -> Result<(), String> {
    let update = check()
        .await?
        .ok_or_else(|| "当前已是最新版本".to_string())?;
    let url = download_url_for(&update.latest)?;
    let dir = temp_update_dir();
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|err| format!("创建更新临时目录失败：{err}"))?;

    let package = download::download(&url, &dir, &on_progress).await?;
    let _ = on_progress.send(UpdateProgress {
        stage: "verifying".to_string(),
        downloaded: 0,
        total: 0,
    });
    let staged = install::prepare(&package, &dir).await?;

    let engine = app.state::<crate::AppState>().engine.clone();
    if engine.needs_cleanup() {
        let _ = on_progress.send(UpdateProgress {
            stage: "disconnecting".to_string(),
            downloaded: 0,
            total: 0,
        });
        engine
            .disconnect()
            .await
            .map_err(|err| format!("更新前断开连接失败：{err}"))?;
        if engine.needs_cleanup() {
            return Err("更新前仍存在未清理的网络配置，已取消更新".to_string());
        }
    }

    let _ = on_progress.send(UpdateProgress {
        stage: "installing".to_string(),
        downloaded: 0,
        total: 0,
    });
    install::apply(&staged, &package).await?;
    let _ = on_progress.send(UpdateProgress {
        stage: "restarting".to_string(),
        downloaded: 0,
        total: 0,
    });

    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(600)).await;
        app.exit(0);
    });
    Ok(())
}

/// 解析 `v1.2.3` 形式的版本号，忽略前后空白与最多四段数字
fn parse_version(text: &str) -> Option<Vec<u64>> {
    let line = text.lines().next()?.trim();
    let line = line.strip_prefix(['v', 'V']).unwrap_or(line);
    if line.is_empty() || !line.chars().all(|ch| ch.is_ascii_digit() || ch == '.') {
        return None;
    }
    let parts: Vec<u64> = line
        .split('.')
        .map(str::parse)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if parts.is_empty() || parts.len() > 4 {
        return None;
    }
    Some(parts)
}

fn is_newer(latest: &[u64], current: &[u64]) -> bool {
    let len = latest.len().max(current.len());
    for index in 0..len {
        let left = latest.get(index).copied().unwrap_or(0);
        let right = current.get(index).copied().unwrap_or(0);
        if left != right {
            return left > right;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 解析版本号_允许前缀与空白() {
        assert_eq!(parse_version("1.2.2"), Some(vec![1, 2, 2]));
        assert_eq!(parse_version(" v1.2.3 \n"), Some(vec![1, 2, 3]));
        assert_eq!(parse_version("1.2.3.4"), Some(vec![1, 2, 3, 4]));
    }

    #[test]
    fn 解析版本号_拒绝非法内容() {
        assert_eq!(parse_version(""), None);
        assert_eq!(parse_version("1.2.x"), None);
        assert_eq!(parse_version("v"), None);
        assert_eq!(parse_version("1..2"), None);
        assert_eq!(parse_version("1.2.3.4.5"), None);
    }

    #[test]
    fn 比较_仅远端更高时为真() {
        assert!(is_newer(&[1, 2, 3], &[1, 2, 2]));
        assert!(is_newer(&[1, 10], &[1, 9]));
        assert!(is_newer(&[2], &[1, 9, 9]));
        assert!(!is_newer(&[1, 2], &[1, 2, 0]));
        assert!(!is_newer(&[1, 2, 2], &[1, 2, 2]));
        assert!(!is_newer(&[1, 2, 1], &[1, 2, 2]));
    }

    #[test]
    fn 安装包路径_与分享目录一致() {
        assert_eq!(
            asset_path_for("windows", "x86_64", "1.4.0").unwrap(),
            "/Windows/topsap-client_1.4.0_windows_amd64.zip"
        );
        assert_eq!(
            asset_path_for("windows", "aarch64", "1.4.0").unwrap(),
            "/Windows/topsap-client_1.4.0_windows_arm64.zip"
        );
        assert_eq!(
            asset_path_for("macos", "x86_64", "1.4.0").unwrap(),
            "/Mac/TopSAP.Client_1.4.0_x64.dmg"
        );
        assert_eq!(
            asset_path_for("macos", "aarch64", "1.4.0").unwrap(),
            "/Mac/TopSAP.Client_1.4.0_aarch64.dmg"
        );
        assert!(asset_path_for("linux", "x86_64", "1.4.0").is_err());
    }

    #[test]
    fn 下载路径_斜杠被编码() {
        assert_eq!(
            encode_path("/Mac/TopSAP.Client_1.4.0_x64.dmg"),
            "%2FMac%2FTopSAP.Client_1.4.0_x64.dmg"
        );
    }
}
