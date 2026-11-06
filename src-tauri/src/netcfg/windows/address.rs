//! Windows 网卡 IPv4 地址：IPHLPAPI 直写、静态配置持久化与回读。

use std::net::Ipv4Addr;

use super::{ipv4_dword, NO_ERROR};

const ERROR_NOT_FOUND: u32 = 1168;
const ERROR_BUFFER_OVERFLOW: u32 = 111;
const AF_INET: u16 = 2;
const NL_PREFIX_ORIGIN_MANUAL: u32 = 1;
const NL_SUFFIX_ORIGIN_MANUAL: u32 = 1;
const INFINITE_LIFETIME: u32 = 0xFFFF_FFFF;
const GAA_FLAG_SKIP_ANYCAST: u32 = 0x2;
const GAA_FLAG_SKIP_MULTICAST: u32 = 0x4;
const GAA_FLAG_SKIP_DNS_SERVER: u32 = 0x8;
const TCPIP_INTERFACES: &str = r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces";

/// Windows `SOCKADDR_INET` 的 IPv4 形态；总长 28 字节，与 `SOCKADDR_IN6` 对齐
#[repr(C)]
#[derive(Clone, Copy)]
#[allow(dead_code)]
struct SockaddrInet {
    family: u16,
    port: u16,
    addr: u32,
    reserved: [u8; 20],
}

/// Windows `MIB_UNICASTIPADDRESS_ROW`
#[repr(C)]
#[derive(Clone, Copy)]
#[allow(dead_code)]
struct MibUnicastIpAddressRow {
    address: SockaddrInet,
    interface_luid: u64,
    interface_index: u32,
    prefix_origin: u32,
    suffix_origin: u32,
    valid_lifetime: u32,
    preferred_lifetime: u32,
    on_link_prefix_length: u8,
    skip_as_source: u8,
    dad_state: u32,
    scope_id: u32,
    creation_time_stamp: u32,
}

#[repr(C)]
struct MibUnicastIpAddressTable {
    num_entries: u32,
    table: [MibUnicastIpAddressRow; 1],
}

/// `IP_ADAPTER_ADDRESSES` 的前缀；只读取遍历所需的字段
#[repr(C)]
#[allow(dead_code)]
struct IpAdapterAddresses {
    length: u32,
    if_index: u32,
    next: *mut IpAdapterAddresses,
    adapter_name: *const u8,
}

#[allow(non_snake_case)]
#[link(name = "iphlpapi")]
extern "system" {
    fn GetUnicastIpAddressTable(family: u16, table: *mut *mut MibUnicastIpAddressTable) -> u32;
    fn CreateUnicastIpAddressEntry(row: *const MibUnicastIpAddressRow) -> u32;
    fn DeleteUnicastIpAddressEntry(row: *const MibUnicastIpAddressRow) -> u32;
    fn InitializeUnicastIpAddressEntry(row: *mut MibUnicastIpAddressRow);
    fn FreeMibTable(memory: *const std::ffi::c_void);
    fn GetAdaptersAddresses(
        family: u32,
        flags: u32,
        reserved: *mut std::ffi::c_void,
        addresses: *mut IpAdapterAddresses,
        size: *mut u32,
    ) -> u32;
}

/// 读取 IPv4 单播地址表；失败返回 Win32 错误码
fn unicast_ipv4_table() -> Result<Vec<MibUnicastIpAddressRow>, u32> {
    let mut table: *mut MibUnicastIpAddressTable = std::ptr::null_mut();
    let code = unsafe { GetUnicastIpAddressTable(AF_INET, &mut table) };
    if code != NO_ERROR {
        return Err(code);
    }
    if table.is_null() {
        return Ok(Vec::new());
    }

    let count = unsafe { (*table).num_entries as usize };
    let offset = std::mem::offset_of!(MibUnicastIpAddressTable, table);
    let rows = unsafe {
        std::slice::from_raw_parts(
            (table as *const u8).add(offset) as *const MibUnicastIpAddressRow,
            count,
        )
    };
    let out = rows.to_vec();
    unsafe { FreeMibTable(table as *const std::ffi::c_void) };
    Ok(out)
}

/// 构造手动配置的 IPv4 地址行
fn build_unicast_row(index: u32, vip: Ipv4Addr, prefix: u8) -> MibUnicastIpAddressRow {
    let mut row: MibUnicastIpAddressRow = unsafe { std::mem::zeroed() };
    unsafe { InitializeUnicastIpAddressEntry(&mut row) };
    row.address.family = AF_INET;
    row.address.addr = ipv4_dword(vip);
    row.interface_index = index;
    row.prefix_origin = NL_PREFIX_ORIGIN_MANUAL;
    row.suffix_origin = NL_SUFFIX_ORIGIN_MANUAL;
    row.valid_lifetime = INFINITE_LIFETIME;
    row.preferred_lifetime = INFINITE_LIFETIME;
    row.on_link_prefix_length = prefix;
    row
}

fn unicast_row_ipv4(row: &MibUnicastIpAddressRow) -> Ipv4Addr {
    Ipv4Addr::from(row.address.addr.to_ne_bytes())
}

/// 通过 IPHLPAPI 写入 IPv4 地址；失败返回 Win32 错误码
pub(super) fn set_unicast_address(index: u32, vip: Ipv4Addr, prefix: u8) -> Result<(), u32> {
    let table = unicast_ipv4_table()?;
    if table
        .iter()
        .any(|row| row.interface_index == index && unicast_row_ipv4(row) == vip)
    {
        return Ok(());
    }

    for row in &table {
        if row.interface_index == index && row.address.family == AF_INET {
            let _ = unsafe { DeleteUnicastIpAddressEntry(row) };
        }
    }

    let row = build_unicast_row(index, vip, prefix);
    let code = unsafe { CreateUnicastIpAddressEntry(&row) };
    if code != NO_ERROR {
        return Err(code);
    }

    let after = unicast_ipv4_table()?;
    if after
        .iter()
        .any(|row| row.interface_index == index && unicast_row_ipv4(row) == vip)
    {
        Ok(())
    } else {
        Err(ERROR_NOT_FOUND)
    }
}

/// 按接口索引读取网卡 GUID（`{...}`），用于定位 Tcpip 配置键
fn adapter_guid_by_index(index: u32) -> Option<String> {
    let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
    let mut size = 0u32;
    let code = unsafe {
        GetAdaptersAddresses(
            AF_INET as u32,
            flags,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut size,
        )
    };
    if code != ERROR_BUFFER_OVERFLOW && code != NO_ERROR {
        return None;
    }
    if size == 0 {
        return None;
    }

    let mut buffer = vec![0u8; size as usize];
    let code = unsafe {
        GetAdaptersAddresses(
            AF_INET as u32,
            flags,
            std::ptr::null_mut(),
            buffer.as_mut_ptr() as *mut IpAdapterAddresses,
            &mut size,
        )
    };
    if code != NO_ERROR {
        return None;
    }

    let mut current = buffer.as_ptr() as *const IpAdapterAddresses;
    while !current.is_null() {
        let row = unsafe { &*current };
        if row.if_index == index {
            return unsafe { read_c_string(row.adapter_name) };
        }
        current = row.next;
    }
    None
}

unsafe fn read_c_string(ptr: *const u8) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let mut bytes = Vec::new();
    for offset in 0..256 {
        let byte = unsafe { *ptr.add(offset) };
        if byte == 0 {
            return Some(String::from_utf8_lossy(&bytes).into_owned());
        }
        bytes.push(byte);
    }
    None
}

/// 把静态 IP 与掩码写入网卡的持久配置；仅用于 IPv4 属性窗口展示
pub(super) fn persist_static_address(index: u32, vip: Ipv4Addr, mask: &str) -> Result<(), String> {
    let guid = adapter_guid_by_index(index)
        .ok_or_else(|| format!("未找到接口索引 {index} 对应的网卡 GUID"))?;
    let path = format!(r"{TCPIP_INTERFACES}\{guid}");
    let key = windows_registry::LOCAL_MACHINE
        .create(&path)
        .map_err(|err| format!("打开注册表失败：{err}"))?;
    let vip_text = vip.to_string();
    key.set_u32("EnableDHCP", 0)
        .map_err(|err| format!("写入 EnableDHCP 失败：{err}"))?;
    key.set_multi_string("IPAddress", &[vip_text.as_str()])
        .map_err(|err| format!("写入 IPAddress 失败：{err}"))?;
    key.set_multi_string("SubnetMask", &[mask])
        .map_err(|err| format!("写入 SubnetMask 失败：{err}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 单播地址行_内存布局正确() {
        assert_eq!(std::mem::size_of::<MibUnicastIpAddressRow>(), 80);
        assert_eq!(
            std::mem::offset_of!(MibUnicastIpAddressRow, interface_index),
            40
        );
        assert_eq!(
            std::mem::offset_of!(MibUnicastIpAddressRow, on_link_prefix_length),
            60
        );
        assert_eq!(std::mem::offset_of!(MibUnicastIpAddressRow, dad_state), 64);
        assert_eq!(std::mem::offset_of!(MibUnicastIpAddressTable, table), 8);
    }

    #[test]
    fn 单播地址行_字段与网络字节序正确() {
        let vip = Ipv4Addr::new(198, 51, 100, 10);
        let row = build_unicast_row(17, vip, 24);

        assert_eq!(row.address.family, AF_INET);
        assert_eq!(unicast_row_ipv4(&row), vip);
        assert_eq!(row.interface_index, 17);
        assert_eq!(row.on_link_prefix_length, 24);
        assert_eq!(row.prefix_origin, NL_PREFIX_ORIGIN_MANUAL);
        assert_eq!(row.suffix_origin, NL_SUFFIX_ORIGIN_MANUAL);
        assert_eq!(row.valid_lifetime, INFINITE_LIFETIME);
        assert_eq!(row.preferred_lifetime, INFINITE_LIFETIME);
    }

    #[test]
    fn 单播地址表_可读取() {
        let table = unicast_ipv4_table().expect("GetUnicastIpAddressTable failed");
        for row in &table {
            assert_eq!(row.address.family, AF_INET);
            assert!(row.on_link_prefix_length <= 32);
        }
    }

    #[test]
    fn 网卡_guid_可读取() {
        let table = unicast_ipv4_table().expect("GetUnicastIpAddressTable failed");
        let guid = table
            .iter()
            .find_map(|row| adapter_guid_by_index(row.interface_index));
        assert!(
            guid.as_deref().is_some_and(|value| value.starts_with('{')),
            "adapter GUID not resolved"
        );
    }
}
