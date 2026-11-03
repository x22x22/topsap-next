//! Unix TUN 设备：通过 tun crate 创建三层接口，使用 poll 等待可读。

use std::os::fd::{AsRawFd, RawFd};
use std::sync::atomic::{AtomicU64, Ordering};

use tun::Device;

use super::TunError;

pub struct TunDevice {
    _device: Device,
    fd: RawFd,
    packets_in: AtomicU64,
    packets_out: AtomicU64,
    dropped_out: AtomicU64,
}

unsafe impl Send for TunDevice {}
unsafe impl Sync for TunDevice {}

impl TunDevice {
    pub fn open(_dll_path: &std::path::Path, name: &str) -> Result<Self, TunError> {
        let mut config = tun::Configuration::default();
        config.name(name).mtu(1400).up();

        #[cfg(target_os = "linux")]
        config.platform_config(|platform| {
            platform.ensure_root_privileges(true);
        });

        let device = tun::create(&config).map_err(|err| {
            TunError::Adapter(format!(
                "创建 TUN 设备失败：{err}（请以 root 或 sudo 运行）"
            ))
        })?;
        let fd = device.as_raw_fd();

        Ok(Self {
            _device: device,
            fd,
            packets_in: AtomicU64::new(0),
            packets_out: AtomicU64::new(0),
            dropped_out: AtomicU64::new(0),
        })
    }

    pub fn wait_readable(&self, timeout_ms: u32) -> bool {
        let mut pollfd = libc::pollfd {
            fd: self.fd,
            events: libc::POLLIN,
            revents: 0,
        };
        unsafe { libc::poll(&mut pollfd, 1, timeout_ms as i32) > 0 }
    }

    pub fn recv_packet(&self) -> Option<Vec<u8>> {
        let mut buf = vec![0u8; 65_535];
        let read = unsafe { libc::read(self.fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
        if read <= 0 {
            return None;
        }
        buf.truncate(read as usize);
        self.packets_in.fetch_add(1, Ordering::Relaxed);
        Some(buf)
    }

    pub fn send_packet(&self, packet: &[u8]) -> bool {
        if packet.is_empty() {
            return false;
        }
        let written = unsafe {
            libc::write(
                self.fd,
                packet.as_ptr() as *const libc::c_void,
                packet.len(),
            )
        };
        if written == packet.len() as isize {
            self.packets_out.fetch_add(1, Ordering::Relaxed);
            true
        } else {
            self.dropped_out.fetch_add(1, Ordering::Relaxed);
            false
        }
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

    pub fn close(&mut self) {}
}
