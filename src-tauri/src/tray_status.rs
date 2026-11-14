//! 托盘图标状态：保留应用原图标形状，按连接状态着色。
//!
//! - 未连接（尚未连接过）：灰色
//! - 已连接：绿色
//! - 已断开（连接过之后断开/失败）：橙色
//!
//! 图标由应用原图标派生：先按亮度提取盾牌剪影（深色底与齿轮被压成透明，
//! 保留抗锯齿），再用状态色填充，因此形状仍是原图标而不是另做的符号。
//!
//! 状态由 `Engine::set_phase` 驱动；仅在状态真正变化时才调用系统接口。

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::OnceLock;

use tauri::image::Image;
use tauri::AppHandle;

/// 托盘图标 id，与 `setup_tray` 创建时保持一致
pub const TRAY_ID: &str = "topsap-status";

/// 菜单栏图标边长（像素，约 18pt @2x）
const ICON_SIZE: u32 = 36;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayState {
    /// 从未连接成功
    Idle,
    /// 当前已连接
    Connected,
    /// 连接过之后断开或失败
    Disconnected,
}

impl TrayState {
    fn code(self) -> u8 {
        match self {
            Self::Idle => 0,
            Self::Connected => 1,
            Self::Disconnected => 2,
        }
    }

    /// 状态色：灰 / 绿 / 橙
    pub fn color(self) -> [u8; 3] {
        match self {
            Self::Idle => [142, 142, 147],
            Self::Connected => [52, 199, 89],
            Self::Disconnected => [255, 149, 0],
        }
    }

    pub fn tooltip(self) -> &'static str {
        match self {
            Self::Idle => "TopSAP Client · 未连接",
            Self::Connected => "TopSAP Client · 已连接",
            Self::Disconnected => "TopSAP Client · 已断开",
        }
    }
}

/// 原图标缩放后的像素数据（RGBA），着色时复用
struct SourceIcon {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
}

static SOURCE: OnceLock<Option<SourceIcon>> = OnceLock::new();
static EVER_CONNECTED: AtomicBool = AtomicBool::new(false);
static LAST_STATE: AtomicU8 = AtomicU8::new(0);

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
        .map(|icon| tint(icon, TrayState::Idle.color()))
}

/// 按引擎阶段更新托盘状态：只有 `Connected` 视为已连接
pub fn update_for_phase(app: &AppHandle, connected: bool) {
    let ever = EVER_CONNECTED.load(Ordering::SeqCst);
    let (state, ever_after) = resolve(connected, ever);
    EVER_CONNECTED.store(ever_after, Ordering::SeqCst);
    apply(app, state);
}

/// 状态推导：连接成功会记录「连接过」，此后断开显示已断开而不是未连接
fn resolve(connected: bool, ever_connected: bool) -> (TrayState, bool) {
    if connected {
        (TrayState::Connected, true)
    } else if ever_connected {
        (TrayState::Disconnected, true)
    } else {
        (TrayState::Idle, false)
    }
}

fn apply(app: &AppHandle, state: TrayState) {
    if LAST_STATE.swap(state.code(), Ordering::SeqCst) == state.code() {
        return;
    }
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    if let Some(icon) = icon_for(state) {
        let _ = tray.set_icon(Some(icon));
        // 运行期替换图标后再次关闭模板渲染，保证状态色不会被系统抹平
        #[cfg(target_os = "macos")]
        let _ = tray.set_icon_as_template(false);
    }
    let _ = tray.set_tooltip(Some(state.tooltip()));
}

fn icon_for(state: TrayState) -> Option<Image<'static>> {
    SOURCE
        .get()
        .and_then(|source| source.as_ref())
        .map(|source| tint(source, state.color()))
}

/// 把原图标转成指定颜色的剪影：
/// 先做盒式降采样，再按亮度把深色底与齿轮压成透明，保留盾牌轮廓与抗锯齿。
fn tint(source: &SourceIcon, color: [u8; 3]) -> Image<'static> {
    let size = ICON_SIZE;
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let x0 = x * source.width / size;
            let x1 = ((x + 1) * source.width / size).max(x0 + 1);
            let y0 = y * source.height / size;
            let y1 = ((y + 1) * source.height / size).max(y0 + 1);
            let (mut r, mut g, mut b, mut a, mut n) = (0u64, 0u64, 0u64, 0u64, 0u64);
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let index = ((sy * source.width + sx) * 4) as usize;
                    if index + 3 >= source.rgba.len() {
                        continue;
                    }
                    r += u64::from(source.rgba[index]);
                    g += u64::from(source.rgba[index + 1]);
                    b += u64::from(source.rgba[index + 2]);
                    a += u64::from(source.rgba[index + 3]);
                    n += 1;
                }
            }
            let n = n.max(1) as f64;
            let luma = 0.2126 * (r as f64 / n) + 0.7152 * (g as f64 / n) + 0.0722 * (b as f64 / n);
            let alpha_in = a as f64 / n;
            // 原图标深色圆角底与齿轮亮度约 10~30，盾牌约 145：阈值把前者压成透明
            let shape = ((luma - 48.0) / 96.0).clamp(0.0, 1.0);
            let alpha = (alpha_in * shape).round() as u8;
            out.extend_from_slice(&[color[0], color[1], color[2], alpha]);
        }
    }
    Image::new_owned(out, size, size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 状态推导区分未连接已连接与已断开() {
        let (idle, ever) = resolve(false, false);
        assert_eq!(idle, TrayState::Idle);
        assert!(!ever);

        let (connected, ever) = resolve(true, false);
        assert_eq!(connected, TrayState::Connected);
        assert!(ever);

        let (disconnected, ever) = resolve(false, true);
        assert_eq!(disconnected, TrayState::Disconnected);
        assert!(ever);
    }

    #[test]
    fn 三种状态颜色与提示都不同() {
        let idle = TrayState::Idle;
        let connected = TrayState::Connected;
        let disconnected = TrayState::Disconnected;
        assert_ne!(idle.color(), connected.color());
        assert_ne!(connected.color(), disconnected.color());
        assert_ne!(idle.color(), disconnected.color());
        assert_ne!(idle.tooltip(), connected.tooltip());
        assert_ne!(connected.tooltip(), disconnected.tooltip());
        assert!(disconnected.tooltip().contains("已断开"));
    }

    #[test]
    fn 着色保留盾牌剪影并压掉深色背景() {
        // 2x2 源图：左上深色底（应透明），右下亮色盾牌（应保留）
        let source = SourceIcon {
            rgba: vec![
                20, 24, 30, 255, 20, 24, 30, 255, 20, 24, 30, 255, 42, 168, 224, 255,
            ],
            width: 2,
            height: 2,
        };
        let image = tint(&source, [52, 199, 89]);
        assert_eq!(image.width(), ICON_SIZE);
        assert_eq!(image.height(), ICON_SIZE);
        let rgba = image.rgba();
        // 左上角来自深色底 -> 透明
        assert_eq!(rgba[3], 0);
        // 右下角来自盾牌 -> 不透明且使用状态色
        let last = rgba.len() - 4;
        assert!(rgba[last + 3] > 200, "盾牌区域应保留 alpha");
        assert_eq!(&rgba[last..last + 3], &[52, 199, 89]);
    }
}
