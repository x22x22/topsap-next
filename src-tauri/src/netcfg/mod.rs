//! 系统网络配置：按平台分发到 Windows 与 Unix 实现。
//!
//! 公共类型放在本模块，平台实现各自维护网卡地址、路由与 DNS 的差异；
//! hosts 模块仅用于旧版残留检测与清理。

pub mod hosts;
pub mod shell;

#[cfg(windows)]
pub mod routes;

#[cfg(any(target_os = "macos", test))]
mod macos_text;

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
pub use unix::{has_hosts_residue, is_elevated, purge_hosts_residue, NetConfig};
#[cfg(windows)]
pub use windows::{has_hosts_residue, is_elevated, purge_hosts_residue, NetConfig};

#[derive(Debug, thiserror::Error)]
pub enum NetError {
    #[error("{0}")]
    Msg(String),
    #[error("需要管理员权限：{0}")]
    NotElevated(String),
    #[error("虚拟网卡操作失败：{0}")]
    Interface(String),
    #[error("hosts 写入失败：{0}")]
    Hosts(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteReport {
    pub total: usize,
    pub ok: usize,
    pub failed: Vec<String>,
}

impl RouteReport {
    pub fn all_ok(&self) -> bool {
        self.total == 0 || self.ok == self.total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 路由报告_all_ok_语义() {
        assert!(RouteReport {
            total: 0,
            ok: 0,
            failed: vec![]
        }
        .all_ok());
        assert!(RouteReport {
            total: 3,
            ok: 3,
            failed: vec![]
        }
        .all_ok());
        assert!(!RouteReport {
            total: 3,
            ok: 2,
            failed: vec!["a".into()]
        }
        .all_ok());
    }
}
