//! hosts 文件旧版标记段清理。
//!
//! 新版本不再写入 hosts；这里只保留读取与删除旧版本标记段的能力，
//! 清理时按标记定位，避免误删用户的其他条目。

#[cfg(windows)]
pub const HOSTS_PATH: &str = r"C:\Windows\System32\drivers\etc\hosts";
#[cfg(unix)]
pub const HOSTS_PATH: &str = "/etc/hosts";
pub const MARK_BEGIN: &str = "# >>> topsap-next begin >>>";
pub const MARK_END: &str = "# <<< topsap-next end <<<";

#[cfg(windows)]
const EOL: &str = "\r\n";
#[cfg(unix)]
const EOL: &str = "\n";

/// 删除标记段（含标记行本身），保留其余内容。
pub fn strip_block(content: &str) -> String {
    let Some(begin) = content.find(MARK_BEGIN) else {
        return content.to_string();
    };
    let Some(end_rel) = content[begin..].find(MARK_END) else {
        // 只有开头没有结尾：保留原文，交给人工处理，避免误删
        return content.to_string();
    };
    let end = begin + end_rel + MARK_END.len();

    let mut out = String::with_capacity(content.len());
    out.push_str(content[..begin].trim_end());
    out.push_str(&content[end..]);

    // 折叠多余空行
    let mut collapsed = String::with_capacity(out.len());
    let mut blank_run = 0;
    for line in out.split_inclusive('\n') {
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run > 1 {
                continue;
            }
        } else {
            blank_run = 0;
        }
        collapsed.push_str(line);
    }
    // 保证以单个换行结尾
    collapsed.trim_end().to_string() + EOL
}

/// 当前内容是否已有本客户端的标记段（用于启动时检测残留）。
pub fn has_block(content: &str) -> bool {
    content.contains(MARK_BEGIN)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# Copyright (c) 1993-2009 Microsoft Corp.\r\n\
127.0.0.1 localhost\r\n\
::1 localhost\r\n";

    fn sample_with_block() -> String {
        format!("{SAMPLE}\r\n{MARK_BEGIN}\r\n10.0.0.1 a.test\r\n{MARK_END}\r\n")
    }

    #[test]
    fn 清理后恢复原内容() {
        let cleaned = strip_block(&sample_with_block());
        assert!(!cleaned.contains(MARK_BEGIN));
        assert!(!cleaned.contains("a.test"));
        assert!(cleaned.contains("127.0.0.1 localhost"));
    }

    #[test]
    fn 只有开头标记时不改动内容() {
        // 缺少结束标记说明文件被外部改坏，此时删段风险高于保留
        let broken = format!("{SAMPLE}\r\n{MARK_BEGIN}\r\n10.0.0.1 a.test\r\n");
        assert_eq!(strip_block(&broken), broken);
    }

    #[test]
    fn 残留检测() {
        assert!(!has_block(SAMPLE));
        assert!(has_block(&sample_with_block()));
    }
}
