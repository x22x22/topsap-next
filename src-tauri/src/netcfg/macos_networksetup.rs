//! macOS helper 的 networksetup / scutil 白名单参数校验（纯函数，便于单测）。

use crate::proxy_bypass;

/// 允许的 networksetup 子命令与参数形态
pub(crate) fn allowed_networksetup(args: &[String]) -> bool {
    match args.first().map(String::as_str) {
        Some("-listnetworkserviceorder") | Some("-listallnetworkservices") => args.len() == 1,
        Some("-getdnsservers") | Some("-getinfo") | Some("-getproxybypassdomains") => {
            args.len() == 2 && valid_service(&args[1])
        }
        Some("setdnsservers") => {
            args.len() >= 3
                && args.len() <= 10
                && valid_service(&args[1])
                && args[2..]
                    .iter()
                    .all(|value| value == "Empty" || value.parse::<std::net::IpAddr>().is_ok())
        }
        Some("-setproxybypassdomains") => {
            args.len() >= 3
                && args.len() <= proxy_bypass::MAX_ENTRIES + 2
                && valid_service(&args[1])
                && args[2..]
                    .iter()
                    .all(|value| value == "Empty" || proxy_bypass::is_safe_entry(value))
        }
        _ => false,
    }
}

/// scutil 只允许只读的 DNS 配置查询
pub(crate) fn allowed_scutil(args: &[String]) -> bool {
    args.len() == 1 && args[0] == "--dns"
}

fn valid_service(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && !value
            .chars()
            .any(|ch| ch == '\n' || ch == '\r' || ch == '\0')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|item| item.to_string()).collect()
    }

    #[test]
    fn 允许已知的读取命令() {
        assert!(allowed_networksetup(&args(&["-listallnetworkservices"])));
        assert!(allowed_networksetup(&args(&["-listnetworkserviceorder"])));
        assert!(allowed_networksetup(&args(&["-getdnsservers", "Wi-Fi"])));
        assert!(allowed_networksetup(&args(&[
            "-getproxybypassdomains",
            "Wi-Fi"
        ])));
    }

    #[test]
    fn 允许写入_dns_与代理绕过() {
        assert!(allowed_networksetup(&args(&[
            "setdnsservers",
            "Wi-Fi",
            "198.51.100.5",
            "2001:db8::1"
        ])));
        assert!(allowed_networksetup(&args(&[
            "-setproxybypassdomains",
            "Wi-Fi",
            "*.sunline.cn",
            "198.51.100.1"
        ])));
        assert!(allowed_networksetup(&args(&[
            "-setproxybypassdomains",
            "Wi-Fi",
            "Empty"
        ])));
    }

    #[test]
    fn 拒绝未列入的子命令() {
        assert!(!allowed_networksetup(&args(&[
            "-setwebproxy",
            "Wi-Fi",
            "198.51.100.1",
            "8080"
        ])));
        assert!(!allowed_networksetup(&args(&["-setproxybypassdomains"])));
    }

    #[test]
    fn 拒绝非法服务名与条目() {
        assert!(!allowed_networksetup(&args(&[
            "-getproxybypassdomains",
            "Wi-Fi\nbad"
        ])));
        assert!(!allowed_networksetup(&args(&[
            "-setproxybypassdomains",
            "Wi-Fi",
            "bad domain"
        ])));
        assert!(!allowed_networksetup(&args(&[
            "-setproxybypassdomains",
            "Wi-Fi",
            "*"
        ])));
        assert!(!allowed_networksetup(&args(&[
            "-setproxybypassdomains",
            "Wi-Fi",
            "a/b"
        ])));
    }

    #[test]
    fn 代理绕过条目数量上限() {
        let mut list = vec!["-setproxybypassdomains".to_string(), "Wi-Fi".to_string()];
        for index in 0..proxy_bypass::MAX_ENTRIES {
            list.push(format!("host{index}.example.com"));
        }
        assert!(allowed_networksetup(&list));
        list.push("extra.example.com".to_string());
        assert!(!allowed_networksetup(&list));
    }

    #[test]
    fn scutil_只允许_dns() {
        assert!(allowed_scutil(&args(&["--dns"])));
        assert!(!allowed_scutil(&args(&["--dns", "extra"])));
        assert!(!allowed_scutil(&args(&["--proxy"])));
        assert!(!allowed_scutil(&[]));
    }
}
