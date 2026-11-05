//! 版本检查：读取发布目录中的纯文本版本号，与当前版本比较。

use std::time::Duration;

use serde::Serialize;

use crate::auth::http::HttpClient;

const VERSION_HOST: &str = "pan.sunline.cn";
const VERSION_PATH: &str = "/index.php?share/fileDownload&user=123&sid=I5NWa27M";
pub const DOWNLOAD_URL: &str =
    "https://pan.sunline.cn/index.php?share/folder&user=123&sid=ra6BDZXP";
const CHECK_TIMEOUT: Duration = Duration::from_secs(6);

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

/// 查询发布目录中的版本号；成功时返回是否有更新，失败时返回原因
pub async fn check() -> Result<Option<UpdateInfo>, String> {
    let mut client = HttpClient::new(VERSION_HOST, 443, CHECK_TIMEOUT);
    let response = client
        .request("GET", VERSION_PATH, None, &[])
        .await
        .map_err(|err| err.to_string())?;
    if response.status != 200 {
        return Err(format!("HTTP {}", response.status));
    }

    let latest =
        parse_version(&response.text()).ok_or_else(|| "版本文件内容无法识别".to_string())?;
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
}
