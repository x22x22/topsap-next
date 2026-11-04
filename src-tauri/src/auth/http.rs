//! 极简 HTTP/1.1 客户端（TLS 之上）。
//!
//! 只服务登录的四个固定接口，因此不引入通用 HTTP 依赖：
//! 每次请求新建连接并带 `Connection: close`，读满响应即关闭。
//! 支持 `Content-Length` 与 `Transfer-Encoding: chunked` 两种响应体。

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;

use super::error::AuthError;
use crate::tls;

/// 与官方客户端 3.6.6.19.64.1 保持一致；网关会按版本后缀选择账号/密码编码
pub const USER_AGENT: &str = "VONE-AUTH,WINDOWS,NULL,TOPCLINET,NULL,VERSION=3.6.6.19.64.1";
const MAX_HEADER_BYTES: usize = 64 * 1024;
const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub set_cookies: Vec<String>,
}

impl HttpResponse {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

pub struct HttpClient {
    host: String,
    port: u16,
    timeout: Duration,
    cookie: Option<String>,
}

impl HttpClient {
    pub fn new(host: impl Into<String>, port: u16, timeout: Duration) -> Self {
        Self {
            host: host.into(),
            port,
            timeout,
            cookie: None,
        }
    }

    pub fn set_cookie(&mut self, value: impl Into<String>) {
        self.cookie = Some(value.into());
    }

    /// 替换并返回旧 Cookie，供注销时临时切换身份后还原
    pub fn replace_cookie(&mut self, value: Option<String>) -> Option<String> {
        std::mem::replace(&mut self.cookie, value)
    }

    pub fn cookie(&self) -> Option<&str> {
        self.cookie.as_deref()
    }

    /// 读取指定 Cookie 的值；同名项按浏览器语义取最后一个。
    pub fn cookie_value(&self, name: &str) -> Option<String> {
        find_cookie(self.cookie.as_deref()?, name)
    }

    /// 合并写入 Cookie：同名覆盖，其余保留。
    pub fn upsert_cookies(&mut self, pairs: &[(&str, &str)]) {
        self.cookie = Some(upsert_cookie_string(self.cookie.as_deref(), pairs));
    }

    /// 删除指定 Cookie（原客户端短信登录不会携带 sv_un）。
    pub fn remove_cookie(&mut self, name: &str) {
        let Some(cookie) = self.cookie.take() else {
            return;
        };
        let kept: Vec<&str> = cookie
            .split(';')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .filter(|item| {
                item.split_once('=')
                    .map(|(key, _)| key != name)
                    .unwrap_or(true)
            })
            .collect();
        self.cookie = if kept.is_empty() {
            None
        } else {
            Some(kept.join("; "))
        };
    }

    pub async fn request(
        &mut self,
        method: &str,
        path: &str,
        body: Option<&[u8]>,
        extra_headers: &[(&str, &str)],
    ) -> Result<HttpResponse, AuthError> {
        let timeout = self.timeout;
        match tokio::time::timeout(
            timeout,
            self.request_inner(method, path, body, extra_headers),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(AuthError::Timeout(timeout.as_secs())),
        }
    }

    async fn request_inner(
        &mut self,
        method: &str,
        path: &str,
        body: Option<&[u8]>,
        extra_headers: &[(&str, &str)],
    ) -> Result<HttpResponse, AuthError> {
        let (mut stream, _ip) = tls::connect(&self.host, self.port).await?;

        let mut head = String::with_capacity(512);
        head.push_str(&format!("{method} {path} HTTP/1.1\r\n"));
        head.push_str(&format!("Host: {}\r\n", host_header(&self.host, self.port)));
        head.push_str(&format!("User-Agent: {USER_AGENT}\r\n"));
        head.push_str("Accept: */*\r\n");
        head.push_str("Accept-Encoding: identity\r\n");
        head.push_str("Connection: close\r\n");
        if let Some(cookie) = &self.cookie {
            head.push_str(&format!("Cookie: {cookie}\r\n"));
        }
        for (key, value) in extra_headers {
            head.push_str(&format!("{key}: {value}\r\n"));
        }
        if let Some(payload) = body {
            head.push_str(&format!("Content-Length: {}\r\n", payload.len()));
        }
        head.push_str("\r\n");

        stream
            .write_all(head.as_bytes())
            .await
            .map_err(|e| AuthError::Network(format!("发送请求失败：{e}")))?;
        if let Some(payload) = body {
            stream
                .write_all(payload)
                .await
                .map_err(|e| AuthError::Network(format!("发送请求体失败：{e}")))?;
        }
        stream
            .flush()
            .await
            .map_err(|e| AuthError::Network(format!("刷新请求失败：{e}")))?;

        let response = read_response(&mut stream).await?;
        self.absorb_cookies(&response.set_cookies);
        Ok(response)
    }

    fn absorb_cookies(&mut self, cookies: &[String]) {
        if cookies.is_empty() {
            return;
        }
        let mut jar: Vec<String> = self
            .cookie
            .take()
            .map(|c| {
                c.split(';')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        for cookie in cookies {
            let name = cookie.split('=').next().unwrap_or("").trim();
            if name.is_empty() {
                continue;
            }
            let prefix = format!("{name}=");
            jar.retain(|item| !item.starts_with(&prefix));
            jar.push(cookie.trim().to_string());
        }
        self.cookie = Some(jar.join("; "));
    }
}

fn host_header(host: &str, port: u16) -> String {
    // 标准端口省略端口号，非标准端口必须带上
    if port == 443 {
        host.to_string()
    } else {
        format!("{host}:{port}")
    }
}

async fn read_response(stream: &mut TlsStream<TcpStream>) -> Result<HttpResponse, AuthError> {
    let (head, rest) = read_head(stream).await?;

    let mut lines = head.split("\r\n");
    let status_line = lines.next().unwrap_or_default();
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or_else(|| {
            AuthError::BadResponse(format!("状态行无法解析：{}", truncate(status_line, 120)))
        })?;

    let mut headers = Vec::new();
    let mut set_cookies = Vec::new();
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_string();
            let value = value.trim().to_string();
            if key.eq_ignore_ascii_case("set-cookie") {
                let pair = value.split(';').next().unwrap_or("").trim().to_string();
                if !pair.is_empty() {
                    set_cookies.push(pair);
                }
            }
            headers.push((key, value));
        }
    }

    let chunked = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("transfer-encoding"))
        .map(|(_, v)| v.to_ascii_lowercase().contains("chunked"))
        .unwrap_or(false);

    let content_length = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.trim().parse::<usize>().ok());

    let body = if chunked {
        read_chunked(stream, rest).await?
    } else if let Some(len) = content_length {
        read_exact_body(stream, rest, len).await?
    } else {
        read_until_eof(stream, rest).await?
    };

    Ok(HttpResponse {
        status,
        headers,
        body,
        set_cookies,
    })
}

async fn read_head(stream: &mut TlsStream<TcpStream>) -> Result<(String, Vec<u8>), AuthError> {
    let mut buf = Vec::with_capacity(4096);
    let mut tmp = [0u8; 8192];

    loop {
        let n = stream
            .read(&mut tmp)
            .await
            .map_err(|e| AuthError::Network(format!("读取响应失败：{e}")))?;
        if n == 0 {
            return Err(AuthError::BadResponse(
                "连接在响应头完成前被关闭".to_string(),
            ));
        }
        buf.extend_from_slice(&tmp[..n]);

        if let Some(pos) = find_subsequence(&buf, b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buf[..pos]).into_owned();
            let rest = buf[pos + 4..].to_vec();
            return Ok((head, rest));
        }
        if buf.len() > MAX_HEADER_BYTES {
            return Err(AuthError::BadResponse("响应头超过 64 KB".to_string()));
        }
    }
}

async fn read_exact_body(
    stream: &mut TlsStream<TcpStream>,
    pending: Vec<u8>,
    len: usize,
) -> Result<Vec<u8>, AuthError> {
    if len > MAX_BODY_BYTES {
        return Err(AuthError::BadResponse(format!("响应体过大：{len} 字节")));
    }
    let mut out = Vec::with_capacity(len);
    out.extend_from_slice(&pending[..pending.len().min(len)]);

    let mut tmp = [0u8; 8192];
    while out.len() < len {
        let n = stream
            .read(&mut tmp)
            .await
            .map_err(|e| AuthError::Network(format!("读取响应体失败：{e}")))?;
        if n == 0 {
            return Err(AuthError::BadResponse(format!(
                "响应体不完整：期望 {len} 字节，实际 {}",
                out.len()
            )));
        }
        out.extend_from_slice(&tmp[..n]);
        if out.len() > MAX_BODY_BYTES {
            return Err(AuthError::BadResponse("响应体超过 8 MB".to_string()));
        }
    }
    out.truncate(len);
    Ok(out)
}

async fn read_until_eof(
    stream: &mut TlsStream<TcpStream>,
    pending: Vec<u8>,
) -> Result<Vec<u8>, AuthError> {
    let mut out = pending;
    let mut tmp = [0u8; 8192];
    loop {
        let n = stream
            .read(&mut tmp)
            .await
            .map_err(|e| AuthError::Network(format!("读取响应体失败：{e}")))?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(&tmp[..n]);
        if out.len() > MAX_BODY_BYTES {
            return Err(AuthError::BadResponse("响应体超过 8 MB".to_string()));
        }
    }
    Ok(out)
}

async fn read_chunked(
    stream: &mut TlsStream<TcpStream>,
    pending: Vec<u8>,
) -> Result<Vec<u8>, AuthError> {
    let mut reader = ChunkReader { stream, pending };
    let mut out = Vec::new();

    loop {
        let line = reader.read_line().await?;
        let size_text = line.split(';').next().unwrap_or("").trim();
        if size_text.is_empty() {
            continue;
        }
        let size = usize::from_str_radix(size_text, 16).map_err(|_| {
            AuthError::BadResponse(format!("chunk 长度无法解析：{}", truncate(&line, 60)))
        })?;
        if size == 0 {
            break;
        }
        if out.len() + size > MAX_BODY_BYTES {
            return Err(AuthError::BadResponse("响应体超过 8 MB".to_string()));
        }
        out.extend_from_slice(&reader.read_n(size).await?);
        // 每个 chunk 后跟 CRLF
        let _ = reader.read_n(2).await?;
    }
    Ok(out)
}

struct ChunkReader<'a> {
    stream: &'a mut TlsStream<TcpStream>,
    pending: Vec<u8>,
}

impl ChunkReader<'_> {
    async fn fill(&mut self) -> Result<bool, AuthError> {
        let mut tmp = [0u8; 8192];
        let n = self
            .stream
            .read(&mut tmp)
            .await
            .map_err(|e| AuthError::Network(format!("读取 chunk 失败：{e}")))?;
        if n == 0 {
            return Ok(false);
        }
        self.pending.extend_from_slice(&tmp[..n]);
        Ok(true)
    }

    async fn read_line(&mut self) -> Result<String, AuthError> {
        loop {
            if let Some(pos) = find_subsequence(&self.pending, b"\r\n") {
                let line = String::from_utf8_lossy(&self.pending[..pos]).into_owned();
                self.pending.drain(..pos + 2);
                return Ok(line);
            }
            if self.pending.len() > 4096 {
                return Err(AuthError::BadResponse("chunk 头过长".to_string()));
            }
            if !self.fill().await? {
                return Err(AuthError::BadResponse("chunk 头读取时连接关闭".to_string()));
            }
        }
    }

    async fn read_n(&mut self, n: usize) -> Result<Vec<u8>, AuthError> {
        while self.pending.len() < n {
            if !self.fill().await? {
                return Err(AuthError::BadResponse(format!(
                    "chunk 数据不完整：期望 {n} 字节，实际 {}",
                    self.pending.len()
                )));
            }
        }
        let out = self.pending[..n].to_vec();
        self.pending.drain(..n);
        Ok(out)
    }
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        text.chars().take(max).collect::<String>() + "…"
    }
}

/// 表单值编码，语义等价于 JS 的 `encodeURIComponent` + 空格转 `+`。
/// `+` 必须编码为 `%2B`，否则网关按 URL 解码会把它变成空格。
pub fn form_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 3);
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' => out.push(*byte as char),
            b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')' => {
                out.push(*byte as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// 在 Cookie 串中查找指定名称的值。
fn find_cookie(cookie: &str, name: &str) -> Option<String> {
    cookie
        .split(';')
        .filter_map(|item| item.trim().split_once('='))
        .filter(|(key, _)| *key == name)
        .map(|(_, value)| value.to_string())
        .next_back()
}

/// 合并 Cookie；同名项被替换并移到末尾。
fn upsert_cookie_string(cookie: Option<&str>, pairs: &[(&str, &str)]) -> String {
    let mut jar: Vec<(String, String)> = cookie
        .map(|value| {
            value
                .split(';')
                .filter_map(|item| item.trim().split_once('='))
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect()
        })
        .unwrap_or_default();

    for (name, value) in pairs {
        jar.retain(|(key, _)| key != name);
        jar.push(((*name).to_string(), (*value).to_string()));
    }

    jar.into_iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 表单编码_加号必须转义() {
        // 这是官方客户端也踩过的坑：+ 不转义会被网关解成空格，导致 Ret=-7
        assert_eq!(form_encode("a+b"), "a%2Bb");
    }

    #[test]
    fn 表单编码_空格转为加号() {
        assert_eq!(form_encode("a b"), "a+b");
    }

    #[test]
    fn 表单编码_保留非保留字符() {
        assert_eq!(form_encode("A-z_0.9!~*'()"), "A-z_0.9!~*'()");
    }

    #[test]
    fn 表单编码_中文按_utf8_百分号编码() {
        assert_eq!(form_encode("中"), "%E4%B8%AD");
    }

    #[test]
    fn host_头_标准端口省略端口号() {
        assert_eq!(host_header("vpn.example.com", 443), "vpn.example.com");
        assert_eq!(host_header("vpn.example.com", 8443), "vpn.example.com:8443");
    }

    #[test]
    fn cookie_同名覆盖_异名保留() {
        let mut c = HttpClient::new("h", 443, Duration::from_secs(1));
        c.absorb_cookies(&["a=1".to_string(), "b=2".to_string()]);
        assert_eq!(c.cookie(), Some("a=1; b=2"));
        c.absorb_cookies(&["a=9".to_string()]);
        assert_eq!(c.cookie(), Some("b=2; a=9"));
    }

    #[test]
    fn cookie_读取指定项() {
        let mut c = HttpClient::new("h", 443, Duration::from_secs(1));
        c.absorb_cookies(&["topsecsveee=60".to_string(), "topsecsvkkk=a=b".to_string()]);
        assert_eq!(c.cookie_value("topsecsveee").as_deref(), Some("60"));
        assert_eq!(c.cookie_value("topsecsvkkk").as_deref(), Some("a=b"));
        assert_eq!(c.cookie_value("missing"), None);
    }

    #[test]
    fn cookie_合并覆盖并删除() {
        let mut c = HttpClient::new("h", 443, Duration::from_secs(1));
        c.upsert_cookies(&[("sv_un", "plain"), ("seccheck", "0")]);
        c.upsert_cookies(&[("sv_un", "YmFzZTY0")]);
        assert_eq!(c.cookie(), Some("seccheck=0; sv_un=YmFzZTY0"));
        c.remove_cookie("sv_un");
        assert_eq!(c.cookie(), Some("seccheck=0"));
    }

    #[test]
    fn 子序列查找() {
        assert_eq!(find_subsequence(b"abc\r\n\r\nxyz", b"\r\n\r\n"), Some(3));
        assert_eq!(find_subsequence(b"abc", b"\r\n\r\n"), None);
    }
}
