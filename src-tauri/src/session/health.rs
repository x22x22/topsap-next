//! 数据面健康判定：按窗口观察需求、上行成功、下行响应与下行投递。

use std::time::Duration;

pub const WINDOW: Duration = Duration::from_secs(15);
pub const GRACE: Duration = Duration::from_secs(30);
const UPLINK_STALL_WINDOWS: u32 = 3;
const DOWNLINK_SILENT_WINDOWS: u32 = 4;
const DELIVERY_STALL_WINDOWS: u32 = 3;
const MIN_PACKETS: u64 = 20;
const MIN_UPLINK_DROPS: u64 = 100;

/// 一次采样的累计计数器；`generation` 用于识别隧道重连后的计数归零
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HealthSample {
    pub generation: u64,
    pub packets_in: u64,
    pub packets_up: u64,
    pub packets_down: u64,
    pub packets_out: u64,
    pub dropped_out: u64,
    pub uplink_drops: u64,
    pub tcp_in: u64,
    pub icmp_in: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthIssue {
    UplinkStalled,
    DownlinkSilent,
    DeliveryStalled,
}

impl HealthIssue {
    pub fn describe(self) -> &'static str {
        match self {
            Self::UplinkStalled => "上行报文持续没有写入隧道",
            Self::DownlinkSilent => "有上行流量但持续没有下行响应",
            Self::DeliveryStalled => "下行报文未能注入系统网卡",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Delta {
    packets_in: u64,
    packets_up: u64,
    packets_down: u64,
    packets_out: u64,
    dropped_out: u64,
    uplink_drops: u64,
    tcp_in: u64,
    icmp_in: u64,
}

impl Delta {
    fn between(previous: HealthSample, current: HealthSample) -> Self {
        Self {
            packets_in: current.packets_in.saturating_sub(previous.packets_in),
            packets_up: current.packets_up.saturating_sub(previous.packets_up),
            packets_down: current.packets_down.saturating_sub(previous.packets_down),
            packets_out: current.packets_out.saturating_sub(previous.packets_out),
            dropped_out: current.dropped_out.saturating_sub(previous.dropped_out),
            uplink_drops: current.uplink_drops.saturating_sub(previous.uplink_drops),
            tcp_in: current.tcp_in.saturating_sub(previous.tcp_in),
            icmp_in: current.icmp_in.saturating_sub(previous.icmp_in),
        }
    }
}

/// 按固定窗口累计异常，连续多个窗口后才给出结论，避免瞬时抖动误判
pub struct HealthTracker {
    generation: u64,
    last_window: Duration,
    baseline: HealthSample,
    grace_until: Duration,
    initialized: bool,
    uplink_windows: u32,
    uplink_packets: u64,
    uplink_drops: u64,
    downlink_windows: u32,
    downlink_packets: u64,
    delivery_windows: u32,
    delivery_packets: u64,
}

impl HealthTracker {
    pub fn new() -> Self {
        Self {
            generation: 0,
            last_window: Duration::ZERO,
            baseline: HealthSample::default(),
            grace_until: Duration::ZERO,
            initialized: false,
            uplink_windows: 0,
            uplink_packets: 0,
            uplink_drops: 0,
            downlink_windows: 0,
            downlink_packets: 0,
            delivery_windows: 0,
            delivery_packets: 0,
        }
    }

    pub fn observe(&mut self, now: Duration, sample: HealthSample) -> Option<HealthIssue> {
        if !self.initialized || sample.generation != self.generation {
            self.reset(now, sample);
            return None;
        }
        if now < self.grace_until {
            self.baseline = sample;
            self.last_window = now;
            return None;
        }
        if now.saturating_sub(self.last_window) < WINDOW {
            return None;
        }
        self.last_window = now;

        let delta = Delta::between(self.baseline, sample);
        self.baseline = sample;

        if delta.packets_in == 0 && delta.packets_down == 0 {
            self.clear_counters();
            return None;
        }

        self.observe_uplink(delta);
        self.observe_downlink(delta);
        self.observe_delivery(delta);

        if self.uplink_windows >= UPLINK_STALL_WINDOWS
            && (self.uplink_packets >= MIN_PACKETS || self.uplink_drops >= MIN_UPLINK_DROPS)
        {
            self.clear_counters();
            return Some(HealthIssue::UplinkStalled);
        }
        if self.downlink_windows >= DOWNLINK_SILENT_WINDOWS
            && self.downlink_packets >= MIN_PACKETS
        {
            self.clear_counters();
            return Some(HealthIssue::DownlinkSilent);
        }
        if self.delivery_windows >= DELIVERY_STALL_WINDOWS
            && self.delivery_packets >= MIN_PACKETS
        {
            self.clear_counters();
            return Some(HealthIssue::DeliveryStalled);
        }
        None
    }

    fn observe_uplink(&mut self, delta: Delta) {
        if delta.packets_in > 0 && delta.packets_up == 0 {
            self.uplink_windows += 1;
            self.uplink_packets = self.uplink_packets.saturating_add(delta.packets_in);
        } else if delta.uplink_drops >= MIN_UPLINK_DROPS {
            self.uplink_windows += 1;
            self.uplink_drops = self.uplink_drops.saturating_add(delta.uplink_drops);
        } else {
            self.uplink_windows = 0;
            self.uplink_packets = 0;
            self.uplink_drops = 0;
        }
    }

    fn observe_downlink(&mut self, delta: Delta) {
        let response_expected = delta.tcp_in > 0 || delta.icmp_in > 0;
        if delta.packets_up > 0 && delta.packets_down == 0 && response_expected {
            self.downlink_windows += 1;
            self.downlink_packets = self.downlink_packets.saturating_add(delta.packets_up);
        } else {
            self.downlink_windows = 0;
            self.downlink_packets = 0;
        }
    }

    fn observe_delivery(&mut self, delta: Delta) {
        if delta.packets_down > 0 && delta.packets_out == 0 && delta.dropped_out > 0 {
            self.delivery_windows += 1;
            self.delivery_packets = self.delivery_packets.saturating_add(delta.packets_down);
        } else {
            self.delivery_windows = 0;
            self.delivery_packets = 0;
        }
    }

    fn clear_counters(&mut self) {
        self.uplink_windows = 0;
        self.uplink_packets = 0;
        self.uplink_drops = 0;
        self.downlink_windows = 0;
        self.downlink_packets = 0;
        self.delivery_windows = 0;
        self.delivery_packets = 0;
    }

    fn reset(&mut self, now: Duration, sample: HealthSample) {
        self.initialized = true;
        self.generation = sample.generation;
        self.last_window = now;
        self.baseline = sample;
        self.grace_until = now + GRACE;
        self.clear_counters();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(generation: u64, packets_in: u64, packets_up: u64, packets_down: u64) -> HealthSample {
        HealthSample {
            generation,
            packets_in,
            packets_up,
            packets_down,
            packets_out: packets_down,
            tcp_in: packets_in,
            ..HealthSample::default()
        }
    }

    fn observe(
        tracker: &mut HealthTracker,
        seconds: u64,
        sample: HealthSample,
    ) -> Option<HealthIssue> {
        tracker.observe(Duration::from_secs(seconds), sample)
    }

    #[test]
    fn 空闲不判定() {
        let mut tracker = HealthTracker::new();
        assert_eq!(
            observe(&mut tracker, 0, sample(1, 0, 0, 0)),
            None
        );
        for second in (15..=600).step_by(15) {
            assert_eq!(
                observe(&mut tracker, second, sample(1, 0, 0, 0)),
                None
            );
        }
    }

    #[test]
    fn 正常双向不判定() {
        let mut tracker = HealthTracker::new();
        observe(&mut tracker, 0, sample(1, 0, 0, 0));
        for (index, second) in (30..=300).step_by(15).enumerate() {
            let base = (index as u64 + 1) * 30;
            assert_eq!(
                observe(
                    &mut tracker,
                    second,
                    sample(1, base, base, base)
                ),
                None
            );
        }
    }

    #[test]
    fn 上行卡死连续三个窗口后才判定() {
        let mut tracker = HealthTracker::new();
        observe(&mut tracker, 0, sample(1, 0, 0, 0));
        assert_eq!(observe(&mut tracker, 30, sample(1, 0, 0, 0)), None);
        assert_eq!(observe(&mut tracker, 45, sample(1, 30, 0, 0)), None);
        assert_eq!(observe(&mut tracker, 60, sample(1, 60, 0, 0)), None);
        assert_eq!(
            observe(&mut tracker, 75, sample(1, 90, 0, 0)),
            Some(HealthIssue::UplinkStalled)
        );
    }

    #[test]
    fn 下行静默连续四个窗口后才判定() {
        let mut tracker = HealthTracker::new();
        observe(&mut tracker, 0, sample(1, 0, 0, 0));
        observe(&mut tracker, 30, sample(1, 0, 0, 0));
        assert_eq!(observe(&mut tracker, 45, sample(1, 30, 30, 0)), None);
        assert_eq!(observe(&mut tracker, 60, sample(1, 60, 60, 0)), None);
        assert_eq!(observe(&mut tracker, 75, sample(1, 90, 90, 0)), None);
        assert_eq!(
            observe(&mut tracker, 90, sample(1, 120, 120, 0)),
            Some(HealthIssue::DownlinkSilent)
        );
    }

    #[test]
    fn 纯_udp_单向下行静默不判定() {
        let mut tracker = HealthTracker::new();
        observe(&mut tracker, 0, sample(1, 0, 0, 0));
        observe(&mut tracker, 30, sample(1, 0, 0, 0));
        for (index, second) in (45..=180).step_by(15).enumerate() {
            let base = (index as u64 + 1) * 30;
            let mut current = sample(1, base, base, 0);
            current.tcp_in = 0;
            assert_eq!(observe(&mut tracker, second, current), None);
        }
    }

    #[test]
    fn 下行投递失败连续三个窗口后判定() {
        let mut tracker = HealthTracker::new();
        observe(&mut tracker, 0, sample(1, 0, 0, 0));
        observe(&mut tracker, 30, sample(1, 0, 0, 0));
        for (index, second) in (45..=90).step_by(15).enumerate() {
            let base = (index as u64 + 1) * 30;
            let mut current = sample(1, base, base, base);
            current.packets_out = 0;
            current.dropped_out = base;
            let expected = if index == 2 {
                Some(HealthIssue::DeliveryStalled)
            } else {
                None
            };
            assert_eq!(observe(&mut tracker, second, current), expected);
        }
    }

    #[test]
    fn 宽限期内不判定() {
        let mut tracker = HealthTracker::new();
        observe(&mut tracker, 0, sample(1, 0, 0, 0));
        assert_eq!(observe(&mut tracker, 15, sample(1, 100, 0, 0)), None);
    }

    #[test]
    fn 代际变化重置基线并重新宽限() {
        let mut tracker = HealthTracker::new();
        observe(&mut tracker, 0, sample(1, 0, 0, 0));
        observe(&mut tracker, 30, sample(1, 0, 0, 0));
        for second in [45, 60, 75] {
            observe(&mut tracker, second, sample(1, second * 2, 0, 0));
        }
        assert_eq!(
            observe(&mut tracker, 90, sample(2, 0, 0, 0)),
            None
        );
        assert_eq!(
            observe(&mut tracker, 100, sample(2, 100, 0, 0)),
            None
        );
    }
}
