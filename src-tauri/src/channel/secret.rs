//! 渠道凭据存储。macOS 走系统钥匙串，其他平台回退到应用数据目录下的 0600 文件。

use std::fs;
use std::path::PathBuf;

use tauri::{AppHandle, Manager};

const KEYCHAIN_SERVICE: &str = "multara.channel";

fn file_path(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("channel-secrets");
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    Ok(dir.join(format!("{id}.secret")))
}

#[cfg(target_os = "macos")]
fn keychain_get(id: &str) -> Option<String> {
    let output = crate::process::command("security")
        .args(["find-generic-password", "-s", KEYCHAIN_SERVICE, "-a", id, "-w"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!value.is_empty()).then_some(value)
}

#[cfg(target_os = "macos")]
fn keychain_set(id: &str, value: &str) -> Result<(), String> {
    let output = crate::process::command("security")
        .args([
            "add-generic-password",
            "-U",
            "-s",
            KEYCHAIN_SERVICE,
            "-a",
            id,
            "-w",
            value,
        ])
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

#[cfg(target_os = "macos")]
fn keychain_delete(id: &str) {
    let _ = crate::process::command("security")
        .args(["delete-generic-password", "-s", KEYCHAIN_SERVICE, "-a", id])
        .output();
}

pub fn store(app: &AppHandle, id: &str, value: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    if keychain_set(id, value).is_ok() {
        return Ok(());
    }
    let path = file_path(app, id)?;
    fs::write(&path, value).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub fn load(app: &AppHandle, id: &str) -> Option<String> {
    #[cfg(target_os = "macos")]
    if let Some(value) = keychain_get(id) {
        return Some(value);
    }
    let path = file_path(app, id).ok()?;
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn delete(app: &AppHandle, id: &str) {
    #[cfg(target_os = "macos")]
    keychain_delete(id);
    if let Ok(path) = file_path(app, id) {
        let _ = fs::remove_file(path);
    }
}
