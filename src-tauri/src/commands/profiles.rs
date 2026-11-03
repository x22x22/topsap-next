//! 连接配置命令。

use tauri::AppHandle;

use crate::profiles::{self, ConnectionProfile, ProfileStore};

#[tauri::command]
pub fn list_connections(app: AppHandle) -> ProfileStore {
    profiles::load(&app)
}

#[tauri::command]
pub fn save_connection(
    app: AppHandle,
    profile: ConnectionProfile,
) -> Result<ConnectionProfile, String> {
    profiles::upsert(&app, profile)
}

#[tauri::command]
pub fn delete_connection(app: AppHandle, name: String) -> Result<(), String> {
    profiles::remove(&app, &name)
}

#[tauri::command]
pub fn set_active_connection(app: AppHandle, name: String) -> Result<(), String> {
    profiles::set_active(&app, &name)
}

#[tauri::command]
pub fn import_connections(app: AppHandle, path: String) -> Result<usize, String> {
    profiles::import_from(&app, &path)
}

#[tauri::command]
pub fn export_connections(app: AppHandle, path: String) -> Result<usize, String> {
    profiles::export_to(&app, &path)
}

#[tauri::command]
pub fn clear_connections(app: AppHandle) -> Result<(), String> {
    profiles::clear(&app)
}
