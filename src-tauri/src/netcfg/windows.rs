//! Windows 网络配置实现：网卡地址、路由注入；hosts 仅用于旧版残留检测与清理。

use std::time::Duration;

use crate::auth::RouteEntry;

use super::{hosts, routes, shell, NetError, RouteReport};

#[derive(Debug, Clone)]
enum LedgerEntry {
    Route {
        dest: String,
        mask: String,
        preexisting: bool,
    },
    Bypass {
        ip: String,
    },
}

pub struct NetConfig {
    alias: String,
    if_index: Option<u32>,
    ledger: Vec<LedgerEntry>,
}

impl NetConfig {
    pub fn new(alias: impl Into<String>) -> Self {
        Self {
            alias: alias.into(),
            if_index: None,
            ledger: Vec::new(),
        }
    }

    pub fn if_index(&self) -> Option<u32> {
        self.if_index
    }

    pub fn route_count(&self) -> usize {
        self.ledger
            .iter()
            .filter(|e| matches!(e, LedgerEntry::Route { .. }))
            .count()
    }

    /// 配置网卡 IPv4 地址。
    ///
    /// 统一使用 `netsh`，避免 PowerShell 启动开销；地址生效通常只需 100-300ms，
    /// 因此改为短间隔轮询而不是固定等待。最多三轮，每轮换一种写法并回读校验。
    pub async fn configure_interface(&mut self, vip: &str, mask: &str) -> Result<u32, NetError> {
        let index = self.if_index_with_retry().await.ok_or_else(|| {
            NetError::Interface(format!(
                "未找到虚拟网卡「{}」，驱动可能尚未就绪",
                self.alias
            ))
        })?;
        self.if_index = Some(index);

        let name_arg = format!("name={}", self.alias);
        let _ = shell::run(
            "netsh",
            &["interface", "set", "interface", &name_arg, "admin=enable"],
        )
        .await;

        if routes::mask_to_prefix_len(mask).is_none() {
            return Err(NetError::Interface(format!(
                "掩码「{mask}」不是合法的连续掩码"
            )));
        }

        self.configure_interface_netsh(index, &name_arg, vip, mask, !crate::platform::is_windows7())
            .await
    }

    /// 统一用 netsh 配址并回读；Windows 10/11 使用 100ms 轮询，Windows 7 保持 600ms 固定等待。
    async fn configure_interface_netsh(
        &mut self,
        index: u32,
        name_arg: &str,
        vip: &str,
        mask: &str,
        fast_poll: bool,
    ) -> Result<u32, NetError> {
        let mut attempts = Vec::new();
        let mut actual: Vec<String> = Vec::new();

        for round in 1..=3u32 {
            let out = match round {
                2 => {
                    let addr_arg = format!("addr={vip}");
                    let _ = shell::run(
                        "netsh",
                        &[
                            "interface",
                            "ipv4",
                            "delete",
                            "address",
                            name_arg,
                            addr_arg.as_str(),
                        ],
                    )
                    .await;
                    shell::run(
                        "netsh",
                        &["interface", "ipv4", "add", "address", name_arg, vip, mask],
                    )
                    .await
                }
                _ => {
                    shell::run(
                        "netsh",
                        &[
                            "interface",
                            "ipv4",
                            "set",
                            "address",
                            name_arg,
                            "static",
                            vip,
                            mask,
                        ],
                    )
                    .await
                }
            };
            attempts.push(format!(
                "第{round}轮 netsh={}",
                if out.ok { "ok" } else { "fail" }
            ));

            actual = if fast_poll {
                self.wait_for_ipv4(vip).await
            } else {
                tokio::time::sleep(Duration::from_millis(600)).await;
                self.interface_ipv4_netsh().await
            };
            if actual.iter().any(|a| a == vip) {
                break;
            }
        }

        if !actual.iter().any(|a| a == vip) {
            return Err(NetError::Interface(format!(
                "设置网卡地址失败（当前：{}；{}）",
                if actual.is_empty() {
                    "无地址".to_string()
                } else {
                    actual.join(",")
                },
                attempts.join("，")
            )));
        }

        self.apply_mtu().await;
        Ok(index)
    }

    async fn apply_mtu(&self) {
        let _ = shell::run(
            "netsh",
            &[
                "interface",
                "ipv4",
                "set",
                "subinterface",
                &self.alias,
                "mtu=1400",
                "store=active",
            ],
        )
        .await;
    }

    /// 给网关 IP 加一条旁路路由，否则隧道连接会自己走进隧道形成环路。
    pub async fn bypass_gateway(&mut self, gateway_ip: &str) -> bool {
        let (active, default_gateway) = self.route_snapshot().await;
        if active.contains(&(gateway_ip.to_string(), "255.255.255.255".to_string())) {
            // 已有路由（可能是上次残留或用户手配），不接管、不删除
            return true;
        }

        let Some((default_gw, _)) = default_gateway else {
            return false;
        };

        let out = shell::run(
            "route",
            &[
                "ADD",
                gateway_ip,
                "MASK",
                "255.255.255.255",
                &default_gw,
                "METRIC",
                "1",
            ],
        )
        .await;

        if out.ok || self.has_route(gateway_ip, "255.255.255.255").await {
            self.ledger.push(LedgerEntry::Bypass {
                ip: gateway_ip.to_string(),
            });
            true
        } else {
            false
        }
    }

    /// 注入内网路由。
    ///
    /// 批量执行后再回读，避免每条路由都跑一次 `route print`；
    /// 第一轮失败的用 PowerShell 补一次，仍失败则计入 failed。
    pub async fn add_routes(&mut self, list: &[RouteEntry], takeover: bool) -> RouteReport {
        // Windows 的 on-link 路由由系统自动合并，不存在 macOS 的精确前缀冲突
        let _ = takeover;
        let Some(index) = self.if_index else {
            return RouteReport {
                total: list.len(),
                ok: 0,
                failed: vec!["接口尚未初始化".to_string()],
            };
        };

        let mut ok = 0usize;
        let mut pending: Vec<&RouteEntry> = Vec::new();
        let active = self.active_routes().await;

        for route in list {
            if active.contains(&(route.dest.clone(), route.mask.clone())) {
                // 已存在：记录下来但不接管（preexisting 不删）
                self.ledger.push(LedgerEntry::Route {
                    dest: route.dest.clone(),
                    mask: route.mask.clone(),
                    preexisting: true,
                });
                ok += 1;
            } else {
                pending.push(route);
            }
        }

        if !pending.is_empty() {
            const BATCH: usize = 50;
            let if_str = index.to_string();
            for chunk in pending.chunks(BATCH) {
                let script = route_add_script(&if_str, chunk);
                let _ = shell::run("cmd", &["/D", "/C", &script]).await;
            }

            let mut still = self.record_applied(&pending).await;

            // New-NetRoute 属于 PowerShell 3.0+ 的 NetTCPIP 模块，Windows 7 不可用
            if !still.is_empty() && !crate::platform::is_windows7() {
                for chunk in still.chunks(BATCH) {
                    let script = new_net_route_script(index, chunk);
                    let _ = shell::powershell(&script).await;
                }
                still = self.record_applied(&still).await;
            }

            ok += pending.len() - still.len();
        }

        let failed = list
            .iter()
            .filter(|route| {
                !self.ledger.iter().any(|e| {
                    matches!(
                        e,
                        LedgerEntry::Route { dest, mask, .. }
                            if dest == &route.dest && mask == &route.mask
                    )
                })
            })
            .map(|route| format!("{}/{}", route.dest, route.prefix))
            .collect();

        RouteReport {
            total: list.len(),
            ok,
            failed,
        }
    }

    /// 回读路由表，把真正生效的记入账本，返回仍失败的路由
    async fn record_applied<'a>(&mut self, routes: &[&'a RouteEntry]) -> Vec<&'a RouteEntry> {
        let active = self.active_routes().await;
        let mut still = Vec::new();
        for route in routes {
            if active.contains(&(route.dest.clone(), route.mask.clone())) {
                self.ledger.push(LedgerEntry::Route {
                    dest: route.dest.clone(),
                    mask: route.mask.clone(),
                    preexisting: false,
                });
            } else {
                still.push(*route);
            }
        }
        still
    }

    pub async fn set_dns(&mut self, dns1: &str, dns2: &str) -> bool {
        if dns1.is_empty() || dns1 == "0.0.0.0" {
            return false;
        }
        let name_arg = format!("name={}", self.alias);
        let out = shell::run(
            "netsh",
            &[
                "interface",
                "ipv4",
                "set",
                "dnsservers",
                &name_arg,
                "static",
                dns1,
                "primary",
            ],
        )
        .await;
        if !dns2.is_empty() && dns2 != "0.0.0.0" {
            let _ = shell::run(
                "netsh",
                &[
                    "interface",
                    "ipv4",
                    "add",
                    "dnsservers",
                    &name_arg,
                    dns2,
                    "index=2",
                ],
            )
            .await;
        }
        out.ok
    }

    /// 撤销全部改动。幂等：可重复调用。
    pub async fn cleanup(&mut self) {
        let if_index = self.if_index.map(|i| i.to_string());

        for entry in self.ledger.drain(..).rev() {
            match entry {
                LedgerEntry::Route {
                    dest,
                    mask,
                    preexisting,
                } => {
                    if preexisting {
                        continue;
                    }
                    let mut args = vec!["DELETE", dest.as_str(), "MASK", mask.as_str()];
                    if let Some(index) = &if_index {
                        args.push("IF");
                        args.push(index.as_str());
                    }
                    let _ = shell::run("route", &args).await;
                }
                LedgerEntry::Bypass { ip } => {
                    let _ = shell::run("route", &["DELETE", &ip, "MASK", "255.255.255.255"]).await;
                }
            }
        }
    }

    async fn if_index_with_retry(&self) -> Option<u32> {
        for attempt in 0..10u32 {
            if let Some(index) = self.if_index_by_netsh().await {
                return Some(index);
            }
            if attempt < 9 {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
        None
    }

    async fn if_index_by_netsh(&self) -> Option<u32> {
        let out = shell::run("netsh", &["interface", "ipv4", "show", "interfaces"]).await;
        routes::parse_netsh_interfaces(&out.stdout, &self.alias)
    }

    /// 短间隔轮询地址是否生效，避免固定等待 600ms
    async fn wait_for_ipv4(&self, vip: &str) -> Vec<String> {
        let mut actual = Vec::new();
        for attempt in 0..10u32 {
            actual = self.interface_ipv4_netsh().await;
            if actual.iter().any(|a| a == vip) {
                return actual;
            }
            if attempt < 9 {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
        actual
    }

    async fn interface_ipv4_netsh(&self) -> Vec<String> {
        let name_arg = format!("name={}", self.alias);
        let out = shell::run(
            "netsh",
            &["interface", "ipv4", "show", "addresses", &name_arg],
        )
        .await;
        routes::parse_netsh_addresses(&out.stdout)
    }

    async fn active_routes(&self) -> std::collections::HashSet<(String, String)> {
        let out = shell::run("route", &["print", "-4"]).await;
        routes::parse_active_routes(&out.stdout)
    }

    async fn has_route(&self, dest: &str, mask: &str) -> bool {
        self.active_routes()
            .await
            .contains(&(dest.to_string(), mask.to_string()))
    }

    /// 一次读取同时拿到活动路由与默认网关，避免重复执行 route print
    async fn route_snapshot(
        &self,
    ) -> (
        std::collections::HashSet<(String, String)>,
        Option<(String, String)>,
    ) {
        let out = shell::run("route", &["print", "-4"]).await;
        (
            routes::parse_active_routes(&out.stdout),
            routes::parse_default_gateway(&out.stdout),
        )
    }
}

/// 拼接一批 `route ADD` 命令，交给一次 cmd 进程执行
fn route_add_script(if_index: &str, routes: &[&RouteEntry]) -> String {
    routes
        .iter()
        .map(|route| {
            format!(
                "route ADD {} MASK {} 0.0.0.0 IF {} METRIC 1",
                route.dest, route.mask, if_index
            )
        })
        .collect::<Vec<_>>()
        .join(" & ")
}

/// 拼接一批 `New-NetRoute` 语句，交给一次 PowerShell 执行
fn new_net_route_script(index: u32, routes: &[&RouteEntry]) -> String {
    routes
        .iter()
        .map(|route| {
            format!(
                "New-NetRoute -DestinationPrefix {}/{} -InterfaceIndex {index} -NextHop 0.0.0.0 -RouteMetric 1 -ErrorAction SilentlyContinue",
                route.dest, route.prefix
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// 当前进程是否具备管理员权限
pub async fn is_elevated() -> bool {
    shell::run("net", &["session"]).await.ok
}

/// 检查系统是否存在上次异常退出留下的 hosts 标记段
pub async fn has_hosts_residue() -> bool {
    match tokio::fs::read(hosts::HOSTS_PATH).await {
        Ok(bytes) => hosts::has_block(&shell::decode(&bytes)),
        Err(_) => false,
    }
}

/// 清理残留的 hosts 标记段（启动时提示后调用）
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
    let _ = shell::run("ipconfig", &["/flushdns"]).await;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route_entry(dest: &str, mask: &str, prefix: u8) -> RouteEntry {
        RouteEntry {
            dest: dest.to_string(),
            mask: mask.to_string(),
            prefix,
        }
    }

    #[test]
    fn 批量路由命令_拼接正确() {
        let routes = [
            route_entry("198.51.100.0", "255.255.255.0", 24),
            route_entry("203.0.113.0", "255.255.255.0", 24),
        ];
        let refs: Vec<&RouteEntry> = routes.iter().collect();
        assert_eq!(
            route_add_script("7", &refs),
            "route ADD 198.51.100.0 MASK 255.255.255.0 0.0.0.0 IF 7 METRIC 1 & \
             route ADD 203.0.113.0 MASK 255.255.255.0 0.0.0.0 IF 7 METRIC 1"
        );
    }

    #[test]
    fn 批量_netroute_命令_拼接正确() {
        let routes = [route_entry("198.51.100.0", "255.255.255.0", 24)];
        let refs: Vec<&RouteEntry> = routes.iter().collect();
        assert_eq!(
            new_net_route_script(7, &refs),
            "New-NetRoute -DestinationPrefix 198.51.100.0/24 -InterfaceIndex 7 \
             -NextHop 0.0.0.0 -RouteMetric 1 -ErrorAction SilentlyContinue"
        );
    }

    #[test]
    fn 路由报告_all_ok_语义() {
        assert!(RouteReport {
            total: 0,
            ok: 0,
            failed: vec![]
        }
        .all_ok());
        assert!(RouteReport {
            total: 3,
            ok: 3,
            failed: vec![]
        }
        .all_ok());
        assert!(!RouteReport {
            total: 3,
            ok: 2,
            failed: vec!["a".into()]
        }
        .all_ok());
    }
}
