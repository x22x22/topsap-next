//! 托盘图标状态：保留应用原图标外观，只把盾牌主色按连接状态替换。
//!
//! - 未连接（尚未连接成功）：灰色
//! - 已连接：绿色
//! - 已断开（连接过之后断开/失败）：橙色，并以约 0.7 秒间隔在亮/暗橙色间闪烁
//!
//! 图标直接使用原图标的完整内容（深色圆角底、盾牌轮廓、齿轮全部保留），
//! 仅把「青色盾牌」像素替换成状态色，因此尺寸与细节和原图标一致，不会变小。
//!
//! 状态由 `Engine::set_phase` 驱动；仅在状态真正变化时才调用系统接口。

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use tauri::image::Image;
use tauri::AppHandle;

/// 托盘图标 id，与 `setup_tray` 创建时保持一致
pub const TRAY_ID: &str = "topsap-status";

/// 闪烁间隔：亮橙与暗橙交替
const BLINK_INTERVAL: Duration = Duration::from_millis(700);
/// 暗态亮度系数（闪烁的“灭”相）
const DIM_FACTOR: f32 = 0.55;
/// 判定盾牌主色的亮度下限（原图标青色盾牌约 145，深色底与齿轮约 10~30）
const SHIELD_LUMA: f32 = 55.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayState {
    /// 从未连接成功
    Idle,
    /// 正在连接（探测、登录、建链、重连）
    Connecting,
    /// 当前已连接
    Connected,
    /// 人为断开进行中
    Disconnecting,
    /// 连接过之后异常断开或失败
    Disconnected,
}

impl TrayState {
    fn code(self) -> u8 {
        match self {
            Self::Idle => 0,
            Self::Connecting => 1,
            Self::Connected => 2,
            Self::Disconnecting => 3,
            Self::Disconnected => 4,
        }
    }

    pub fn from_code(code: u8) -> Self {
        match code {
            1 => Self::Connecting,
            2 => Self::Connected,
            3 => Self::Disconnecting,
            4 => Self::Disconnected,
            _ => Self::Idle,
        }
    }

    /// 状态色：灰（未连接）/ 蓝（连接中）/ 绿（已连接）/ 浅蓝（正在断开）/ 红（异常断开）
    pub fn color(self) -> [u8; 3] {
        match self {
            Self::Idle => [142, 142, 147],
            Self::Connecting => [10, 132, 255],
            Self::Connected => [52, 199, 89],
            Self::Disconnecting => [100, 210, 255],
            Self::Disconnected => [255, 69, 58],
        }
    }

    pub fn tooltip(self) -> &'static str {
        match self {
            Self::Idle => "TopSAP Client · 未连接",
            Self::Connecting => "TopSAP Client · 连接中",
            Self::Connected => "TopSAP Client · 已连接",
            Self::Disconnecting => "TopSAP Client · 正在断开",
            Self::Disconnected => "TopSAP Client · 已断开",
        }
    }
}

/// 原图标像素数据（RGBA），着色时复用
struct SourceIcon {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
}

static SOURCE: OnceLock<Option<SourceIcon>> = OnceLock::new();
static EVER_CONNECTED: AtomicBool = AtomicBool::new(false);
static LAST_STATE: AtomicU8 = AtomicU8::new(0);
static BLINK_ON: AtomicBool = AtomicBool::new(false);

/// 初始化托盘图标：缓存原图标并返回「未连接」状态的图标
pub fn init(app: &AppHandle) -> Option<Image<'static>> {
    let source = SOURCE.get_or_init(|| {
        app.default_window_icon().map(|icon| SourceIcon {
            rgba: icon.rgba().to_vec(),
            width: icon.width(),
            height: icon.height(),
        })
    });
    source
        .as_ref()
        .map(|source| render(source, TrayState::Idle, false))
}

/// 按引擎阶段更新托盘状态：只有 `Connected` 视为已连接
pub fn update_for_phase(
    app: &AppHandle,
    connected: bool,
    connecting: bool,
    disconnecting: bool,
    abnormal: bool,
) {
    let ever = EVER_CONNECTED.load(Ordering::SeqCst);
    let (state, ever_after) = resolve(connected, connecting, disconnecting, abnormal, ever);
    EVER_CONNECTED.store(ever_after, Ordering::SeqCst);
    apply(app, state);
}

/// 启动闪烁任务：仅在「已断开」状态时交替亮/暗，其他状态保持静态
pub fn start_blink_task(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(BLINK_INTERVAL).await;
            let state = TrayState::from_code(LAST_STATE.load(Ordering::SeqCst));
            if state != TrayState::Disconnected {
                BLINK_ON.store(false, Ordering::SeqCst);
                continue;
            }
            let lit = !BLINK_ON.load(Ordering::SeqCst);
            BLINK_ON.store(lit, Ordering::SeqCst);
            set_icon(&app, state, lit);
        }
    });
}

/// 状态推导：连接成功会记录「连接过」，此后断开显示已断开而不是未连接
fn resolve(
    connected: bool,
    connecting: bool,
    disconnecting: bool,
    abnormal: bool,
    ever_connected: bool,
) -> (TrayState, bool) {
    if connected {
        (TrayState::Connected, true)
    } else if disconnecting {
        // 人为断开进行中：用浅蓝，不按异常红色处理
        (TrayState::Disconnecting, ever_connected)
    } else if connecting {
        (TrayState::Connecting, ever_connected)
    } else if abnormal && ever_connected {
        // 掉线/重连失败等非人为断开：红色提醒
        (TrayState::Disconnected, true)
    } else {
        // 从未连接，或人为断开完成：回到未连接
        (TrayState::Idle, ever_connected)
    }
}

fn apply(app: &AppHandle, state: TrayState) {
    if LAST_STATE.swap(state.code(), Ordering::SeqCst) == state.code() {
        return;
    }
    BLINK_ON.store(false, Ordering::SeqCst);
    set_icon(app, state, true);
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(state.tooltip()));
    }
}

/// 仅替换图标（不改状态），`lit` 为 false 时使用暗态用于闪烁
fn set_icon(app: &AppHandle, state: TrayState, lit: bool) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let Some(source) = SOURCE.get().and_then(|source| source.as_ref()) else {
        return;
    };
    let icon = render(source, state, !lit);
    let _ = tray.set_icon(Some(icon));
    // 运行期替换图标后再次关闭模板渲染，保证状态色不会被系统抹平
    #[cfg(target_os = "macos")]
    let _ = tray.set_icon_as_template(false);
}

fn render(source: &SourceIcon, state: TrayState, dim: bool) -> Image<'static> {
    let rgba = recolor(&source.rgba, state.color(), dim);
    Image::new_owned(rgba, source.width, source.height)
}

/// 把盾牌主色替换成状态色，其余像素（深色底、齿轮、圆角边缘）原样保留。
fn recolor(source: &[u8], color: [u8; 3], dim: bool) -> Vec<u8> {
    let factor = if dim { DIM_FACTOR } else { 1.0 };
    let target = [
        (f32::from(color[0]) * factor).round() as u8,
        (f32::from(color[1]) * factor).round() as u8,
        (f32::from(color[2]) * factor).round() as u8,
    ];
    let mut out = Vec::with_capacity(source.len());
    for chunk in source.chunks_exact(4) {
        let (r, g, b, a) = (chunk[0], chunk[1], chunk[2], chunk[3]);
        let luma = 0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b);
        if luma >= SHIELD_LUMA && b > r {
            out.extend_from_slice(&[target[0], target[1], target[2], a]);
        } else {
            out.extend_from_slice(chunk);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 状态推导区分未连接连接中已连接正在断开与已断开() {
        let (idle, ever) = resolve(false, false, false, false, false);
        assert_eq!(idle, TrayState::Idle);
        assert!(!ever);

        let (connecting, ever) = resolve(false, true, false, false, false);
        assert_eq!(connecting, TrayState::Connecting);
        assert!(!ever);

        let (connected, ever) = resolve(true, false, false, false, false);
        assert_eq!(connected, TrayState::Connected);
        assert!(ever);

        // 人为断开进行中：浅蓝，不是红色
        let (closing, ever) = resolve(false, false, true, false, true);
        assert_eq!(closing, TrayState::Disconnecting);
        assert!(ever);

        // 人为断开完成：回到未连接（灰），不提示异常
        let (idle_after_manual, ever) = resolve(false, false, false, false, true);
        assert_eq!(idle_after_manual, TrayState::Idle);
        assert!(ever);

        // 异常断开（掉线/重连失败）：红色
        let (abnormal, ever) = resolve(false, false, false, true, true);
        assert_eq!(abnormal, TrayState::Disconnected);
        assert!(ever);

        // 已连接过之后进入重连：显示连接中，仍保留「曾经连接」标记
        let (reconnecting, ever) = resolve(false, true, false, false, true);
        assert_eq!(reconnecting, TrayState::Connecting);
        assert!(ever);
    }

    #[test]
    fn 四种状态颜色与提示都不同() {
        let states = [
            TrayState::Idle,
            TrayState::Connecting,
            TrayState::Connected,
            TrayState::Disconnecting,
            TrayState::Disconnected,
        ];
        for i in 0..states.len() {
            for j in (i + 1)..states.len() {
                assert_ne!(states[i].color(), states[j].color());
                assert_ne!(states[i].tooltip(), states[j].tooltip());
            }
        }
        assert!(TrayState::Connecting.tooltip().contains("连接中"));
        assert!(TrayState::Disconnecting.tooltip().contains("正在断开"));
        assert!(TrayState::Disconnected.tooltip().contains("已断开"));
        for state in states {
            assert_eq!(TrayState::from_code(state.code()), state);
        }
    }

    #[test]
    fn 只替换盾牌主色并保留深色底座与齿轮() {
        // 依次：深色圆角底、青色盾牌、黑色齿轮、半透明边缘
        let source = vec![
            20, 24, 30, 255, // 深色底
            42, 168, 224, 255, // 青色盾牌
            10, 13, 18, 255, // 黑齿轮
            20, 24, 30, 128, // 半透明边缘
        ];
        let out = recolor(&source, [52, 199, 89], false);
        assert_eq!(&out[0..4], &[20, 24, 30, 255], "深色底应原样保留");
        assert_eq!(&out[4..8], &[52, 199, 89, 255], "盾牌应替换为状态色");
        assert_eq!(&out[8..12], &[10, 13, 18, 255], "齿轮应原样保留");
        assert_eq!(&out[12..16], &[20, 24, 30, 128], "边缘透明度应保留");
    }

    #[test]
    fn 暗态按比例压暗状态色() {
        let source = vec![42, 168, 224, 255];
        let out = recolor(&source, [255, 149, 0], true);
        assert!(out[0] < 255 && out[1] < 149, "暗态应整体压暗");
        assert_eq!(out[3], 255, "透明度保持不变");
    }
}
