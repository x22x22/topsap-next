//! 系统命令执行与输出解码。
//!
//! Windows 控制台输出使用系统代码页（简体中文为 GBK），直接按 UTF-8 读会得到乱码。
//! 这里先尝试 UTF-8，失败再按 GBK 解码。

use std::process::Stdio;

#[derive(Debug, Clone)]
pub struct CmdOutput {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
    pub code: Option<i32>,
}

impl CmdOutput {
    pub fn text(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }
}

/// 执行命令，不经过 shell，避免注入；窗口隐藏。
pub async fn run(program: &str, args: &[&str]) -> CmdOutput {
    let mut std_cmd = std::process::Command::new(program);
    std_cmd
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW：避免命令行执行时闪出黑框
        std_cmd.creation_flags(0x0800_0000);
    }

    match tokio::process::Command::from(std_cmd).output().await {
        Ok(output) => CmdOutput {
            ok: output.status.success(),
            stdout: decode(&output.stdout),
            stderr: decode(&output.stderr),
            code: output.status.code(),
        },
        Err(err) => CmdOutput {
            ok: false,
            stdout: String::new(),
            stderr: format!("无法执行 {program}：{err}"),
            code: None,
        },
    }
}

/// 执行 PowerShell 单行脚本
pub async fn powershell(script: &str) -> CmdOutput {
    run(
        "powershell",
        &[
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ],
    )
    .await
}

pub fn decode(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => {
            let (text, _, _) = encoding_rs::GBK.decode(bytes);
            text.into_owned()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_原样解码() {
        assert_eq!(decode("正在 Ping".as_bytes()), "正在 Ping");
    }

    #[test]
    fn gbk_字节可正确解码() {
        // “正在 Ping” 的 GBK 编码
        let gbk = [0xD5, 0xFD, 0xD4, 0xDA, 0x20, 0x50, 0x69, 0x6E, 0x67];
        assert_eq!(decode(&gbk), "正在 Ping");
    }

    #[test]
    fn 空字节返回空串() {
        assert_eq!(decode(&[]), "");
    }
}
