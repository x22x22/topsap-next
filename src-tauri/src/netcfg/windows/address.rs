//! Windows 网卡 IPv4 地址：IPHLPAPI 直写与回读。

use std::net::Ipv4Addr;

use super::{ipv4_dword, NO_ERROR};

const ERROR_NOT_FOUND: u32 = 1168;
const AF_INET: u16 = 2;
const NL_PREFIX_ORIGIN_MANUAL: u32 = 1;
const NL_SUFFIX_ORIGIN_MANUAL: u32 = 1;
const INFINITE_LIFETIME: u32 = 0xFFFF_FFFF;

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

#[allow(non_snake_case)]
#[link(name = "iphlpapi")]
extern "system" {
    fn GetUnicastIpAddressTable(family: u16, table: *mut *mut MibUnicastIpAddressTable) -> u32;
    fn CreateUnicastIpAddressEntry(row: *const MibUnicastIpAddressRow) -> u32;
    fn DeleteUnicastIpAddressEntry(row: *const MibUnicastIpAddressRow) -> u32;
    fn InitializeUnicastIpAddressEntry(row: *mut MibUnicastIpAddressRow);
    fn FreeMibTable(memory: *const std::ffi::c_void);
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
}
