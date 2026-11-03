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
    /// Wintun 网卡刚创建时需要一点时间才可配置，且 `netsh` 在部分系统上会
    /// 「返回成功但地址没生效」，因此最多三轮、每轮换一种方式并回读校验。
    pub async fn configure_interface(&mut self, vip: &str, mask: &str) -> Result<u32, NetError> {
        let index = self.if_index_by_name().await.ok_or_else(|| {
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

        if crate::platform::is_windows7() {
            return self
                .configure_interface_win7(index, &name_arg, vip, mask)
                .await;
        }

        let prefix_len = routes::mask_to_prefix_len(mask)
            .ok_or_else(|| NetError::Interface(format!("掩码「{mask}」不是合法的连续掩码")))?;

        let mut attempts = Vec::new();
        let mut actual: Vec<String> = Vec::new();

        for round in 1..=3u32 {
            if round == 1 || round == 3 {
                let out = shell::run(
                    "netsh",
                    &[
                        "interface",
                        "ipv4",
                        "set",
                        "address",
                        &name_arg,
                        "static",
                        vip,
                        mask,
                    ],
                )
                .await;
                attempts.push(format!(
                    "第{round}轮 netsh={}",
                    if out.ok { "ok" } else { "fail" }
                ));
            } else {
                let script = format!(
                    "Remove-NetIPAddress -InterfaceIndex {index} -AddressFamily IPv4 -Confirm:$false -ErrorAction SilentlyContinue; \
                     New-NetIPAddress -InterfaceIndex {index} -IPAddress {vip} -PrefixLength {prefix_len} -ErrorAction SilentlyContinue"
                );
                let out = shell::powershell(&script).await;
                attempts.push(format!(
                    "第{round}轮 powershell={}",
                    if out.ok { "ok" } else { "fail" }
                ));
            }

            tokio::time::sleep(Duration::from_millis(600)).await;
            actual = self.interface_ipv4(index).await;
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

    /// Windows 7 兼容路径：系统只有 PowerShell 2.0，没有 NetTCPIP 模块，
    /// 因此全部使用 netsh 并回读校验。
    async fn configure_interface_win7(
        &mut self,
        index: u32,
        name_arg: &str,
        vip: &str,
        mask: &str,
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

            tokio::time::sleep(Duration::from_millis(600)).await;
            actual = self.interface_ipv4_netsh().await;
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
        if self
            .active_routes()
            .await
            .contains(&(gateway_ip.to_string(), "255.255.255.255".to_string()))
        {
            // 已有路由（可能是上次残留或用户手配），不接管、不删除
            return true;
        }

        let Some((default_gw, _)) = self.default_gateway().await else {
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
    pub async fn add_routes(&mut self, list: &[RouteEntry]) -> RouteReport {
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
            let if_str = index.to_string();
            for route in &pending {
                let _ = shell::run(
                    "route",
                    &[
                        "ADD",
                        &route.dest,
                        "MASK",
                        &route.mask,
                        "0.0.0.0",
                        "IF",
                        &if_str,
                        "METRIC",
                        "1",
                    ],
                )
                .await;
            }

            let mut still = self.record_applied(&pending).await;

            // New-NetRoute 属于 PowerShell 3.0+ 的 NetTCPIP 模块，Windows 7 不可用
            if !still.is_empty() && !crate::platform::is_windows7() {
                for route in &still {
                    let prefix = format!("{}/{}", route.dest, route.prefix);
                    let script = format!(
                        "New-NetRoute -DestinationPrefix {prefix} -InterfaceIndex {index} \
                         -NextHop 0.0.0.0 -RouteMetric 1 -ErrorAction SilentlyContinue"
                    );
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

    pub async fn set_dns(&mut self, dns1: &str) -> bool {
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

    async fn if_index_by_name(&self) -> Option<u32> {
        if crate::platform::is_windows7() {
            return self.if_index_by_netsh().await;
        }

        let script = format!(
            "(Get-NetAdapter -Name '{}' -ErrorAction SilentlyContinue | Select-Object -First 1).ifIndex",
            self.alias
        );
        let ps = shell::powershell(&script).await;
        if ps.ok {
            if let Ok(index) = ps.stdout.trim().parse::<u32>() {
                if index > 0 {
                    return Some(index);
                }
            }
        }
        self.if_index_by_netsh().await
    }

    async fn if_index_by_netsh(&self) -> Option<u32> {
        let out = shell::run("netsh", &["interface", "ipv4", "show", "interfaces"]).await;
        routes::parse_netsh_interfaces(&out.stdout, &self.alias)
    }

    async fn interface_ipv4(&self, index: u32) -> Vec<String> {
        if crate::platform::is_windows7() {
            return self.interface_ipv4_netsh().await;
        }

        let script = format!(
            "(Get-NetIPAddress -InterfaceIndex {index} -AddressFamily IPv4 -ErrorAction SilentlyContinue \
             | Select-Object -ExpandProperty IPAddress) -join ','"
        );
        let out = shell::powershell(&script).await;
        let parsed: Vec<String> = out
            .stdout
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if parsed.is_empty() {
            return self.interface_ipv4_netsh().await;
        }
        parsed
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

    async fn default_gateway(&self) -> Option<(String, String)> {
        let out = shell::run("route", &["print", "-4"]).await;
        routes::parse_default_gateway(&out.stdout)
    }
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
