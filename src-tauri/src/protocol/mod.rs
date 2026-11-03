//! TopSAP 隧道分帧协议。
//!
//! 16 字节固定头（全部大端）：
//! ```text
//! +0..3    保留，恒为 0
//! +4..7    消息类型：1=登录 2=确认 7=通知 15=IPv4 数据
//! +8..11   载荷长度
//! +12..15  方向：1=上行 0=下行
//! ```
//!
//! 登录帧载荷固定 328 字节：偏移 0 写常量 4，偏移 72 写会话 ID（ASCII），其余补零。

pub mod frame;
pub mod parser;

pub use frame::{build_data_frame, build_login_frame, build_notify_frame, Frame};
pub use parser::FrameParser;

/// 帧头长度
pub const HEADER_LEN: usize = 16;

pub const MSG_LOGIN: u32 = 1;
pub const MSG_ACK: u32 = 2;
pub const MSG_NOTIFY: u32 = 7;
pub const MSG_DATA: u32 = 15;

pub const DIR_UP: u32 = 1;
pub const DIR_DOWN: u32 = 0;

/// 登录帧载荷长度
pub const LOGIN_PAYLOAD_SIZE: usize = 328;
/// 会话 ID 在登录帧载荷中的偏移
pub const SESSION_ID_OFFSET: usize = 72;
/// 单个载荷上限（IPv4 最大报文 65535）
pub const MAX_PAYLOAD: usize = 65_535;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProtoError {
    #[error("帧长度异常：{got} 字节，超过上限 {max}")]
    FrameTooLarge { got: usize, max: usize },
    #[error("会话 ID 过长：{len} 字节，最多 {max} 字节")]
    SessionIdTooLong { len: usize, max: usize },
    #[error("会话 ID 为空")]
    EmptySessionId,
}

/// 载荷是否为完整的 IPv4 报文（版本号 4 且至少 20 字节）
pub fn is_ipv4(buf: &[u8]) -> bool {
    buf.len() >= 20 && (buf[0] >> 4) == 4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 识别_ipv4_报文() {
        let mut pkt = vec![0u8; 20];
        pkt[0] = 0x45;
        assert!(is_ipv4(&pkt));
    }

    #[test]
    fn 拒绝_ipv6_与过短载荷() {
        // 0x60 是 IPv6 版本号
        let mut pkt = vec![0u8; 40];
        pkt[0] = 0x60;
        assert!(!is_ipv4(&pkt));
        // 19 字节不足最小 IP 头
        assert!(!is_ipv4(&[0x45; 19]));
    }

    #[test]
    fn 登录帧布局符合实测() {
        let id = "100&demo0000000000";
        let frame = build_login_frame(id).expect("构建登录帧");
        assert_eq!(frame.len(), HEADER_LEN + LOGIN_PAYLOAD_SIZE);
        // 帧头：保留位 0、类型 1、长度 328、方向 1
        assert_eq!(&frame[0..4], &[0, 0, 0, 0]);
        assert_eq!(
            u32::from_be_bytes(frame[4..8].try_into().unwrap()),
            MSG_LOGIN
        );
        assert_eq!(
            u32::from_be_bytes(frame[8..12].try_into().unwrap()) as usize,
            LOGIN_PAYLOAD_SIZE
        );
        assert_eq!(
            u32::from_be_bytes(frame[12..16].try_into().unwrap()),
            DIR_UP
        );
        // 载荷偏移 0 的常量 4
        let payload = &frame[HEADER_LEN..];
        assert_eq!(&payload[0..4], &[0, 0, 0, 4]);
        // 偏移 72 起的会话 ID
        assert_eq!(
            &payload[SESSION_ID_OFFSET..SESSION_ID_OFFSET + id.len()],
            id.as_bytes()
        );
        // 其余补零
        assert!(payload[SESSION_ID_OFFSET + id.len()..]
            .iter()
            .all(|b| *b == 0));
    }

    #[test]
    fn 空会话_id_报错() {
        assert!(matches!(
            build_login_frame(""),
            Err(ProtoError::EmptySessionId)
        ));
    }

    #[test]
    fn 超长会话_id_报错() {
        let long = "x".repeat(LOGIN_PAYLOAD_SIZE - SESSION_ID_OFFSET + 1);
        assert!(matches!(
            build_login_frame(&long),
            Err(ProtoError::SessionIdTooLong { .. })
        ));
    }

    #[test]
    fn 通知帧为八字节零载荷() {
        let frame = build_notify_frame();
        assert_eq!(frame.len(), HEADER_LEN + 8);
        assert_eq!(
            u32::from_be_bytes(frame[4..8].try_into().unwrap()),
            MSG_NOTIFY
        );
        assert!(frame[HEADER_LEN..].iter().all(|b| *b == 0));
    }
}
