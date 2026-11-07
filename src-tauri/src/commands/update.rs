//! 应用自动更新的命令入口。

use tauri::ipc::Channel;

use crate::update::{start_update as run_update, UpdateProgress};

/// 下载、断开连接并安装更新；成功后主程序会在数秒内退出
#[tauri::command]
pub async fn start_update(
    app: tauri::AppHandle,
    on_progress: Channel<UpdateProgress>,
) -> Result<(), String> {
    run_update(app, on_progress).await
}
