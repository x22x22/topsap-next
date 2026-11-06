//! macOS 网络配置文本解析（纯函数，便于单测）。

use std::net::IpAddr;

use crate::auth::RouteEntry;

use super::{RouteConflict, RouteConflictKind};

/// `route -n get <dest>` 中与路由匹配相关的字段
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteLookup {
    pub destination: String,
    pub mask: Option<String>,
    pub interface: Option<String>,
}

/// 解析 `networksetup -listnetworkserviceorder`，按设备名找到网络服务
pub fn parse_network_service_order(text: &str, ifname: &str) -> Option<String> {
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
pub fn parse_dns_servers(text: &str) -> Vec<String> {
    text.lines().filter_map(dns_address).collect()
}

/// 归一化 DNS 地址：去掉链路本地 IPv6 的作用域后缀
fn dns_address(line: &str) -> Option<String> {
    let value = line.trim();
    let base = value.split('%').next().unwrap_or(value);
    base.parse::<IpAddr>().ok().map(|_| base.to_string())
}

/// 解析 `scutil --dns`，优先返回默认接口对应的解析器服务器
pub fn parse_scutil_dns(text: &str, ifname: &str) -> Vec<String> {
    let mut blocks: Vec<(bool, Vec<String>)> = Vec::new();
    let mut matches_if = false;
    let mut servers: Vec<String> = Vec::new();
    let marker = format!("({ifname})");

    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with("resolver #") {
            if !servers.is_empty() {
                blocks.push((matches_if, std::mem::take(&mut servers)));
            }
            matches_if = false;
            continue;
        }
        if let Some(rest) = line.strip_prefix("if_index") {
            matches_if = rest.contains(&marker);
            continue;
        }
        if let Some(rest) = line.strip_prefix("nameserver[") {
            if let Some((_, value)) = rest.split_once(']') {
                if let Some(addr) = value.trim().strip_prefix(':').and_then(dns_address) {
                    if !servers.iter().any(|item| item == &addr) {
                        servers.push(addr);
                    }
                }
            }
        }
    }
    if !servers.is_empty() {
        blocks.push((matches_if, servers));
    }

    blocks
        .iter()
        .find(|(matched, servers)| *matched && !servers.is_empty())
        .or_else(|| blocks.iter().find(|(_, servers)| !servers.is_empty()))
        .map(|(_, servers)| servers.clone())
        .unwrap_or_default()
}

/// 解析 `networksetup -getinfo` 的 Router 行；自动获取 DNS 时作为原 DNS 回退
pub fn parse_router_info(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let value = line.trim().strip_prefix("Router:")?.trim();
        value
            .parse::<std::net::Ipv4Addr>()
            .is_ok()
            .then(|| value.to_string())
    })
}

/// 解析 `route -n get <dest>` 的关键字段
pub fn parse_route_lookup(text: &str) -> Option<RouteLookup> {
    let mut lookup = RouteLookup {
        destination: String::new(),
        mask: None,
        interface: None,
    };
    for raw in text.lines() {
        let line = raw.trim();
        if lookup.destination.is_empty() {
            if let Some(value) = line.strip_prefix("destination:") {
                lookup.destination = value.trim().to_string();
                continue;
            }
        }
        if lookup.mask.is_none() {
            if let Some(value) = line.strip_prefix("mask:") {
                lookup.mask = Some(value.trim().to_string());
                continue;
            }
        }
        if lookup.interface.is_none() {
            if let Some(value) = line.strip_prefix("interface:") {
                lookup.interface = Some(value.trim().to_string());
            }
        }
    }
    (!lookup.destination.is_empty()).then_some(lookup)
}

/// 判断 `route -n get` 结果是否精确命中目标网段与接口
pub fn route_lookup_matches(text: &str, dest: &str, prefix: u8, ifname: &str) -> bool {
    let Some(lookup) = parse_route_lookup(text) else {
        return false;
    };
    if lookup.interface.as_deref() != Some(ifname) {
        return false;
    }
    if lookup.mask.as_deref().and_then(mask_text_to_prefix) != Some(prefix) {
        return false;
    }
    let (Some(actual), Some(expected)) = (ipv4_value(&lookup.destination), ipv4_value(dest)) else {
        return false;
    };
    network_value(actual, prefix) == network_value(expected, prefix)
}

/// `netstat -rn -f inet` 中的一行路由
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteTableEntry {
    pub dest: String,
    pub prefix: u8,
    pub mask: String,
    pub gateway: String,
    pub ifname: String,
}

/// 解析 macOS `netstat -rn -f inet` 输出；跳过表头与无法识别的行
pub fn parse_route_table(text: &str) -> Vec<RouteTableEntry> {
    let mut out = Vec::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 4 {
            continue;
        }
        let Some((dest, prefix)) = parse_netstat_destination(fields[0]) else {
            continue;
        };
        let ifname = fields[3];
        if !is_ifname(ifname) {
            continue;
        }
        out.push(RouteTableEntry {
            dest: u32_to_ipv4(dest),
            prefix,
            mask: prefix_mask(prefix),
            gateway: fields[1].to_string(),
            ifname: ifname.to_string(),
        });
    }
    out
}

/// `ifconfig -a` 中带 IPv4 地址的 utun 接口
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IfconfigUtun {
    pub name: String,
    pub address: String,
    pub peer: String,
    pub mtu: u32,
}

/// 解析 `ifconfig -a`，只返回带 IPv4 地址的 utun 接口
pub fn parse_ifconfig_utuns(text: &str) -> Vec<IfconfigUtun> {
    let mut out = Vec::new();
    let mut current: Option<IfconfigUtun> = None;

    for raw in text.lines() {
        if !raw.starts_with(' ') && !raw.starts_with('\t') {
            if let Some(item) = current.take() {
                if !item.address.is_empty() {
                    out.push(item);
                }
            }
            let Some(name) = raw.split(':').next() else {
                continue;
            };
            if !name.starts_with("utun") {
                continue;
            }
            let fields: Vec<&str> = raw.split_whitespace().collect();
            let mtu = fields
                .iter()
                .position(|field| *field == "mtu")
                .and_then(|index| fields.get(index + 1))
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(0);
            current = Some(IfconfigUtun {
                name: name.to_string(),
                address: String::new(),
                peer: String::new(),
                mtu,
            });
            continue;
        }

        let Some(item) = current.as_mut() else {
            continue;
        };
        let Some(rest) = raw.trim().strip_prefix("inet ") else {
            continue;
        };
        let fields: Vec<&str> = rest.split_whitespace().collect();
        item.address = fields.first().copied().unwrap_or_default().to_string();
        if let Some(index) = fields.iter().position(|field| *field == "-->") {
            item.peer = fields
                .get(index + 1)
                .copied()
                .unwrap_or_default()
                .to_string();
        }
    }

    if let Some(item) = current {
        if !item.address.is_empty() {
            out.push(item);
        }
    }
    out
}

/// 找出与待注入网段完全相同、但位于其他接口的路由
pub fn find_route_conflicts(
    routes: &[RouteEntry],
    table: &[RouteTableEntry],
) -> Vec<RouteConflict> {
    let mut out: Vec<RouteConflict> = Vec::new();
    for route in routes {
        let Some(value) = ipv4_value(&route.dest) else {
            continue;
        };
        let expected = network_value(value, route.prefix);
        for entry in table {
            if entry.prefix != route.prefix {
                continue;
            }
            let Some(entry_value) = ipv4_value(&entry.dest) else {
                continue;
            };
            if network_value(entry_value, route.prefix) != expected {
                continue;
            }
            if out.iter().any(|item| {
                item.dest == route.dest
                    && item.prefix == route.prefix
                    && item.ifname == entry.ifname
            }) {
                continue;
            }
            out.push(RouteConflict {
                dest: route.dest.clone(),
                mask: route.mask.clone(),
                prefix: route.prefix,
                gateway: entry.gateway.clone(),
                ifname: entry.ifname.clone(),
                kind: classify_route_interface(&entry.ifname),
            });
        }
    }
    out
}

/// 前缀长度 -> 点分十进制掩码
fn prefix_mask(prefix: u8) -> String {
    let bits = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix.min(32))
    };
    u32_to_ipv4(bits)
}

fn parse_netstat_destination(text: &str) -> Option<(u32, u8)> {
    if text == "default" {
        return Some((0, 0));
    }
    let (addr, explicit) = match text.split_once('/') {
        Some((addr, prefix)) => (addr, Some(prefix.parse::<u8>().ok()?)),
        None => (text, None),
    };
    let value = ipv4_value(addr)?;
    let prefix = match explicit {
        Some(prefix) if prefix <= 32 => prefix,
        Some(_) => return None,
        None => match addr.split('.').count() {
            1 => 8,
            2 => 16,
            3 => 24,
            _ => 32,
        },
    };
    Some((network_value(value, prefix), prefix))
}

fn is_ifname(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_'))
}

fn classify_route_interface(ifname: &str) -> RouteConflictKind {
    if ifname.starts_with("utun") {
        RouteConflictKind::Tunnel
    } else if ifname == "lo0" {
        RouteConflictKind::Loopback
    } else if ifname.starts_with("en")
        || ifname.starts_with("eth")
        || ifname.starts_with("bridge")
        || ifname.starts_with("awdl")
        || ifname.starts_with("llw")
    {
        RouteConflictKind::Physical
    } else {
        RouteConflictKind::Other
    }
}

/// 只保留 IPv4 地址，避免把 IPv6 解析器写入网络服务 DNS
pub fn ipv4_only(values: &[String]) -> Vec<String> {
    values
        .iter()
        .filter(|value| value.parse::<std::net::Ipv4Addr>().is_ok())
        .cloned()
        .collect()
}

/// 供 `route get` 探测的目标地址：使用网段内第一个主机地址，避免直接查询网络地址
pub fn route_probe_address(dest: &str, prefix: u8) -> Option<String> {
    let value = ipv4_value(dest)?;
    let probe = if prefix >= 31 { value } else { value + 1 };
    Some(u32_to_ipv4(probe))
}

fn u32_to_ipv4(value: u32) -> String {
    let octets = value.to_be_bytes();
    format!("{}.{}.{}.{}", octets[0], octets[1], octets[2], octets[3])
}

fn mask_text_to_prefix(text: &str) -> Option<u8> {
    let text = text.trim();
    let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) else {
        return crate::auth::model::mask_to_prefix(text);
    };
    let value = u32::from_str_radix(hex, 16).ok()?;
    let prefix = value.leading_ones() as u8;
    let expected = network_value(u32::MAX, prefix);
    (value == expected).then_some(prefix)
}

/// `route get` 可能省略末尾的 0 段或带 `/前缀`，缺失段按 0 补齐
fn ipv4_value(text: &str) -> Option<u32> {
    let text = text.split('/').next().unwrap_or(text);
    let mut octets = [0u8; 4];
    let mut count = 0;
    for part in text.trim().split('.') {
        if count == 4 || part.is_empty() {
            return None;
        }
        octets[count] = part.parse().ok()?;
        count += 1;
    }
    (count > 0).then(|| u32::from_be_bytes(octets))
}

fn network_value(value: u32, prefix: u8) -> u32 {
    if prefix == 0 {
        0
    } else {
        value & (u32::MAX << (32 - prefix.min(32)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn 解析_dns_列表() {
        assert_eq!(
            parse_dns_servers(
                "192.0.2.53\n2001:db8::53\n2001:db8::1%en0\nThere aren't any DNS Servers set on Wi-Fi.\n"
            ),
            vec!["192.0.2.53", "2001:db8::53", "2001:db8::1"]
        );
        assert!(parse_dns_servers("There aren't any DNS Servers set on Wi-Fi.\n").is_empty());
    }

    #[test]
    fn 解析系统解析器_优先默认接口() {
        let text = "DNS configuration\n\n\
                    resolver #1\n\
                    \x20 search domain[0] : lan\n\
                    \x20 nameserver[0] : 192.0.2.53\n\
                    \x20 nameserver[1] : 2001:db8::53\n\
                    \x20 if_index : 14 (en0)\n\n\
                    resolver #2\n\
                    \x20 domain : local\n\
                    \x20 nameserver[0] : 192.0.2.1\n\
                    \x20 if_index : 5 (utun4)\n";
        assert_eq!(
            parse_scutil_dns(text, "en0"),
            vec!["192.0.2.53", "2001:db8::53"]
        );
    }

    #[test]
    fn 解析系统解析器_无接口匹配时回退首个() {
        let text = "resolver #1\n  nameserver[0] : 203.0.113.53\n\n\
                    resolver #2\n  nameserver[0] : 203.0.113.54\n";
        assert_eq!(parse_scutil_dns(text, "en0"), vec!["203.0.113.53"]);
        assert!(parse_scutil_dns("No DNS configuration available\n", "en0").is_empty());
    }

    #[test]
    fn 解析路由器地址() {
        let text = "IP address: 192.0.2.10\nSubnet mask: 255.255.255.240\nRouter: 192.0.2.53\n";
        assert_eq!(parse_router_info(text).as_deref(), Some("192.0.2.53"));
        assert_eq!(parse_router_info("Router: none\n"), None);
        assert_eq!(parse_router_info("IP address: 192.0.2.10\n"), None);
    }

    #[test]
    fn 路由精确匹配_不能被更宽网段顶替() {
        let text = "   route to: 198.51.100.5\n\
                    destination: 198.51.100\n\
                    \x20      mask: 255.255.255.0\n\
                    \x20   gateway: 192.0.2.1\n\
                    \x20 interface: utun4\n\
                    \x20     flags: <UP,GATEWAY,DONE,STATIC,PRCLONING>\n";
        assert!(route_lookup_matches(text, "198.51.100.0", 24, "utun4"));
        assert!(!route_lookup_matches(text, "198.51.100.5", 32, "utun4"));
        assert!(!route_lookup_matches(text, "198.51.100.0", 24, "utun5"));
    }

    #[test]
    fn 路由精确匹配_主机路由与十六进制掩码() {
        let text = "   route to: 198.51.100.5\ndestination: 198.51.100.5\n\
                    \x20      mask: 255.255.255.255\n\
                    \x20 interface: utun4\n";
        assert!(route_lookup_matches(text, "198.51.100.5", 32, "utun4"));
        assert!(!route_lookup_matches(text, "198.51.100.0", 24, "utun4"));

        let hex = "destination: 203.0.113.160\n       mask: 0xfffffffe\n    interface: utun4\n";
        assert!(route_lookup_matches(hex, "203.0.113.160", 31, "utun4"));
        assert!(!route_lookup_matches(hex, "203.0.113.0", 24, "utun4"));
    }

    #[test]
    fn 默认路由不匹配具体网段() {
        let text = "destination: default\n       mask: default\n    interface: en0\n";
        assert!(!route_lookup_matches(text, "198.51.100.0", 24, "en0"));
    }

    #[test]
    fn 探测地址_使用网段内主机地址() {
        assert_eq!(
            route_probe_address("203.0.113.0", 24).as_deref(),
            Some("203.0.113.1")
        );
        assert_eq!(
            route_probe_address("198.51.100.5", 32).as_deref(),
            Some("198.51.100.5")
        );
        assert_eq!(
            route_probe_address("203.0.113.160", 31).as_deref(),
            Some("203.0.113.160")
        );
        assert_eq!(route_probe_address("bad", 24), None);
    }

    #[test]
    fn 只保留_ipv4_地址() {
        let values = vec![
            "198.51.100.5".to_string(),
            "2001:db8::1".to_string(),
            "192.0.2.53".to_string(),
        ];
        assert_eq!(ipv4_only(&values), vec!["198.51.100.5", "192.0.2.53"]);
    }

    #[test]
    fn 路由精确匹配_兼容带前缀的目标格式() {
        let text =
            "destination: 198.51.100.0/24\n       mask: 255.255.255.0\n    interface: utun4\n";
        assert!(route_lookup_matches(text, "198.51.100.0", 24, "utun4"));
    }

    fn route_entry(dest: &str, mask: &str, prefix: u8) -> RouteEntry {
        RouteEntry {
            dest: dest.to_string(),
            mask: mask.to_string(),
            prefix,
        }
    }

    const ROUTE_TABLE: &str = "Routing tables\n\n\
        Internet:\n\
        Destination        Gateway            Flags        Netif Expire\n\
        default            10.186.33.1        UGScg          en7\n\
        8.130.175.152/32   172.21.8.107       UGSc           utun8\n\
        10.10.20/23        172.21.8.107       UGSc           utun8\n\
        10.22.1/24         172.21.8.107       UGSc           utun8\n\
        10.22.1.10/32      172.21.8.107       UGSc           utun8\n\
        127                127.0.0.1          UCS            lo0\n\
        192.168.1          link#12            UCS            en0      !\n";

    #[test]
    fn 解析路由表_识别目标前缀与接口() {
        let table = parse_route_table(ROUTE_TABLE);
        assert_eq!(table.len(), 7);
        assert_eq!(table[0].dest, "0.0.0.0");
        assert_eq!(table[0].prefix, 0);
        assert_eq!(table[0].ifname, "en7");
        assert_eq!(table[0].mask, "0.0.0.0");
        assert_eq!(table[1].dest, "8.130.175.152");
        assert_eq!(table[1].prefix, 32);
        assert_eq!(table[1].mask, "255.255.255.255");
        assert_eq!(table[2].dest, "10.10.20.0");
        assert_eq!(table[2].prefix, 23);
        assert_eq!(table[2].mask, "255.255.254.0");
        assert_eq!(table[5].dest, "127.0.0.0");
        assert_eq!(table[5].prefix, 8);
        assert_eq!(table[6].dest, "192.168.1.0");
        assert_eq!(table[6].prefix, 24);
        assert_eq!(table[6].gateway, "link#12");
    }

    #[test]
    fn 解析_ifconfig_只保留带地址的_utun() {
        let text = concat!(
            "lo0: flags=8049<UP,LOOPBACK,RUNNING,MULTICAST> mtu 16384\n",
            "    inet 127.0.0.1 netmask 0xff000000\n",
            "utun8: flags=8051<UP,POINTOPOINT,RUNNING,MULTICAST> mtu 1500\n",
            "    inet 172.21.8.108 --> 172.21.8.107 netmask 0xfffff800\n",
            "utun9: flags=8051<UP,POINTOPOINT,RUNNING,MULTICAST> mtu 1400\n",
            "    inet6 fe80::1%utun9 prefixlen 64\n",
            "gif0: flags=8010<POINTOPOINT,MULTICAST> mtu 1280\n",
        );
        let utuns = parse_ifconfig_utuns(text);
        assert_eq!(utuns.len(), 1);
        assert_eq!(utuns[0].name, "utun8");
        assert_eq!(utuns[0].address, "172.21.8.108");
        assert_eq!(utuns[0].peer, "172.21.8.107");
        assert_eq!(utuns[0].mtu, 1500);
    }

    #[test]
    fn 解析_ifconfig_无_inet_地址时忽略() {
        let text = concat!(
            "utun0: flags=8051<UP,POINTOPOINT,RUNNING,MULTICAST> mtu 1380\n",
            "    inet6 fe80::1%utun0 prefixlen 64\n",
        );
        assert!(parse_ifconfig_utuns(text).is_empty());
    }

    #[test]
    fn 路由冲突_只匹配目标与前缀完全一致() {
        let table = parse_route_table(ROUTE_TABLE);
        let routes = vec![
            route_entry("10.22.1.0", "255.255.255.0", 24),
            route_entry("10.22.2.0", "255.255.255.0", 24),
            route_entry("10.22.1.10", "255.255.255.255", 32),
            route_entry("10.10.20.0", "255.255.254.0", 23),
        ];
        let conflicts = find_route_conflicts(&routes, &table);
        assert_eq!(conflicts.len(), 3);
        assert_eq!(conflicts[0].dest, "10.22.1.0");
        assert_eq!(conflicts[0].ifname, "utun8");
        assert_eq!(conflicts[0].kind, RouteConflictKind::Tunnel);
        assert_eq!(conflicts[1].dest, "10.22.1.10");
        assert_eq!(conflicts[1].prefix, 32);
        assert_eq!(conflicts[2].dest, "10.10.20.0");
    }

    #[test]
    fn 路由冲突_区分隧道物理与环回接口() {
        let table = parse_route_table(ROUTE_TABLE);
        let routes = vec![route_entry("192.168.1.0", "255.255.255.0", 24)];
        let conflicts = find_route_conflicts(&routes, &table);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].kind, RouteConflictKind::Physical);

        let table = parse_route_table("127/8 127.0.0.1 UCS lo0\n");
        let conflicts = find_route_conflicts(&[route_entry("127.0.0.0", "255.0.0.0", 8)], &table);
        assert_eq!(conflicts[0].kind, RouteConflictKind::Loopback);
    }

    #[test]
    fn 路由冲突_无精确匹配时为空() {
        let table = parse_route_table(ROUTE_TABLE);
        let routes = vec![route_entry("10.22.0.0", "255.255.0.0", 16)];
        assert!(find_route_conflicts(&routes, &table).is_empty());
    }
}
