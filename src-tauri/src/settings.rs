//! 应用设置读写。

use std::path::PathBuf;

use tauri::AppHandle;

use crate::dto::AppSettings;

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = crate::paths::config_dir(app)?;
    Ok(dir.join("settings.json"))
}

pub fn load(app: &AppHandle) -> AppSettings {
    let Ok(path) = settings_path(app) else {
        return AppSettings::default();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return AppSettings::default();
    };
    serde_json::from_str::<AppSettings>(&text)
        .map(AppSettings::sanitized)
        .unwrap_or_default()
}

pub fn save(app: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    let path = settings_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建配置目录失败：{e}"))?;
    }
    let text = serde_json::to_string_pretty(&settings.clone().sanitized())
        .map_err(|e| format!("序列化设置失败：{e}"))?;
    std::fs::write(&path, text).map_err(|e| format!("写入设置失败：{e}"))
}
