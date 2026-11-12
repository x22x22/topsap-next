//! 隧道路由生效范围。
//!
//! 登录后网关会下发一组需要走隧道的网段（ACL 目标 + 门户资源网段）。用户可能
//! 不希望其中某些网段被接管（例如与本地网络同网段、或临时只想访问部分内网），
//! 这里提供按网段关闭注入的能力：
//!
//! - `disabled_routes` 以 `目的地址/前缀` 保存，随设置持久化；
//! - 连接时过滤 `LoginSession::routes`，被关闭的网段既不写入系统路由表，
//!   也不参与连接前的路由冲突检测与自动重连恢复；
//! - 每次登录会把网关下发的完整路由写进 `routes_cache.json`，未连接时设置页
//!   仍能展示并勾选。

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::auth::RouteEntry;

/// 关闭条数上限，避免异常配置文件把设置页拖垮
pub const MAX_DISABLED: usize = 256;
const CACHE_FILE: &str = "routes_cache.json";

/// 设置页展示的一条路由
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteRuleDto {
    pub dest: String,
    pub mask: String,
    pub prefix: u8,
    /// false 表示该网段不写入系统路由表
    pub enabled: bool,
}

/// 路由生效范围快照
#[derive(Debug, Clone, Serialize)]
pub struct RoutePolicySnapshot {
    pub routes: Vec<RouteRuleDto>,
    /// 当前是否存在已登录会话
    pub online: bool,
    /// 路由来源：session（当前会话）/ cache（上次登录）
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedRoute {
    dest: String,
    mask: String,
    prefix: u8,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Cache {
    routes: Vec<CachedRoute>,
}

fn cache_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(crate::paths::config_dir(app)?.join(CACHE_FILE))
}

fn parse_ipv4(raw: &str) -> Option<[u8; 4]> {
    let mut out = [0u8; 4];
    let parts: Vec<&str> = raw.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() || part.len() > 3 || !part.chars().all(|ch| ch.is_ascii_digit()) {
            return None;
        }
        out[index] = part.parse::<u8>().ok()?;
    }
    Some(out)
}

fn format_ipv4(octets: [u8; 4]) -> String {
    format!("{}.{}.{}.{}", octets[0], octets[1], octets[2], octets[3])
}

fn prefix_mask(prefix: u8) -> [u8; 4] {
    let value = if prefix == 0 {
        0u32
    } else {
        u32::MAX << (32 - u32::from(prefix))
    };
    value.to_be_bytes()
}

fn network_address(dest: [u8; 4], prefix: u8) -> [u8; 4] {
    let mask = prefix_mask(prefix);
    [
        dest[0] & mask[0],
        dest[1] & mask[1],
        dest[2] & mask[2],
        dest[3] & mask[3],
    ]
}

/// `目的地址/前缀` 形式的掩码文本，用于展示与缓存
pub fn mask_text(prefix: u8) -> String {
    format_ipv4(prefix_mask(prefix))
}

fn parse_mask(raw: &str, prefix: u8) -> Option<String> {
    let mask = parse_ipv4(raw)?;
    if mask == prefix_mask(prefix) {
        Some(format_ipv4(mask))
    } else {
        None
    }
}

/// 把用户/缓存里的网段归一化成 `网络地址/前缀`。
///
/// 支持 `10.1.2.3/8` 与 `10.0.0.0 255.0.0.0` 两种写法；非法值返回 None。
pub fn normalize_key(raw: &str) -> Option<String> {
    let text = raw.trim();
    if text.is_empty() || text.len() > 64 {
        return None;
    }
    let (dest_text, prefix) = if let Some((dest, prefix_text)) = text.split_once('/') {
        let prefix = prefix_text.trim().parse::<u8>().ok()?;
        (dest.trim(), prefix)
    } else {
        let mut parts = text.split_whitespace();
        let dest = parts.next()?;
        let mask = parts.next()?;
        if parts.next().is_some() {
            return None;
        }
        let mask_text = parse_ipv4(mask)?;
        (dest, prefix_from_mask(mask_text)?)
    };
    if prefix > 32 {
        return None;
    }
    let dest = parse_ipv4(dest_text)?;
    let network = network_address(dest, prefix);
    Some(format!("{}/{}", format_ipv4(network), prefix))
}

fn prefix_from_mask(mask: [u8; 4]) -> Option<u8> {
    let value = u32::from_be_bytes(mask);
    let prefix = value.leading_ones() as u8;
    if prefix_mask(prefix) == mask {
        Some(prefix)
    } else {
        None
    }
}

/// 清洗设置中的关闭列表：非法跳过、按网段去重、超过上限截断。
pub fn sanitize_disabled(input: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for raw in input {
        let Some(key) = normalize_key(raw) else {
            continue;
        };
        if seen.insert(key.clone()) {
            out.push(key);
        }
        if out.len() == MAX_DISABLED {
            break;
        }
    }
    out
}

fn disabled_set(disabled: &[String]) -> BTreeSet<String> {
    disabled
        .iter()
        .filter_map(|item| normalize_key(item))
        .collect()
}

fn route_key(route: &RouteEntry) -> String {
    // 统一按「网络地址/前缀」比较：设置页保存的排除项已归一化，这里若直接用原始
    // 目的地址（例如 198.51.100.5/24），会出现排除项对不上、被排除网段仍被注入的问题
    match parse_ipv4(&route.dest) {
        Some(dest) if route.prefix <= 32 => format!(
            "{}/{}",
            format_ipv4(network_address(dest, route.prefix)),
            route.prefix
        ),
        _ => format!("{}/{}", route.dest, route.prefix),
    }
}

/// 按关闭列表过滤路由；返回仍需要注入的路由。
pub fn filter_routes(routes: &[RouteEntry], disabled: &[String]) -> Vec<RouteEntry> {
    if disabled.is_empty() {
        return routes.to_vec();
    }
    let blocked = disabled_set(disabled);
    routes
        .iter()
        .filter(|route| !blocked.contains(&route_key(route)))
        .cloned()
        .collect()
}

/// 生成设置页所需的路由规则列表，按目的地址 + 前缀稳定排序。
pub fn rules(routes: &[RouteEntry], disabled: &[String]) -> Vec<RouteRuleDto> {
    let blocked = disabled_set(disabled);
    let mut out: Vec<RouteRuleDto> = routes
        .iter()
        .filter_map(|route| {
            let dest = parse_ipv4(&route.dest)?;
            if route.prefix > 32 {
                return None;
            }
            let network = format_ipv4(network_address(dest, route.prefix));
            let mask = match route.mask.is_empty() {
                true => mask_text(route.prefix),
                false => {
                    parse_mask(&route.mask, route.prefix).unwrap_or_else(|| mask_text(route.prefix))
                }
            };
            let key = format!("{}/{}", network, route.prefix);
            Some(RouteRuleDto {
                dest: network,
                mask,
                prefix: route.prefix,
                enabled: !blocked.contains(&key),
            })
        })
        .collect();
    out.sort_by(|a, b| {
        let a_ip = parse_ipv4(&a.dest).unwrap_or([0; 4]);
        let b_ip = parse_ipv4(&b.dest).unwrap_or([0; 4]);
        a_ip.cmp(&b_ip).then(a.prefix.cmp(&b.prefix))
    });
    out.dedup_by(|a, b| a.dest == b.dest && a.prefix == b.prefix);
    out
}

/// 把本次登录下发的完整路由写入缓存，供未连接时勾选。
pub fn save_cache(app: &AppHandle, routes: &[RouteEntry]) {
    let cache = Cache {
        routes: routes
            .iter()
            .filter_map(|route| {
                let dest = parse_ipv4(&route.dest)?;
                if route.prefix > 32 {
                    return None;
                }
                Some(CachedRoute {
                    dest: format_ipv4(network_address(dest, route.prefix)),
                    mask: mask_text(route.prefix),
                    prefix: route.prefix,
                })
            })
            .collect(),
    };
    let Ok(path) = cache_path(app) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(&cache) {
        Ok(text) => {
            if let Err(err) = std::fs::write(&path, text) {
                log::warn!("写入路由缓存失败：{err}");
            }
        }
        Err(err) => log::warn!("序列化路由缓存失败：{err}"),
    }
}

/// 读取上次登录缓存的路由；文件缺失或损坏时返回空列表。
pub fn load_cache(app: &AppHandle) -> Vec<RouteEntry> {
    let Ok(path) = cache_path(app) else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(cache) = serde_json::from_str::<Cache>(&text) else {
        return Vec::new();
    };
    cache
        .routes
        .into_iter()
        .filter_map(|item| {
            let dest = parse_ipv4(&item.dest)?;
            if item.prefix > 32 {
                return None;
            }
            Some(RouteEntry {
                dest: format_ipv4(network_address(dest, item.prefix)),
                mask: mask_text(item.prefix),
                prefix: item.prefix,
            })
        })
        .collect()
}

/// 汇总当前可勾选的路由：优先使用当前会话，其次是上次登录缓存。
pub fn snapshot(app: &AppHandle, catalog: &[RouteEntry], online: bool) -> RoutePolicySnapshot {
    let settings = crate::settings::load(app);
    let (routes, source) = if !catalog.is_empty() {
        (catalog.to_vec(), if online { "session" } else { "cache" })
    } else {
        (load_cache(app), "cache")
    };
    RoutePolicySnapshot {
        routes: rules(&routes, &settings.disabled_routes),
        online,
        source: source.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(dest: &str, prefix: u8) -> RouteEntry {
        RouteEntry {
            dest: dest.to_string(),
            mask: mask_text(prefix),
            prefix,
        }
    }

    #[test]
    fn 归一化_cidr_与点分掩码() {
        assert_eq!(normalize_key("10.1.2.3/8").as_deref(), Some("10.0.0.0/8"));
        assert_eq!(
            normalize_key("192.168.10.5 255.255.255.0").as_deref(),
            Some("192.168.10.0/24")
        );
        assert_eq!(normalize_key("0.0.0.0/0").as_deref(), Some("0.0.0.0/0"));
    }

    #[test]
    fn 归一化_拒绝非法输入() {
        assert!(normalize_key("").is_none());
        assert!(normalize_key("10.0.0.0/33").is_none());
        assert!(normalize_key("10.0.0.256/8").is_none());
        assert!(normalize_key("10.0.0.0").is_none());
        assert!(normalize_key("10.0.0.0/8/16").is_none());
        assert!(normalize_key("10.0.0.0 255.0.255.0").is_none());
    }

    #[test]
    fn 关闭列表_清洗去重并保序() {
        let input = vec![
            "10.1.2.3/8".to_string(),
            "bad".to_string(),
            "10.0.0.0/8".to_string(),
            "192.168.0.0/16".to_string(),
        ];
        assert_eq!(
            sanitize_disabled(&input),
            vec!["10.0.0.0/8".to_string(), "192.168.0.0/16".to_string()]
        );
    }

    #[test]
    fn 过滤路由_只保留未关闭网段() {
        let routes = vec![
            route("10.0.0.0", 8),
            route("192.168.0.0", 16),
            route("172.16.0.0", 12),
        ];
        let disabled = vec!["10.0.0.0/8".to_string(), "172.16.0.0/12".to_string()];
        let kept = filter_routes(&routes, &disabled);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].dest, "192.168.0.0");
    }

    #[test]
    fn 关闭列表为空时不做任何过滤() {
        let routes = vec![route("10.0.0.0", 8)];
        let kept = filter_routes(&routes, &[]);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].dest, "10.0.0.0");
        assert_eq!(kept[0].prefix, 8);
    }

    #[test]
    fn 过滤路由_原始目的地址未归一化也能匹配排除项() {
        // 网关侧可能给出 198.51.100.5/24 这种非网络地址形式，排除项保存的是
        // 198.51.100.0/24，两者必须被视为同一条路由
        let routes = vec![RouteEntry {
            dest: "198.51.100.5".to_string(),
            mask: "255.255.255.0".to_string(),
            prefix: 24,
        }];
        let kept = filter_routes(&routes, &["198.51.100.0/24".to_string()]);
        assert!(kept.is_empty());
    }

    #[test]
    fn 规则列表_按网段排序并标记启用状态() {
        let routes = vec![
            route("192.168.0.0", 16),
            route("10.0.0.0", 8),
            route("10.0.0.0", 8),
        ];
        let rules = rules(&routes, &["192.168.0.0/16".to_string()]);
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].dest, "10.0.0.0");
        assert!(rules[0].enabled);
        assert_eq!(rules[1].dest, "192.168.0.0");
        assert!(!rules[1].enabled);
        assert_eq!(rules[1].mask, "255.255.0.0");
    }

    #[test]
    fn 规则列表_忽略非法目的地址() {
        let routes = vec![
            RouteEntry {
                dest: "not-an-ip".to_string(),
                mask: "255.0.0.0".to_string(),
                prefix: 8,
            },
            route("10.0.0.0", 8),
        ];
        assert_eq!(rules(&routes, &[]).len(), 1);
    }

    #[test]
    fn 掩码文本与网络地址() {
        assert_eq!(mask_text(0), "0.0.0.0");
        assert_eq!(mask_text(8), "255.0.0.0");
        assert_eq!(mask_text(24), "255.255.255.0");
        assert_eq!(mask_text(32), "255.255.255.255");
        assert_eq!(
            format_ipv4(network_address([10, 20, 30, 40], 16)),
            "10.20.0.0"
        );
    }
}
