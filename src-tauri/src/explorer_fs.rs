//! 资源管理器的文件操作：带撤销备份的回收站删除、复制与移动、二进制导入、
//! 目录监听、Git 忽略状态、系统剪贴板文件列表。行为对照 VS Code 的 explorer。

use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        mpsc::{self, RecvTimeoutError},
        Mutex, OnceLock,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use notify::{EventKind, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{
    commands::{setting_value, workspace_root},
    database::AppState,
    files,
};

/// 与 VS Code 的 `MAX_UNDO_FILE_SIZE` 一样：太大的条目不留备份，删除后不能撤销。
const MAX_BACKUP_BYTES: u64 = 64 * 1024 * 1024;
const MAX_BACKUP_FILES: usize = 5_000;
const BACKUP_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_COPY_DEPTH: usize = 64;
const WATCH_DEBOUNCE: Duration = Duration::from_millis(150);
const MAX_CHANGED_PATHS: usize = 500;
/// 回收站失败时的错误前缀。前端据此询问是否改为永久删除，与 VS Code 相同。
pub const TRASH_FAILED: &str = "TRASH_FAILED:";
/// 目标已存在时的错误前缀。前端据此询问是否覆盖。
pub const TARGET_EXISTS: &str = "EXISTS:";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerEntry {
    name: String,
    path: String,
    is_directory: bool,
    depth: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    symlink_target: Option<String>,
    mtime: u64,
    size: u64,
    readonly: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerStat {
    size: u64,
    mtime: u64,
    ctime: u64,
    readonly: bool,
    is_directory: bool,
    is_symlink: bool,
    absolute_path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteResult {
    path: String,
    backup_id: Option<String>,
    error: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferRequest {
    /// 为空时 `source` 是本机绝对路径（从系统剪贴板或拖入导入）。
    source_workspace_id: Option<String>,
    source: String,
    target_workspace_id: String,
    target: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferResult {
    target: String,
    error: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePathRef {
    workspace_id: String,
    path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardFile {
    path: String,
    name: String,
    is_directory: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitRemoteInfo {
    remote: String,
    branch: String,
    commit: String,
    prefix: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct FsChange {
    workspace_id: String,
    paths: Vec<String>,
    git: bool,
    overflow: bool,
}

fn root_for(app: &AppHandle, workspace_id: &str) -> Result<String, String> {
    let state = app.state::<AppState>();
    workspace_root(&state, workspace_id)
}

fn git_program(app: &AppHandle) -> String {
    let state = app.state::<AppState>();
    let Ok(db) = state.lock_db() else {
        return "git".to_string();
    };
    setting_value(&db, "gitExecutable", "git")
}

fn millis(time: std::io::Result<SystemTime>) -> u64 {
    time.ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_millis() as u64)
        .unwrap_or(0)
}

fn join_relative(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}/{name}")
    }
}

/// 工作区内的条目路径。不解析条目本身的符号链接，删除链接时只删链接。
fn entry_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let relative = files::normalize_relative(relative)?;
    if relative.is_empty() {
        return Err("不能操作工作区根目录".to_string());
    }
    let target = root.join(Path::new(&relative));
    let parent = target.parent().ok_or_else(|| "无效路径".to_string())?;
    let name = target.file_name().ok_or_else(|| "无效路径".to_string())?;
    let canonical_parent = if parent == root {
        root.to_path_buf()
    } else {
        parent
            .canonicalize()
            .map_err(|_| format!("父目录不存在：{}", parent.display()))?
    };
    Ok(canonical_parent.join(name))
}

/// 和 `entry_path` 一样，但会先建好缺失的父目录，用于新建 `a/b/c.ts`、粘贴和导入。
fn entry_path_creating_parents(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let normalized = files::normalize_relative(relative)?;
    if normalized.is_empty() {
        return Err("不能操作工作区根目录".to_string());
    }
    if let Some(parent) = Path::new(&normalized).parent() {
        if !parent.as_os_str().is_empty() {
            let parent_path = root.join(parent);
            if parent_path.exists() && !parent_path.is_dir() {
                return Err(format!("{} 不是文件夹", parent.display()));
            }
            fs::create_dir_all(&parent_path).map_err(|error| error.to_string())?;
        }
    }
    entry_path(root, &normalized)
}

fn exists_no_follow(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn remove_path(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() {
        return fs::remove_file(path)
            .or_else(|_| fs::remove_dir(path))
            .map_err(|error| error.to_string());
    }
    if metadata.is_dir() {
        fs::remove_dir_all(path).map_err(|error| error.to_string())
    } else {
        fs::remove_file(path).map_err(|error| error.to_string())
    }
}

fn copy_recursive(source: &Path, target: &Path, depth: usize) -> Result<(), String> {
    if depth > MAX_COPY_DEPTH {
        return Err("文件夹层级太深，无法复制".to_string());
    }
    let metadata = fs::metadata(source).map_err(|error| error.to_string())?;
    if metadata.is_dir() {
        fs::create_dir(target).map_err(|error| error.to_string())?;
        for child in fs::read_dir(source).map_err(|error| error.to_string())? {
            let child = child.map_err(|error| error.to_string())?;
            copy_recursive(&child.path(), &target.join(child.file_name()), depth + 1)?;
        }
        return Ok(());
    }
    fs::copy(source, target).map_err(|error| error.to_string())?;
    Ok(())
}

fn move_path(source: &Path, target: &Path) -> Result<(), String> {
    if fs::rename(source, target).is_ok() {
        return Ok(());
    }
    // 跨盘或跨卷时 rename 会失败，退回到复制后删除。
    copy_recursive(source, target, 0)?;
    remove_path(source)
}

fn measure(path: &Path, bytes: &mut u64, count: &mut usize) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    if metadata.file_type().is_symlink() {
        return false;
    }
    *count += 1;
    if *count > MAX_BACKUP_FILES {
        return false;
    }
    if metadata.is_dir() {
        let Ok(children) = fs::read_dir(path) else {
            return false;
        };
        for child in children.flatten() {
            if !measure(&child.path(), bytes, count) {
                return false;
            }
        }
        return true;
    }
    *bytes += metadata.len();
    *bytes <= MAX_BACKUP_BYTES
}

fn backup_root(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("explorer-undo");
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    Ok(dir)
}

fn prune_backups(dir: &Path) {
    let Ok(children) = fs::read_dir(dir) else {
        return;
    };
    let now = SystemTime::now();
    for child in children.flatten() {
        let old = child
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .map(|age| age > BACKUP_TTL)
            .unwrap_or(false);
        if old {
            let _ = fs::remove_dir_all(child.path());
        }
    }
}

fn backup_entry(target: &Path, dir: &Path) -> Option<String> {
    let mut bytes = 0;
    let mut count = 0;
    if !measure(target, &mut bytes, &mut count) {
        return None;
    }
    let id = uuid::Uuid::new_v4().to_string();
    let holder = dir.join(&id);
    fs::create_dir_all(&holder).ok()?;
    let name = target.file_name()?;
    if copy_recursive(target, &holder.join(name), 0).is_err() {
        let _ = fs::remove_dir_all(&holder);
        return None;
    }
    Some(id)
}

fn valid_backup_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|ch| ch.is_ascii_hexdigit() || ch == '-')
}

pub fn list_directory(root: &str, relative: &str) -> Result<Vec<ExplorerEntry>, String> {
    let root = files::canonical_root(root)?;
    let relative = files::normalize_relative(relative.trim_matches('/'))?;
    let directory = if relative.is_empty() {
        root.clone()
    } else {
        root.join(Path::new(&relative))
    };
    if !directory.is_dir() {
        return Err("只能展开文件夹".to_string());
    }
    let mut entries = Vec::new();
    for child in fs::read_dir(&directory).map_err(|error| error.to_string())? {
        let child = child.map_err(|error| error.to_string())?;
        let child_path = child.path();
        let name = child.file_name().to_string_lossy().to_string();
        let file_type = child.file_type().map_err(|error| error.to_string())?;
        let is_directory = files::followed_is_dir(&child_path, file_type);
        let metadata = fs::metadata(&child_path).or_else(|_| fs::symlink_metadata(&child_path));
        let (mtime, size, readonly) = match &metadata {
            Ok(meta) => (
                millis(meta.modified()),
                if is_directory { 0 } else { meta.len() },
                meta.permissions().readonly(),
            ),
            Err(_) => (0, 0, false),
        };
        entries.push(ExplorerEntry {
            path: join_relative(&relative, &name),
            name,
            is_directory,
            depth: if relative.is_empty() { 0 } else { relative.split('/').count() },
            symlink_target: if file_type.is_symlink() {
                files::symlink_target(&child_path)
            } else {
                None
            },
            mtime,
            size,
            readonly,
        });
    }
    entries.sort_by_key(|entry| (!entry.is_directory, entry.name.to_lowercase()));
    Ok(entries)
}

pub fn delete_entries(
    root: &str,
    paths: &[String],
    use_trash: bool,
    backups: &Path,
) -> Result<Vec<DeleteResult>, String> {
    let root = files::canonical_root(root)?;
    prune_backups(backups);
    let mut results = Vec::with_capacity(paths.len());
    for relative in paths {
        let normalized = files::normalize_relative(relative)?;
        let outcome = (|| -> Result<Option<String>, String> {
            let target = entry_path(&root, &normalized)?;
            if !exists_no_follow(&target) {
                return Err(format!("文件不存在：{normalized}"));
            }
            let backup_id = backup_entry(&target, backups);
            let removed = if use_trash {
                trash::delete(&target).map_err(|error| format!("{TRASH_FAILED}{error}"))
            } else {
                remove_path(&target)
            };
            if let Err(error) = removed {
                if let Some(id) = &backup_id {
                    let _ = fs::remove_dir_all(backups.join(id));
                }
                return Err(error);
            }
            Ok(backup_id)
        })();
        results.push(match outcome {
            Ok(backup_id) => DeleteResult { path: normalized, backup_id, error: None },
            Err(error) => DeleteResult { path: normalized, backup_id: None, error: Some(error) },
        });
    }
    Ok(results)
}

pub fn restore_entry(root: &str, relative: &str, backup_id: &str, backups: &Path) -> Result<(), String> {
    if !valid_backup_id(backup_id) {
        return Err("无效的撤销记录".to_string());
    }
    let holder = backups.join(backup_id);
    let stored = fs::read_dir(&holder)
        .map_err(|_| "撤销记录已过期".to_string())?
        .flatten()
        .next()
        .ok_or_else(|| "撤销记录已过期".to_string())?
        .path();
    let root = files::canonical_root(root)?;
    let target = entry_path_creating_parents(&root, relative)?;
    if exists_no_follow(&target) {
        return Err(format!("{TARGET_EXISTS}{relative}"));
    }
    move_path(&stored, &target)?;
    let _ = fs::remove_dir_all(holder);
    Ok(())
}

fn transfer_one(
    roots: &HashMap<String, PathBuf>,
    request: &TransferRequest,
    copy: bool,
    overwrite: bool,
) -> Result<String, String> {
    let source = match &request.source_workspace_id {
        Some(id) => {
            let root = roots.get(id).ok_or_else(|| "工作区不存在".to_string())?;
            entry_path(root, &request.source)?
        }
        None => {
            let path = PathBuf::from(&request.source);
            if !path.is_absolute() {
                return Err("无效的源路径".to_string());
            }
            path
        }
    };
    if !exists_no_follow(&source) {
        return Err(format!("文件不存在：{}", request.source));
    }
    let root = roots
        .get(&request.target_workspace_id)
        .ok_or_else(|| "工作区不存在".to_string())?;
    let target_relative = files::normalize_relative(&request.target)?;
    let target = entry_path_creating_parents(root, &target_relative)?;
    if source == target {
        return Err("源和目标相同".to_string());
    }
    let source_is_dir = fs::metadata(&source).map(|meta| meta.is_dir()).unwrap_or(false);
    if source_is_dir {
        let canonical_source = source.canonicalize().unwrap_or_else(|_| source.clone());
        let canonical_target_parent = target
            .parent()
            .and_then(|parent| parent.canonicalize().ok())
            .unwrap_or_default();
        if canonical_target_parent.starts_with(&canonical_source) {
            return Err("不能把文件夹放进它自己里面".to_string());
        }
    }
    if exists_no_follow(&target) {
        if !overwrite {
            return Err(format!("{TARGET_EXISTS}{target_relative}"));
        }
        remove_path(&target)?;
    }
    if copy {
        copy_recursive(&source, &target, 0)?;
    } else {
        move_path(&source, &target)?;
    }
    Ok(target_relative)
}

pub fn create_entry(root: &str, relative: &str, is_directory: bool) -> Result<(), String> {
    let root = files::canonical_root(root)?;
    let target = entry_path_creating_parents(&root, relative)?;
    if exists_no_follow(&target) {
        return Err(format!("{TARGET_EXISTS}{relative}"));
    }
    if is_directory {
        fs::create_dir(&target).map_err(|error| error.to_string())
    } else {
        fs::File::create(&target)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

pub fn write_bytes(root: &str, relative: &str, bytes: &[u8], overwrite: bool) -> Result<(), String> {
    let root = files::canonical_root(root)?;
    let target = entry_path_creating_parents(&root, relative)?;
    if exists_no_follow(&target) {
        if !overwrite {
            return Err(format!("{TARGET_EXISTS}{relative}"));
        }
        if fs::metadata(&target).map(|meta| meta.is_dir()).unwrap_or(false) {
            return Err("目标是文件夹，不能写入".to_string());
        }
    }
    fs::write(&target, bytes).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn explorer_list_directory(
    workspace_id: String,
    path: Option<String>,
    app: AppHandle,
) -> Result<Vec<ExplorerEntry>, String> {
    let root = root_for(&app, &workspace_id)?;
    tauri::async_runtime::spawn_blocking(move || list_directory(&root, path.as_deref().unwrap_or("")))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub fn explorer_stat(
    workspace_id: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<ExplorerStat, String> {
    let root = files::canonical_root(&workspace_root(&state, &workspace_id)?)?;
    let target = if path.trim_matches('/').is_empty() {
        root.clone()
    } else {
        entry_path(&root, &path)?
    };
    let link = fs::symlink_metadata(&target).map_err(|error| error.to_string())?;
    let metadata = fs::metadata(&target).unwrap_or_else(|_| link.clone());
    Ok(ExplorerStat {
        size: if metadata.is_dir() { 0 } else { metadata.len() },
        mtime: millis(metadata.modified()),
        ctime: millis(metadata.created()),
        readonly: metadata.permissions().readonly(),
        is_directory: metadata.is_dir(),
        is_symlink: link.file_type().is_symlink(),
        absolute_path: target.to_string_lossy().trim_start_matches(r"\\?\").to_string(),
    })
}

#[tauri::command]
pub async fn explorer_delete_entries(
    workspace_id: String,
    paths: Vec<String>,
    use_trash: bool,
    app: AppHandle,
) -> Result<Vec<DeleteResult>, String> {
    let root = root_for(&app, &workspace_id)?;
    let backups = backup_root(&app)?;
    tauri::async_runtime::spawn_blocking(move || delete_entries(&root, &paths, use_trash, &backups))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn explorer_restore_entry(
    workspace_id: String,
    path: String,
    backup_id: String,
    app: AppHandle,
) -> Result<(), String> {
    let root = root_for(&app, &workspace_id)?;
    let backups = backup_root(&app)?;
    tauri::async_runtime::spawn_blocking(move || restore_entry(&root, &path, &backup_id, &backups))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn explorer_transfer_entries(
    operations: Vec<TransferRequest>,
    copy: bool,
    overwrite: bool,
    app: AppHandle,
) -> Result<Vec<TransferResult>, String> {
    let mut roots = HashMap::new();
    for request in &operations {
        for id in [Some(&request.target_workspace_id), request.source_workspace_id.as_ref()]
            .into_iter()
            .flatten()
        {
            if !roots.contains_key(id) {
                let root = files::canonical_root(&root_for(&app, id)?)?;
                roots.insert(id.clone(), root);
            }
        }
    }
    tauri::async_runtime::spawn_blocking(move || {
        operations
            .iter()
            .map(|request| match transfer_one(&roots, request, copy, overwrite) {
                Ok(target) => TransferResult { target, error: None },
                Err(error) => TransferResult {
                    target: request.target.clone(),
                    error: Some(error),
                },
            })
            .collect()
    })
    .await
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn explorer_create_entry(
    workspace_id: String,
    path: String,
    is_directory: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let root = workspace_root(&state, &workspace_id)?;
    create_entry(&root, &path, is_directory)
}

#[tauri::command]
pub async fn explorer_write_file(
    workspace_id: String,
    path: String,
    base64: String,
    overwrite: bool,
    app: AppHandle,
) -> Result<(), String> {
    let root = root_for(&app, &workspace_id)?;
    let bytes = STANDARD.decode(base64.as_bytes()).map_err(|error| error.to_string())?;
    tauri::async_runtime::spawn_blocking(move || write_bytes(&root, &path, &bytes, overwrite))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn explorer_git_ignored(
    workspace_id: String,
    paths: Vec<String>,
    app: AppHandle,
) -> Result<Vec<String>, String> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let root = root_for(&app, &workspace_id)?;
    let git = git_program(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let mut child = match crate::process::command(&git)
            .args(["-C", &root, "check-ignore", "--stdin", "-z"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => child,
            Err(_) => return Ok(Vec::new()),
        };
        if let Some(mut stdin) = child.stdin.take() {
            let input = paths.join("\0");
            let _ = stdin.write_all(input.as_bytes());
            let _ = stdin.write_all(b"\0");
        }
        let output = child.wait_with_output().map_err(|error| error.to_string())?;
        // 0 表示有忽略项，1 表示没有，128 表示不在仓库里。
        if !matches!(output.status.code(), Some(0)) {
            return Ok(Vec::new());
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .split('\0')
            .filter(|item| !item.is_empty())
            .map(|item| item.trim_end_matches('/').to_string())
            .collect())
    })
    .await
    .map_err(|error| error.to_string())?
}

fn git_line(git: &str, root: &str, args: &[&str]) -> Option<String> {
    let output = crate::process::command(git)
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[tauri::command]
pub async fn explorer_git_remote(
    workspace_id: String,
    app: AppHandle,
) -> Result<Option<GitRemoteInfo>, String> {
    let root = root_for(&app, &workspace_id)?;
    let git = git_program(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let Some(remote) = git_line(&git, &root, &["remote", "get-url", "origin"]) else {
            return Ok(None);
        };
        Ok(Some(GitRemoteInfo {
            remote,
            branch: git_line(&git, &root, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_default(),
            commit: git_line(&git, &root, &["rev-parse", "HEAD"]).unwrap_or_default(),
            prefix: git_line(&git, &root, &["rev-parse", "--show-prefix"]).unwrap_or_default(),
        }))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub fn explorer_open_external(
    workspace_id: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let root = files::canonical_root(&workspace_root(&state, &workspace_id)?)?;
    let target = entry_path(&root, &path)?;
    if !exists_no_follow(&target) {
        return Err("文件不存在".to_string());
    }
    #[cfg(target_os = "windows")]
    let program = "explorer";
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(all(unix, not(target_os = "macos")))]
    let program = "xdg-open";
    crate::process::command(program)
        .arg(&target)
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn explorer_absolute_path(
    workspace_id: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let root = files::canonical_root(&workspace_root(&state, &workspace_id)?)?;
    let target = if path.trim_matches('/').is_empty() {
        root
    } else {
        entry_path(&root, &path)?
    };
    Ok(target.to_string_lossy().trim_start_matches(r"\\?\").to_string())
}

#[tauri::command]
pub fn explorer_clipboard_write_files(
    entries: Vec<WorkspacePathRef>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut paths = Vec::with_capacity(entries.len());
    for entry in entries {
        let root = files::canonical_root(&workspace_root(&state, &entry.workspace_id)?)?;
        let target = entry_path(&root, &entry.path)?;
        paths.push(target.to_string_lossy().trim_start_matches(r"\\?\").to_string());
    }
    write_clipboard_files(&paths)
}

#[tauri::command]
pub fn explorer_clipboard_read_files() -> Result<Vec<ClipboardFile>, String> {
    Ok(read_clipboard_files()
        .into_iter()
        .filter_map(|path| {
            let buf = PathBuf::from(&path);
            let metadata = fs::metadata(&buf).ok()?;
            Some(ClipboardFile {
                name: buf.file_name()?.to_string_lossy().to_string(),
                is_directory: metadata.is_dir(),
                path,
            })
        })
        .collect())
}

#[cfg(windows)]
fn write_clipboard_files(paths: &[String]) -> Result<(), String> {
    let _clip = clipboard_win::Clipboard::new_attempts(10).map_err(|error| error.to_string())?;
    clipboard_win::raw::empty().map_err(|error| error.to_string())?;
    clipboard_win::raw::set_file_list(paths).map_err(|error| error.to_string())
}

#[cfg(windows)]
fn read_clipboard_files() -> Vec<String> {
    let Ok(_clip) = clipboard_win::Clipboard::new_attempts(10) else {
        return Vec::new();
    };
    let mut files = Vec::new();
    if clipboard_win::raw::get_file_list(&mut files).is_err() {
        return Vec::new();
    }
    files
}

#[cfg(target_os = "macos")]
fn write_clipboard_files(paths: &[String]) -> Result<(), String> {
    let list = paths
        .iter()
        .map(|path| format!("POSIX file \"{}\"", path.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect::<Vec<_>>()
        .join(", ");
    let status = crate::process::command("osascript")
        .args(["-e", &format!("set the clipboard to {{{list}}}")])
        .status()
        .map_err(|error| error.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("无法写入系统剪贴板".to_string())
    }
}

#[cfg(target_os = "macos")]
fn read_clipboard_files() -> Vec<String> {
    let script = "try\nset theItems to the clipboard as «class furl»\nreturn POSIX path of theItems\non error\ntry\nset out to \"\"\nrepeat with f in (the clipboard as list)\ntry\nset out to out & POSIX path of (f as alias) & linefeed\nend try\nend repeat\nreturn out\non error\nreturn \"\"\nend try\nend try";
    crate::process::command("osascript")
        .args(["-e", script])
        .output()
        .ok()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(|line| line.trim().trim_end_matches('/').to_string())
                .filter(|line| line.starts_with('/'))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(all(unix, not(target_os = "macos")))]
fn write_clipboard_files(_paths: &[String]) -> Result<(), String> {
    Err("当前系统不支持把文件写入剪贴板".to_string())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn read_clipboard_files() -> Vec<String> {
    Vec::new()
}

struct WatchRegistry {
    /// 每个工作台实例（owner）各自要监听的工作区，实际监听取并集。
    owners: HashMap<String, HashSet<String>>,
    watchers: HashMap<String, notify::RecommendedWatcher>,
}

fn registry() -> &'static Mutex<WatchRegistry> {
    static REGISTRY: OnceLock<Mutex<WatchRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        Mutex::new(WatchRegistry {
            owners: HashMap::new(),
            watchers: HashMap::new(),
        })
    })
}

fn strip_verbatim(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) => PathBuf::from(rest),
        None => path,
    }
}

fn relative_change(path: &Path, roots: &[PathBuf]) -> Option<String> {
    for root in roots {
        if let Ok(rest) = path.strip_prefix(root) {
            return Some(rest.to_string_lossy().replace('\\', "/"));
        }
    }
    None
}

/// `.git` 里只有这些变化会影响 Git 状态；对象库和日志的写入不需要刷新。
fn is_git_state_change(relative: &str) -> bool {
    relative == ".git/index"
        || relative == ".git/HEAD"
        || relative == ".git/MERGE_HEAD"
        || relative.starts_with(".git/refs/")
}

fn flush_changes(app: &AppHandle, workspace_id: &str, roots: &[PathBuf], events: Vec<notify::Event>) {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    let mut git = false;
    let mut overflow = false;
    for event in events {
        if matches!(event.kind, EventKind::Access(_)) {
            continue;
        }
        if event.need_rescan() {
            overflow = true;
        }
        for path in &event.paths {
            let Some(relative) = relative_change(path, roots) else {
                continue;
            };
            if relative == ".git" || relative.starts_with(".git/") {
                git |= is_git_state_change(&relative);
                continue;
            }
            if relative.is_empty() || !seen.insert(relative.clone()) {
                continue;
            }
            if paths.len() >= MAX_CHANGED_PATHS {
                overflow = true;
                continue;
            }
            paths.push(relative);
        }
    }
    if paths.is_empty() && !git && !overflow {
        return;
    }
    if overflow {
        paths.clear();
    }
    let _ = app.emit(
        "explorer-fs-changed",
        FsChange {
            workspace_id: workspace_id.to_string(),
            paths,
            git,
            overflow,
        },
    );
}

fn start_watch(app: AppHandle, workspace_id: String, root: String) -> Result<notify::RecommendedWatcher, String> {
    let root_path = PathBuf::from(&root);
    let mut roots = vec![root_path.clone()];
    if let Ok(canonical) = root_path.canonicalize() {
        roots.push(strip_verbatim(canonical.clone()));
        roots.push(canonical);
    }
    let (sender, receiver) = mpsc::channel::<notify::Event>();
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
        if let Ok(event) = result {
            let _ = sender.send(event);
        }
    })
    .map_err(|error| error.to_string())?;
    watcher
        .watch(&root_path, RecursiveMode::Recursive)
        .map_err(|error| error.to_string())?;
    thread::spawn(move || loop {
        // 监听器被移除后发送端随之释放，这里收到断开就退出线程。
        let Ok(first) = receiver.recv() else {
            return;
        };
        let mut batch = vec![first];
        let deadline = Instant::now() + WATCH_DEBOUNCE;
        let mut closed = false;
        loop {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            match receiver.recv_timeout(deadline - now) {
                Ok(event) => batch.push(event),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => {
                    closed = true;
                    break;
                }
            }
        }
        flush_changes(&app, &workspace_id, &roots, batch);
        if closed {
            return;
        }
    });
    Ok(watcher)
}

#[tauri::command]
pub fn explorer_watch_workspaces(
    owner: String,
    workspace_ids: Vec<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut registry = registry().lock().map_err(|error| error.to_string())?;
    if workspace_ids.is_empty() {
        registry.owners.remove(&owner);
    } else {
        registry.owners.insert(owner, workspace_ids.into_iter().collect());
    }
    let wanted: HashSet<String> = registry.owners.values().flatten().cloned().collect();
    registry.watchers.retain(|id, _| wanted.contains(id));
    for id in wanted {
        if registry.watchers.contains_key(&id) {
            continue;
        }
        let Ok(root) = workspace_root(&state, &id) else {
            continue;
        };
        match start_watch(app.clone(), id.clone(), root) {
            Ok(watcher) => {
                registry.watchers.insert(id, watcher);
            }
            Err(error) => crate::diag::write(&format!("监听工作区 {id} 失败：{error}")),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{
        create_entry, delete_entries, list_directory, restore_entry, transfer_one, write_bytes,
        TransferRequest, TARGET_EXISTS,
    };

    fn roots(path: &std::path::Path) -> std::collections::HashMap<String, std::path::PathBuf> {
        let mut map = std::collections::HashMap::new();
        map.insert("w".to_string(), path.canonicalize().unwrap());
        map
    }

    #[test]
    fn creates_nested_paths_and_lists_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_str().unwrap();
        create_entry(root, "src/deep/a.ts", false).unwrap();
        create_entry(root, "src/empty", true).unwrap();
        assert!(temp.path().join("src/deep/a.ts").is_file());
        let error = create_entry(root, "src/empty", true).unwrap_err();
        assert!(error.starts_with(TARGET_EXISTS));
        let listed = list_directory(root, "src").unwrap();
        assert_eq!(listed.len(), 2);
        assert!(listed[0].is_directory);
        assert_eq!(listed[0].path, "src/deep");
        assert!(listed.iter().all(|entry| entry.mtime > 0));
    }

    #[test]
    fn deletes_with_backup_and_restores() {
        let temp = tempfile::tempdir().unwrap();
        let backups = tempfile::tempdir().unwrap();
        let root = temp.path().to_str().unwrap();
        fs::create_dir(temp.path().join("docs")).unwrap();
        fs::write(temp.path().join("docs/a.md"), "hello").unwrap();
        let results = delete_entries(root, &["docs".to_string()], false, backups.path()).unwrap();
        assert!(results[0].error.is_none());
        assert!(!temp.path().join("docs").exists());
        let backup = results[0].backup_id.clone().unwrap();
        restore_entry(root, "docs", &backup, backups.path()).unwrap();
        assert_eq!(fs::read_to_string(temp.path().join("docs/a.md")).unwrap(), "hello");
    }

    #[test]
    fn copies_moves_and_rejects_conflicts() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("a")).unwrap();
        fs::write(temp.path().join("a/x.txt"), "x").unwrap();
        fs::create_dir(temp.path().join("b")).unwrap();
        let map = roots(temp.path());
        let request = |source: &str, target: &str| TransferRequest {
            source_workspace_id: Some("w".to_string()),
            source: source.to_string(),
            target_workspace_id: "w".to_string(),
            target: target.to_string(),
        };
        transfer_one(&map, &request("a/x.txt", "b/x.txt"), true, false).unwrap();
        assert!(temp.path().join("a/x.txt").exists());
        let conflict = transfer_one(&map, &request("a/x.txt", "b/x.txt"), true, false).unwrap_err();
        assert!(conflict.starts_with(TARGET_EXISTS));
        transfer_one(&map, &request("a/x.txt", "b/x.txt"), false, true).unwrap();
        assert!(!temp.path().join("a/x.txt").exists());
        let into_self = transfer_one(&map, &request("b", "b/inner/b"), false, false).unwrap_err();
        assert!(into_self.contains("自己"));
    }

    #[test]
    fn writes_binary_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_str().unwrap();
        let bytes = [0x89_u8, 0x50, 0x4E, 0x47, 0, 1, 2];
        write_bytes(root, "img/logo.png", &bytes, false).unwrap();
        assert_eq!(fs::read(temp.path().join("img/logo.png")).unwrap(), bytes);
    }
}
