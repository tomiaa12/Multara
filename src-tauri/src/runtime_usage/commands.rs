//! 前端调用的用量探测命令。

use tauri::AppHandle;

use super::{probe, RuntimeUsage};

#[tauri::command]
pub async fn probe_runtime_usage(
    runtime_id: String,
    _app: AppHandle,
) -> Result<RuntimeUsage, String> {
    let _span = crate::diag::span("probe_runtime_usage");
    tauri::async_runtime::spawn_blocking(move || probe(&runtime_id))
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn probe_runtime_usages(
    runtime_ids: Vec<String>,
    _app: AppHandle,
) -> Result<Vec<RuntimeUsage>, String> {
    let _span = crate::diag::span("probe_runtime_usages");
    tauri::async_runtime::spawn_blocking(move || {
        runtime_ids
            .into_iter()
            .filter(|id| super::is_supported(id))
            .map(|id| probe(&id))
            .collect()
    })
    .await
    .map_err(|error| error.to_string())
}
