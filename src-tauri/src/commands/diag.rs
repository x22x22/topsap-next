//! 诊断命令：网卡自检、残留检测与清理。

use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::dto::{SelfTestDto, SelfTestStepDto};
use crate::netcfg;
use crate::tun::selftest::{self, StepStatus};
use crate::AppState;

static SELF_TEST_RUNNING: AtomicBool = AtomicBool::new(false);

/// 供连接命令判断是否与自检冲突
pub fn self_test_running() -> bool {
    SELF_TEST_RUNNING.load(Ordering::SeqCst)
}

/// 自检结束后恢复运行标记，panic 也不会永久卡住
struct SelfTestGuard;

impl Drop for SelfTestGuard {
    fn drop(&mut self) {
        SELF_TEST_RUNNING.store(false, Ordering::SeqCst);
    }
}

#[tauri::command]
pub async fn run_self_test(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SelfTestDto, String> {
    if state.engine.is_connected() {
        return Err("请先断开连接后再运行自检，避免误删正在使用的虚拟网卡".to_string());
    }
    if state.engine.is_busy() || state.engine.needs_cleanup() {
        return Err("请先等待当前连接流程结束或断开连接后再运行自检".to_string());
    }
    if SELF_TEST_RUNNING.swap(true, Ordering::SeqCst) {
        return Err("自检正在进行中，请稍候".to_string());
    }
    let _guard = SelfTestGuard;

    let report = selftest::run(&state.dll_path, |step| {
        let _ = app.emit(
            "self_test://step",
            SelfTestStepDto {
                name: step.name.to_string(),
                status: match step.status {
                    StepStatus::Ok => "ok",
                    StepStatus::Fail => "fail",
                    StepStatus::Info => "info",
                }
                .to_string(),
                detail: step.detail.clone(),
            },
        );
    })
    .await;

    Ok(SelfTestDto {
        ok: report.ok,
        passed: report.passed,
        total: report.total,
        steps: report
            .steps
            .into_iter()
            .map(|step| SelfTestStepDto {
                name: step.name.to_string(),
                status: match step.status {
                    StepStatus::Ok => "ok",
                    StepStatus::Fail => "fail",
                    StepStatus::Info => "info",
                }
                .to_string(),
                detail: step.detail,
            })
            .collect(),
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct ResidueReport {
    pub hosts: bool,
}

#[tauri::command]
pub async fn check_residue() -> ResidueReport {
    ResidueReport {
        hosts: netcfg::has_hosts_residue().await,
    }
}

#[tauri::command]
pub async fn purge_residue() -> Result<bool, String> {
    netcfg::purge_hosts_residue()
        .await
        .map_err(|e| e.to_string())
}
