//! 引擎状态机。

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Probing,
    CaptchaPending,
    SmsPending,
    Authenticating,
    CreatingTun,
    Handshaking,
    Configuring,
    Connected,
    Reconnecting,
    Disconnecting,
    Failed,
}

impl Phase {
    pub fn key(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Probing => "probing",
            Self::CaptchaPending => "captcha_pending",
            Self::SmsPending => "sms_pending",
            Self::Authenticating => "authenticating",
            Self::CreatingTun => "creating_tun",
            Self::Handshaking => "handshaking",
            Self::Configuring => "configuring",
            Self::Connected => "connected",
            Self::Reconnecting => "reconnecting",
            Self::Disconnecting => "disconnecting",
            Self::Failed => "failed",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "未连接",
            Self::Probing => "正在探测网关",
            Self::CaptchaPending => "等待验证码",
            Self::SmsPending => "等待动态验证码",
            Self::Authenticating => "正在登录",
            Self::CreatingTun => "正在创建网卡",
            Self::Handshaking => "正在建立隧道",
            Self::Configuring => "正在配置网络",
            Self::Connected => "已连接",
            Self::Reconnecting => "正在重连",
            Self::Disconnecting => "正在断开",
            Self::Failed => "连接失败",
        }
    }

    /// 是否处于「用户已在流程中」的状态，用于禁用重复操作
    pub fn busy(self) -> bool {
        matches!(
            self,
            Self::Probing
                | Self::CaptchaPending
                | Self::SmsPending
                | Self::Authenticating
                | Self::CreatingTun
                | Self::Handshaking
                | Self::Configuring
                | Self::Disconnecting
        )
    }

    pub fn connected(self) -> bool {
        self == Self::Connected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 忙碌状态覆盖全流程() {
        assert!(Phase::Probing.busy());
        assert!(Phase::Handshaking.busy());
        assert!(!Phase::Connected.busy());
        assert!(!Phase::Idle.busy());
    }

    #[test]
    fn 每个状态都有中文标签与稳定_key() {
        let all = [
            Phase::Idle,
            Phase::Probing,
            Phase::CaptchaPending,
            Phase::Authenticating,
            Phase::CreatingTun,
            Phase::Handshaking,
            Phase::Configuring,
            Phase::Connected,
            Phase::Reconnecting,
            Phase::Disconnecting,
            Phase::Failed,
        ];
        for phase in all {
            assert!(!phase.key().is_empty());
            assert!(!phase.label().is_empty());
        }
    }
}
