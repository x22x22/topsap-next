//! 托盘图标状态：未连接与已连接使用不同颜色的图标。
//!
//! - 未连接：灰色空心圆环
//! - 已连接：绿色实心圆点
//!
//! 状态由 `Engine::set_phase` 驱动，仅在「是否已连接」真正变化时才调用系统接口，
//! 避免每次阶段切换都重设图标。

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::image::Image;
use tauri::AppHandle;

/// 托盘图标 id，与 `setup_tray` 创建时保持一致
pub const TRAY_ID: &str = "topsap-status";

const ICON_IDLE: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/icons/tray-off.png"));
const ICON_CONNECTED: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/icons/tray-on.png"));

static CONNECTED: AtomicBool = AtomicBool::new(false);

/// 按连接状态加载对应的托盘图标
pub fn icon(connected: bool) -> Option<Image<'static>> {
    Image::from_bytes(if connected { ICON_CONNECTED } else { ICON_IDLE }).ok()
}

/// 托盘提示文案，鼠标悬停时显示当前状态
pub fn tooltip(connected: bool) -> &'static str {
    if connected {
        "TopSAP Client · 已连接"
    } else {
        "TopSAP Client · 未连接"
    }
}

/// 更新托盘图标与提示；状态未变化时不重复调用系统接口
pub fn set_connected(app: &AppHandle, connected: bool) {
    if CONNECTED.swap(connected, Ordering::SeqCst) == connected {
        return;
    }
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    if let Some(icon) = icon(connected) {
        let _ = tray.set_icon(Some(icon));
        // 运行期替换图标后再次关闭模板渲染，保证颜色区分不会被系统抹平
        #[cfg(target_os = "macos")]
        let _ = tray.set_icon_as_template(false);
    }
    let _ = tray.set_tooltip(Some(tooltip(connected)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 两种状态图标都能解析且尺寸一致() {
        let idle = icon(false).expect("未连接图标应可解析");
        let on = icon(true).expect("已连接图标应可解析");
        assert_eq!(idle.width(), on.width());
        assert_eq!(idle.height(), on.height());
        assert!(idle.width() > 0);
    }

    #[test]
    fn 提示文案区分连接状态() {
        assert!(tooltip(true).contains("已连接"));
        assert!(tooltip(false).contains("未连接"));
        assert_ne!(tooltip(true), tooltip(false));
    }
}
