//! 运行环境探测：区分 Windows 7 与较新系统，供兼容分支使用。
//!
//! `GetVersionEx` 会受兼容模式影响，因此使用 `RtlGetVersion` 获取真实版本。

#[cfg(windows)]
mod imp {
    use std::sync::OnceLock;

    #[repr(C)]
    struct OsVersionInfoExW {
        size: u32,
        major: u32,
        minor: u32,
        build: u32,
        platform_id: u32,
        csd_version: [u16; 128],
        service_pack_major: u16,
        service_pack_minor: u16,
        suite_mask: u16,
        product_type: u8,
        reserved: u8,
    }

    #[link(name = "ntdll")]
    extern "system" {
        fn RtlGetVersion(info: *mut OsVersionInfoExW) -> i32;
    }

    pub fn is_windows7() -> bool {
        static CACHE: OnceLock<bool> = OnceLock::new();
        *CACHE.get_or_init(|| unsafe {
            let mut info: OsVersionInfoExW = std::mem::zeroed();
            info.size = std::mem::size_of::<OsVersionInfoExW>() as u32;
            if RtlGetVersion(&mut info) != 0 {
                return false;
            }
            info.major == 6 && info.minor == 1
        })
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn is_windows7() -> bool {
        false
    }
}

pub use imp::is_windows7;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 版本判断在任意平台都可调用() {
        let _ = is_windows7();
    }
}
