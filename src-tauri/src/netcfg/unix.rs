//! Unix（Linux / macOS）网络配置实现。
//!
//! 通过系统命令配置 TUN 地址与路由；需要以 root 或具备 CAP_NET_ADMIN 运行。
//! hosts 仅用于旧版残留检测与清理。

use std::time::Duration;

use crate::auth::RouteEntry;

use super::{hosts, shell, NetError, RouteReport};

#[derive(Debug, Clone)]
enum LedgerEntry {
    Route {
        dest: String,
        mask: String,
        prefix: u8,
        preexisting: bool,
    },
    Bypass {
        ip: String,
    },
    #[cfg(target_os = "macos")]
    Dns {
        service: String,
        original: Vec<String>,
    },
}

pub struct NetConfig {
    ifname: String,
    ledger: Vec<LedgerEntry>,
    #[cfg(target_os = "macos")]
    vip: Option<String>,
}

impl NetConfig {
    pub fn new(alias: impl Into<String>) -> Self {
        Self {
            ifname: alias.into(),
            ledger: Vec::new(),
            #[cfg(target_os = "macos")]
            vip: None,
        }
    }

    pub fn if_index(&self) -> Option<u32> {
        None
    }

    pub fn route_count(&self) -> usize {
        self.ledger
            .iter()
            .filter(|entry| matches!(entry, LedgerEntry::Route { .. }))
            .count()
    }

    pub async fn configure_interface(&mut self, vip: &str, mask: &str) -> Result<u32, NetError> {
        #[cfg(target_os = "linux")]
        {
            let prefix = crate::auth::model::mask_to_prefix(mask)
                .ok_or_else(|| NetError::Interface(format!("掩码「{mask}」不是合法的连续掩码")))?;
            let cidr = format!("{vip}/{prefix}");
            let _ = shell::run("ip", &["link", "set", "dev", &self.ifname, "up"]).await;
            let _ = shell::run("ip", &["addr", "add", &cidr, "dev", &self.ifname]).await;
            let _ = shell::run("ip", &["link", "set", "dev", &self.ifname, "mtu", "1400"]).await;
        }

        #[cfg(target_os = "macos")]
        {
            let _ = shell::run(
                "ifconfig",
                &[&self.ifname, "inet", vip, vip, "netmask", mask, "up"],
            )
            .await;
            let _ = shell::run("ifconfig", &[&self.ifname, "mtu", "1400"]).await;
            self.vip = Some(vip.to_string());
        }

        tokio::time::sleep(Duration::from_millis(300)).await;
        let actual = self.interface_ipv4().await;
        if !actual.iter().any(|addr| addr == vip) {
            #[cfg(target_os = "linux")]
            let hint = "请以 root 运行（sudo /usr/bin/topsap-client）";
            #[cfg(target_os = "macos")]
            let hint = "请确认特权 helper 已安装，或以 root/sudo 运行";
            return Err(NetError::Interface(format!(
                "配置网卡地址失败（当前：{}）；{hint}",
                if actual.is_empty() {
                    "无地址".to_string()
                } else {
                    actual.join(",")
                }
            )));
        }
        Ok(0)
    }

    pub async fn bypass_gateway(&mut self, gateway_ip: &str) -> bool {
        let Some(gateway) = self.default_gateway().await else {
            return false;
        };
        if self.route_ok(gateway_ip, 32, Some(&gateway)).await {
            return true;
        }

        #[cfg(target_os = "linux")]
        let out = shell::run(
            "ip",
            &["route", "add", &format!("{gateway_ip}/32"), "via", &gateway],
        )
        .await;
        #[cfg(target_os = "macos")]
        let out = shell::run("route", &["-n", "add", "-host", gateway_ip, &gateway]).await;

        if out.ok || self.route_ok(gateway_ip, 32, Some(&gateway)).await {
            self.ledger.push(LedgerEntry::Bypass {
                ip: gateway_ip.to_string(),
            });
            true
        } else {
            false
        }
    }

    pub async fn add_routes(&mut self, list: &[RouteEntry]) -> RouteReport {
        let mut ok = 0usize;
        let mut failed = Vec::new();

        for route in list {
            if self.has_route(&route.dest, route.prefix).await {
                self.ledger.push(LedgerEntry::Route {
                    dest: route.dest.clone(),
                    mask: route.mask.clone(),
                    prefix: route.prefix,
                    preexisting: true,
                });
                ok += 1;
                continue;
            }

            let target = format!("{}/{}", route.dest, route.prefix);

            #[cfg(target_os = "linux")]
            let out = shell::run("ip", &["route", "add", &target, "dev", &self.ifname]).await;
            #[cfg(target_os = "macos")]
            let out = match self.vip.as_deref() {
                Some(vip) => {
                    shell::run(
                        "route",
                        &[
                            "-n",
                            "add",
                            "-net",
                            &route.dest,
                            "-netmask",
                            &route.mask,
                            vip,
                        ],
                    )
                    .await
                }
                None => {
                    shell::run(
                        "route",
                        &[
                            "-n",
                            "add",
                            "-net",
                            &route.dest,
                            "-netmask",
                            &route.mask,
                            "-interface",
                            &self.ifname,
                        ],
                    )
                    .await
                }
            };

            if out.ok && self.has_route(&route.dest, route.prefix).await {
                self.ledger.push(LedgerEntry::Route {
                    dest: route.dest.clone(),
                    mask: route.mask.clone(),
                    prefix: route.prefix,
                    preexisting: false,
                });
                ok += 1;
            } else {
                failed.push(target);
            }
        }

        RouteReport {
            total: list.len(),
            ok,
            failed,
        }
    }

    pub async fn set_dns(&mut self, dns1: &str, dns2: &str) -> bool {
        #[cfg(target_os = "macos")]
        {
            let Some(service) = self.active_service().await else {
                return false;
            };
            let original = self.read_dns(&service).await;
            let mut servers: Vec<String> = [dns1, dns2]
                .into_iter()
                .filter(|value| !value.is_empty() && *value != "0.0.0.0")
                .map(str::to_string)
                .collect();
            if servers.is_empty() {
                return false;
            }
            servers.extend(original.iter().cloned());
            let mut args: Vec<String> = vec!["setdnsservers".to_string(), service.clone()];
            args.extend(servers);
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            let out = shell::run("networksetup", &refs).await;
            if out.ok {
                self.ledger.push(LedgerEntry::Dns { service, original });
                true
            } else {
                false
            }
        }
        #[cfg(target_os = "linux")]
        {
            let _ = (dns1, dns2);
            // Linux 不修改全局 DNS：内网域名仅解析保留，系统解析器保持原样
            false
        }
    }

    /// 找到当前默认路由对应的网络服务（Wi-Fi / Ethernet 等）
    #[cfg(target_os = "macos")]
    async fn active_service(&self) -> Option<String> {
        let default_if = self.default_interface().await;
        let out = shell::run("networksetup", &["-listnetworkserviceorder"]).await;
        if out.ok {
            if let Some(ifname) = default_if.as_deref() {
                if let Some(service) = parse_network_service_order(&out.stdout, ifname) {
                    return Some(service);
                }
            }
        }
        for candidate in ["Wi-Fi", "Ethernet"] {
            let out = shell::run("networksetup", &["-getinfo", candidate]).await;
            if out.ok {
                return Some(candidate.to_string());
            }
        }
        None
    }

    #[cfg(target_os = "macos")]
    async fn default_interface(&self) -> Option<String> {
        let out = shell::run("route", &["-n", "get", "default"]).await;
        out.stdout.lines().find_map(|line| {
            line.trim()
                .strip_prefix("interface:")
                .map(|value| value.trim().to_string())
        })
    }

    /// 读取网络服务当前的 DNS；自动获取时返回空列表
    #[cfg(target_os = "macos")]
    async fn read_dns(&self, service: &str) -> Vec<String> {
        let out = shell::run("networksetup", &["-getdnsservers", service]).await;
        if !out.ok {
            return Vec::new();
        }
        parse_dns_servers(&out.stdout)
    }

    pub async fn cleanup(&mut self) {
        for entry in self.ledger.drain(..).rev() {
            match entry {
                LedgerEntry::Route {
                    dest,
                    mask,
                    prefix,
                    preexisting,
                } => {
                    if preexisting {
                        continue;
                    }
                    #[cfg(target_os = "linux")]
                    {
                        let _ = &mask;
                        let target = format!("{dest}/{prefix}");
                        let _ =
                            shell::run("ip", &["route", "del", &target, "dev", &self.ifname]).await;
                    }
                    #[cfg(target_os = "macos")]
                    {
                        let _ = prefix;
                        let _ = shell::run(
                            "route",
                            &[
                                "-n",
                                "delete",
                                "-net",
                                &dest,
                                "-netmask",
                                &mask,
                                "-interface",
                                &self.ifname,
                            ],
                        )
                        .await;
                    }
                }
                LedgerEntry::Bypass { ip } => {
                    #[cfg(target_os = "linux")]
                    let _ = shell::run("ip", &["route", "del", &format!("{ip}/32")]).await;
                    #[cfg(target_os = "macos")]
                    let _ = shell::run("route", &["-n", "delete", "-host", &ip]).await;
                }
                #[cfg(target_os = "macos")]
                LedgerEntry::Dns { service, original } => {
                    let mut args: Vec<&str> = vec!["setdnsservers", service.as_str()];
                    if original.is_empty() {
                        args.push("Empty");
                    } else {
                        args.extend(original.iter().map(String::as_str));
                    }
                    let _ = shell::run("networksetup", &args).await;
                }
            }
        }
    }

    async fn interface_ipv4(&self) -> Vec<String> {
        #[cfg(target_os = "linux")]
        let out = shell::run("ip", &["-4", "addr", "show", "dev", &self.ifname]).await;
        #[cfg(target_os = "macos")]
        let out = shell::run("ifconfig", &[&self.ifname]).await;

        parse_interface_addresses(&out.stdout)
    }

    async fn has_route(&self, dest: &str, prefix: u8) -> bool {
        #[cfg(target_os = "linux")]
        {
            let target = format!("{dest}/{prefix}");
            let out = shell::run("ip", &["route", "show", &target]).await;
            out.ok && !out.stdout.trim().is_empty()
        }
        #[cfg(target_os = "macos")]
        {
            let _ = prefix;
            let out = shell::run("route", &["-n", "get", dest]).await;
            let expected = format!("interface: {}", self.ifname);
            out.ok
                && out
                    .stdout
                    .lines()
                    .any(|line| line.trim() == expected.as_str())
        }
    }

    #[cfg(target_os = "macos")]
    async fn has_route_via(&self, dest: &str, gateway: &str) -> bool {
        let out = shell::run("route", &["-n", "get", dest]).await;
        let expected = format!("gateway: {gateway}");
        out.ok
            && out
                .stdout
                .lines()
                .any(|line| line.trim() == expected.as_str())
    }

    async fn route_ok(&self, dest: &str, prefix: u8, gateway: Option<&str>) -> bool {
        #[cfg(target_os = "linux")]
        {
            let _ = gateway;
            self.has_route(dest, prefix).await
        }
        #[cfg(target_os = "macos")]
        {
            match gateway {
                Some(gateway) => self.has_route_via(dest, gateway).await,
                None => self.has_route(dest, prefix).await,
            }
        }
    }

    async fn default_gateway(&self) -> Option<String> {
        #[cfg(target_os = "linux")]
        {
            let out = shell::run("ip", &["route", "show", "default"]).await;
            out.stdout
                .split_whitespace()
                .skip_while(|token| *token != "via")
                .nth(1)
                .map(|value| value.to_string())
        }
        #[cfg(target_os = "macos")]
        {
            let out = shell::run("route", &["-n", "get", "default"]).await;
            out.stdout.lines().find_map(|line| {
                line.trim()
                    .strip_prefix("gateway:")
                    .map(|value| value.trim().to_string())
            })
        }
    }
}

/// 解析 `networksetup -listnetworkserviceorder`，按设备名找到网络服务
#[cfg(target_os = "macos")]
fn parse_network_service_order(text: &str, ifname: &str) -> Option<String> {
    let mut pending: Option<String> = None;
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix('(') else {
            continue;
        };
        let Some(end) = rest.find(')') else {
            continue;
        };
        let inner = &rest[..end];
        if !inner.is_empty() && inner.chars().all(|ch| ch.is_ascii_digit()) {
            pending = Some(rest[end + 1..].trim().to_string());
            continue;
        }
        if let Some(device) = line.split("Device:").nth(1) {
            let device = device.trim().trim_end_matches(')').trim();
            if device == ifname {
                if let Some(service) = pending.take() {
                    return Some(service);
                }
            } else {
                pending = None;
            }
        }
    }
    None
}

/// 解析 `networksetup -getdnsservers` 输出；自动获取时没有 IP 行
#[cfg(target_os = "macos")]
fn parse_dns_servers(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| line.parse::<std::net::IpAddr>().is_ok())
        .map(str::to_string)
        .collect()
}

/// 解析 Linux `ip addr` 或 macOS `ifconfig` 输出中的 IPv4 地址。
pub fn parse_interface_addresses(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("inet ") {
            continue;
        }
        if let Some(ip) = first_ipv4(trimmed) {
            if !out.contains(&ip) {
                out.push(ip);
            }
        }
    }
    out
}

fn first_ipv4(text: &str) -> Option<String> {
    for token in text.split(|c: char| !(c.is_ascii_digit() || c == '.')) {
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() == 4 && parts.iter().all(|part| part.parse::<u8>().is_ok()) {
            return Some(token.to_string());
        }
    }
    None
}

pub async fn is_elevated() -> bool {
    #[cfg(target_os = "macos")]
    {
        if crate::privileged::available() {
            return true;
        }
    }
    unsafe { libc::geteuid() == 0 }
}

pub async fn has_hosts_residue() -> bool {
    match tokio::fs::read(hosts::HOSTS_PATH).await {
        Ok(bytes) => hosts::has_block(&shell::decode(&bytes)),
        Err(_) => false,
    }
}

pub async fn purge_hosts_residue() -> Result<bool, NetError> {
    let bytes = tokio::fs::read(hosts::HOSTS_PATH)
        .await
        .map_err(|e| NetError::Hosts(format!("读取 hosts 失败：{e}")))?;
    let content = shell::decode(&bytes);
    if !hosts::has_block(&content) {
        return Ok(false);
    }
    let next = hosts::strip_block(&content);
    tokio::fs::write(hosts::HOSTS_PATH, next.as_bytes())
        .await
        .map_err(|e| NetError::Hosts(format!("写入 hosts 失败：{e}")))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 解析_linux_ip_addr_输出() {
        let text = "2: topsap0: <POINTOPOINT,UP> mtu 1400\n    inet 10.8.0.10/24 scope global topsap0\n       valid_lft forever\n";
        assert_eq!(parse_interface_addresses(text), vec!["10.8.0.10"]);
    }

    #[test]
    fn 解析_macos_ifconfig_输出() {
        let text = "utun0: flags=8051<UP,POINTOPOINT,RUNNING,MULTICAST> mtu 1400\n\tinet 10.8.0.10 --> 10.8.0.10 netmask 0xffffff00\n";
        assert_eq!(parse_interface_addresses(text), vec!["10.8.0.10"]);
    }

    #[test]
    fn 忽略_ipv6_与无关行() {
        let text =
            "inet6 fe80::1%topsap0 prefixlen 64\n    inet 192.0.2.10/24 scope global topsap0\n";
        assert_eq!(parse_interface_addresses(text), vec!["192.0.2.10"]);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn 解析网络服务顺序() {
        let text = "An asterisk (*) denotes that a network service is disabled.\n\
                    (1) Wi-Fi\n\
                    (Hardware Port: Wi-Fi, Device: en0)\n\
                    (2) Ethernet\n\
                    (Hardware Port: Ethernet, Device: en1)\n";
        assert_eq!(
            parse_network_service_order(text, "en0").as_deref(),
            Some("Wi-Fi")
        );
        assert_eq!(
            parse_network_service_order(text, "en1").as_deref(),
            Some("Ethernet")
        );
        assert_eq!(parse_network_service_order(text, "en9"), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn 解析DNS列表() {
        assert_eq!(
            parse_dns_servers(
                "8.8.8.8\n2001:4860:4860::8888\nThere aren't any DNS Servers set on Wi-Fi.\n"
            ),
            vec!["8.8.8.8", "2001:4860:4860::8888"]
        );
        assert!(parse_dns_servers("There aren't any DNS Servers set on Wi-Fi.\n").is_empty());
    }
}
