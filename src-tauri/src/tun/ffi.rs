//! wintun.dll 的 FFI 绑定。
//!
//! 选择 Wintun 的原因：微软签名的驱动、用户态 DLL 自带驱动文件，
//! `WintunCreateAdapter` 会自动安装，不需要单独分发 .sys。
//! 代价是必须管理员权限运行。
//!
//! 注意：wintun 不导出 `WintunGetLastError`，错误码走 Win32 `GetLastError`。
//! 另外 `libloading` 在解析符号时就会失败，因此逐个绑定并报告具体缺失项。

use std::ffi::c_void;
use std::path::Path;

use libloading::Library;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Guid {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

/// 固定 GUID，保证多次运行复用同一块虚拟网卡
pub const ADAPTER_GUID: Guid = Guid {
    data1: 0x7a4d_1c62,
    data2: 0x3b8f,
    data3: 0x4e21,
    data4: [0x9c, 0x5a, 0x11, 0x7e, 0x42, 0x88, 0x01, 0xd3],
};

pub type CreateAdapterFn =
    unsafe extern "system" fn(*const u16, *const u16, *const Guid) -> *mut c_void;
pub type OpenAdapterFn = unsafe extern "system" fn(*const u16) -> *mut c_void;
pub type CloseAdapterFn = unsafe extern "system" fn(*mut c_void);
pub type StartSessionFn = unsafe extern "system" fn(*mut c_void, u32) -> *mut c_void;
pub type EndSessionFn = unsafe extern "system" fn(*mut c_void);
pub type GetReadWaitEventFn = unsafe extern "system" fn(*mut c_void) -> *mut c_void;
pub type ReceivePacketFn = unsafe extern "system" fn(*mut c_void, *mut u32) -> *mut u8;
pub type ReleaseReceivePacketFn = unsafe extern "system" fn(*mut c_void, *mut u8);
pub type AllocateSendPacketFn = unsafe extern "system" fn(*mut c_void, u32) -> *mut u8;
pub type SendPacketFn = unsafe extern "system" fn(*mut c_void, *mut u8);

extern "system" {
    fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
}

const WAIT_OBJECT_0: u32 = 0;

/// 已解析的函数表。`_lib` 必须活到所有句柄释放之后。
pub struct WintunApi {
    _lib: Library,
    pub create_adapter: CreateAdapterFn,
    pub open_adapter: OpenAdapterFn,
    pub close_adapter: CloseAdapterFn,
    pub start_session: StartSessionFn,
    pub end_session: EndSessionFn,
    pub get_read_wait_event: GetReadWaitEventFn,
    pub receive_packet: ReceivePacketFn,
    pub release_receive_packet: ReleaseReceivePacketFn,
    pub allocate_send_packet: AllocateSendPacketFn,
    pub send_packet: SendPacketFn,
}

impl WintunApi {
    pub fn load(path: &Path) -> Result<Self, super::TunError> {
        if !path.exists() {
            return Err(super::TunError::DllMissing(path.display().to_string()));
        }

        let lib = unsafe { Library::new(path) }
            .map_err(|e| super::TunError::DllLoad(format!("{}：{e}", path.display())))?;

        unsafe {
            Ok(Self {
                create_adapter: sym(&lib, "WintunCreateAdapter")?,
                open_adapter: sym(&lib, "WintunOpenAdapter")?,
                close_adapter: sym(&lib, "WintunCloseAdapter")?,
                start_session: sym(&lib, "WintunStartSession")?,
                end_session: sym(&lib, "WintunEndSession")?,
                get_read_wait_event: sym(&lib, "WintunGetReadWaitEvent")?,
                receive_packet: sym(&lib, "WintunReceivePacket")?,
                release_receive_packet: sym(&lib, "WintunReleaseReceivePacket")?,
                allocate_send_packet: sym(&lib, "WintunAllocateSendPacket")?,
                send_packet: sym(&lib, "WintunSendPacket")?,
                _lib: lib,
            })
        }
    }
}

unsafe fn sym<T: Copy>(lib: &Library, name: &str) -> Result<T, super::TunError> {
    let c_name = format!("{name}\0");
    let symbol = lib
        .get::<T>(c_name.as_bytes())
        .map_err(|e| super::TunError::SymbolMissing {
            name: name.to_string(),
            reason: e.to_string(),
        })?;
    Ok(*symbol)
}

/// 阻塞等待网卡可读；返回 true 表示有数据待取。
///
/// # Safety
/// `event` 必须是 `WintunGetReadWaitEvent` 返回的有效事件句柄，且会话仍存活。
pub unsafe fn wait_readable(event: *mut c_void, timeout_ms: u32) -> bool {
    if event.is_null() {
        return false;
    }
    WaitForSingleObject(event, timeout_ms) == WAIT_OBJECT_0
}

/// UTF-8 -> 以 NUL 结尾的 UTF-16
pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 取最近一次 Win32 错误码
pub fn last_error_code() -> u32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 宽字符串以_nul_结尾() {
        let w = wide("TopSAP");
        assert_eq!(w.last(), Some(&0));
        assert_eq!(w.len(), 7);
    }

    #[test]
    fn guid_布局为_16_字节() {
        assert_eq!(std::mem::size_of::<Guid>(), 16);
    }
}
