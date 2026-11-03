//! 网关返回的是「伪 JSON」：键名不带引号。这里做容错解析。

use serde_json::Value;

/// 解析伪 JSON：先按标准 JSON 尝试，失败后给裸键名补引号再试。
pub fn loose_parse(text: &str) -> Option<Value> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end <= start {
        return None;
    }
    let body = &text[start..=end];

    if let Ok(v) = serde_json::from_str::<Value>(body) {
        return Some(v);
    }
    let patched = quote_bare_keys(body);
    serde_json::from_str::<Value>(&patched).ok()
}

/// 给字符串外的裸键名补引号，并去掉尾随逗号。
///
/// 只处理 `{` 与 `,` 之后、形如 `key:` 的位置；字符串内部原样保留。
pub fn quote_bare_keys(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len() + 32);
    let mut i = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    while i < chars.len() {
        let c = chars[i];

        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }

        match c {
            '"' => {
                in_string = true;
                out.push(c);
                i += 1;
            }
            '{' | ',' => {
                out.push(c);

                let mut j = i + 1;
                while j < chars.len() && chars[j].is_whitespace() {
                    j += 1;
                }

                if j < chars.len() && (chars[j].is_ascii_alphabetic() || chars[j] == '_') {
                    let mut k = j;
                    while k < chars.len() && (chars[k].is_ascii_alphanumeric() || chars[k] == '_') {
                        k += 1;
                    }
                    let mut m = k;
                    while m < chars.len() && chars[m].is_whitespace() {
                        m += 1;
                    }

                    if m < chars.len() && chars[m] == ':' {
                        // 保留原有空白，再插入带引号的键
                        for ch in &chars[i + 1..j] {
                            out.push(*ch);
                        }
                        out.push('"');
                        out.extend(chars[j..k].iter());
                        out.push('"');
                        i = k;
                        continue;
                    }
                }
                i += 1;
            }
            ']' | '}' => {
                while matches!(out.chars().last(), Some(' ' | '\t' | '\r' | '\n')) {
                    out.pop();
                }
                if out.ends_with(',') {
                    out.pop();
                }
                out.push(c);
                i += 1;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 标准_json_直接解析() {
        let v = loose_parse(r#"{"Ret":0,"ErrMsg":""}"#).expect("解析");
        assert_eq!(v["Ret"], 0);
    }

    #[test]
    fn 裸键名补引号后可解析() {
        let text = r#"{Ret:0,ErrMsg:"",user_param:{session_id:"470&abc"}}"#;
        let v = loose_parse(text).expect("解析");
        assert_eq!(v["Ret"], 0);
        assert_eq!(v["user_param"]["session_id"], "470&abc");
    }

    #[test]
    fn 字符串内部的冒号与逗号不受影响() {
        let text = r#"{Ret:0,ErrMsg:"a:b,c",nested:{k:1}}"#;
        let v = loose_parse(text).expect("解析");
        assert_eq!(v["ErrMsg"], "a:b,c");
        assert_eq!(v["nested"]["k"], 1);
    }

    #[test]
    fn 去除尾随逗号() {
        let text = r#"{Ret:0,"list":[1,2,3,],}"#;
        let v = loose_parse(text).expect("解析");
        assert_eq!(v["list"][2], 3);
    }

    #[test]
    fn 前后夹杂非_json_文本也能截取() {
        let text = "some log line\n{\"Ret\":0}\ntrailing";
        let v = loose_parse(text).expect("解析");
        assert_eq!(v["Ret"], 0);
    }

    #[test]
    fn 完全没有对象时返回_none() {
        assert!(loose_parse("not json at all").is_none());
        assert!(loose_parse("{}").is_some());
    }

    #[test]
    fn 转义引号不会提前结束字符串() {
        let text = r#"{ErrMsg:"say \"hi\"",Ret:0}"#;
        let v = loose_parse(text).expect("解析");
        assert!(v["ErrMsg"].as_str().unwrap().contains("hi"));
    }
}
