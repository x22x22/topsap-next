//! macOS 特权 helper 客户端与安装。
//!
//! helper 由 LaunchDaemon 以 root 运行；本模块只负责安装、探活、
//! 请求 TUN fd 与执行 ifconfig/route 白名单命令。

use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use serde_json::{json, Value};

use crate::netcfg::shell::CmdOutput;

const SOCKET_DIR: &str = "/var/run/topsap-next";
const LABEL: &str = "com.topsap.next.helper";
const INSTALL_DIR: &str = "/Library/Application Support/TopSAP-Next";
const INSTALLED_BIN: &str = "/Library/Application Support/TopSAP-Next/topsap-helper";
const PLIST_PATH: &str = "/Library/LaunchDaemons/com.topsap.next.helper.plist";

fn uid() -> u32 {
    unsafe { libc::getuid() }
}

fn socket_path() -> PathBuf {
    PathBuf::from(SOCKET_DIR).join(format!("helper-{}.sock", uid()))
}

/// 启动时确保 helper 已安装且版本一致；用户取消授权时返回错误但不阻止 GUI 启动
pub fn ensure_helper() -> Result<(), String> {
    if ping().as_deref() == Some(env!("CARGO_PKG_VERSION")) {
        return Ok(());
    }
    install_helper()?;
    for _ in 0..50 {
        if ping().as_deref() == Some(env!("CARGO_PKG_VERSION")) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err("特权 helper 未在预期时间内启动".to_string())
}

/// helper 是否可用（自检的提权判断使用）
pub fn available() -> bool {
    ping().is_some()
}

fn ping() -> Option<String> {
    let value = request(json!({ "op": "ping" }))?;
    if !value.get("ok")?.as_bool()? {
        return None;
    }
    value.get("version")?.as_str().map(str::to_string)
}

/// 通过 helper 执行白名单命令；helper 不可用且本进程为 root 时返回 None 以走本地回退
pub fn run(program: &str, args: &[String]) -> Option<CmdOutput> {
    let value = request(json!({
        "op": "run",
        "program": program,
        "args": args,
    }));

    if let Some(value) = value {
        let ok = value.get("ok").and_then(Value::as_bool).unwrap_or(false);
        let stdout = value
            .get("stdout")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let stderr = value
            .get("stderr")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let code = value
            .get("code")
            .and_then(Value::as_i64)
            .map(|value| value as i32);
        if let Some(error) = value.get("error").and_then(Value::as_str) {
            return Some(CmdOutput {
                ok: false,
                stdout,
                stderr: error.to_string(),
                code,
            });
        }
        return Some(CmdOutput {
            ok,
            stdout,
            stderr,
            code,
        });
    }

    if unsafe { libc::geteuid() } == 0 {
        None
    } else {
        Some(CmdOutput {
            ok: false,
            stdout: String::new(),
            stderr: "特权 helper 未运行，请重新启动应用以安装或修复 helper".to_string(),
            code: None,
        })
    }
}

/// 请求 helper 创建 utun 并把 fd 传回；helper 不可用时返回 None
pub fn create_tun(
    vip: Option<&str>,
    mask: Option<&str>,
) -> Result<Option<(RawFd, String)>, String> {
    let mut stream = match UnixStream::connect(socket_path()) {
        Ok(stream) => stream,
        Err(_) => return Ok(None),
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));

    let mut body = serde_json::to_vec(&json!({
        "op": "create_tun",
        "vip": vip,
        "mask": mask,
    }))
    .map_err(|err| format!("序列化 helper 请求失败：{err}"))?;
    body.push(b'\n');
    stream
        .write_all(&body)
        .map_err(|err| format!("发送 helper 请求失败：{err}"))?;

    let line = read_line_raw(&mut stream).map_err(|err| format!("读取 helper 响应失败：{err}"))?;
    let value: Value =
        serde_json::from_str(&line).map_err(|err| format!("解析 helper 响应失败：{err}"))?;
    if !value.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        let error = value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("helper 创建 TUN 失败");
        return Err(error.to_string());
    }
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("utun")
        .to_string();
    let fd = recv_fd(stream.as_raw_fd()).map_err(|err| format!("接收 TUN fd 失败：{err}"))?;
    Ok(Some((fd, name)))
}

fn request(value: Value) -> Option<Value> {
    let mut stream = UnixStream::connect(socket_path()).ok()?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(3)));

    let mut body = serde_json::to_vec(&value).ok()?;
    body.push(b'\n');
    stream.write_all(&body).ok()?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    serde_json::from_str(&line).ok()
}

fn read_line_raw(stream: &mut UnixStream) -> io::Result<String> {
    let mut bytes = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        if stream.read(&mut byte)? == 0 {
            break;
        }
        if byte[0] == b'\n' {
            break;
        }
        bytes.push(byte[0]);
        if bytes.len() > 8192 {
            return Err(io::Error::other("helper 响应过长"));
        }
    }
    String::from_utf8(bytes).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

fn recv_fd(socket: RawFd) -> io::Result<RawFd> {
    let mut byte = [0u8; 1];
    let mut iov = libc::iovec {
        iov_base: byte.as_mut_ptr() as *mut libc::c_void,
        iov_len: 1,
    };
    let space = unsafe { libc::CMSG_SPACE(std::mem::size_of::<RawFd>() as u32) } as usize;
    let mut control = vec![0u8; space];
    let mut message: libc::msghdr = unsafe { std::mem::zeroed() };
    message.msg_iov = &mut iov;
    message.msg_iovlen = 1;
    message.msg_control = control.as_mut_ptr() as *mut libc::c_void;
    message.msg_controllen = control.len() as _;

    let received = unsafe { libc::recvmsg(socket, &mut message, 0) };
    if received <= 0 {
        return Err(io::Error::last_os_error());
    }
    unsafe {
        let header = libc::CMSG_FIRSTHDR(&message);
        if header.is_null() {
            return Err(io::Error::other("helper 未返回 TUN fd"));
        }
        let mut fd: RawFd = -1;
        std::ptr::copy_nonoverlapping(
            libc::CMSG_DATA(header),
            &mut fd as *mut RawFd as *mut u8,
            std::mem::size_of::<RawFd>(),
        );
        if fd < 0 {
            return Err(io::Error::other("TUN fd 无效"));
        }
        Ok(fd)
    }
}

fn install_helper() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|err| format!("无法定位当前程序：{err}"))?;
    let user = uid();
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{LABEL}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{INSTALLED_BIN}</string>
    <string>--helper</string>
    <string>--uid</string>
    <string>{user}</string>
  </array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>StandardOutPath</key><string>/var/log/topsap-next-helper.log</string>
  <key>StandardErrorPath</key><string>/var/log/topsap-next-helper.log</string>
</dict>
</plist>
"#
    );

    let plist_file = std::env::temp_dir().join(format!("topsap-helper-{user}.plist"));
    std::fs::write(&plist_file, plist).map_err(|err| format!("写入 helper plist 失败：{err}"))?;

    let command = format!(
        "set -e; \
         /bin/launchctl bootout system/{label} 2>/dev/null || true; \
         /bin/sleep 1; \
         /usr/bin/install -d -m 0755 -o root -g wheel {install_dir}; \
         /usr/bin/install -m 0755 -o root -g wheel {exe} {installed_bin}; \
         (/usr/bin/xattr -dr com.apple.quarantine {installed_bin} 2>/dev/null || true); \
         /usr/bin/install -m 0644 -o root -g wheel {plist_file} {plist_path}; \
         /bin/launchctl bootstrap system {plist_path} 2>/dev/null || true; \
         /bin/launchctl enable system/{label} 2>/dev/null || true; \
         /bin/launchctl kickstart -k system/{label} 2>/dev/null || true",
        install_dir = shell_quote(INSTALL_DIR),
        exe = shell_quote(exe.to_string_lossy().as_ref()),
        installed_bin = shell_quote(INSTALLED_BIN),
        label = LABEL,
        plist_file = shell_quote(plist_file.to_string_lossy().as_ref()),
        plist_path = shell_quote(PLIST_PATH),
    );

    let script = format!(
        "do shell script {} with administrator privileges with prompt \"{}\"",
        apple_script_quote(&command),
        "TopSAP Client 需要安装一次特权 helper 以创建虚拟网卡并配置路由"
    );
    let status = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(&script)
        .status()
        .map_err(|err| format!("启动系统授权失败：{err}"))?;
    let _ = std::fs::remove_file(&plist_file);
    if !status.success() {
        return Err("未获得管理员权限，特权 helper 未安装".to_string());
    }
    Ok(())
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn apple_script_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}
