//! 帧的构造与编码。

use super::{
    ProtoError, DIR_UP, HEADER_LEN, LOGIN_PAYLOAD_SIZE, MSG_DATA, MSG_LOGIN, MSG_NOTIFY,
    SESSION_ID_OFFSET,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub msg_type: u32,
    pub dir: u32,
    pub payload: Vec<u8>,
}

impl Frame {
    /// 编码为可直接写入 TLS 的字节流
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER_LEN + self.payload.len());
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(&self.msg_type.to_be_bytes());
        out.extend_from_slice(&(self.payload.len() as u32).to_be_bytes());
        out.extend_from_slice(&self.dir.to_be_bytes());
        out.extend_from_slice(&self.payload);
        out
    }
}

/// 建立隧道时发送的登录帧，把会话 ID 填入固定结构
pub fn build_login_frame(session_id: &str) -> Result<Vec<u8>, ProtoError> {
    if session_id.is_empty() {
        return Err(ProtoError::EmptySessionId);
    }
    let id_bytes = session_id.as_bytes();
    let max = LOGIN_PAYLOAD_SIZE - SESSION_ID_OFFSET;
    if id_bytes.len() > max {
        return Err(ProtoError::SessionIdTooLong {
            len: id_bytes.len(),
            max,
        });
    }

    let mut payload = vec![0u8; LOGIN_PAYLOAD_SIZE];
    payload[0..4].copy_from_slice(&4u32.to_be_bytes());
    payload[SESSION_ID_OFFSET..SESSION_ID_OFFSET + id_bytes.len()].copy_from_slice(id_bytes);
    Ok(Frame {
        msg_type: MSG_LOGIN,
        dir: DIR_UP,
        payload,
    }
    .encode())
}

/// 登录确认后立刻发送的通知帧（8 字节零载荷）
pub fn build_notify_frame() -> Vec<u8> {
    Frame {
        msg_type: MSG_NOTIFY,
        dir: DIR_UP,
        payload: vec![0u8; 8],
    }
    .encode()
}

/// 承载一个完整 IPv4 报文
pub fn build_data_frame(packet: &[u8]) -> Vec<u8> {
    Frame {
        msg_type: MSG_DATA,
        dir: DIR_UP,
        payload: packet.to_vec(),
    }
    .encode()
}
