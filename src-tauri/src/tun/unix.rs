//! Unix TUN 设备：通过 tun crate 创建三层接口，使用 poll 等待可读。
//!
//! 设备 fd 统一设为非阻塞，避免读线程卡在 read 上无法响应取消。
//! macOS 默认通过特权 helper 创建 utun 并把文件描述符传回本进程；
//! 仅当 helper 不可用且本进程已是 root 时才回退到本地创建。

#[cfg(target_os = "macos")]
use std::os::fd::IntoRawFd;
use std::os::fd::{AsRawFd, RawFd};
use std::sync::atomic::{AtomicU64, Ordering};

use tun::{AbstractDevice, Device};

use super::TunError;

pub struct TunDevice {
    _device: Device,
    fd: RawFd,
    name: String,
    packets_in: AtomicU64,
    packets_out: AtomicU64,
    dropped_out: AtomicU64,
}

unsafe impl Send for TunDevice {}
unsafe impl Sync for TunDevice {}

/// 以当前可执行文件生成 sudo 启动命令；仅 Linux 使用
#[cfg(target_os = "linux")]
pub(crate) fn sudo_command() -> String {
    std::env::current_exe()
        .map(|path| format!("sudo {}", path.display()))
        .unwrap_or_else(|_| "sudo /usr/bin/topsap-client".to_string())
}

/// 按 errno 区分权限不足与网卡占用，返回可操作的提示
fn tun_create_error(err: &tun::Error, name: &str) -> String {
    #[cfg(target_os = "macos")]
    let _ = name;
    #[cfg(target_os = "linux")]
    if let tun::Error::Io(io) = err {
        match io.raw_os_error() {
            Some(libc::EPERM) | Some(libc::EACCES) => {
                return format!(
                    "创建 TUN 设备失败：当前用户没有创建 TUN 接口的权限；Linux 版需要以 root 运行，请在终端执行 {} 启动应用",
                    sudo_command()
                );
            }
            Some(libc::EBUSY) => {
                return format!(
                    "创建 TUN 设备失败：网卡 {name} 已被占用，可能是上次自检或连接未退出；请退出所有 TopSAP 实例，或执行 sudo ip link del {name} 后重试"
                );
            }
            _ => {}
        }
    }
    #[cfg(target_os = "linux")]
    {
        format!("创建 TUN 设备失败：{err}（请检查 /dev/net/tun 是否可用以及运行权限）")
    }
    #[cfg(target_os = "macos")]
    {
        format!("创建 TUN 设备失败：{err}（请检查特权 helper 或 root 权限）")
    }
}

impl TunDevice {
    pub fn open(dll_path: &std::path::Path, name: &str) -> Result<Self, TunError> {
        Self::open_with(dll_path, name, None, None)
    }

    /// 连接时使用：macOS helper 会在创建 utun 时一并写入虚拟 IP/掩码
    pub fn open_for(
        dll_path: &std::path::Path,
        name: &str,
        vip: &str,
        mask: &str,
    ) -> Result<Self, TunError> {
        Self::open_with(dll_path, name, Some(vip), Some(mask))
    }

    fn open_with(
        dll_path: &std::path::Path,
        name: &str,
        vip: Option<&str>,
        mask: Option<&str>,
    ) -> Result<Self, TunError> {
        #[cfg(target_os = "macos")]
        {
            if let Some((fd, actual)) =
                crate::privileged::create_tun(vip, mask).map_err(TunError::Adapter)?
            {
                return Self::from_fd(fd, actual);
            }
            if unsafe { libc::geteuid() } != 0 {
                return Err(TunError::Adapter(
                    "特权 helper 未运行，请重新启动应用以安装或修复 helper".to_string(),
                ));
            }
        }
        Self::open_local(dll_path, name, vip, mask)
    }

    /// 由特权 helper 调用：真正创建 utun，不经过 helper 客户端
    pub fn open_local(
        _dll_path: &std::path::Path,
        name: &str,
        vip: Option<&str>,
        mask: Option<&str>,
    ) -> Result<Self, TunError> {
        #[cfg(not(target_os = "macos"))]
        let _ = (vip, mask);
        let mut config = tun::Configuration::default();
        #[cfg(target_os = "macos")]
        {
            // macOS 的 utun 名称由内核分配，不能使用 `topsap0`
            config.mtu(1400).up();
            // 路由由 NetConfig 账本统一注入与清理，避免 tun crate 留下无法追踪的静态路由
            config.platform_config(|platform| {
                platform.enable_routing(false);
            });
            if let (Some(vip), Some(mask)) = (vip, mask) {
                // macOS 点对点隧道必须有对端地址，官方客户端使用 VIP-1；不能把 VIP 同时当本机地址和对端
                let peer =
                    crate::auth::model::virtual_peer(vip, mask).unwrap_or_else(|| vip.to_string());
                if let (Ok(vip), Ok(peer), Ok(mask)) = (
                    vip.parse::<std::net::IpAddr>(),
                    peer.parse::<std::net::IpAddr>(),
                    mask.parse::<std::net::IpAddr>(),
                ) {
                    config.address(vip).destination(peer).netmask(mask);
                }
            }
        }
        #[cfg(target_os = "linux")]
        {
            config.name(name).mtu(1400).up();
            config.platform_config(|platform| {
                platform.ensure_root_privileges(true);
            });
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            config.name(name).mtu(1400).up();
        }

        let device =
            tun::create(&config).map_err(|err| TunError::Adapter(tun_create_error(&err, name)))?;
        device
            .set_nonblock()
            .map_err(|err| TunError::Adapter(format!("配置 TUN 非阻塞模式失败：{err}")))?;
        let fd = device.as_raw_fd();
        let actual = device.tun_name().unwrap_or_else(|_| name.to_string());

        Ok(Self {
            _device: device,
            fd,
            name: actual,
            packets_in: AtomicU64::new(0),
            packets_out: AtomicU64::new(0),
            dropped_out: AtomicU64::new(0),
        })
    }

    /// 接管 helper 传来的 utun 文件描述符
    #[cfg(target_os = "macos")]
    pub fn from_fd(fd: RawFd, name: String) -> Result<Self, TunError> {
        let mut config = tun::Configuration::default();
        config.raw_fd(fd).close_fd_on_drop(true).mtu(1400);
        let device = tun::create(&config)
            .map_err(|err| TunError::Adapter(format!("接管 TUN 失败：{err}")))?;
        device
            .set_nonblock()
            .map_err(|err| TunError::Adapter(format!("配置 TUN 非阻塞模式失败：{err}")))?;
        Ok(Self {
            _device: device,
            fd,
            name,
            packets_in: AtomicU64::new(0),
            packets_out: AtomicU64::new(0),
            dropped_out: AtomicU64::new(0),
        })
    }

    /// helper 发送前调用：交出 fd 与真实接口名，避免 Drop 关闭 fd
    #[cfg(target_os = "macos")]
    pub fn into_raw_fd(self) -> (RawFd, String) {
        let Self { _device, name, .. } = self;
        (_device.into_raw_fd(), name)
    }

    pub fn interface_name(&self) -> &str {
        &self.name
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
        // macOS utun 在 IP 包前固定带 4 字节地址族头
        let mut buf = vec![0u8; 65_539];
        let read = unsafe { libc::read(self.fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
        if read <= 0 {
            return None;
        }
        buf.truncate(read as usize);
        #[cfg(target_os = "macos")]
        {
            if buf.len() <= 4 {
                return None;
            }
            buf.drain(..4);
        }
        self.packets_in.fetch_add(1, Ordering::Relaxed);
        Some(buf)
    }

    pub fn send_packet(&self, packet: &[u8]) -> bool {
        if packet.is_empty() {
            return false;
        }
        #[cfg(target_os = "macos")]
        let framed = {
            let mut framed = Vec::with_capacity(packet.len() + 4);
            // AF_INET（大端）头；utun 要求写包时带同样的 4 字节头
            framed.extend_from_slice(&[0, 0, 0, 2]);
            framed.extend_from_slice(packet);
            framed
        };
        #[cfg(target_os = "macos")]
        let data = framed.as_slice();
        #[cfg(not(target_os = "macos"))]
        let data = packet;
        let written =
            unsafe { libc::write(self.fd, data.as_ptr() as *const libc::c_void, data.len()) };
        if written == data.len() as isize {
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
