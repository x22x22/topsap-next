//! 更新包下载：流式写盘、.part 中转与进度推送；不校验哈希。

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use tauri::ipc::Channel;

use super::{UpdateProgress, PROGRESS_STEP_BYTES, USER_AGENT};

/// 流式下载更新包到临时目录；完成后返回正式文件路径
pub async fn download(
    url: &str,
    dir: &Path,
    on_progress: &Channel<UpdateProgress>,
) -> Result<PathBuf, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(30))
        .user_agent(USER_AGENT)
        .build()
        .map_err(|err| format!("创建下载客户端失败：{err}"))?;
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|err| format!("连接下载源失败：{err}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "下载更新包失败：HTTP {}",
            response.status().as_u16()
        ));
    }
    let total = response.content_length().unwrap_or(0);
    let name = if cfg!(windows) {
        "update.zip"
    } else {
        "update.dmg"
    };
    let dest = dir.join(name);
    let part = dir.join(format!("{name}.part"));

    let mut file = std::fs::File::create(&part).map_err(|err| format!("写入更新包失败：{err}"))?;
    let mut downloaded: u64 = 0;
    let mut last_emit: u64 = 0;
    let _ = on_progress.send(UpdateProgress {
        stage: "downloading".to_string(),
        downloaded,
        total,
    });

    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                file.write_all(&chunk)
                    .map_err(|err| format!("写入更新包失败：{err}"))?;
                downloaded += chunk.len() as u64;
                if downloaded - last_emit >= PROGRESS_STEP_BYTES {
                    last_emit = downloaded;
                    let _ = on_progress.send(UpdateProgress {
                        stage: "downloading".to_string(),
                        downloaded,
                        total,
                    });
                }
            }
            Ok(None) => break,
            Err(err) => {
                let _ = std::fs::remove_file(&part);
                return Err(format!("下载更新包中断：{err}"));
            }
        }
    }
    file.flush()
        .map_err(|err| format!("写入更新包失败：{err}"))?;

    if total > 0 && downloaded != total {
        let _ = std::fs::remove_file(&part);
        return Err("下载不完整，请重试".to_string());
    }
    if downloaded == 0 {
        let _ = std::fs::remove_file(&part);
        return Err("下载到的文件为空".to_string());
    }

    if cfg!(windows) {
        let mut header = [0u8; 4];
        std::fs::File::open(&part)
            .and_then(|mut file| file.read_exact(&mut header))
            .map_err(|err| format!("读取更新包失败：{err}"))?;
        if &header != b"PK\x03\x04" {
            let _ = std::fs::remove_file(&part);
            return Err("下载到的文件不是有效的 ZIP 更新包".to_string());
        }
    }

    std::fs::rename(&part, &dest).map_err(|err| format!("更新包落盘失败：{err}"))?;
    let _ = on_progress.send(UpdateProgress {
        stage: "downloading".to_string(),
        downloaded,
        total,
    });
    Ok(dest)
}
