//! 流式拆帧：处理 TCP 上的粘包与半包。

use super::{Frame, ProtoError, HEADER_LEN, MAX_PAYLOAD};

pub struct FrameParser {
    buf: Vec<u8>,
    max_payload: usize,
}

impl Default for FrameParser {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameParser {
    pub fn new() -> Self {
        Self {
            buf: Vec::with_capacity(8 * 1024),
            max_payload: MAX_PAYLOAD,
        }
    }

    /// 追加一段字节，返回本次可完整解析的所有帧。
    ///
    /// 未凑满一帧的残余会留在内部缓冲，等待下一次 push。
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<Frame>, ProtoError> {
        self.buf.extend_from_slice(chunk);

        let mut frames = Vec::new();
        let mut consumed = 0usize;

        while self.buf.len() - consumed >= HEADER_LEN {
            let header = &self.buf[consumed..consumed + HEADER_LEN];
            let msg_type = u32::from_be_bytes(header[4..8].try_into().unwrap());
            let len = u32::from_be_bytes(header[8..12].try_into().unwrap()) as usize;
            let dir = u32::from_be_bytes(header[12..16].try_into().unwrap());

            if len > self.max_payload {
                // 长度不可信时说明流已错位，抛错让上层断开重连，避免无限增长
                return Err(ProtoError::FrameTooLarge {
                    got: len,
                    max: self.max_payload,
                });
            }

            let total = HEADER_LEN + len;
            if self.buf.len() - consumed < total {
                break;
            }

            let payload = self.buf[consumed + HEADER_LEN..consumed + total].to_vec();
            frames.push(Frame {
                msg_type,
                dir,
                payload,
            });
            consumed += total;
        }

        if consumed > 0 {
            self.buf.drain(..consumed);
        }
        Ok(frames)
    }

    /// 尚未消费的字节数（用于测试与诊断）
    pub fn pending(&self) -> usize {
        self.buf.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{build_data_frame, build_notify_frame, MSG_DATA, MSG_NOTIFY};

    #[test]
    fn 单帧完整解析() {
        let raw = build_notify_frame();
        let mut p = FrameParser::new();
        let frames = p.push(&raw).expect("解析");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].msg_type, MSG_NOTIFY);
        assert_eq!(frames[0].payload.len(), 8);
        assert_eq!(p.pending(), 0);
    }

    #[test]
    fn 粘包一次解析多帧() {
        let mut raw = build_notify_frame();
        raw.extend_from_slice(&build_data_frame(&[0x45; 40]));
        let mut p = FrameParser::new();
        let frames = p.push(&raw).expect("解析");
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].msg_type, MSG_NOTIFY);
        assert_eq!(frames[1].msg_type, MSG_DATA);
        assert_eq!(frames[1].payload.len(), 40);
    }

    #[test]
    fn 半包等待后续字节() {
        let mut raw = build_data_frame(&[0x45; 40]);
        let split = 10;
        let tail = raw.split_off(split);

        let mut p = FrameParser::new();
        assert!(p.push(&raw).expect("首批").is_empty());
        assert_eq!(p.pending(), split);

        let frames = p.push(&tail).expect("补齐");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].msg_type, MSG_DATA);
        assert_eq!(p.pending(), 0);
    }

    #[test]
    fn 逐字节喂入也能解析() {
        let raw = build_notify_frame();
        let mut p = FrameParser::new();
        let mut got = 0;
        for b in raw {
            got += p.push(&[b]).expect("逐字节").len();
        }
        assert_eq!(got, 1);
    }

    #[test]
    fn 超长帧直接报错() {
        let mut raw = vec![0u8; HEADER_LEN];
        raw[8..12].copy_from_slice(&(MAX_PAYLOAD as u32 + 1).to_be_bytes());
        let mut p = FrameParser::new();
        assert!(matches!(
            p.push(&raw),
            Err(ProtoError::FrameTooLarge { .. })
        ));
    }
}
