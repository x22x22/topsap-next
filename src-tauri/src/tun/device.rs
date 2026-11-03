//! 虚拟网卡设备：创建/复用适配器、启动会话、收发 IP 包。

use std::ffi::c_void;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use super::ffi::{self, WintunApi, ADAPTER_GUID};
use super::TunError;

const RING_CAPACITY: u32 = 0x40_0000; // 4 MB
const TUNNEL_TYPE: &str = "TopSAP";

pub struct TunDevice {
    api: Arc<WintunApi>,
    adapter: *mut c_void,
    session: *mut c_void,
    wait_event: *mut c_void,
    packets_in: AtomicU64,
    packets_out: AtomicU64,
    dropped_out: AtomicU64,
}

// Wintun 的会话句柄支持多线程并发读写，且本结构内部不做共享可变状态访问
unsafe impl Send for TunDevice {}
unsafe impl Sync for TunDevice {}

impl TunDevice {
    /// 创建或复用同名适配器并启动会话。
    pub fn open(dll_path: &Path, adapter_name: &str) -> Result<Self, TunError> {
        let api = Arc::new(WintunApi::load(dll_path)?);

        let name = ffi::wide(adapter_name);
        let mut adapter = unsafe { (api.open_adapter)(name.as_ptr()) };
        if adapter.is_null() {
            let tunnel_type = ffi::wide(TUNNEL_TYPE);
            adapter =
                unsafe { (api.create_adapter)(name.as_ptr(), tunnel_type.as_ptr(), &ADAPTER_GUID) };
        }
        if adapter.is_null() {
            let code = ffi::last_error_code();
            return Err(TunError::Adapter(describe_win32(code, "创建虚拟网卡失败")));
        }

        let session = unsafe { (api.start_session)(adapter, RING_CAPACITY) };
        if session.is_null() {
            let code = ffi::last_error_code();
            unsafe { (api.close_adapter)(adapter) };
            return Err(TunError::Session(describe_win32(code, "启动网卡会话失败")));
        }

        let wait_event = unsafe { (api.get_read_wait_event)(session) };

        Ok(Self {
            api,
            adapter,
            session,
            wait_event,
            packets_in: AtomicU64::new(0),
            packets_out: AtomicU64::new(0),
            dropped_out: AtomicU64::new(0),
        })
    }

    /// 阻塞等待可读事件，返回 false 表示超时或事件句柄不可用
    pub fn wait_readable(&self, timeout_ms: u32) -> bool {
        // SAFETY: wait_event 来自本次会话的 GetReadWaitEvent，且会话在 close 前保持有效
        unsafe { ffi::wait_readable(self.wait_event, timeout_ms) }
    }

    /// 取一个报文；没有数据时返回 None
    pub fn recv_packet(&self) -> Option<Vec<u8>> {
        if self.session.is_null() {
            return None;
        }
        let mut size: u32 = 0;
        let ptr = unsafe { (self.api.receive_packet)(self.session, &mut size) };
        if ptr.is_null() {
            return None;
        }

        let packet = if size == 0 {
            Vec::new()
        } else {
            // 指针只在 Release 前有效，立刻拷贝
            unsafe { std::slice::from_raw_parts(ptr, size as usize).to_vec() }
        };
        unsafe { (self.api.release_receive_packet)(self.session, ptr) };

        if !packet.is_empty() {
            self.packets_in.fetch_add(1, Ordering::Relaxed);
            Some(packet)
        } else {
            None
        }
    }

    /// 把一个 IP 包写进网卡（注入系统网络栈）
    pub fn send_packet(&self, packet: &[u8]) -> bool {
        if self.session.is_null() || packet.is_empty() {
            return false;
        }
        let ptr = unsafe { (self.api.allocate_send_packet)(self.session, packet.len() as u32) };
        if ptr.is_null() {
            self.dropped_out.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        unsafe {
            std::ptr::copy_nonoverlapping(packet.as_ptr(), ptr, packet.len());
            (self.api.send_packet)(self.session, ptr);
        }
        self.packets_out.fetch_add(1, Ordering::Relaxed);
        true
    }

    pub fn open_for(
        dll_path: &Path,
        adapter_name: &str,
        _vip: &str,
        _mask: &str,
    ) -> Result<Self, TunError> {
        Self::open(dll_path, adapter_name)
    }

    pub fn interface_name(&self) -> &str {
        crate::tun::ADAPTER_NAME
    }

    pub fn packets_in(&self) -> u64 {
        self.packets_in.load(Ordering::Relaxed)
    }

    pub fn packets_out(&self) -> u64 {
        self.packets_out.load(Ordering::Relaxed)
    }

    pub fn dropped_out(&self) -> u64 {
        self.dropped_out.load(Ordering::Relaxed)
    }

    /// 结束会话并关闭适配器（不删除网卡，下次可复用）
    pub fn close(&mut self) {
        unsafe {
            if !self.session.is_null() {
                (self.api.end_session)(self.session);
                self.session = std::ptr::null_mut();
            }
            if !self.adapter.is_null() {
                (self.api.close_adapter)(self.adapter);
                self.adapter = std::ptr::null_mut();
            }
        }
        self.wait_event = std::ptr::null_mut();
    }
}

impl Drop for TunDevice {
    fn drop(&mut self) {
        self.close();
    }
}

fn describe_win32(code: u32, context: &str) -> String {
    let hint = match code {
        5 => "拒绝访问，需要管理员权限运行",
        31 => "无法连接到设备，驱动可能未加载",
        87 => "参数错误",
        1077 => "驱动尚未安装，请重新安装客户端",
        1168 => "找不到设备元素",
        _ => "未知 Win32 错误",
    };
    format!("{context}：{hint}（Win32 {code}）")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn win32_错误带中文提示与错误码() {
        let msg = describe_win32(5, "创建虚拟网卡失败");
        assert!(msg.contains("管理员权限"));
        assert!(msg.contains("5"));
    }

    #[test]
    fn 未知错误码也保留原始值() {
        let msg = describe_win32(9999, "操作失败");
        assert!(msg.contains("9999"));
    }
}
