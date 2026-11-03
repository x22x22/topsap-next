//! 虚拟网卡层：Windows 使用 Wintun FFI，Unix 使用系统 TUN。

#[cfg(windows)]
pub mod device;
#[cfg(windows)]
pub mod ffi;
pub mod selftest;
#[cfg(unix)]
mod unix;

#[cfg(windows)]
pub use device::TunDevice;
#[cfg(windows)]
pub use ffi::WintunApi;
#[cfg(unix)]
pub use unix::TunDevice;

/// 虚拟网卡显示名称
#[cfg(windows)]
pub const ADAPTER_NAME: &str = "TopSAP";
#[cfg(unix)]
pub const ADAPTER_NAME: &str = "topsap0";

#[derive(Debug, thiserror::Error)]
pub enum TunError {
    #[error("找不到 wintun.dll：{0}")]
    DllMissing(String),
    #[error("加载 wintun.dll 失败：{0}")]
    DllLoad(String),
    #[error("wintun.dll 缺少导出函数 {name}：{reason}")]
    SymbolMissing { name: String, reason: String },
    #[error("{0}")]
    Adapter(String),
    #[error("{0}")]
    Session(String),
}

/// 按候选顺序定位 wintun.dll（仅 Windows 使用）
#[cfg(windows)]
pub fn resolve_dll(candidates: &[std::path::PathBuf]) -> Option<std::path::PathBuf> {
    candidates.iter().find(|path| path.exists()).cloned()
}
