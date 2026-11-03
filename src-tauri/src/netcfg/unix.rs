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
        prefix: u8,
        preexisting: bool,
    },
    Bypass {
        ip: String,
    },
}

pub struct NetConfig {
    ifname: String,
    ledger: Vec<LedgerEntry>,
}

impl NetConfig {
    pub fn new(alias: impl Into<String>) -> Self {
        Self {
            ifname: alias.into(),
            ledger: Vec::new(),
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
        let prefix = crate::auth::model::mask_to_prefix(mask)
            .ok_or_else(|| NetError::Interface(format!("掩码「{mask}」不是合法的连续掩码")))?;
        let cidr = format!("{vip}/{prefix}");

        #[cfg(target_os = "linux")]
        {
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
        }

        tokio::time::sleep(Duration::from_millis(300)).await;
        let actual = self.interface_ipv4().await;
        if !actual.iter().any(|addr| addr == vip) {
            return Err(NetError::Interface(format!(
                "配置网卡地址失败（当前：{}）；请确认以 root 或 sudo 运行",
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
        if self.has_route(gateway_ip, 32).await {
            return true;
        }
        let Some(gateway) = self.default_gateway().await else {
            return false;
        };

        #[cfg(target_os = "linux")]
        let out = shell::run(
            "ip",
            &["route", "add", &format!("{gateway_ip}/32"), "via", &gateway],
        )
        .await;
        #[cfg(target_os = "macos")]
        let out = shell::run("route", &["-n", "add", "-host", gateway_ip, &gateway]).await;

        if out.ok || self.has_route(gateway_ip, 32).await {
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
            let out = shell::run(
                "route",
                &["-n", "add", "-net", &target, "-interface", &self.ifname],
            )
            .await;

            if out.ok && self.has_route(&route.dest, route.prefix).await {
                self.ledger.push(LedgerEntry::Route {
                    dest: route.dest.clone(),
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

    pub async fn set_dns(&mut self, _dns1: &str) -> bool {
        // Unix 不修改全局 DNS：内网域名仅解析保留，系统解析器保持原样
        false
    }

    pub async fn cleanup(&mut self) {
        for entry in self.ledger.drain(..).rev() {
            match entry {
                LedgerEntry::Route {
                    dest,
                    prefix,
                    preexisting,
                } => {
                    if preexisting {
                        continue;
                    }
                    let target = format!("{dest}/{prefix}");
                    #[cfg(target_os = "linux")]
                    let _ = shell::run("ip", &["route", "del", &target, "dev", &self.ifname]).await;
                    #[cfg(target_os = "macos")]
                    let _ = shell::run("route", &["-n", "delete", "-net", &target]).await;
                }
                LedgerEntry::Bypass { ip } => {
                    #[cfg(target_os = "linux")]
                    let _ = shell::run("ip", &["route", "del", &format!("{ip}/32")]).await;
                    #[cfg(target_os = "macos")]
                    let _ = shell::run("route", &["-n", "delete", "-host", &ip]).await;
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
        let target = format!("{dest}/{prefix}");
        #[cfg(target_os = "linux")]
        {
            let out = shell::run("ip", &["route", "show", &target]).await;
            out.ok && !out.stdout.trim().is_empty()
        }
        #[cfg(target_os = "macos")]
        {
            let out = shell::run("route", &["-n", "get", &target]).await;
            out.ok
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
}
