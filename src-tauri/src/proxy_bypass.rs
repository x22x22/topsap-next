//! 系统代理绕过列表：让指定域名/泛域名/IP 不经过本地代理。

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::dto::{AppSettings, ProxyBypassStatus};

pub const DEFAULT_ENTRIES: [&str; 2] = ["*.sunline.cn", "*.sunline-i.com"];
const MAX_ENTRIES: usize = 128;
const MAX_LEN: usize = 255;

#[derive(Debug, Default, Serialize, Deserialize)]
struct Ledger {
    #[serde(default)]
    applied: BTreeMap<String, Vec<String>>,
}

fn ledger_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(crate::paths::config_dir(app)?.join("proxy_bypass.json"))
}

pub fn default_entries() -> Vec<String> {
    DEFAULT_ENTRIES
        .iter()
        .map(|item| item.to_string())
        .collect()
}

fn normalize_one(raw: &str) -> Result<String, String> {
    let entry = raw.trim().to_ascii_lowercase();
    if entry.is_empty() {
        return Err("代理绕过条目不能为空".to_string());
    }
    if entry.len() > MAX_LEN {
        return Err(format!("代理绕过条目过长：{entry}"));
    }
    if entry == "*" {
        return Err("不能使用单独的 * 绕过所有代理".to_string());
    }
    if entry
        .chars()
        .any(|ch| ch.is_whitespace() || matches!(ch, ';' | ',' | '/' | '\\' | '=' | '"' | '\''))
    {
        return Err(format!("代理绕过条目包含非法字符：{entry}"));
    }
    Ok(entry)
}

pub fn normalize_entries(input: &[String]) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for raw in input {
        let entry = normalize_one(raw)?;
        if !out.iter().any(|item| item == &entry) {
            out.push(entry);
        }
    }
    if out.len() > MAX_ENTRIES {
        return Err(format!("代理绕过条目不能超过 {MAX_ENTRIES} 条"));
    }
    Ok(out)
}

/// 清洗配置中的非法条目，避免坏配置阻断应用启动。
pub fn sanitize_entries(input: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for raw in input {
        let Ok(entry) = normalize_one(raw) else {
            continue;
        };
        if !out.iter().any(|item| item == &entry) {
            out.push(entry);
        }
        if out.len() == MAX_ENTRIES {
            break;
        }
    }
    out
}

/// 移除上次托管的条目，再追加本次条目；返回合并结果与本次真正新增的条目。
fn merge_entries(
    current: &[String],
    previous: &[String],
    desired: &[String],
) -> (Vec<String>, Vec<String>) {
    let mut entries: Vec<String> = current
        .iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect();
    let previous: Vec<String> = previous
        .iter()
        .map(|item| item.trim().to_ascii_lowercase())
        .filter(|item| !item.is_empty())
        .collect();
    entries.retain(|item| !previous.iter().any(|old| old.eq_ignore_ascii_case(item)));

    let mut managed = Vec::new();
    for entry in desired {
        if entries.iter().any(|item| item.eq_ignore_ascii_case(entry)) {
            if previous.iter().any(|old| old.eq_ignore_ascii_case(entry)) {
                managed.push(entry.clone());
            }
        } else {
            entries.push(entry.clone());
            managed.push(entry.clone());
        }
    }
    (entries, managed)
}

fn contains_entry(list: &[String], entry: &str) -> bool {
    list.iter().any(|item| item.eq_ignore_ascii_case(entry))
}

fn read_ledger(app: &AppHandle) -> Ledger {
    let Ok(path) = ledger_path(app) else {
        return Ledger::default();
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return Ledger::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

fn write_ledger(app: &AppHandle, applied: BTreeMap<String, Vec<String>>) -> Result<(), String> {
    let path = ledger_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建配置目录失败：{e}"))?;
    }
    let text = serde_json::to_string_pretty(&Ledger { applied })
        .map_err(|e| format!("序列化代理绕过记录失败：{e}"))?;
    std::fs::write(path, text).map_err(|e| format!("写入代理绕过记录失败：{e}"))
}

pub fn apply(app: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    if !platform::supported() {
        return Ok(());
    }
    let entries = normalize_entries(&settings.proxy_bypass)?;
    let desired = if settings.proxy_bypass_enabled {
        entries
    } else {
        Vec::new()
    };
    let previous = read_ledger(app).applied;
    let current = platform::read_all()?;
    let mut updates = Vec::new();
    let mut managed = BTreeMap::new();
    for (scope, list) in current {
        let previous_list = previous.get(&scope).cloned().unwrap_or_default();
        let (merged, scope_managed) = merge_entries(&list, &previous_list, &desired);
        if merged != list {
            updates.push((scope.clone(), merged));
        }
        if !scope_managed.is_empty() {
            managed.insert(scope, scope_managed);
        }
    }
    platform::write_all(&updates)?;
    write_ledger(app, managed)
}

pub fn status(app: &AppHandle) -> ProxyBypassStatus {
    let settings = crate::settings::load(app);
    let defaults = default_entries();
    if !platform::supported() {
        return ProxyBypassStatus {
            supported: false,
            enabled: settings.proxy_bypass_enabled,
            entries: settings.proxy_bypass,
            defaults,
            applied: false,
            message: "当前系统不支持或未检测到可用的系统代理设置".to_string(),
        };
    }
    let current = platform::read_all().unwrap_or_default();
    let entries = settings.proxy_bypass.clone();
    let applied = !settings.proxy_bypass_enabled
        || (!entries.is_empty()
            && current
                .iter()
                .all(|(_, list)| entries.iter().all(|entry| contains_entry(list, entry))));
    let message = if !settings.proxy_bypass_enabled {
        "代理绕过已关闭".to_string()
    } else if entries.is_empty() {
        "未设置代理绕过条目".to_string()
    } else if applied {
        format!("已写入系统代理绕过列表（{} 条）", entries.len())
    } else {
        "尚未写入系统代理绕过列表，保存设置后重试".to_string()
    };
    ProxyBypassStatus {
        supported: true,
        enabled: settings.proxy_bypass_enabled,
        entries,
        defaults,
        applied,
        message,
    }
}

#[cfg(windows)]
mod platform {
    use windows_registry::CURRENT_USER;
    use windows_sys::Win32::Networking::WinInet::{
        InternetSetOptionW, INTERNET_OPTION_REFRESH, INTERNET_OPTION_SETTINGS_CHANGED,
    };

    const KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";
    const SCOPE: &str = "windows";

    pub fn supported() -> bool {
        true
    }

    pub fn read_all() -> Result<Vec<(String, Vec<String>)>, String> {
        let key = CURRENT_USER
            .open(KEY)
            .map_err(|e| format!("打开系统代理设置失败：{}", e.message()))?;
        let value = key.get_string("ProxyOverride").unwrap_or_default();
        let list = value
            .split(';')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_string)
            .collect();
        Ok(vec![(SCOPE.to_string(), list)])
    }

    pub fn write_all(updates: &[(String, Vec<String>)]) -> Result<(), String> {
        for (_, entries) in updates {
            let key = CURRENT_USER
                .create(KEY)
                .map_err(|e| format!("打开系统代理设置失败：{}", e.message()))?;
            let value = entries.join(";");
            if value.is_empty() {
                let _ = key.remove_value("ProxyOverride");
            } else {
                key.set_string("ProxyOverride", value)
                    .map_err(|e| format!("写入系统代理绕过列表失败：{}", e.message()))?;
            }
        }
        unsafe {
            InternetSetOptionW(
                std::ptr::null(),
                INTERNET_OPTION_SETTINGS_CHANGED,
                std::ptr::null(),
                0,
            );
            InternetSetOptionW(
                std::ptr::null(),
                INTERNET_OPTION_REFRESH,
                std::ptr::null(),
                0,
            );
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::process::Command;

    fn run(program: &str, args: &[&str]) -> Result<String, String> {
        let output = Command::new(program)
            .args(args)
            .output()
            .map_err(|e| format!("执行 {program} 失败：{e}"))?;
        if !output.status.success() {
            return Err(format!(
                "{program} 返回错误：{}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn looks_like_entry(line: &str) -> bool {
        !line.is_empty()
            && line.chars().all(|ch| {
                ch.is_ascii_alphanumeric()
                    || matches!(
                        ch,
                        '.' | '-' | '_' | '*' | '?' | ':' | '[' | ']' | '<' | '>'
                    )
            })
    }

    fn services() -> Result<Vec<String>, String> {
        let text = run("networksetup", &["-listallnetworkservices"])?;
        let services: Vec<String> = text
            .lines()
            .skip(1)
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('*'))
            .map(str::to_string)
            .collect();
        if services.is_empty() {
            return Err("未找到可用的网络服务".to_string());
        }
        Ok(services)
    }

    pub fn supported() -> bool {
        true
    }

    pub fn read_all() -> Result<Vec<(String, Vec<String>)>, String> {
        let mut out = Vec::new();
        for service in services()? {
            let text = run(
                "networksetup",
                &["-getproxybypassdomains", service.as_str()],
            )?;
            let entries = text
                .lines()
                .map(str::trim)
                .filter(|line| looks_like_entry(line))
                .map(str::to_string)
                .collect();
            out.push((service, entries));
        }
        Ok(out)
    }

    pub fn write_all(updates: &[(String, Vec<String>)]) -> Result<(), String> {
        for (service, entries) in updates {
            let mut args = vec!["-setproxybypassdomains", service.as_str()];
            if entries.is_empty() {
                args.push("Empty");
            } else {
                args.extend(entries.iter().map(String::as_str));
            }
            run("networksetup", &args)?;
        }
        Ok(())
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod platform {
    use std::process::Command;

    const SCHEMA: &str = "org.gnome.system.proxy";
    const KEY: &str = "ignore-hosts";
    const SCOPE: &str = "gnome";

    fn gsettings_command() -> Command {
        if unsafe { libc::geteuid() } == 0 {
            if let (Ok(user), Ok(uid)) = (std::env::var("SUDO_USER"), std::env::var("SUDO_UID")) {
                if !user.is_empty() && !uid.is_empty() {
                    let bus = format!("DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/{uid}/bus");
                    let runtime = format!("XDG_RUNTIME_DIR=/run/user/{uid}");
                    let mut command = Command::new("sudo");
                    command.args([
                        "-u",
                        user.as_str(),
                        "env",
                        bus.as_str(),
                        runtime.as_str(),
                        "gsettings",
                    ]);
                    return command;
                }
            }
        }
        Command::new("gsettings")
    }

    fn run_gsettings(args: &[&str]) -> Result<String, String> {
        let output = gsettings_command()
            .args(args)
            .output()
            .map_err(|e| format!("执行 gsettings 失败：{e}"))?;
        if !output.status.success() {
            return Err(format!(
                "gsettings 返回错误：{}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn parse_list(text: &str) -> Vec<String> {
        let chars: Vec<char> = text.chars().collect();
        let mut out = Vec::new();
        let mut index = 0;
        while index < chars.len() {
            if chars[index] != '\'' {
                index += 1;
                continue;
            }
            index += 1;
            let mut value = String::new();
            while index < chars.len() {
                match chars[index] {
                    '\\' if index + 1 < chars.len() => {
                        value.push(chars[index + 1]);
                        index += 2;
                    }
                    '\'' => {
                        index += 1;
                        break;
                    }
                    ch => {
                        value.push(ch);
                        index += 1;
                    }
                }
            }
            out.push(value);
        }
        out
    }

    fn format_list(entries: &[String]) -> String {
        let items: Vec<String> = entries
            .iter()
            .map(|entry| {
                let escaped = entry.replace('\\', "\\\\").replace('\'', "\\'");
                format!("'{escaped}'")
            })
            .collect();
        format!("[{}]", items.join(", "))
    }

    pub fn supported() -> bool {
        run_gsettings(&["get", SCHEMA, KEY]).is_ok()
    }

    pub fn read_all() -> Result<Vec<(String, Vec<String>)>, String> {
        let text = run_gsettings(&["get", SCHEMA, KEY])?;
        Ok(vec![(SCOPE.to_string(), parse_list(&text))])
    }

    pub fn write_all(updates: &[(String, Vec<String>)]) -> Result<(), String> {
        for (_, entries) in updates {
            let value = format_list(entries);
            run_gsettings(&["set", SCHEMA, KEY, value.as_str()])?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    #[test]
    fn 默认条目为两个泛域名() {
        assert_eq!(default_entries(), vec!["*.sunline.cn", "*.sunline-i.com"]);
    }

    #[test]
    fn 条目规范化为小写并去重() {
        let entries = list(&[" *.Sunline.CN ", "192.0.2.10", "*.sunline.cn"]);
        assert_eq!(
            normalize_entries(&entries).unwrap(),
            vec!["*.sunline.cn", "192.0.2.10"]
        );
    }

    #[test]
    fn 拒绝危险或非法条目() {
        assert!(normalize_entries(&list(&["*"])).is_err());
        assert!(normalize_entries(&list(&["a;b"])).is_err());
        assert!(normalize_entries(&list(&["http://example.com"])).is_err());
        assert!(normalize_entries(&list(&["a b"])).is_err());
    }

    #[test]
    fn 清洗配置时逐条丢弃非法值() {
        let entries = list(&["*.sunline.cn", "*", "a b", "192.0.2.10"]);
        assert_eq!(
            sanitize_entries(&entries),
            vec!["*.sunline.cn", "192.0.2.10"]
        );
    }

    #[test]
    fn 合并时保留用户已有条目并移除上次托管条目() {
        let (merged, managed) = merge_entries(
            &list(&[
                "localhost",
                "*.example.com",
                "*.sunline.cn",
                "*.old.example",
            ]),
            &list(&["*.sunline.cn", "*.old.example"]),
            &list(&["*.sunline.cn", "*.sunline-i.com"]),
        );
        assert_eq!(
            merged,
            vec![
                "localhost",
                "*.example.com",
                "*.sunline.cn",
                "*.sunline-i.com"
            ]
        );
        assert_eq!(managed, vec!["*.sunline.cn", "*.sunline-i.com"]);
    }

    #[test]
    fn 不接管用户原本已有的条目() {
        let (merged, managed) =
            merge_entries(&list(&["*.sunline.cn"]), &[], &list(&["*.sunline.cn"]));
        assert_eq!(merged, vec!["*.sunline.cn"]);
        assert!(managed.is_empty(), "原本已存在的条目不能记成托管");
    }

    #[test]
    fn 已托管条目继续保留() {
        let (_, managed) = merge_entries(
            &list(&["*.sunline.cn"]),
            &list(&["*.sunline.cn"]),
            &list(&["*.sunline.cn"]),
        );
        assert_eq!(managed, vec!["*.sunline.cn"]);
    }

    #[test]
    fn 关闭后只移除托管条目() {
        let (merged, managed) = merge_entries(
            &list(&["localhost", "*.example.com", "*.sunline.cn"]),
            &list(&["*.sunline.cn"]),
            &[],
        );
        assert_eq!(merged, vec!["localhost", "*.example.com"]);
        assert!(managed.is_empty());
    }
}
