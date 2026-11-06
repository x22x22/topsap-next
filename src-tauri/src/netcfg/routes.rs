//! 路由表文本解析（纯函数，便于单测）。

use std::collections::HashSet;

/// 解析 `route print -4` 的活动路由段，返回 `(目的网段, 掩码)` 集合。
///
/// 只取活动路由，跳过永久路由段——永久路由可能是用户自行配置的，不能误删。
pub fn parse_active_routes(text: &str) -> HashSet<(String, String)> {
    let mut out = HashSet::new();
    let mut in_active = false;

    for raw in text.lines() {
        let line = raw.trim();

        if line.contains("Active Routes") || line.contains("活动路由") {
            in_active = true;
            continue;
        }
        if line.contains("Persistent Routes") || line.contains("永久路由") {
            in_active = false;
            continue;
        }
        if !in_active {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 5 {
            continue;
        }
        let (dest, mask) = (parts[0], parts[1]);
        if !is_dotted_quad(dest) || !is_dotted_quad(mask) {
            continue;
        }
        out.insert((dest.to_string(), mask.to_string()));
    }
    out
}

/// 解析默认路由，返回 `(网关, 出口接口 IP)`。
pub fn parse_default_gateway(text: &str) -> Option<(String, String)> {
    let mut in_active = false;
    for raw in text.lines() {
        let line = raw.trim();

        if line.contains("Active Routes") || line.contains("活动路由") {
            in_active = true;
            continue;
        }
        if line.contains("Persistent Routes") || line.contains("永久路由") {
            in_active = false;
            continue;
        }
        if !in_active {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 5 && parts[0] == "0.0.0.0" && parts[1] == "0.0.0.0" {
            return Some((parts[2].to_string(), parts[3].to_string()));
        }
    }
    None
}

fn is_dotted_quad(text: &str) -> bool {
    let parts: Vec<&str> = text.split('.').collect();
    parts.len() == 4 && parts.iter().all(|p| p.parse::<u8>().is_ok())
}

/// 从 `netsh interface ipv4 show addresses` 输出中提取 IPv4 地址。
///
/// 该命令在 Windows 7 到 11 上均可用，且输出随系统语言变化，
/// 因此同时匹配英文 `IP Address` 与中文 `IP 地址`。
pub fn parse_netsh_addresses(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        let lower = trimmed.to_ascii_lowercase();
        let is_address_line = lower.starts_with("ip address")
            || trimmed.contains("IP 地址")
            || trimmed.contains("IP地址");
        if !is_address_line {
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

/// 从 `netsh interface ipv4 show interfaces` 输出中按别名查找接口索引。
pub fn parse_netsh_interfaces(text: &str, alias: &str) -> Option<u32> {
    for line in text.lines() {
        if !line.contains(alias) {
            continue;
        }
        let Some(first) = line.split_whitespace().next() else {
            continue;
        };
        if let Ok(index) = first.parse::<u32>() {
            return Some(index);
        }
    }
    None
}

/// 从 `netsh interface ipv4 show interfaces` 输出中按别名查找接口跃点数。
pub fn parse_netsh_interface_metric(text: &str, alias: &str) -> Option<u32> {
    for line in text.lines() {
        if !line.contains(alias) {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }
        if parts[0].parse::<u32>().is_ok() {
            return parts[1].parse::<u32>().ok();
        }
    }
    None
}

fn first_ipv4(text: &str) -> Option<String> {
    for token in text.split(|c: char| !(c.is_ascii_digit() || c == '.')) {
        if is_dotted_quad(token) {
            return Some(token.to_string());
        }
    }
    None
}

/// 点分十进制掩码 -> 前缀长度
pub fn mask_to_prefix_len(mask: &str) -> Option<u8> {
    crate::auth::model::mask_to_prefix(mask)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
===========================================================================
Interface List
 20...50 3c 2e 1a 00 00 ......WLAN
 42...........................TopSAP
===========================================================================

IPv4 Route Table
===========================================================================
Active Routes:
Network Destination        Netmask          Gateway       Interface  Metric
          0.0.0.0          0.0.0.0     192.0.2.1    192.0.2.2     25
        10.9.0.0    255.255.248.0         On-link      10.8.0.10      1
        10.8.0.0      255.255.248.0         On-link      10.8.0.10      1
       10.8.0.10  255.255.255.255         On-link      10.8.0.10    261
===========================================================================
Persistent Routes:
  Network Address          Netmask  Gateway Address  Metric
       192.168.50.0    255.255.255.0     10.0.0.1       1
===========================================================================
"#;

    #[test]
    fn 解析活动路由_排除永久路由() {
        let set = parse_active_routes(SAMPLE);
        assert!(set.contains(&("10.9.0.0".to_string(), "255.255.248.0".to_string())));
        assert!(set.contains(&("10.8.0.0".to_string(), "255.255.248.0".to_string())));
        // 永久路由段里的条目不应被当成活动路由
        assert!(!set.contains(&("192.168.50.0".to_string(), "255.255.255.0".to_string())));
    }

    #[test]
    fn 解析默认网关() {
        let (gw, iface) = parse_default_gateway(SAMPLE).expect("应找到默认路由");
        assert_eq!(gw, "192.0.2.1");
        assert_eq!(iface, "192.0.2.2");
    }

    #[test]
    fn 空文本不报错() {
        assert!(parse_active_routes("").is_empty());
        assert!(parse_default_gateway("").is_none());
    }

    #[test]
    fn 中文系统输出也能识别() {
        let text = "\n活动路由:\n网络目标 网络掩码 网关 接口 跃点数\n10.8.0.0 255.255.248.0 0.0.0.0 10.8.0.10 1\n\n永久路由:\n";
        let set = parse_active_routes(text);
        assert!(set.contains(&("10.8.0.0".to_string(), "255.255.248.0".to_string())));
    }

    #[test]
    fn 掩码前缀换算() {
        assert_eq!(mask_to_prefix_len("255.255.248.0"), Some(21));
        assert_eq!(mask_to_prefix_len("255.255.255.255"), Some(32));
        assert_eq!(mask_to_prefix_len("bad"), None);
    }

    #[test]
    fn netsh_地址解析_英文输出() {
        let text = "Configuration for interface \"TopSAP\"\nDHCP enabled: No\nIP Address: 10.99.99.2\nSubnet Mask: 255.255.255.0\n";
        assert_eq!(parse_netsh_addresses(text), vec!["10.99.99.2"]);
    }

    #[test]
    fn netsh_地址解析_中文输出() {
        let text =
            "接口 TopSAP 的配置\nDHCP 已启用:  否\nIP 地址:  10.99.99.2\n子网掩码: 255.255.255.0\n";
        assert_eq!(parse_netsh_addresses(text), vec!["10.99.99.2"]);
    }

    #[test]
    fn netsh_地址解析_不误取掩码与网关() {
        let text = "IP Address: 10.99.99.2\nSubnet Mask: 255.255.255.0\nDefault Gateway: 0.0.0.0\n";
        assert_eq!(parse_netsh_addresses(text), vec!["10.99.99.2"]);
    }

    #[test]
    fn netsh_接口索引解析() {
        let text = "Idx     Met         MTU          State                Name\n\
                    ---  ----------  ----------  ------------  ---------------------------\n\
                     42           5        1400  disconnected  TopSAP\n\
                     20          25        1500  connected     WLAN\n";
        assert_eq!(parse_netsh_interfaces(text, "TopSAP"), Some(42));
        assert_eq!(parse_netsh_interfaces(text, "Missing"), None);
    }

    #[test]
    fn netsh_接口跃点数解析() {
        let text = "Idx Met MTU State Name\n\
                    42 5 1400 disconnected TopSAP\n\
                    20 25 1500 connected WLAN\n";
        assert_eq!(parse_netsh_interface_metric(text, "TopSAP"), Some(5));
        assert_eq!(parse_netsh_interface_metric(text, "Missing"), None);
    }

    #[test]
    fn netsh_接口索引_跳过标题行() {
        // 标题行也包含别名，必须先跳过非数字行再匹配数据行
        let text = "Configuration for interface \"TopSAP\"\n 7  5  1400  disconnected  TopSAP\n";
        assert_eq!(parse_netsh_interfaces(text, "TopSAP"), Some(7));
    }
}
