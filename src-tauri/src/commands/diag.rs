//! 诊断命令：网卡自检、残留检测与清理。

use serde::Serialize;
use tauri::State;

use crate::dto::{SelfTestDto, SelfTestStepDto};
use crate::netcfg;
use crate::tun::selftest::{self, StepStatus};
use crate::AppState;

#[tauri::command]
pub async fn run_self_test(state: State<'_, AppState>) -> Result<SelfTestDto, String> {
    let report = selftest::run(&state.dll_path).await;
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
