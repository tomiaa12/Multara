//! 系统选择器：一次多选文件或文件夹（macOS 原生支持混选；其他平台退化为多选文件）。

use serde::Serialize;
use std::path::PathBuf;
use tauri::AppHandle;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PickedPath {
    pub path: String,
    pub is_directory: bool,
}

fn classify(path: PathBuf) -> Option<PickedPath> {
    let meta = std::fs::metadata(&path).ok()?;
    Some(PickedPath {
        path: path.to_string_lossy().into_owned(),
        is_directory: meta.is_dir(),
    })
}

fn classify_paths(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PickedPath> {
    paths.into_iter().filter_map(classify).collect()
}

#[cfg(target_os = "macos")]
fn pick_macos() -> Result<Vec<PickedPath>, String> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSModalResponseOK, NSOpenPanel};

    let mtm = MainThreadMarker::new().ok_or_else(|| "文件选择器必须在主线程运行".to_string())?;
    let panel = NSOpenPanel::openPanel(mtm);
    panel.setCanChooseFiles(true);
    panel.setCanChooseDirectories(true);
    panel.setAllowsMultipleSelection(true);
    panel.setCanCreateDirectories(false);
    panel.setResolvesAliases(true);
    let response = panel.runModal();
    if response != NSModalResponseOK {
        return Ok(Vec::new());
    }
    let urls = panel.URLs();
    let mut paths = Vec::new();
    for url in urls {
        let Some(ns_path) = url.path() else {
            continue;
        };
        paths.push(PathBuf::from(ns_path.to_string()));
    }
    Ok(classify_paths(paths))
}

#[cfg(not(target_os = "macos"))]
fn pick_other(app: &AppHandle) -> Result<Vec<PickedPath>, String> {
    use tauri_plugin_dialog::DialogExt;
    // Windows / Linux 系统对话框无法在同一窗口混选文件与文件夹，退化为多选文件。
    let selected = app.dialog().file().blocking_pick_files().unwrap_or_default();
    Ok(classify_paths(
        selected
            .into_iter()
            .filter_map(|path| path.into_path().ok()),
    ))
}

fn pick_blocking(app: AppHandle) -> Result<Vec<PickedPath>, String> {
    #[cfg(target_os = "macos")]
    {
        let (tx, rx) = std::sync::mpsc::sync_channel(0);
        app.run_on_main_thread(move || {
            let _ = tx.send(pick_macos());
        })
        .map_err(|error| error.to_string())?;
        rx.recv().map_err(|error| error.to_string())?
    }
    #[cfg(not(target_os = "macos"))]
    {
        pick_other(&app)
    }
}

#[tauri::command]
pub async fn select_files_or_folders(app: AppHandle) -> Result<Vec<PickedPath>, String> {
    tauri::async_runtime::spawn_blocking(move || pick_blocking(app))
        .await
        .map_err(|error| error.to_string())?
}
