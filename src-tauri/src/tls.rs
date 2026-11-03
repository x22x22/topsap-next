//! 统一 TLS 配置。
//!
//! 两条硬性约束来自实测：
//! 1. 网关证书普遍自签或已过期，官方客户端不校验证书链，这里同样跳过。
//! 2. 官方客户端不发送 SNI，因此用 IP 作为 `ServerName`（RFC 6066 规定 IP 不得写入 SNI）。
//!
//! 安全后果：TLS 层没有身份认证能力，中间人可冒充网关。UI 与 README 必须提示。

use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, OnceLock};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;

#[derive(Debug, thiserror::Error)]
pub enum TlsError {
    #[error("无法解析网关地址「{host}」：{reason}")]
    Resolve { host: String, reason: String },
    #[error("无法连接网关 {addr}：{reason}")]
    Tcp { addr: String, reason: String },
    #[error("TLS 握手失败：{0}")]
    Handshake(String),
    #[error("TLS 配置初始化失败：{0}")]
    Config(String),
}

impl TlsError {
    /// 面向界面的说明：保留可操作信息，避免直接展示系统本地化文案
    pub fn user_message(&self) -> String {
        match self {
            Self::Resolve { host, reason } => {
                format!("无法解析网关地址「{host}」：{reason}，请检查域名拼写或网络连接")
            }
            Self::Tcp { addr, reason } => {
                format!("无法连接网关 {addr}：{reason}，请检查端口是否正确以及网关是否可达")
            }
            Self::Handshake(reason) => format!("TLS 握手失败：{reason}"),
            Self::Config(reason) => format!("TLS 初始化失败：{reason}"),
        }
    }
}

/// 全链路统一使用 ring provider，避免 ring 与 aws-lc-rs 并存导致 panic
pub fn provider() -> &'static rustls::crypto::CryptoProvider {
    static PROVIDER: OnceLock<rustls::crypto::CryptoProvider> = OnceLock::new();
    PROVIDER.get_or_init(rustls::crypto::ring::default_provider)
}

/// 在应用启动时调用一次，确保全局默认 provider 与隧道链路一致
pub fn install_default_provider() {
    // 失败只说明已被其他路径安装，忽略即可
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// 跳过证书链验证，但保留签名校验（仍能发现报文被篡改）
#[derive(Debug)]
struct NoVerifyVerifier;

impl ServerCertVerifier for NoVerifyVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

pub fn client_config() -> Arc<ClientConfig> {
    static CONFIG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let cfg = ClientConfig::builder_with_provider(Arc::new(provider().clone()))
                .with_safe_default_protocol_versions()
                .expect("rustls 协议版本配置失败")
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(NoVerifyVerifier))
                .with_no_client_auth();
            Arc::new(cfg)
        })
        .clone()
}

/// 把网关地址解析为 IP，IPv4 优先。
pub async fn resolve_ip(host: &str) -> Result<IpAddr, TlsError> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(ip);
    }
    let addrs = tokio::net::lookup_host((host, 0))
        .await
        .map_err(|e| TlsError::Resolve {
            host: host.to_string(),
            reason: describe_dns_error(&e),
        })?;

    let mut ipv6 = None;
    for addr in addrs {
        match addr.ip() {
            IpAddr::V4(_) => return Ok(addr.ip()),
            IpAddr::V6(v6) => ipv6 = Some(ipv6.unwrap_or(IpAddr::V6(v6))),
        }
    }
    ipv6.ok_or_else(|| TlsError::Resolve {
        host: host.to_string(),
        reason: "未解析到任何地址".to_string(),
    })
}

/// 建立到网关的 TLS 连接，返回连接与解析后的 IP（供旁路路由使用）。
pub async fn connect(host: &str, port: u16) -> Result<(TlsStream<TcpStream>, IpAddr), TlsError> {
    let ip = resolve_ip(host).await?;
    let addr = SocketAddr::new(ip, port);

    let tcp = TcpStream::connect(addr).await.map_err(|e| TlsError::Tcp {
        addr: addr.to_string(),
        reason: describe_tcp_error(&e),
    })?;
    // 隧道承载交互式流量，禁用 Nagle 减少小包延迟
    let _ = tcp.set_nodelay(true);

    let server_name = ServerName::IpAddress(ip.into());
    let connector = TlsConnector::from(client_config());
    let stream = connector
        .connect(server_name, tcp)
        .await
        .map_err(|e| TlsError::Handshake(describe_handshake_error(&e)))?;

    Ok((stream, ip))
}

/// 把系统 DNS 错误归一成可读原因，避免展示本地化的「不知道这样的主机」
fn describe_dns_error(err: &std::io::Error) -> String {
    match err.raw_os_error() {
        Some(11001) => "未知主机（错误码 11001）".to_string(),
        Some(11002) => "DNS 解析暂时失败（错误码 11002）".to_string(),
        Some(11004) => "域名没有可用的地址记录（错误码 11004）".to_string(),
        _ => match err.kind() {
            std::io::ErrorKind::NotFound => "未知主机".to_string(),
            std::io::ErrorKind::TimedOut => "DNS 解析超时".to_string(),
            _ => format!("DNS 解析失败：{err}"),
        },
    }
}

/// 归一 TCP 连接错误，常见原因是端口填错或网关不可达
fn describe_tcp_error(err: &std::io::Error) -> String {
    match err.raw_os_error() {
        Some(10060) => "连接超时（错误码 10060）".to_string(),
        Some(10061) => "连接被拒绝，目标端口未开放（错误码 10061）".to_string(),
        Some(10051) => "网络不可达（错误码 10051）".to_string(),
        Some(10065) => "主机不可达（错误码 10065）".to_string(),
        Some(10013) => "访问被拒绝（错误码 10013）".to_string(),
        _ => match err.kind() {
            std::io::ErrorKind::ConnectionRefused => "连接被拒绝，目标端口未开放".to_string(),
            std::io::ErrorKind::TimedOut => "连接超时".to_string(),
            std::io::ErrorKind::PermissionDenied => "访问被拒绝".to_string(),
            _ => err.to_string(),
        },
    }
}

fn describe_handshake_error(err: &std::io::Error) -> String {
    let text = err.to_string();
    if text.contains("timed out") || text.contains("timeout") {
        "握手超时，网关端口可能不可达".to_string()
    } else if text.contains("corrupt message")
        || text.contains("InvalidMessage")
        || text.contains("received fatal alert")
    {
        "目标端口未返回有效的 TLS 响应，可能不是 HTTPS 服务".to_string()
    } else if text.contains("certificate") {
        format!("{text}（已按协议跳过证书链校验，若仍失败请检查网关端口与 TLS 版本）")
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 未知主机错误归一为专业描述() {
        let err = std::io::Error::from_raw_os_error(11001);
        assert_eq!(describe_dns_error(&err), "未知主机（错误码 11001）");
    }

    #[test]
    fn 端口未开放错误归一为连接被拒绝() {
        let err = std::io::Error::from_raw_os_error(10061);
        assert!(describe_tcp_error(&err).contains("连接被拒绝"));
        assert!(describe_tcp_error(&err).contains("10061"));
    }

    #[test]
    fn 解析错误提示包含排查方向() {
        let err = TlsError::Resolve {
            host: "vpn.example.com".to_string(),
            reason: "未知主机（错误码 11001）".to_string(),
        };
        let msg = err.user_message();
        assert!(msg.contains("未知主机"));
        assert!(msg.contains("域名拼写"));
    }
}
