//! macOS 启动提权：非 root 时通过系统授权以管理员身份重启自身。
//!
//! 官方客户端把特权操作放在 LaunchDaemon 中，GUI 仍以用户身份运行；
//! 本项目当前由单一进程完成建卡与路由，因此启动时申请一次管理员权限。
//! 取消授权时继续以普通用户启动，自检会明确提示需要 root。

#[cfg(target_os = "macos")]
pub fn ensure_admin() {
    use std::env;
    use std::process::Command;

    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    if env::var_os("TOPSAP_ELEVATED").is_some() || env::var_os("TOPSAP_NO_ELEVATE").is_some() {
        return;
    }

    let Ok(exe) = env::current_exe() else {
        return;
    };
    let home = env::var("HOME").unwrap_or_default();
    let user = env::var("USER").unwrap_or_default();
    let command = format!(
        "(sleep 1; HOME={} USER={} LOGNAME={} TOPSAP_ELEVATED=1 \
         PATH=/usr/bin:/bin:/usr/sbin:/sbin nohup {}) \
         >/tmp/topsap-elevated.log 2>&1 &",
        shell_quote(&home),
        shell_quote(&user),
        shell_quote(&user),
        shell_quote(exe.to_string_lossy().as_ref()),
    );
    let script = format!(
        "do shell script {} with administrator privileges",
        apple_script_quote(&command)
    );

    let launched = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(&script)
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    if launched {
        std::process::exit(0);
    }
}

#[cfg(target_os = "macos")]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(target_os = "macos")]
fn apple_script_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn shell_参数中的单引号被转义() {
        assert_eq!(shell_quote("a'b"), "'a'\"'\"'b'");
    }

    #[test]
    fn applescript_字符串中的引号被转义() {
        assert_eq!(apple_script_quote("a\"b"), "\"a\\\"b\"");
    }
}
