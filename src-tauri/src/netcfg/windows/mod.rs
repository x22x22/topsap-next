//! Windows 网络配置实现：网卡地址、路由注入；hosts 仅用于旧版残留检测与清理。

use std::net::Ipv4Addr;
use std::time::Duration;

use crate::auth::RouteEntry;

use super::{hosts, routes, shell, NetError, RouteReport};

mod address;

use address::{persist_static_address, set_unicast_address};

/// 配址兜底总等待时长；新建 Wintun 网卡的地址生效可能晚至 7-10 秒
const CONFIG_TIMEOUT: Duration = Duration::from_secs(20);
const CONFIG_RETRY_AFTER: Duration = Duration::from_secs(6);
const CONFIG_POLL_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone)]
enum LedgerEntry {
    Route {
        dest: String,
        mask: String,
        next_hop: Ipv4Addr,
        if_index: u32,
        preexisting: bool,
    },
    Bypass {
        ip: String,
    },
}

pub struct NetConfig {
    alias: String,
    if_index: Option<u32>,
    if_metric: u32,
    vip: Option<Ipv4Addr>,
    peer: Option<Ipv4Addr>,
    ledger: Vec<LedgerEntry>,
    notes: Vec<String>,
}

impl NetConfig {
    pub fn new(alias: impl Into<String>) -> Self {
        Self {
            alias: alias.into(),
            if_index: None,
            if_metric: WINDOWS_ROUTE_METRIC,
            vip: None,
            peer: None,
            ledger: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// 取出本次配置过程中产生的兜底说明，供连接日志逐条记录
    pub fn take_notes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notes)
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
    /// Windows 10/11 直接调用 IPHLPAPI 写入地址，避免 `netsh` 返回成功但地址未生效；
    /// 失败时回退 `netsh` 并按总时长轮询。Windows 7 保持 `netsh` + 固定等待。
    pub async fn configure_interface(&mut self, vip: &str, mask: &str) -> Result<u32, NetError> {
        let index = self.if_index_with_retry().await.ok_or_else(|| {
            NetError::Interface(format!(
                "未找到虚拟网卡「{}」，驱动可能尚未就绪",
                self.alias
            ))
        })?;
        self.if_index = Some(index);
        self.if_metric = self
            .interface_metric_netsh()
            .await
            .unwrap_or(WINDOWS_ROUTE_METRIC);

        let name_arg = format!("name={}", self.alias);
        let _ = shell::run(
            "netsh",
            &["interface", "set", "interface", &name_arg, "admin=enable"],
        )
        .await;

        let prefix = routes::mask_to_prefix_len(mask)
            .ok_or_else(|| NetError::Interface(format!("掩码「{mask}」不是合法的连续掩码")))?;

        self.vip = vip.parse().ok();
        self.peer =
            crate::auth::model::virtual_peer(vip, mask).and_then(|value| value.parse().ok());

        if crate::platform::is_windows7() {
            return self
                .configure_interface_win7(index, &name_arg, vip, mask)
                .await;
        }

        let mut api_error = None;
        if let Ok(vip_addr) = vip.parse::<Ipv4Addr>() {
            match set_unicast_address(index, vip_addr, prefix) {
                Ok(()) => {
                    if let Err(err) = persist_static_address(index, vip_addr, mask) {
                        self.notes.push(format!("网卡静态配置写入失败：{err}"));
                    }
                    self.apply_mtu().await;
                    return Ok(index);
                }
                Err(code) => {
                    self.notes
                        .push(format!("网卡配址：IPHLPAPI 未生效（{code}），回退 netsh"));
                    api_error = Some(code);
                }
            }
        }

        self.configure_interface_netsh(index, &name_arg, vip, mask, api_error)
            .await
    }

    /// Windows 7 兼容路径：系统只有 PowerShell 2.0，没有 NetTCPIP 模块，全部使用 netsh 并固定等待回读。
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

    /// IPHLPAPI 失败后的 netsh 兜底：先写入一次，再按总时长轮询，避免短窗口提前失败。
    async fn configure_interface_netsh(
        &mut self,
        index: u32,
        name_arg: &str,
        vip: &str,
        mask: &str,
        api_error: Option<u32>,
    ) -> Result<u32, NetError> {
        let mut attempts = Vec::new();
        if let Some(code) = api_error {
            attempts.push(format!("IPHLPAPI={code}"));
        }

        let out = shell::run(
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
        .await;
        attempts.push(format!("netsh set={}", if out.ok { "ok" } else { "fail" }));

        let started = std::time::Instant::now();
        let reset_at = started + CONFIG_RETRY_AFTER;
        let deadline = started + CONFIG_TIMEOUT;
        let mut reset_done = false;
        let mut actual: Vec<String>;

        loop {
            actual = self.interface_ipv4_netsh().await;
            if actual.iter().any(|a| a == vip) {
                self.apply_mtu().await;
                return Ok(index);
            }

            let now = std::time::Instant::now();
            if !reset_done && now >= reset_at {
                reset_done = true;
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
                let out = shell::run(
                    "netsh",
                    &["interface", "ipv4", "add", "address", name_arg, vip, mask],
                )
                .await;
                attempts.push(format!(
                    "netsh delete+add={}",
                    if out.ok { "ok" } else { "fail" }
                ));
            }

            if now >= deadline {
                break;
            }

            tokio::time::sleep(CONFIG_POLL_INTERVAL).await;
        }

        Err(NetError::Interface(format!(
            "设置网卡地址失败（当前：{}；{}）",
            if actual.is_empty() {
                "无地址".to_string()
            } else {
                actual.join(",")
            },
            attempts.join("，")
        )))
    }

    async fn apply_mtu(&mut self) {
        let out = shell::run(
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
        if !out.ok {
            self.notes.push("网卡 MTU 设置失败（1400）".to_string());
        }
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
    /// 使用 IPHLPAPI 直接写入路由表，NextHop 为虚拟网关 VIP-1；查询与回读同样走
    /// `GetIpForwardTable`，避免逐条 `route.exe` 的进程开销。API 失败时才回退 `route add`。
    pub async fn add_routes(&mut self, list: &[RouteEntry], takeover: bool) -> RouteReport {
        // Windows 不存在 macOS 的精确前缀冲突接管
        let _ = takeover;
        let Some(if_index) = self.if_index else {
            return RouteReport {
                total: list.len(),
                ok: 0,
                failed: vec!["接口尚未初始化".to_string()],
            };
        };
        if list.is_empty() {
            return RouteReport {
                total: 0,
                ok: 0,
                failed: Vec::new(),
            };
        }

        let next_hop = self.peer.unwrap_or(Ipv4Addr::UNSPECIFIED);
        let table = match ip_forward_table() {
            Ok(table) => table,
            Err(code) => {
                self.notes.push(format!(
                    "路由注入：GetIpForwardTable 失败（{code}），回退 route add"
                ));
                return self
                    .add_routes_fallback(list, next_hop, if_index, Some(code))
                    .await;
            }
        };

        let mut skipped = std::collections::HashSet::new();
        let mut added = std::collections::HashSet::new();
        let mut invalid = Vec::new();
        let mut add_errors = std::collections::HashMap::new();
        let mut fallback_ok = 0usize;
        let mut create_errors: std::collections::BTreeMap<u32, usize> =
            std::collections::BTreeMap::new();

        for route in list {
            let Some((dest, mask)) = route_ipv4(route) else {
                invalid.push(format!("{}/{} (地址格式错误)", route.dest, route.prefix));
                continue;
            };
            let key = (dest, mask);

            if table
                .iter()
                .any(|row| row_matches(row, dest, mask, next_hop, if_index))
            {
                self.note_route(route, next_hop, if_index, true);
                skipped.insert(key);
                continue;
            }

            if table
                .iter()
                .any(|row| row_matches_dest_mask(row, dest, mask) && row.if_index != if_index)
            {
                // 其他接口已有同网段路由：保持不接管
                self.note_route(route, next_hop, if_index, true);
                skipped.insert(key);
                continue;
            }

            // 接口自动生成的直连/本机/广播路由不能被改写
            if let Some(vip) = self.vip {
                if table.iter().any(|row| {
                    row_matches_dest_mask(row, dest, mask)
                        && interface_route_matches(row, vip, mask, if_index)
                }) {
                    self.note_route(route, next_hop, if_index, true);
                    skipped.insert(key);
                    continue;
                }
            }

            // 本接口同网段但下一跳不同：旧版 on-link 路由，先删掉再按 VIP-1 重建
            if let Some(old) = table
                .iter()
                .find(|row| row_matches_dest_mask(row, dest, mask) && row.if_index == if_index)
            {
                let _ = delete_forward_entry(old);
            }

            let metric = self.if_metric.saturating_add(WINDOWS_ROUTE_METRIC);
            let row = build_forward_row(dest, mask, next_hop, if_index, metric);
            let mut code = create_forward_entry(&row);
            if code != NO_ERROR {
                if let Some(old) = table
                    .iter()
                    .find(|row| row_matches_dest_mask(row, dest, mask) && row.if_index == if_index)
                {
                    let _ = delete_forward_entry(old);
                }
                code = create_forward_entry(&row);
            }

            let installed = if code == NO_ERROR {
                true
            } else {
                *create_errors.entry(code).or_default() += 1;
                if self.route_add_fallback(route, next_hop, if_index).await {
                    fallback_ok += 1;
                    true
                } else {
                    false
                }
            };
            if installed {
                self.note_route(route, next_hop, if_index, false);
                added.insert(key);
            } else {
                add_errors.insert(
                    key,
                    format!(
                        "{}/{} (CreateIpForwardEntry={code})",
                        route.dest, route.prefix
                    ),
                );
            }
        }

        if fallback_ok > 0 {
            let codes = create_errors
                .iter()
                .map(|(code, count)| format!("{code}×{count}"))
                .collect::<Vec<_>>()
                .join("，");
            let detail = if codes.is_empty() {
                String::new()
            } else {
                format!("（CreateIpForwardEntry 返回 {codes}）")
            };
            self.notes.push(format!(
                "路由注入：{fallback_ok} 条通过 route add 兜底{detail}"
            ));
        }

        let after = ip_forward_table();
        let mut ok = 0usize;
        let mut verified_failed = Vec::new();
        for route in list {
            let Some((dest, mask)) = route_ipv4(route) else {
                continue;
            };
            let key = (dest, mask);
            let installed = match &after {
                Ok(table) => table
                    .iter()
                    .any(|row| row_matches(row, dest, mask, next_hop, if_index)),
                Err(_) => added.contains(&key),
            };
            if installed || skipped.contains(&key) {
                ok += 1;
            } else {
                verified_failed.push(
                    add_errors
                        .remove(&key)
                        .unwrap_or_else(|| format!("{}/{}", route.dest, route.prefix)),
                );
            }
        }

        let mut failed = invalid;
        failed.extend(verified_failed);
        RouteReport {
            total: list.len(),
            ok,
            failed,
        }
    }

    fn note_route(
        &mut self,
        route: &RouteEntry,
        next_hop: Ipv4Addr,
        if_index: u32,
        preexisting: bool,
    ) {
        if let Some(LedgerEntry::Route {
            preexisting: existing,
            ..
        }) = self.ledger.iter_mut().find(|entry| {
            matches!(
                entry,
                LedgerEntry::Route {
                    dest,
                    mask,
                    next_hop: entry_hop,
                    if_index: entry_index,
                    ..
                } if dest == &route.dest
                    && mask == &route.mask
                    && *entry_hop == next_hop
                    && *entry_index == if_index
            )
        }) {
            if *existing && !preexisting {
                *existing = false;
            }
            return;
        }
        self.ledger.push(LedgerEntry::Route {
            dest: route.dest.clone(),
            mask: route.mask.clone(),
            next_hop,
            if_index,
            preexisting,
        });
    }

    /// `GetIpForwardTable` 不可用时的回退：逐条 `route add` + `route print` 回读
    async fn add_routes_fallback(
        &mut self,
        list: &[RouteEntry],
        next_hop: Ipv4Addr,
        if_index: u32,
        code: Option<u32>,
    ) -> RouteReport {
        let before = self.active_routes().await;
        let mut fallback_ok = 0usize;
        for route in list {
            if before.contains(&(route.dest.clone(), route.mask.clone())) {
                self.note_route(route, next_hop, if_index, true);
                continue;
            }
            if self.route_add_fallback(route, next_hop, if_index).await {
                fallback_ok += 1;
            }
            self.note_route(route, next_hop, if_index, false);
        }
        if code.is_some() && fallback_ok > 0 {
            self.notes
                .push(format!("路由注入：{fallback_ok} 条通过 route add 兜底"));
        }

        let after = self.active_routes().await;
        let mut ok = 0usize;
        let mut failed = Vec::new();
        for route in list {
            if after.contains(&(route.dest.clone(), route.mask.clone())) {
                ok += 1;
            } else {
                failed.push(match code {
                    Some(code) => {
                        format!("{}/{} (IPHLPAPI={code})", route.dest, route.prefix)
                    }
                    None => format!("{}/{}", route.dest, route.prefix),
                });
            }
        }
        RouteReport {
            total: list.len(),
            ok,
            failed,
        }
    }

    /// 单条路由的命令行回退，仅在 IPHLPAPI 写入失败时调用
    async fn route_add_fallback(
        &self,
        route: &RouteEntry,
        next_hop: Ipv4Addr,
        if_index: u32,
    ) -> bool {
        let if_str = if_index.to_string();
        let hop = next_hop.to_string();
        let metric = WINDOWS_ROUTE_METRIC.to_string();
        shell::run(
            "route",
            &[
                "ADD",
                &route.dest,
                "MASK",
                &route.mask,
                &hop,
                "IF",
                &if_str,
                "METRIC",
                &metric,
            ],
        )
        .await
        .ok
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
        let entries: Vec<LedgerEntry> = self.ledger.drain(..).rev().collect();
        let table = ip_forward_table();
        let mut removed_routes: Vec<(String, String, Ipv4Addr, u32)> = Vec::new();
        let mut removed_bypass: Vec<String> = Vec::new();
        let mut fallback_count = 0usize;

        for entry in entries {
            match entry {
                LedgerEntry::Route {
                    dest,
                    mask,
                    next_hop,
                    if_index,
                    preexisting,
                } => {
                    if preexisting {
                        continue;
                    }
                    let handled = match &table {
                        Ok(rows) => match (dest.parse::<Ipv4Addr>(), mask.parse::<Ipv4Addr>()) {
                            (Ok(dest_ip), Ok(mask_ip)) => {
                                match rows.iter().find(|row| {
                                    row_matches(row, dest_ip, mask_ip, next_hop, if_index)
                                }) {
                                    Some(row) => delete_forward_entry(row) == NO_ERROR,
                                    // 表已成功读取但没有精确条目，说明路由已不存在
                                    None => true,
                                }
                            }
                            _ => false,
                        },
                        Err(_) => false,
                    };
                    if !handled {
                        fallback_count += 1;
                        let if_str = if_index.to_string();
                        let _ =
                            shell::run("route", &["DELETE", &dest, "MASK", &mask, "IF", &if_str])
                                .await;
                    }
                    removed_routes.push((dest, mask, next_hop, if_index));
                }
                LedgerEntry::Bypass { ip } => {
                    let _ = shell::run("route", &["DELETE", &ip, "MASK", "255.255.255.255"]).await;
                    removed_bypass.push(ip);
                }
            }
        }

        if fallback_count > 0 {
            self.notes.push(format!(
                "路由清理：{fallback_count} 条使用 route DELETE 兜底"
            ));
        }

        let mut residual: Vec<String> = Vec::new();
        match ip_forward_table() {
            Ok(rows) => {
                for (dest, mask, next_hop, if_index) in &removed_routes {
                    let (Ok(dest_ip), Ok(mask_ip)) =
                        (dest.parse::<Ipv4Addr>(), mask.parse::<Ipv4Addr>())
                    else {
                        continue;
                    };
                    if rows
                        .iter()
                        .any(|row| row_matches(row, dest_ip, mask_ip, *next_hop, *if_index))
                    {
                        residual.push(format!("{dest}/{mask}"));
                    }
                }
                for ip in &removed_bypass {
                    let Ok(ip_addr) = ip.parse::<Ipv4Addr>() else {
                        continue;
                    };
                    if rows
                        .iter()
                        .any(|row| row.dest == ipv4_dword(ip_addr) && row.mask == u32::MAX)
                    {
                        residual.push(format!("{ip}/32"));
                    }
                }
            }
            Err(_) => {
                self.notes
                    .push("路由清理：无法回读路由表，残留状态未知".to_string());
            }
        }

        if !residual.is_empty() {
            let examples = residual
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join("、");
            self.notes.push(format!(
                "路由清理失败：{} 条残留（{}）",
                residual.len(),
                examples
            ));
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

    async fn interface_metric_netsh(&self) -> Option<u32> {
        let out = shell::run("netsh", &["interface", "ipv4", "show", "interfaces"]).await;
        routes::parse_netsh_interface_metric(&out.stdout, &self.alias)
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

/// 路由分量跃点；CreateIpForwardEntry 使用接口 Metric + 该值，满足 Metric1 >= 接口 Metric
const WINDOWS_ROUTE_METRIC: u32 = 1;
const NO_ERROR: u32 = 0;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const MIB_IPROUTE_TYPE_DIRECT: u32 = 3;
const MIB_IPROUTE_TYPE_INDIRECT: u32 = 4;
const MIB_IPPROTO_NETMGMT: u32 = 3;

/// Windows `MIB_IPFORWARDROW`；IP 字段为网络字节序
#[repr(C)]
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
struct MibIpForwardRow {
    dest: u32,
    mask: u32,
    policy: u32,
    next_hop: u32,
    if_index: u32,
    route_type: u32,
    proto: u32,
    age: u32,
    next_hop_as: u32,
    metric1: u32,
    metric2: u32,
    metric3: u32,
    metric4: u32,
    metric5: u32,
}

#[repr(C)]
struct MibIpForwardTable {
    num_entries: u32,
    table: [MibIpForwardRow; 1],
}

#[allow(non_snake_case)]
#[link(name = "iphlpapi")]
extern "system" {
    fn CreateIpForwardEntry(route: *const MibIpForwardRow) -> u32;
    fn DeleteIpForwardEntry(route: *const MibIpForwardRow) -> u32;
    fn GetIpForwardTable(table: *mut MibIpForwardTable, size: *mut u32, order: i32) -> u32;
}

/// 读取 IPv4 路由表；失败返回 Win32 错误码
fn ip_forward_table() -> Result<Vec<MibIpForwardRow>, u32> {
    let mut size = 0u32;
    let mut code = unsafe { GetIpForwardTable(std::ptr::null_mut(), &mut size, 0) };
    if code != ERROR_INSUFFICIENT_BUFFER && code != NO_ERROR {
        return Err(code);
    }
    if size == 0 {
        return Ok(Vec::new());
    }

    let mut buffer = vec![0u32; (size as usize).div_ceil(4)];
    let table = buffer.as_mut_ptr() as *mut MibIpForwardTable;
    code = unsafe { GetIpForwardTable(table, &mut size, 0) };
    if code != NO_ERROR {
        return Err(code);
    }

    let count = unsafe { (*table).num_entries as usize };
    let offset = std::mem::offset_of!(MibIpForwardTable, table);
    let rows = unsafe {
        std::slice::from_raw_parts(
            (table as *const u8).add(offset) as *const MibIpForwardRow,
            count,
        )
    };
    Ok(rows.to_vec())
}

fn create_forward_entry(row: &MibIpForwardRow) -> u32 {
    unsafe { CreateIpForwardEntry(row) }
}

fn delete_forward_entry(row: &MibIpForwardRow) -> u32 {
    unsafe { DeleteIpForwardEntry(row) }
}

/// 构造 IPv4 路由行；未使用的 Metric2-5 按 Windows 约定置 0xFFFFFFFF
fn build_forward_row(
    dest: Ipv4Addr,
    mask: Ipv4Addr,
    next_hop: Ipv4Addr,
    if_index: u32,
    metric: u32,
) -> MibIpForwardRow {
    MibIpForwardRow {
        dest: ipv4_dword(dest),
        mask: ipv4_dword(mask),
        policy: 0,
        next_hop: ipv4_dword(next_hop),
        if_index,
        route_type: if next_hop.is_unspecified() {
            MIB_IPROUTE_TYPE_DIRECT
        } else {
            MIB_IPROUTE_TYPE_INDIRECT
        },
        proto: MIB_IPPROTO_NETMGMT,
        age: 0,
        next_hop_as: 0,
        metric1: metric,
        metric2: u32::MAX,
        metric3: u32::MAX,
        metric4: u32::MAX,
        metric5: u32::MAX,
    }
}

fn route_ipv4(route: &RouteEntry) -> Option<(Ipv4Addr, Ipv4Addr)> {
    Some((route.dest.parse().ok()?, route.mask.parse().ok()?))
}

fn row_matches(
    row: &MibIpForwardRow,
    dest: Ipv4Addr,
    mask: Ipv4Addr,
    next_hop: Ipv4Addr,
    if_index: u32,
) -> bool {
    row.dest == ipv4_dword(dest)
        && row.mask == ipv4_dword(mask)
        && row.next_hop == ipv4_dword(next_hop)
        && row.if_index == if_index
}

fn row_matches_dest_mask(row: &MibIpForwardRow, dest: Ipv4Addr, mask: Ipv4Addr) -> bool {
    row.dest == ipv4_dword(dest) && row.mask == ipv4_dword(mask)
}

/// 判断是否为接口自动生成的直连/本机/广播路由
fn interface_route_matches(
    row: &MibIpForwardRow,
    vip: Ipv4Addr,
    mask: Ipv4Addr,
    if_index: u32,
) -> bool {
    if row.if_index != if_index || row.next_hop != ipv4_dword(Ipv4Addr::UNSPECIFIED) {
        return false;
    }
    let vip_value = u32::from(vip);
    let mask_value = u32::from(mask);
    let network = Ipv4Addr::from(vip_value & mask_value);
    let broadcast = Ipv4Addr::from((vip_value & mask_value) | !mask_value);
    let host_mask = Ipv4Addr::new(255, 255, 255, 255);
    let dest = dword_ipv4(row.dest);
    let row_mask = dword_ipv4(row.mask);
    (dest == network && row_mask == mask)
        || (dest == vip && row_mask == host_mask)
        || (dest == broadcast && row_mask == host_mask)
}

/// Windows IP 辅助 API 使用网络字节序的 DWORD；按内存字节写入
fn ipv4_dword(ip: Ipv4Addr) -> u32 {
    u32::from_ne_bytes(ip.octets())
}

fn dword_ipv4(value: u32) -> Ipv4Addr {
    Ipv4Addr::from(value.to_ne_bytes())
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
    fn 路由行_字段与网络字节序正确() {
        let dest = Ipv4Addr::new(198, 51, 100, 0);
        let mask = Ipv4Addr::new(255, 255, 255, 0);
        let hop = Ipv4Addr::new(198, 51, 100, 107);
        let row = build_forward_row(dest, mask, hop, 17, WINDOWS_ROUTE_METRIC);

        assert_eq!(dword_ipv4(row.dest), dest);
        assert_eq!(dword_ipv4(row.mask), mask);
        assert_eq!(dword_ipv4(row.next_hop), hop);
        assert_eq!(row.if_index, 17);
        assert_eq!(row.metric1, 1);
        assert_eq!(row.metric2, u32::MAX);
        assert_eq!(row.metric3, u32::MAX);
        assert_eq!(row.metric4, u32::MAX);
        assert_eq!(row.metric5, u32::MAX);
        assert_eq!(row.route_type, MIB_IPROUTE_TYPE_INDIRECT);
        assert_eq!(row.proto, MIB_IPPROTO_NETMGMT);
        assert!(row_matches(&row, dest, mask, hop, 17));
        assert!(!row_matches(&row, dest, mask, hop, 18));
    }

    #[test]
    fn 路由行_onlink_使用直连类型() {
        let dest = Ipv4Addr::new(203, 0, 113, 0);
        let mask = Ipv4Addr::new(255, 255, 255, 0);
        let row = build_forward_row(dest, mask, Ipv4Addr::UNSPECIFIED, 17, 1);

        assert_eq!(row.route_type, MIB_IPROUTE_TYPE_DIRECT);
        assert_eq!(dword_ipv4(row.next_hop), Ipv4Addr::UNSPECIFIED);
    }

    #[test]
    fn 接口直连路由_识别() {
        let vip = Ipv4Addr::new(198, 51, 100, 10);
        let mask = Ipv4Addr::new(255, 255, 255, 0);
        let network = Ipv4Addr::new(198, 51, 100, 0);
        let broadcast = Ipv4Addr::new(198, 51, 100, 255);
        let host_mask = Ipv4Addr::new(255, 255, 255, 255);

        let net_row = build_forward_row(network, mask, Ipv4Addr::UNSPECIFIED, 17, 1);
        assert!(interface_route_matches(&net_row, vip, mask, 17));

        let vip_row = build_forward_row(vip, host_mask, Ipv4Addr::UNSPECIFIED, 17, 1);
        assert!(interface_route_matches(&vip_row, vip, mask, 17));

        let broadcast_row = build_forward_row(broadcast, host_mask, Ipv4Addr::UNSPECIFIED, 17, 1);
        assert!(interface_route_matches(&broadcast_row, vip, mask, 17));

        let gateway_row = build_forward_row(network, mask, Ipv4Addr::new(198, 51, 100, 1), 17, 1);
        assert!(!interface_route_matches(&gateway_row, vip, mask, 17));
    }

    #[test]
    fn 路由表_可读取() {
        let table = ip_forward_table();
        assert!(table.is_ok(), "GetIpForwardTable failed: {:?}", table.err());
    }

    #[test]
    fn 路由_解析目标与掩码() {
        let route = route_entry("198.51.100.0", "255.255.255.0", 24);
        assert_eq!(
            route_ipv4(&route),
            Some((
                Ipv4Addr::new(198, 51, 100, 0),
                Ipv4Addr::new(255, 255, 255, 0)
            ))
        );
        assert_eq!(route_ipv4(&route_entry("bad", "255.255.255.0", 24)), None);
    }

    #[test]
    fn 路由账本_重复写入不增加条数并升级归属() {
        let mut net = NetConfig::new("topsap0");
        let route = route_entry("10.0.0.0", "255.0.0.0", 8);
        let hop = Ipv4Addr::new(10, 0, 0, 1);

        net.note_route(&route, hop, 17, true);
        net.note_route(&route, hop, 17, true);
        assert_eq!(net.route_count(), 1);

        net.note_route(&route, hop, 17, false);
        assert_eq!(net.route_count(), 1);
        assert!(matches!(
            net.ledger[0],
            LedgerEntry::Route {
                preexisting: false,
                ..
            }
        ));
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
