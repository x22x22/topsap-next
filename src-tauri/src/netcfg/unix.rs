//! Unix（Linux / macOS）网络配置实现。
//!
//! 通过系统命令配置 TUN 地址与路由；需要以 root 或具备 CAP_NET_ADMIN 运行。
//! hosts 仅用于旧版残留检测与清理。

use std::time::Duration;

use crate::auth::RouteEntry;

#[cfg(target_os = "macos")]
use super::macos_text::{
    find_route_conflicts, ipv4_only, parse_dns_servers, parse_ifconfig_utuns,
    parse_network_service_order, parse_route_table, parse_router_info, parse_scutil_dns,
    route_lookup_matches, route_probe_address, RouteTableEntry,
};
use super::{hosts, shell, NetError, RouteReport};
#[cfg(target_os = "macos")]
use super::{RouteConflict, UtunCleanup, UtunInfo};

/// `networksetup setdnsservers` 的参数上限：helper 只允许「命令 + 服务名 + 8 个地址」
#[cfg(target_os = "macos")]
const MAX_DNS_SERVERS: usize = 8;

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
    notes: Vec<String>,
    #[cfg(target_os = "macos")]
    vip: Option<String>,
    #[cfg(target_os = "macos")]
    peer: Option<String>,
}

impl NetConfig {
    pub fn new(alias: impl Into<String>) -> Self {
        Self {
            ifname: alias.into(),
            ledger: Vec::new(),
            notes: Vec::new(),
            #[cfg(target_os = "macos")]
            vip: None,
            #[cfg(target_os = "macos")]
            peer: None,
        }
    }

    /// 取出本次配置过程中产生的兜底说明，供连接日志逐条记录
    pub fn take_notes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notes)
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
            let peer = crate::auth::model::virtual_peer(vip, mask);
            let destination = peer.as_deref().unwrap_or(vip);
            let _ = shell::run(
                "ifconfig",
                &[
                    &self.ifname,
                    "inet",
                    vip,
                    destination,
                    "netmask",
                    mask,
                    "up",
                ],
            )
            .await;
            let _ = shell::run("ifconfig", &[&self.ifname, "mtu", "1400"]).await;
            self.vip = Some(vip.to_string());
            self.peer = peer;
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

    pub async fn add_routes(&mut self, list: &[RouteEntry], takeover: bool) -> RouteReport {
        let mut ok = 0usize;
        let mut failed = Vec::new();

        #[cfg(target_os = "macos")]
        let conflicts = route_conflicts(list).await;
        #[cfg(not(target_os = "macos"))]
        let _ = takeover;

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

            #[cfg(target_os = "macos")]
            if let Some(conflict) = conflicts
                .iter()
                .find(|item| item.dest == route.dest && item.prefix == route.prefix)
            {
                if conflict.ifname == self.ifname {
                    // 同接口同前缀但回读失败，继续按新路由重写一次
                } else if takeover {
                    if !remove_conflict(conflict).await {
                        failed.push(format!(
                            "{target} [无法删除冲突路由 {} {}]",
                            conflict.ifname, conflict.gateway
                        ));
                        continue;
                    }
                } else {
                    failed.push(format!(
                        "{target} [已被 {}（{}）占用]",
                        conflict.ifname, conflict.gateway
                    ));
                    continue;
                }
            }

            #[cfg(target_os = "linux")]
            let out = shell::run("ip", &["route", "add", &target, "dev", &self.ifname]).await;
            #[cfg(target_os = "macos")]
            let used_interface_fallback = self.peer.is_none();
            #[cfg(target_os = "macos")]
            let gateway = self.peer.as_deref().or(self.vip.as_deref());
            #[cfg(target_os = "macos")]
            let out = match gateway {
                Some(peer) => {
                    shell::run(
                        "route",
                        &[
                            "-n",
                            "add",
                            "-net",
                            &route.dest,
                            "-netmask",
                            &route.mask,
                            peer,
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

            #[cfg(target_os = "macos")]
            let installed = self.has_route(&route.dest, route.prefix).await;
            #[cfg(target_os = "linux")]
            let installed = out.ok && self.has_route(&route.dest, route.prefix).await;

            if installed {
                #[cfg(target_os = "macos")]
                if used_interface_fallback {
                    self.notes.push(format!(
                        "路由注入：{target} 没有对端地址，回退 -interface 写入"
                    ));
                }
                self.ledger.push(LedgerEntry::Route {
                    dest: route.dest.clone(),
                    mask: route.mask.clone(),
                    prefix: route.prefix,
                    preexisting: false,
                });
                ok += 1;
            } else {
                #[cfg(target_os = "macos")]
                {
                    let detail = out.text();
                    let detail = detail.trim();
                    if detail.is_empty() {
                        failed.push(target);
                    } else {
                        let brief: String = detail.chars().take(200).collect();
                        failed.push(format!("{target} [{brief}]"));
                    }
                }
                #[cfg(target_os = "linux")]
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
            let Some((service, servers, configured)) = self.macos_dns_plan(dns1, dns2).await else {
                return false;
            };
            let mut args: Vec<&str> = vec!["setdnsservers", service.as_str()];
            args.extend(servers.iter().map(String::as_str));
            let out = shell::run("networksetup", &args).await;
            if out.ok {
                self.ledger.push(LedgerEntry::Dns {
                    service,
                    original: configured,
                });
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

    /// 组装「VPN DNS + 原 DNS」；原 DNS 为自动获取时从系统解析器或路由器补全
    #[cfg(target_os = "macos")]
    async fn macos_dns_plan(
        &mut self,
        dns1: &str,
        dns2: &str,
    ) -> Option<(String, Vec<String>, Vec<String>)> {
        let service = self.active_service().await?;
        let configured = self.read_dns(&service).await;
        let original = if configured.is_empty() {
            self.system_dns(&service).await
        } else {
            configured.clone()
        };
        let mut servers: Vec<String> = [dns1, dns2]
            .into_iter()
            .filter(|value| !value.is_empty() && *value != "0.0.0.0")
            .map(str::to_string)
            .collect();
        if servers.is_empty() {
            return None;
        }
        for value in ipv4_only(&original) {
            if !servers.contains(&value) {
                servers.push(value);
            }
        }
        servers.truncate(MAX_DNS_SERVERS);
        Some((service, servers, configured))
    }

    /// 读取自动获取的原 DNS；仅 IPv4 会写入网络服务，IPv6 保留在系统原配置中
    #[cfg(target_os = "macos")]
    async fn system_dns(&mut self, service: &str) -> Vec<String> {
        let out = shell::run("scutil", &["--dns"]).await;
        let mut servers = if out.ok {
            parse_scutil_dns(&out.stdout, &self.ifname)
        } else {
            Vec::new()
        };
        if servers.is_empty() {
            let info = shell::run("networksetup", &["-getinfo", service]).await;
            if info.ok {
                if let Some(router) = parse_router_info(&info.stdout) {
                    servers.push(router);
                    self.notes
                        .push("DNS：原 DNS 为自动获取，scutil 无结果，回退 Router".to_string());
                }
            }
        }
        servers
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
                        if macos_route_exists(&self.ifname, &dest, prefix).await {
                            for gateway in [self.peer.as_deref(), self.vip.as_deref()]
                                .into_iter()
                                .flatten()
                            {
                                let _ = shell::run(
                                    "route",
                                    &["-n", "delete", "-net", &dest, "-netmask", &mask, gateway],
                                )
                                .await;
                                if !macos_route_exists(&self.ifname, &dest, prefix).await {
                                    break;
                                }
                            }
                        }
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
            macos_route_exists(&self.ifname, dest, prefix).await
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

/// 读取当前路由表，找出与待注入网段完全相同的其他接口路由
#[cfg(target_os = "macos")]
pub(crate) async fn route_conflicts(list: &[RouteEntry]) -> Vec<RouteConflict> {
    find_route_conflicts(list, &route_table().await)
}

/// 列出带 IPv4 地址的 utun 接口，并统计每个接口关联的路由数量
#[cfg(target_os = "macos")]
pub(crate) async fn utun_interfaces() -> Vec<UtunInfo> {
    let out = shell::run_local("/sbin/ifconfig", &["-a"]).await;
    if !out.ok {
        return Vec::new();
    }
    let parsed = parse_ifconfig_utuns(&out.stdout);
    if parsed.is_empty() {
        return Vec::new();
    }
    let table = route_table().await;
    parsed
        .into_iter()
        .map(|item| {
            let route_count = table
                .iter()
                .filter(|entry| entry.ifname == item.name)
                .count();
            UtunInfo {
                name: item.name,
                address: item.address,
                peer: item.peer,
                mtu: item.mtu,
                route_count,
            }
        })
        .collect()
}

/// 关闭指定 utun 并删除其关联路由；仅允许 utun* 名称
#[cfg(target_os = "macos")]
pub(crate) async fn cleanup_utun(name: &str) -> Result<UtunCleanup, String> {
    if !super::is_utun_name(name) {
        return Err("只允许清理 utun* 虚拟网卡".to_string());
    }
    let present_before = shell::run_local("/sbin/ifconfig", &[name]).await.ok;
    if !present_before {
        return Ok(UtunCleanup {
            routes_removed: 0,
            still_present: false,
            message: format!("{name} 已不存在"),
        });
    }

    let targets: Vec<RouteTableEntry> = route_table()
        .await
        .into_iter()
        .filter(|entry| entry.ifname == name)
        .collect();
    let before = targets.len();

    let down = shell::run("ifconfig", &[name, "down"]).await;
    for entry in &targets {
        let out = shell::run(
            "route",
            &[
                "-n",
                "delete",
                "-net",
                &entry.dest,
                "-netmask",
                &entry.mask,
                "-interface",
                name,
            ],
        )
        .await;
        if !out.ok {
            let _ = shell::run(
                "route",
                &[
                    "-n",
                    "delete",
                    "-net",
                    &entry.dest,
                    "-netmask",
                    &entry.mask,
                    &entry.gateway,
                ],
            )
            .await;
        }
    }

    let remaining = route_table()
        .await
        .iter()
        .filter(|entry| entry.ifname == name)
        .count();
    let present = shell::run_local("/sbin/ifconfig", &[name]).await.ok;
    if !down.ok && remaining > 0 && remaining == before {
        let detail: String = down.text().trim().chars().take(200).collect();
        return Err(if detail.is_empty() {
            format!("{name} 关闭失败，仍有 {remaining} 条路由")
        } else {
            format!("{name} 关闭失败，仍有 {remaining} 条路由：{detail}")
        });
    }
    let routes_removed = before.saturating_sub(remaining);
    let message = if !present {
        format!("{name} 已移除，关联路由已清理")
    } else if remaining == 0 {
        format!("{name} 已关闭，关联路由已清理")
    } else if down.ok {
        format!("{name} 仍存在，残留 {remaining} 条路由；可能仍有进程持有该隧道")
    } else {
        format!("{name} 关闭失败，仍有 {remaining} 条路由")
    };
    Ok(UtunCleanup {
        routes_removed,
        still_present: present,
        message,
    })
}

#[cfg(target_os = "macos")]
async fn route_table() -> Vec<RouteTableEntry> {
    let out = shell::run_local("/usr/sbin/netstat", &["-rn", "-f", "inet"]).await;
    if out.ok {
        parse_route_table(&out.stdout)
    } else {
        Vec::new()
    }
}

/// 删除一条冲突路由；优先按接口删除，失败再按网关删除，删除后回读确认
#[cfg(target_os = "macos")]
async fn remove_conflict(conflict: &RouteConflict) -> bool {
    let attempts: [(&str, &str); 2] = [
        ("-interface", conflict.ifname.as_str()),
        ("", conflict.gateway.as_str()),
    ];
    for (flag, value) in attempts {
        let mut args = vec![
            "-n",
            "delete",
            "-net",
            conflict.dest.as_str(),
            "-netmask",
            conflict.mask.as_str(),
        ];
        if !flag.is_empty() {
            args.push(flag);
        }
        args.push(value);
        let _ = shell::run("route", &args).await;
        if !conflict_present(conflict).await {
            return true;
        }
    }
    false
}

#[cfg(target_os = "macos")]
async fn conflict_present(conflict: &RouteConflict) -> bool {
    let probe = route_probe_address(&conflict.dest, conflict.prefix)
        .unwrap_or_else(|| conflict.dest.clone());
    let out = shell::run("route", &["-n", "get", &probe]).await;
    out.ok
        && route_lookup_matches(
            &out.stdout,
            &conflict.dest,
            conflict.prefix,
            &conflict.ifname,
        )
}

/// 判断 macOS 路由表里是否存在精确命中目标网段与本机接口的路由
#[cfg(target_os = "macos")]
async fn macos_route_exists(ifname: &str, dest: &str, prefix: u8) -> bool {
    let probe = route_probe_address(dest, prefix).unwrap_or_else(|| dest.to_string());
    for attempt in 0..2 {
        let out = shell::run("route", &["-n", "get", &probe]).await;
        if out.ok && route_lookup_matches(&out.stdout, dest, prefix, ifname) {
            return true;
        }
        if attempt == 0 {
            tokio::time::sleep(Duration::from_millis(120)).await;
        }
    }
    false
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
        let text = "2: topsap0: <POINTOPOINT,UP> mtu 1400\n    inet 198.51.100.10/24 scope global topsap0\n       valid_lft forever\n";
        assert_eq!(parse_interface_addresses(text), vec!["198.51.100.10"]);
    }

    #[test]
    fn 解析_macos_ifconfig_输出() {
        let text = "utun0: flags=8051<UP,POINTOPOINT,RUNNING,MULTICAST> mtu 1400\n\tinet 198.51.100.10 --> 198.51.100.10 netmask 0xffffff00\n";
        assert_eq!(parse_interface_addresses(text), vec!["198.51.100.10"]);
    }

    #[test]
    fn 忽略_ipv6_与无关行() {
        let text =
            "inet6 fe80::1%topsap0 prefixlen 64\n    inet 192.0.2.10/24 scope global topsap0\n";
        assert_eq!(parse_interface_addresses(text), vec!["192.0.2.10"]);
    }
}
