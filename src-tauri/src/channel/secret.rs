//! 渠道凭据存储。macOS 走系统钥匙串，其他平台回退到应用数据目录下的 0600 文件。

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

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

/// 密钥放在命令行参数里本机任何用户 `ps` 都能看到，这里改用 `security -i` 从 stdin 读命令。
/// 交互模式下命令失败退出码仍是 0，所以写完再读回来比对。
#[cfg(target_os = "macos")]
fn keychain_set(id: &str, value: &str) -> Result<(), String> {
    use std::io::Write;
    if value.contains(['\n', '\r']) || id.contains(['\n', '\r']) {
        return Err("凭据不能包含换行".to_string());
    }
    let quote = |text: &str| format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""));
    let mut child = crate::process::command("security")
        .arg("-i")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    let line = format!(
        "add-generic-password -U -s {} -a {} -w {}\n",
        quote(KEYCHAIN_SERVICE),
        quote(id),
        quote(value)
    );
    let written = child
        .stdin
        .take()
        .ok_or_else(|| "无法写入钥匙串命令".to_string())
        .and_then(|mut stdin| stdin.write_all(line.as_bytes()).map_err(|error| error.to_string()));
    let output = child.wait_with_output().map_err(|error| error.to_string())?;
    written?;
    if keychain_get(id).as_deref() == Some(value) {
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

/// 每次发送都会取凭据，macOS 上每次都是一个 `security` 子进程；只缓存读到的值，读不到下次再试。
static CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

fn cache() -> MutexGuard<'static, HashMap<String, String>> {
    CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

pub fn store(app: &AppHandle, id: &str, value: &str) -> Result<(), String> {
    cache().remove(id);
    #[cfg(target_os = "macos")]
    if keychain_set(id, value).is_ok() {
        cache().insert(id.to_string(), value.to_string());
        return Ok(());
    }
    let path = file_path(app, id)?;
    crate::mcp_vars::write_private_atomic(&path, value.as_bytes())?;
    cache().insert(id.to_string(), value.to_string());
    Ok(())
}

pub fn load(app: &AppHandle, id: &str) -> Option<String> {
    if let Some(value) = cache().get(id) {
        return Some(value.clone());
    }
    let value = load_uncached(app, id)?;
    cache().insert(id.to_string(), value.clone());
    Some(value)
}

fn load_uncached(app: &AppHandle, id: &str) -> Option<String> {
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
    cache().remove(id);
    #[cfg(target_os = "macos")]
    keychain_delete(id);
    if let Ok(path) = file_path(app, id) {
        let _ = fs::remove_file(path);
    }
}
