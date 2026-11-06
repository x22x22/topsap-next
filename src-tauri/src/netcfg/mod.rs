//! 系统网络配置：按平台分发到 Windows 与 Unix 实现。
//!
//! 公共类型放在本模块，平台实现各自维护网卡地址、路由与 DNS 的差异；
//! hosts 模块仅用于旧版残留检测与清理。

use crate::auth::RouteEntry;

pub mod hosts;
pub mod shell;

#[cfg(windows)]
pub mod routes;

#[cfg(any(target_os = "macos", test))]
mod macos_text;

#[cfg(any(target_os = "macos", test))]
pub(crate) mod macos_networksetup;

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

/// 与本次连接目标网段完全相同、但属于其他接口的路由
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteConflict {
    pub dest: String,
    pub mask: String,
    pub prefix: u8,
    pub gateway: String,
    pub ifname: String,
    pub kind: RouteConflictKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteConflictKind {
    Tunnel,
    Physical,
    Loopback,
    Other,
}

impl RouteConflictKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tunnel => "tunnel",
            Self::Physical => "physical",
            Self::Loopback => "loopback",
            Self::Other => "other",
        }
    }
}

/// 读取当前路由表并与待注入网段做精确前缀比对；非 macOS 平台返回空列表。
pub async fn route_conflicts(list: &[RouteEntry]) -> Vec<RouteConflict> {
    #[cfg(target_os = "macos")]
    {
        unix::route_conflicts(list).await
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = list;
        Vec::new()
    }
}

/// 带 IPv4 地址的 utun 接口及关联路由数量
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UtunInfo {
    pub name: String,
    pub address: String,
    pub peer: String,
    pub mtu: u32,
    pub route_count: usize,
}

/// utun 清理结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UtunCleanup {
    pub routes_removed: usize,
    pub still_present: bool,
    pub message: String,
}

/// 列出带 IPv4 地址的 utun 接口；非 macOS 平台返回空列表。
pub async fn utun_interfaces() -> Vec<UtunInfo> {
    #[cfg(target_os = "macos")]
    {
        unix::utun_interfaces().await
    }
    #[cfg(not(target_os = "macos"))]
    {
        Vec::new()
    }
}

/// 关闭指定 utun 并删除其关联路由；仅 macOS 支持。
pub async fn cleanup_utun(name: &str) -> Result<UtunCleanup, String> {
    #[cfg(target_os = "macos")]
    {
        unix::cleanup_utun(name).await
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = name;
        Err("当前平台不支持清理 utun 残留".to_string())
    }
}

/// 判断名称是否为 utun 接口名，避免清理命令被用于其他网卡
pub fn is_utun_name(name: &str) -> bool {
    let Some(suffix) = name.strip_prefix("utun") else {
        return false;
    };
    !suffix.is_empty() && suffix.chars().all(|ch| ch.is_ascii_digit())
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
