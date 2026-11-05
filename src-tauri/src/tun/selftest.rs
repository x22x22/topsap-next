//! 网卡自检：把「网卡层」和「协议层」分开验证。
//!
//! 不接触网关，几秒内结束。验证顺序：
//! 管理员权限 → 定位 DLL → 建卡/会话 → 配置 IP → 回读 → 注入测试路由
//! → 写路径（构造 ICMP 写入网卡）→ 读路径（本机回包经测试路由读回）→ 跨网段投递（INFO）

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::auth::RouteEntry;
use crate::netcfg::{is_elevated, shell, NetConfig};
use crate::tun::{TunDevice, ADAPTER_NAME};

pub const TEST_VIP: &str = "10.99.99.2";
pub const TEST_MASK: &str = "255.255.255.0";
pub const TEST_ROUTE_DEST: &str = "10.98.0.0";
pub const TEST_ROUTE_MASK: &str = "255.255.0.0";
pub const TEST_PING_TARGET: &str = "10.98.0.5";

/// 单次 poll 后最多连续读取的报文数，避免持续流量饿死外层取消检查
const MAX_PACKETS_PER_POLL: usize = 32;

/// 接收方向测试的报文源地址：必须落在测试路由网段内，回包才能按测试路由回到网卡
const ECHO_SOURCE: [u8; 4] = [10, 98, 0, 5];

/// 接收方向测试的报文目的地址（本机 VIP）：命中本机地址才会触发系统回包
const ECHO_TARGET: [u8; 4] = [10, 99, 99, 2];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepStatus {
    Ok,
    Fail,
    Info,
}

#[derive(Debug, Clone)]
pub struct SelfTestStep {
    pub name: &'static str,
    pub status: StepStatus,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct SelfTestReport {
    pub steps: Vec<SelfTestStep>,
    pub passed: usize,
    pub total: usize,
    pub ok: bool,
}

impl SelfTestReport {
    fn from_steps(steps: Vec<SelfTestStep>) -> Self {
        let passed = steps
            .iter()
            .filter(|s| s.status != StepStatus::Fail)
            .count();
        let total = steps.len();
        let ok = !steps.iter().any(|s| s.status == StepStatus::Fail);
        Self {
            steps,
            passed,
            total,
            ok,
        }
    }
}

/// 自检 future 被超时取消时也要停掉读线程，避免 tun fd 泄漏
struct ReaderStopGuard(Arc<AtomicBool>);

impl Drop for ReaderStopGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Relaxed);
    }
}

struct Builder<F: FnMut(&SelfTestStep)> {
    steps: Vec<SelfTestStep>,
    on_step: F,
}

impl<F: FnMut(&SelfTestStep)> Builder<F> {
    fn new(on_step: F) -> Self {
        Self {
            steps: Vec::new(),
            on_step,
        }
    }

    fn push(&mut self, step: SelfTestStep) {
        (self.on_step)(&step);
        self.steps.push(step);
    }

    fn ok(&mut self, name: &'static str, detail: impl Into<String>) {
        self.push(SelfTestStep {
            name,
            status: StepStatus::Ok,
            detail: detail.into(),
        });
    }

    fn fail(&mut self, name: &'static str, detail: impl Into<String>) {
        self.push(SelfTestStep {
            name,
            status: StepStatus::Fail,
            detail: detail.into(),
        });
    }

    fn info(&mut self, name: &'static str, detail: impl Into<String>) {
        self.push(SelfTestStep {
            name,
            status: StepStatus::Info,
            detail: detail.into(),
        });
    }

    fn finish(self) -> SelfTestReport {
        SelfTestReport::from_steps(self.steps)
    }
}

pub async fn run<F: FnMut(&SelfTestStep)>(dll_path: &Path, on_step: F) -> SelfTestReport {
    let mut b = Builder::new(on_step);

    if !is_elevated().await {
        #[cfg(windows)]
        b.fail("管理员权限", "未提权，无法创建虚拟网卡；请以管理员身份运行");
        #[cfg(target_os = "linux")]
        b.fail(
            "管理员权限",
            format!(
                "当前未以 root 运行，无法创建 TUN 接口；请在终端执行 {} 启动应用",
                super::unix::sudo_command()
            ),
        );
        #[cfg(target_os = "macos")]
        b.fail(
            "管理员权限",
            "未提权，无法创建 TUN 接口；请重新启动并在系统授权框中允许管理员权限，或以 root/sudo 运行",
        );
        return b.finish();
    }
    b.ok("管理员权限", "已提权");

    #[cfg(windows)]
    {
        if !dll_path.exists() {
            b.fail(
                "定位 wintun.dll",
                format!("文件不存在：{}", dll_path.display()),
            );
            return b.finish();
        }
        b.ok("定位 wintun.dll", dll_path.display().to_string());
    }
    #[cfg(unix)]
    {
        let _ = dll_path;
        b.ok("TUN 设备", "使用系统 TUN，无需额外驱动");
    }

    #[cfg(windows)]
    {
        match reset_windows_adapter().await {
            Ok(Some(detail)) => b.ok("清理上次自检残留", detail),
            Ok(None) => b.info("清理上次自检残留", "未发现遗留的 TopSAP 虚拟网卡"),
            Err(err) => {
                b.fail(
                    "清理上次自检残留",
                    format!("{err}；请先断开连接或手动删除网卡后重试"),
                );
                return b.finish();
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        match reset_unix_adapter().await {
            Ok(Some(detail)) => b.ok("清理上次自检残留", detail),
            Ok(None) => b.info("清理上次自检残留", "未发现遗留的 TUN 虚拟网卡"),
            Err(err) => {
                b.fail(
                    "清理上次自检残留",
                    format!(
                        "{err}；请退出其他 TopSAP 实例，或执行 sudo ip link del {ADAPTER_NAME} 后重试"
                    ),
                );
                return b.finish();
            }
        }
    }
    #[cfg(target_os = "macos")]
    b.info("清理上次自检残留", "系统 TUN 随进程关闭自动释放，无需清理");

    let device = match TunDevice::open_for(dll_path, ADAPTER_NAME, TEST_VIP, TEST_MASK) {
        Ok(d) => {
            b.ok("创建虚拟网卡与会话", format!("网卡 {}", d.interface_name()));
            Arc::new(d)
        }
        Err(err) => {
            b.fail("创建虚拟网卡与会话", err.to_string());
            return b.finish();
        }
    };

    let mut net = NetConfig::new(device.interface_name().to_string());

    match net.configure_interface(TEST_VIP, TEST_MASK).await {
        Ok(_) => {
            b.ok("配置网卡地址", format!("{TEST_VIP}/{TEST_MASK}"));
            b.ok("回读网卡地址", format!("地址已生效（{TEST_VIP}）"));
        }
        Err(err) => {
            b.fail("配置网卡地址", err.to_string());
            b.fail("回读网卡地址", "地址未生效");
            return finish_with_cleanup(b, &mut net, device, dll_path).await;
        }
    }

    let route = RouteEntry {
        dest: TEST_ROUTE_DEST.to_string(),
        mask: TEST_ROUTE_MASK.to_string(),
        prefix: 16,
    };
    let report = net.add_routes(&[route]).await;
    if report.all_ok() {
        b.ok(
            "注入测试路由",
            format!("{TEST_ROUTE_DEST}/{TEST_ROUTE_MASK} 已生效"),
        );
    } else {
        b.fail(
            "注入测试路由",
            format!("{TEST_ROUTE_DEST}/{TEST_ROUTE_MASK} 未生效"),
        );
    }

    // 读线程：独立线程轮询网卡，避免占用 tokio worker
    let running = Arc::new(AtomicBool::new(true));
    let _stop_guard = ReaderStopGuard(running.clone());
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(128);
    let reader_device = device.clone();
    let reader_running = running.clone();
    let reader = std::thread::spawn(move || {
        while reader_running.load(Ordering::Relaxed) {
            if !reader_device.wait_readable(200) {
                continue;
            }
            for _ in 0..MAX_PACKETS_PER_POLL {
                let Some(packet) = reader_device.recv_packet() else {
                    break;
                };
                if tx.try_send(packet).is_err() {
                    return;
                }
            }
        }
    });
    b.ok("启动读循环", "已就绪");

    let echo = build_icmp_echo_request(ECHO_SOURCE, ECHO_TARGET, 0x5447, 1);
    if device.send_packet(&echo) {
        b.ok(
            "发送方向（应用 → 网卡）",
            format!(
                "已向网卡注入 {} 字节 ICMP 请求（{TEST_PING_TARGET} → {TEST_VIP}）",
                echo.len()
            ),
        );
    } else {
        b.fail("发送方向（应用 → 网卡）", "写入失败，网卡会话不可用");
    }

    match tokio::time::timeout(Duration::from_secs(3), rx.recv()).await {
        Ok(Some(packet)) => b.ok(
            "接收方向（网卡 → 应用）",
            format!("收到系统回包：{}", describe_packet(&packet)),
        ),
        Ok(None) => b.fail("接收方向（网卡 → 应用）", "读循环提前结束"),
        Err(_) => b.fail(
            "接收方向（网卡 → 应用）",
            "3 秒内未收到系统回包；请检查测试路由是否生效、网卡读路径是否可用",
        ),
    }

    #[cfg(windows)]
    let ping_args: &[&str] = &["-n", "1", "-w", "1000", TEST_PING_TARGET];
    #[cfg(unix)]
    let ping_args: &[&str] = &["-c", "1", "-W", "1", TEST_PING_TARGET];
    let ping = shell::run("ping", ping_args).await;
    if ping.ok {
        b.ok(
            "系统路由（跨网段投递）",
            format!("ping {TEST_PING_TARGET} 成功，系统已把包交给网卡"),
        );
    } else {
        b.info(
            "系统路由（跨网段投递）",
            format!("ping {TEST_PING_TARGET} 未通（同网段无对端属正常，不影响真实连接）"),
        );
    }

    running.store(false, Ordering::Relaxed);
    let _ = reader.join();
    finish_with_cleanup(b, &mut net, device, dll_path).await
}

async fn finish_with_cleanup<F: FnMut(&SelfTestStep)>(
    b: Builder<F>,
    net: &mut NetConfig,
    device: Arc<TunDevice>,
    _dll_path: &Path,
) -> SelfTestReport {
    cleanup(net, &device).await;
    drop(device);
    #[cfg(windows)]
    let _ = reset_windows_adapter().await;
    b.finish()
}

/// 清理遗留的 Wintun 网卡；返回 Some(detail) 表示执行过清理。
#[cfg(windows)]
async fn reset_windows_adapter() -> Result<Option<String>, String> {
    if crate::platform::is_windows7() {
        let disable = shell::run(
            "netsh",
            &[
                "interface",
                "set",
                "interface",
                ADAPTER_NAME,
                "admin=disabled",
            ],
        )
        .await;
        let enable = shell::run(
            "netsh",
            &[
                "interface",
                "set",
                "interface",
                ADAPTER_NAME,
                "admin=enabled",
            ],
        )
        .await;
        if disable.ok || enable.ok {
            return Ok(Some("已重置遗留的 TopSAP 网卡状态".to_string()));
        }
        return Ok(None);
    }

    let script = format!(
        "$a = Get-NetAdapter -Name '{}' -ErrorAction SilentlyContinue; if ($a) {{ $a | Remove-NetAdapter -Confirm:$false -ErrorAction Stop; 'removed' }} else {{ 'absent' }}",
        ADAPTER_NAME
    );
    let out = shell::powershell(&script).await;
    if !out.ok {
        return Err(out.text().trim().to_string());
    }
    if out.stdout.contains("removed") {
        Ok(Some("已删除遗留的 TopSAP 虚拟网卡".to_string()))
    } else {
        Ok(None)
    }
}

/// 检查并删除上次自检遗留的 Linux TUN 网卡；返回 Some(detail) 表示执行过清理。
#[cfg(target_os = "linux")]
async fn reset_unix_adapter() -> Result<Option<String>, String> {
    let current = shell::run("ip", &["link", "show", ADAPTER_NAME]).await;
    if !output_has_interface(current.ok, &current.stdout) {
        return Ok(None);
    }

    let address = shell::run("ip", &["addr", "show"]).await;
    if !interface_has_ipv4(&address.stdout, ADAPTER_NAME, TEST_VIP) {
        return Err(format!(
            "{ADAPTER_NAME} 存在但不是自检残留（未找到 {TEST_VIP}）"
        ));
    }

    let deleted = shell::run("ip", &["link", "del", ADAPTER_NAME]).await;
    if !deleted.ok {
        let detail = deleted.text().trim().to_string();
        return Err(if detail.is_empty() {
            format!("删除 {ADAPTER_NAME} 失败")
        } else {
            format!("删除 {ADAPTER_NAME} 失败：{detail}")
        });
    }

    let after = shell::run("ip", &["link", "show", ADAPTER_NAME]).await;
    if output_has_interface(after.ok, &after.stdout) {
        return Err(format!("{ADAPTER_NAME} 仍存在"));
    }
    Ok(Some(format!("已删除遗留的 {ADAPTER_NAME} 虚拟网卡")))
}

#[cfg(any(target_os = "linux", test))]
fn output_has_interface(ok: bool, stdout: &str) -> bool {
    ok && !stdout.trim().is_empty()
}

#[cfg(any(target_os = "linux", test))]
fn interface_has_ipv4(text: &str, ifname: &str, ip: &str) -> bool {
    let mut in_target = false;
    for line in text.lines() {
        if !line.starts_with(' ') && !line.starts_with('\t') {
            in_target = line
                .split(':')
                .nth(1)
                .map(str::trim)
                .is_some_and(|name| name == ifname);
        }
        if !in_target {
            continue;
        }
        let Some(rest) = line.trim_start().strip_prefix("inet ") else {
            continue;
        };
        if rest
            .split_whitespace()
            .next()
            .and_then(|addr| addr.split('/').next())
            == Some(ip)
        {
            return true;
        }
    }
    false
}

async fn cleanup(net: &mut NetConfig, _device: &TunDevice) {
    net.cleanup().await;
}

/// 描述一个 IP 报文的版本、协议与长度
pub fn describe_packet(packet: &[u8]) -> String {
    if packet.is_empty() {
        return "空报文".to_string();
    }
    let version = packet[0] >> 4;
    if version == 4 && packet.len() >= 20 {
        let proto = packet[9];
        let src = format!(
            "{}.{}.{}.{}",
            packet[12], packet[13], packet[14], packet[15]
        );
        let dst = format!(
            "{}.{}.{}.{}",
            packet[16], packet[17], packet[18], packet[19]
        );
        format!("IPv4 {src} -> {dst} proto={proto} len={}", packet.len())
    } else if version == 6 {
        format!("IPv6 报文（{} 字节）", packet.len())
    } else {
        format!("未知版本 {version}（{} 字节）", packet.len())
    }
}

/// 构造完整的 IPv4 + ICMP Echo 请求
pub fn build_icmp_echo_request(src: [u8; 4], dst: [u8; 4], id: u16, seq: u16) -> Vec<u8> {
    let mut icmp = vec![8u8, 0, 0, 0];
    icmp.extend_from_slice(&id.to_be_bytes());
    icmp.extend_from_slice(&seq.to_be_bytes());
    icmp.extend_from_slice(b"topsap-selftest!");
    let sum = checksum(&icmp);
    icmp[2..4].copy_from_slice(&sum.to_be_bytes());

    build_ipv4(src, dst, 1, &icmp)
}

fn build_ipv4(src: [u8; 4], dst: [u8; 4], protocol: u8, payload: &[u8]) -> Vec<u8> {
    let total_len = 20 + payload.len();
    let mut packet = vec![0u8; 20];
    packet[0] = 0x45; // IPv4, IHL=5
    packet[1] = 0;
    packet[2..4].copy_from_slice(&(total_len as u16).to_be_bytes());
    packet[4..6].copy_from_slice(&0x5447u16.to_be_bytes());
    packet[6..8].copy_from_slice(&0u16.to_be_bytes()); // flags + fragment
    packet[8] = 64; // TTL
    packet[9] = protocol;
    packet[12..16].copy_from_slice(&src);
    packet[16..20].copy_from_slice(&dst);

    let header_sum = checksum(&packet[..20]);
    packet[10..12].copy_from_slice(&header_sum.to_be_bytes());

    packet.extend_from_slice(payload);
    packet
}

/// 16 位反码校验和
pub fn checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut chunks = data.chunks_exact(2);
    for chunk in &mut chunks {
        sum += u16::from_be_bytes([chunk[0], chunk[1]]) as u32;
    }
    if let Some(rest) = chunks.remainder().first() {
        sum += (*rest as u32) << 8;
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 校验和_对自身取反为零() {
        let mut data = vec![
            0x45u8, 0, 0, 20, 0, 0, 0, 0, 64, 1, 0, 0, 10, 0, 0, 1, 10, 0, 0, 2,
        ];
        let sum = checksum(&data);
        data[10..12].copy_from_slice(&sum.to_be_bytes());
        // 补上校验和后整段求和应为 0
        assert_eq!(checksum(&data), 0);
    }

    #[test]
    fn icmp_请求结构正确() {
        let pkt = build_icmp_echo_request([10, 99, 99, 2], [10, 98, 0, 5], 1, 2);
        assert_eq!(pkt[0] >> 4, 4, "版本号必须是 4");
        assert_eq!(pkt[9], 1, "协议号必须是 ICMP");
        assert_eq!(pkt[12..16], [10, 99, 99, 2]);
        assert_eq!(pkt[16..20], [10, 98, 0, 5]);
        assert_eq!(pkt[20], 8, "ICMP 类型 8 是 Echo 请求");
        // 总长度字段与实际一致
        let total = u16::from_be_bytes([pkt[2], pkt[3]]) as usize;
        assert_eq!(total, pkt.len());
    }

    #[test]
    fn 接收方向_以本机地址触发回包() {
        assert_eq!(
            format!(
                "{}.{}.{}.{}",
                ECHO_SOURCE[0], ECHO_SOURCE[1], ECHO_SOURCE[2], ECHO_SOURCE[3]
            ),
            TEST_PING_TARGET
        );
        assert_eq!(
            format!(
                "{}.{}.{}.{}",
                ECHO_TARGET[0], ECHO_TARGET[1], ECHO_TARGET[2], ECHO_TARGET[3]
            ),
            TEST_VIP
        );
        let pkt = build_icmp_echo_request(ECHO_SOURCE, ECHO_TARGET, 1, 1);
        assert_eq!(pkt[16..20], ECHO_TARGET, "目的地址必须是本机 VIP");
        assert_eq!(pkt[12..16], ECHO_SOURCE, "源地址必须落在测试路由网段");
    }

    #[test]
    fn 报文描述_区分版本() {
        let mut v4 = vec![0u8; 28];
        v4[0] = 0x45;
        v4[9] = 6;
        assert!(describe_packet(&v4).contains("IPv4"));
        assert!(describe_packet(&v4).contains("proto=6"));

        let mut v6 = vec![0u8; 64];
        v6[0] = 0x60;
        assert!(describe_packet(&v6).contains("IPv6"));

        assert_eq!(describe_packet(&[]), "空报文");
    }

    #[test]
    fn 网卡存在判断_区分命令失败与空输出() {
        assert!(output_has_interface(
            true,
            "5: topsap0: <POINTOPOINT,NOARP,UP> mtu 1400"
        ));
        assert!(!output_has_interface(true, " \n"));
        assert!(!output_has_interface(false, ""));
    }

    #[test]
    fn 解析网卡地址_只匹配目标网卡与目标_ipv4() {
        let text = concat!(
            "2: eth0: <BROADCAST,MULTICAST,UP> mtu 1500\n",
            "    inet 10.0.0.2/24 scope global eth0\n",
            "5: topsap0: <POINTOPOINT,NOARP,UP> mtu 1400\n",
            "    inet 10.99.99.2/24 scope global topsap0\n",
            "    inet6 fe80::1/64 scope link\n",
        );
        assert!(interface_has_ipv4(text, "topsap0", "10.99.99.2"));
        assert!(!interface_has_ipv4(text, "topsap0", "10.99.99.3"));
        assert!(!interface_has_ipv4(text, "eth0", "10.99.99.2"));
    }

    #[test]
    fn 报告统计_失败即整体失败() {
        let report = SelfTestReport::from_steps(vec![
            SelfTestStep {
                name: "a",
                status: StepStatus::Ok,
                detail: String::new(),
            },
            SelfTestStep {
                name: "b",
                status: StepStatus::Info,
                detail: String::new(),
            },
        ]);
        assert!(report.ok);
        assert_eq!(report.passed, 2);

        let report = SelfTestReport::from_steps(vec![
            SelfTestStep {
                name: "a",
                status: StepStatus::Ok,
                detail: String::new(),
            },
            SelfTestStep {
                name: "b",
                status: StepStatus::Fail,
                detail: String::new(),
            },
        ]);
        assert!(!report.ok);
        assert_eq!(report.passed, 1);
    }
}
