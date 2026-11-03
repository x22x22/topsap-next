//! macOS 特权 helper：以 root 运行，提供 TUN 创建与 ifconfig/route 白名单命令。
//!
//! 由 LaunchDaemon 启动，只接受安装时指定 uid 的本地连接；不执行任意命令。

use std::ffi::CString;
use std::io::{self, BufRead, BufReader, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::tun::TunDevice;

const SOCKET_DIR: &str = "/var/run/topsap-next";
const HELPER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Deserialize)]
struct Request {
    op: String,
    #[serde(default)]
    program: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    vip: Option<String>,
    #[serde(default)]
    mask: Option<String>,
}

#[derive(Debug, Serialize)]
struct Response {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stdout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stderr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl Response {
    fn ok() -> Self {
        Self {
            ok: true,
            version: Some(HELPER_VERSION),
            name: None,
            stdout: None,
            stderr: None,
            code: None,
            error: None,
        }
    }

    fn error(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            version: None,
            name: None,
            stdout: None,
            stderr: None,
            code: None,
            error: Some(message.into()),
        }
    }
}

pub fn is_helper_mode() -> bool {
    std::env::args().skip(1).any(|arg| arg == "--helper")
}

pub fn run() {
    let uid = parse_uid();
    if unsafe { libc::geteuid() } != 0 {
        eprintln!("TopSAP helper must run as root");
        std::process::exit(1);
    }

    let dir = Path::new(SOCKET_DIR);
    if let Err(err) = std::fs::create_dir_all(dir) {
        eprintln!("创建 helper 目录失败：{err}");
        std::process::exit(1);
    }
    let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755));

    let path = dir.join(format!("helper-{uid}.sock"));
    let _ = std::fs::remove_file(&path);
    let listener = match UnixListener::bind(&path) {
        Ok(listener) => listener,
        Err(err) => {
            eprintln!("绑定 helper socket 失败：{err}");
            std::process::exit(1);
        }
    };
    if let Err(err) = chown(&path, uid) {
        eprintln!("设置 helper socket 属主失败：{err}");
    }
    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                std::thread::spawn(move || handle(stream, uid));
            }
            Err(err) => eprintln!("helper 接受连接失败：{err}"),
        }
    }
}

fn parse_uid() -> u32 {
    let args: Vec<String> = std::env::args().collect();
    let mut index = 1;
    while index < args.len() {
        if args[index] == "--uid" {
            return args
                .get(index + 1)
                .and_then(|value| value.parse().ok())
                .unwrap_or(0);
        }
        index += 1;
    }
    0
}

fn chown(path: &Path, uid: u32) -> io::Result<()> {
    let path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "路径包含 NUL"))?;
    let result = unsafe { libc::chown(path.as_ptr(), uid, 0) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

fn handle(mut stream: UnixStream, allowed_uid: u32) {
    if !peer_allowed(&stream, allowed_uid) {
        return;
    }

    let Ok(clone) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(clone);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }
    let request: Request = match serde_json::from_str(line.trim()) {
        Ok(request) => request,
        Err(_) => {
            let _ = write_json(&mut stream, &Response::error("请求格式错误"));
            return;
        }
    };

    match request.op.as_str() {
        "ping" => {
            let _ = write_json(&mut stream, &Response::ok());
        }
        "run" => {
            let _ = write_json(&mut stream, &run_command(&request));
        }
        "create_tun" => create_tun(&mut stream, &request),
        _ => {
            let _ = write_json(&mut stream, &Response::error("未知操作"));
        }
    }
}

fn peer_allowed(stream: &UnixStream, allowed_uid: u32) -> bool {
    let mut uid: libc::uid_t = 0;
    let mut gid: libc::gid_t = 0;
    let result = unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) };
    result == 0 && uid == allowed_uid
}

fn run_command(request: &Request) -> Response {
    let program = match request.program.as_str() {
        "ifconfig" => "/sbin/ifconfig",
        "route" => "/sbin/route",
        _ => return Response::error("不允许的命令"),
    };
    match Command::new(program).args(&request.args).output() {
        Ok(output) => Response {
            ok: output.status.success(),
            version: None,
            name: None,
            stdout: Some(limit_output(&output.stdout)),
            stderr: Some(limit_output(&output.stderr)),
            code: output.status.code(),
            error: None,
        },
        Err(err) => Response::error(format!("执行 {program} 失败：{err}")),
    }
}

fn limit_output(bytes: &[u8]) -> String {
    const LIMIT: usize = 64 * 1024;
    let slice = if bytes.len() > LIMIT {
        &bytes[..LIMIT]
    } else {
        bytes
    };
    String::from_utf8_lossy(slice).into_owned()
}

fn create_tun(stream: &mut UnixStream, request: &Request) {
    let device = match TunDevice::open_local(
        Path::new(""),
        "",
        request.vip.as_deref(),
        request.mask.as_deref(),
    ) {
        Ok(device) => device,
        Err(err) => {
            let _ = write_json(stream, &Response::error(err.to_string()));
            return;
        }
    };
    let (fd, name) = device.into_raw_fd();
    let response = Response {
        ok: true,
        version: None,
        name: Some(name),
        stdout: None,
        stderr: None,
        code: None,
        error: None,
    };
    if write_json(stream, &response).is_err() {
        unsafe { libc::close(fd) };
        return;
    }
    if let Err(err) = send_fd(stream.as_raw_fd(), fd) {
        eprintln!("发送 TUN fd 失败：{err}");
    }
    unsafe { libc::close(fd) };
}

fn write_json(stream: &mut UnixStream, response: &Response) -> io::Result<()> {
    let mut body = serde_json::to_vec(response).map_err(io::Error::other)?;
    body.push(b'\n');
    stream.write_all(&body)?;
    stream.flush()
}

fn send_fd(socket: RawFd, fd: RawFd) -> io::Result<()> {
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

    unsafe {
        let header = libc::CMSG_FIRSTHDR(&message);
        if header.is_null() {
            return Err(io::Error::other("无法构造 SCM_RIGHTS"));
        }
        (*header).cmsg_level = libc::SOL_SOCKET;
        (*header).cmsg_type = libc::SCM_RIGHTS;
        (*header).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<RawFd>() as u32) as _;
        std::ptr::copy_nonoverlapping(
            &fd as *const RawFd as *const u8,
            libc::CMSG_DATA(header),
            std::mem::size_of::<RawFd>(),
        );
        if libc::sendmsg(socket, &message, 0) < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 只允许白名单命令() {
        let ok = run_command(&Request {
            op: "run".to_string(),
            program: "ifconfig".to_string(),
            args: vec![],
            vip: None,
            mask: None,
        });
        // 测试机没有 ifconfig 时只要求不抛 panic；错误路径也必须是受控错误
        assert!(ok.ok || ok.error.is_some() || ok.stderr.is_some());

        let denied = run_command(&Request {
            op: "run".to_string(),
            program: "sh".to_string(),
            args: vec!["-c".to_string(), "id".to_string()],
            vip: None,
            mask: None,
        });
        assert!(!denied.ok);
        assert_eq!(denied.error.as_deref(), Some("不允许的命令"));
    }
}
