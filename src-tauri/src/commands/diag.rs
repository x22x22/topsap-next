//! 诊断命令：网卡自检、残留检测与清理。

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::dto::{SelfTestDto, SelfTestStepDto};
use crate::netcfg;
use crate::tun::selftest::{self, StepStatus};
use crate::AppState;

static SELF_TEST_RUNNING: AtomicBool = AtomicBool::new(false);
const SELF_TEST_TIMEOUT: Duration = Duration::from_secs(45);

/// 自检结束后恢复运行标记，超时或 panic 都不会永久卡住
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
    if SELF_TEST_RUNNING.swap(true, Ordering::SeqCst) {
        return Err("自检正在进行中，请稍候".to_string());
    }
    let _guard = SelfTestGuard;

    let report = tokio::time::timeout(
        SELF_TEST_TIMEOUT,
        selftest::run(&state.dll_path, |step| {
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
        }),
    )
    .await
    .map_err(|_| "自检超时（45 秒），已中止；下次自检会先清理上次残留的虚拟网卡".to_string())?;

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
