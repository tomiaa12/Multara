use std::{
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
};

use base64::{engine::general_purpose::STANDARD, Engine as _};

use crate::models::{FileContent, FileEntry};

const MAX_EDITABLE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_READABLE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_MEDIA_PREVIEW_BYTES: u64 = 32 * 1024 * 1024;
const MEDIA_PREVIEW_LIMIT_MB: u64 = MAX_MEDIA_PREVIEW_BYTES / (1024 * 1024);
const MAX_TREE_ENTRIES: usize = 5_000;
const MAX_QUICK_OPEN_FILES: usize = 20_000;
/// 读文件时文件已不在的错误前缀，前端据此把标签标成已删除，不看文案。
pub const FILE_MISSING: &str = "NOT_FOUND:";

/// `files.exclude` 的默认值。资源管理器隐藏这些名字。
const EXPLORER_EXCLUDE: &[&str] = &[".git", ".svn", ".hg", ".jj", ".DS_Store", "Thumbs.db"];
/// `search.exclude` 的默认值，另外还继承 `files.exclude`。全文搜索和快速打开使用。
const SEARCH_EXCLUDE_DIRS: &[&str] = &["node_modules", "bower_components"];

pub fn hidden_in_explorer(name: &str) -> bool {
    EXPLORER_EXCLUDE.contains(&name)
}

pub fn hidden_in_search(name: &str) -> bool {
    hidden_in_explorer(name)
        || SEARCH_EXCLUDE_DIRS.contains(&name)
        || name.ends_with(".code-search")
}
const MAX_TREE_DEPTH: usize = 12;

pub(crate) fn followed_is_dir(path: &Path, file_type: fs::FileType) -> bool {
    if file_type.is_symlink() {
        return fs::metadata(path)
            .map(|meta| meta.is_dir())
            .unwrap_or(false);
    }
    file_type.is_dir()
}

pub(crate) fn symlink_target(path: &Path) -> Option<String> {
    let raw = fs::read_link(path).ok()?;
    let absolute = if raw.is_absolute() {
        raw
    } else {
        path.parent().map(|parent| parent.join(&raw)).unwrap_or(raw)
    };
    let resolved = fs::canonicalize(&absolute).unwrap_or(absolute);
    Some(display_path(&resolved))
}

fn is_worktree_git_pointer(path: &Path, name: &str) -> bool {
    if name != ".git" {
        return false;
    }
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    if !metadata.is_file() || metadata.len() > 4096 {
        return false;
    }
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    String::from_utf8_lossy(&bytes)
        .trim_start()
        .starts_with("gitdir:")
}

fn display_path(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
        .map(|value| value.replace('\\', "/"));
    if let Some(home) = home {
        if text == home {
            return "~".to_string();
        }
        let prefix = format!("{home}/");
        if let Some(rest) = text.strip_prefix(&prefix) {
            return format!("~/{rest}");
        }
    }
    text
}

/// 给 Command/Ctrl+P 用：跳过依赖目录，只返回文件，避免整棵树被 node_modules 撑爆后一次都搜不到。
pub fn list_quick_open_files(root: &str) -> Result<Vec<FileEntry>, String> {
    let root = canonical_root(root)?;
    let mut entries = Vec::new();
    walk_files(&root, &root, 0, &mut entries)?;
    Ok(entries)
}

fn walk_files(
    root: &Path,
    directory: &Path,
    depth: usize,
    output: &mut Vec<FileEntry>,
) -> Result<(), String> {
    if depth > MAX_TREE_DEPTH || output.len() >= MAX_QUICK_OPEN_FILES {
        return Ok(());
    }
    let Ok(children) = fs::read_dir(directory) else {
        return Ok(());
    };
    for child in children.flatten() {
        if output.len() >= MAX_QUICK_OPEN_FILES {
            break;
        }
        let Ok(file_type) = child.file_type() else {
            continue;
        };
        let name = child.file_name().to_string_lossy().to_string();
        if is_worktree_git_pointer(&child.path(), &name) || hidden_in_search(&name) {
            continue;
        }
        let is_directory = followed_is_dir(&child.path(), file_type);
        let relative = child
            .path()
            .strip_prefix(root)
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        if is_directory {
            walk_files(root, &child.path(), depth + 1, output)?;
            continue;
        }
        output.push(FileEntry {
            name,
            path: relative,
            is_directory: false,
            depth,
            symlink_target: symlink_target(&child.path()),
        });
    }
    Ok(())
}

pub fn list_tree(workspace_id: &str, root: &str) -> Result<Vec<FileEntry>, String> {
    let root = canonical_root(root)?;
    let mut entries = Vec::new();
    walk(&root, &root, 0, &mut entries)?;
    let _ = workspace_id;
    Ok(entries)
}

pub fn list_directory(
    workspace_id: &str,
    root: &str,
    relative_path: Option<&str>,
) -> Result<Vec<FileEntry>, String> {
    let root = canonical_root(root)?;
    let relative_path = relative_path.unwrap_or("").trim_matches('/');
    let directory = listing_directory(&root, relative_path)?;
    let mut children = fs::read_dir(&directory)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    children.sort_by_key(|entry| {
        let path = entry.path();
        let is_directory = entry
            .file_type()
            .map(|kind| followed_is_dir(&path, kind))
            .unwrap_or(false);
        (
            !is_directory,
            entry.file_name().to_string_lossy().to_lowercase(),
        )
    });
    let mut entries = Vec::with_capacity(children.len());
    for child in children {
        let file_type = child.file_type().map_err(|error| error.to_string())?;
        let name = child.file_name().to_string_lossy().to_string();
        let child_path = child.path();
        if is_worktree_git_pointer(&child_path, &name) || hidden_in_explorer(&name) {
            continue;
        }
        let path = child_path
            .strip_prefix(&root)
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        entries.push(FileEntry {
            name,
            path,
            is_directory: followed_is_dir(&child_path, file_type),
            depth: 0,
            symlink_target: symlink_target(&child_path),
        });
    }
    let _ = workspace_id;
    Ok(entries)
}

pub fn read(workspace_id: &str, root: &str, relative_path: &str) -> Result<FileContent, String> {
    let root = canonical_root(root)?;
    // 经符号链接读到根目录外的文件仍可查看，但只读：写入会被 `write_or_create` 拒绝。
    let path = root
        .join(normalize_relative(relative_path)?)
        .canonicalize()
        .map_err(|error| format!("{FILE_MISSING}{}", tr!("文件不存在：{error}", error = error)))?;
    let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err(tr!("只能打开文件"));
    }
    if metadata.len() > MAX_READABLE_BYTES {
        return Err(tr!("文件超过 128MB，无法安全载入编辑器"));
    }
    let bytes = fs::read(&path).map_err(|error| error.to_string())?;
    file_content(
        workspace_id.to_string(),
        normalize_relative(relative_path)?,
        &path,
        &bytes,
        metadata.len(),
        metadata.len() > MAX_EDITABLE_BYTES || !path.starts_with(&root),
    )
}

/// 用户拖入的工作区外路径：只读，不要求落在某个需求工作区里。
pub fn read_absolute(path: &str) -> Result<FileContent, String> {
    let path = Path::new(path);
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err(tr!("只能打开文件"));
    }
    if metadata.len() > MAX_READABLE_BYTES {
        return Err(tr!("文件超过 128MB，无法安全载入编辑器"));
    }
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    file_content(
        String::new(),
        path.to_string_lossy().replace('\\', "/"),
        path,
        &bytes,
        metadata.len(),
        true,
    )
}

fn file_content(
    workspace_id: String,
    path_label: String,
    path: &Path,
    bytes: &[u8],
    size: u64,
    read_only: bool,
) -> Result<FileContent, String> {
    if let Some(media) = media_preview(path) {
        if size > MAX_MEDIA_PREVIEW_BYTES {
            return Err(tr!(
                "文件超过 {limit}MB，无法预览",
                limit = MEDIA_PREVIEW_LIMIT_MB
            ));
        }
        let encoded = STANDARD.encode(bytes);
        return Ok(FileContent {
            workspace_id,
            path: path_label,
            content: format!("data:{};base64,{encoded}", media.mime),
            language: language_for_path(path).to_string(),
            size,
            read_only: true,
            media: media.kind.to_string(),
        });
    }
    Ok(FileContent {
        workspace_id,
        path: path_label,
        content: decode_text(bytes)?,
        language: language_for_path(path).to_string(),
        size,
        read_only,
        media: String::new(),
    })
}

struct MediaKind {
    kind: &'static str,
    mime: &'static str,
}

/// 与 VS Code media-preview 相同：图片、音频、视频。
fn media_preview(path: &Path) -> Option<MediaKind> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match extension.as_str() {
        "jpg" | "jpe" | "jpeg" => MediaKind {
            kind: "image",
            mime: "image/jpeg",
        },
        "png" => MediaKind {
            kind: "image",
            mime: "image/png",
        },
        "bmp" => MediaKind {
            kind: "image",
            mime: "image/bmp",
        },
        "gif" => MediaKind {
            kind: "image",
            mime: "image/gif",
        },
        "ico" => MediaKind {
            kind: "image",
            mime: "image/x-icon",
        },
        "webp" => MediaKind {
            kind: "image",
            mime: "image/webp",
        },
        "avif" => MediaKind {
            kind: "image",
            mime: "image/avif",
        },
        "svg" => MediaKind {
            kind: "image",
            mime: "image/svg+xml",
        },
        "mp3" => MediaKind {
            kind: "audio",
            mime: "audio/mpeg",
        },
        "wav" => MediaKind {
            kind: "audio",
            mime: "audio/wav",
        },
        "ogg" | "oga" => MediaKind {
            kind: "audio",
            mime: "audio/ogg",
        },
        "mp4" => MediaKind {
            kind: "video",
            mime: "video/mp4",
        },
        "webm" => MediaKind {
            kind: "video",
            mime: "video/webm",
        },
        _ => return None,
    })
}

pub fn list_absolute(path: &str) -> Result<Vec<FileEntry>, String> {
    list_directory("", &expand_user_path(path), None)
}

fn expand_user_path(path: &str) -> String {
    if path != "~" && !path.starts_with("~/") && !path.starts_with("~\\") {
        return path.to_string();
    }
    let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) else {
        return path.to_string();
    };
    if path == "~" {
        return home;
    }
    format!("{}{}{}", home, std::path::MAIN_SEPARATOR, &path[2..])
}

pub fn write(root: &str, relative_path: &str, content: &str) -> Result<(), String> {
    write_or_create(root, relative_path, content)
}

/// 写入文本文件；路径不存在时创建（回滚被删除的文件）。
pub fn write_or_create(root: &str, relative_path: &str, content: &str) -> Result<(), String> {
    if content.len() as u64 > MAX_EDITABLE_BYTES {
        return Err(tr!("文件超过 64MB，只能只读打开"));
    }
    let root = canonical_root(root)?;
    let relative = normalize_relative(relative_path)?;
    if relative.is_empty() {
        return Err(tr!("无效路径"));
    }
    let relative = Path::new(&relative);
    let canonical_parent =
        create_parents_inside(&root, relative.parent().unwrap_or(Path::new("")))?;
    let name = relative.file_name().ok_or_else(|| tr!("无效路径"))?;
    write_atomic(&canonical_parent.join(name), content)
}

/// 同目录 tempfile + persist，给编辑器保存共用。
pub fn write_atomic(target: &Path, content: &str) -> Result<(), String> {
    if content.len() as u64 > MAX_EDITABLE_BYTES {
        return Err(tr!("文件超过 64MB，只能只读打开"));
    }
    let parent = target.parent().ok_or_else(|| tr!("无效文件路径"))?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
    temporary
        .write_all(content.as_bytes())
        .map_err(|error| error.to_string())?;
    temporary.flush().map_err(|error| error.to_string())?;
    temporary
        .persist(target)
        .map_err(|error| tr!("保存文件失败：{error}", error = error.error))?;
    Ok(())
}

pub fn create_file(root: &str, relative_path: &str, content: &str) -> Result<(), String> {
    if content.len() as u64 > MAX_EDITABLE_BYTES {
        return Err(tr!("文件超过 64MB，无法创建"));
    }
    let root = canonical_root(root)?;
    let target = resolve_inside(&root, relative_path)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::AlreadyExists => tr!("文件已存在"),
            _ => error.to_string(),
        })?;
    file.write_all(content.as_bytes())
        .map_err(|error| error.to_string())
}

pub fn create_directory(root: &str, relative_path: &str) -> Result<(), String> {
    let root = canonical_root(root)?;
    let target = resolve_inside(&root, relative_path)?;
    if fs::symlink_metadata(&target).is_ok() {
        return Err(tr!("文件夹已存在"));
    }
    fs::create_dir(&target).map_err(|error| error.to_string())?;
    Ok(())
}

pub fn rename_entry(root: &str, from: &str, to: &str) -> Result<(), String> {
    let root = canonical_root(root)?;
    if normalize_relative(from)?.is_empty() {
        return Err(tr!("不能重命名工作区根目录"));
    }
    let source = resolve_inside(&root, from)?;
    fs::symlink_metadata(&source).map_err(|error| tr!("文件不存在：{error}", error = error))?;
    let dest = resolve_inside(&root, to)?;
    if fs::symlink_metadata(&dest).is_ok() {
        return Err(tr!("目标已存在"));
    }
    fs::rename(source, dest).map_err(|error| error.to_string())?;
    Ok(())
}

pub fn delete_entry(root: &str, relative_path: &str) -> Result<(), String> {
    let root = canonical_root(root)?;
    if normalize_relative(relative_path)?.is_empty() {
        return Err(tr!("不能删除工作区根目录"));
    }
    remove_path(&resolve_inside(&root, relative_path)?)
}

/// 符号链接只删链接本身，不跟随到目标。
pub(crate) fn remove_path(path: &Path) -> Result<(), String> {
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

pub fn reveal(root: &str, relative_path: &str) -> Result<(), String> {
    let root = canonical_root(root)?;
    let target = if relative_path.trim_matches('/').is_empty() {
        root
    } else {
        resolve_existing(&root, relative_path)?
    };
    reveal_path(&target)
}

/// 工作区内的条目路径：父目录解析后必须仍在根目录内；最后一段不跟随符号链接，
/// 所以删除、重命名、覆盖都作用在链接本身。
pub(crate) fn resolve_inside(root: &Path, relative_path: &str) -> Result<PathBuf, String> {
    let relative = normalize_relative(relative_path)?;
    if relative.is_empty() {
        return Err(tr!("无效路径"));
    }
    let target = root.join(Path::new(&relative));
    let parent = target.parent().ok_or_else(|| tr!("无效路径"))?;
    let canonical_parent = if parent == root {
        root.to_path_buf()
    } else {
        ensure_inside(root, parent).map_err(|error| {
            if parent.exists() {
                error
            } else {
                tr!("父目录不存在")
            }
        })?
    };
    let name = target.file_name().ok_or_else(|| tr!("无效路径"))?;
    Ok(canonical_parent.join(name))
}

/// `root` 必须已经 canonicalize。
pub(crate) fn ensure_inside(root: &Path, path: &Path) -> Result<PathBuf, String> {
    let canonical = path
        .canonicalize()
        .map_err(|error| tr!("文件不存在：{error}", error = error))?;
    if !canonical.starts_with(root) {
        return Err(tr!("路径指向工作区之外"));
    }
    Ok(canonical)
}

/// 逐级创建缺失的父目录，每一级都校验没有经符号链接跳出根目录，返回 canonical 的父目录。
pub(crate) fn create_parents_inside(
    root: &Path,
    relative_parent: &Path,
) -> Result<PathBuf, String> {
    let mut current = root.to_path_buf();
    for component in relative_parent.components() {
        let next = current.join(component);
        if fs::symlink_metadata(&next).is_err() {
            fs::create_dir(&next).map_err(|error| error.to_string())?;
        }
        current = ensure_inside(root, &next)?;
        if !current.is_dir() {
            return Err(tr!(
                "{path} 不是文件夹",
                path = relative_parent.display()
            ));
        }
    }
    Ok(current)
}

pub(crate) fn reveal_path(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn()
            .map_err(|error| error.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .args(["-R", &path.to_string_lossy()])
            .spawn()
            .map_err(|error| error.to_string())?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let directory = if path.is_dir() {
            path
        } else {
            path.parent().unwrap_or(path)
        };
        std::process::Command::new("xdg-open")
            .arg(directory)
            .spawn()
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn walk(
    root: &Path,
    directory: &Path,
    depth: usize,
    output: &mut Vec<FileEntry>,
) -> Result<(), String> {
    if depth > MAX_TREE_DEPTH || output.len() >= MAX_TREE_ENTRIES {
        return Ok(());
    }
    let mut children = fs::read_dir(directory)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    children.sort_by_key(|entry| {
        let path = entry.path();
        let is_directory = entry
            .file_type()
            .map(|kind| followed_is_dir(&path, kind))
            .unwrap_or(false);
        (
            !is_directory,
            entry.file_name().to_string_lossy().to_lowercase(),
        )
    });

    for child in children {
        if output.len() >= MAX_TREE_ENTRIES {
            break;
        }
        let file_type = child.file_type().map_err(|error| error.to_string())?;
        let name = child.file_name().to_string_lossy().to_string();
        if is_worktree_git_pointer(&child.path(), &name) || hidden_in_explorer(&name) {
            continue;
        }
        let is_directory = followed_is_dir(&child.path(), file_type);
        let relative = child
            .path()
            .strip_prefix(root)
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        output.push(FileEntry {
            name,
            path: relative,
            is_directory,
            depth,
            symlink_target: symlink_target(&child.path()),
        });
        if is_directory {
            walk(root, &child.path(), depth + 1, output)?;
        }
    }
    Ok(())
}

pub(crate) fn canonical_root(root: &str) -> Result<PathBuf, String> {
    Path::new(root)
        .canonicalize()
        .map_err(|error| tr!("工作区不可用：{error}", error = error))
}

pub fn resolve_existing_or_new(root: &str, relative_path: &str) -> Result<PathBuf, String> {
    let root = canonical_root(root)?;
    let relative = normalize_relative(relative_path)?;
    if relative.is_empty() {
        return Err(tr!("无效路径"));
    }
    let target = root.join(Path::new(&relative));
    if target.exists() {
        return target.canonicalize().map_err(|error| error.to_string());
    }
    Ok(target)
}

fn listing_directory(root: &Path, relative_path: &str) -> Result<PathBuf, String> {
    if relative_path.is_empty() {
        return Ok(root.to_path_buf());
    }
    let relative = normalize_relative(relative_path)?;
    let directory = root.join(&relative);
    if directory.is_dir() {
        return Ok(directory);
    }
    Err(tr!("只能展开文件夹"))
}

pub(crate) fn resolve_existing(root: &Path, relative_path: &str) -> Result<PathBuf, String> {
    ensure_inside(root, &root.join(normalize_relative(relative_path)?))
}

pub fn normalize_relative(path: &str) -> Result<String, String> {
    let path = Path::new(path);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(tr!("无效的相对路径"));
    }
    Ok(path.to_string_lossy().replace('\\', "/"))
}

const BINARY_SNIFF_BYTES: usize = 8_192;

fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(BINARY_SNIFF_BYTES).any(|byte| *byte == 0)
}

fn strip_utf8_bom(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text)
}

fn decode_utf16(bytes: &[u8], little: bool) -> Option<String> {
    let body = bytes.get(2..)?;
    if body.len() % 2 != 0 {
        return None;
    }
    let units: Vec<u16> = body
        .chunks_exact(2)
        .map(|chunk| {
            if little {
                u16::from_le_bytes([chunk[0], chunk[1]])
            } else {
                u16::from_be_bytes([chunk[0], chunk[1]])
            }
        })
        .collect();
    String::from_utf16(&units).ok()
}

fn utf8_invalid_byte_count(bytes: &[u8]) -> usize {
    let mut invalid = 0;
    let mut rest = bytes;
    loop {
        match std::str::from_utf8(rest) {
            Ok(_) => return invalid,
            Err(error) => {
                let valid_up_to = error.valid_up_to();
                match error.error_len() {
                    Some(len) => {
                        invalid += len;
                        rest = &rest[valid_up_to + len..];
                    }
                    None => return invalid + rest.len() - valid_up_to,
                }
            }
        }
    }
}

fn decode_text(bytes: &[u8]) -> Result<String, String> {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        if let Some(text) = decode_utf16(bytes, true) {
            return Ok(text);
        }
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        if let Some(text) = decode_utf16(bytes, false) {
            return Ok(text);
        }
    }
    if looks_binary(bytes) {
        return Err(tr!("暂不支持编辑二进制文件"));
    }
    if let Ok(text) = std::str::from_utf8(bytes) {
        return Ok(strip_utf8_bom(text).to_string());
    }
    // 合法 UTF-8 超过约 80% 时按 UTF-8 容错，避免个别非法字节把整篇中文按 GB18030 解成乱码。
    let invalid = utf8_invalid_byte_count(bytes);
    let valid = bytes.len().saturating_sub(invalid);
    if invalid > 0 && valid.saturating_mul(5) >= bytes.len().saturating_mul(4) {
        return Ok(strip_utf8_bom(&String::from_utf8_lossy(bytes)).to_string());
    }
    let (text, _, had_errors) = encoding_rs::GB18030.decode(bytes);
    if !had_errors {
        return Ok(text.into_owned());
    }
    Err(tr!("暂不支持编辑二进制文件"))
}

pub fn language_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
    {
        "ts" | "tsx" => "typescript",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "vue" => "html",
        "rs" => "rust",
        "py" => "python",
        "go" => "go",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "swift" => "swift",
        "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" => "cpp",
        "cs" => "csharp",
        "rb" => "ruby",
        "php" => "php",
        "sql" => "sql",
        "graphql" | "gql" => "graphql",
        "xml" | "svg" => "xml",
        "json" => "json",
        "md" => "markdown",
        "css" | "scss" | "less" => "css",
        "html" | "htm" => "html",
        "toml" | "ini" | "conf" => "ini",
        "yml" | "yaml" => "yaml",
        "sh" | "zsh" | "bash" => "shell",
        _ => "plaintext",
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use super::{
        create_directory, create_file, decode_text, delete_entry, list_absolute, list_directory,
        list_tree, normalize_relative, read, read_absolute, rename_entry, write,
    };

    #[test]
    fn rejects_parent_and_absolute_paths() {
        assert!(normalize_relative("../secret").is_err());
        assert!(normalize_relative("/tmp/secret").is_err());
    }

    #[test]
    fn reads_and_atomically_writes_text_inside_root() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("src")).unwrap();
        fs::write(temp.path().join("src/main.ts"), "const before = true;").unwrap();

        let tree = list_tree("workspace", temp.path().to_str().unwrap()).unwrap();
        assert!(tree.iter().any(|entry| entry.path == "src/main.ts"));
        let root = list_directory("workspace", temp.path().to_str().unwrap(), None).unwrap();
        assert_eq!(root.len(), 1);
        assert_eq!(root[0].path, "src");
        let source =
            list_directory("workspace", temp.path().to_str().unwrap(), Some("src")).unwrap();
        assert_eq!(source[0].path, "src/main.ts");
        let file = read("workspace", temp.path().to_str().unwrap(), "src/main.ts").unwrap();
        assert_eq!(file.language, "typescript");
        write(
            temp.path().to_str().unwrap(),
            "src/main.ts",
            "const after = true;",
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(temp.path().join("src/main.ts")).unwrap(),
            "const after = true;"
        );
    }

    #[test]
    fn opens_text_files_larger_than_two_megabytes() {
        let temp = tempfile::tempdir().unwrap();
        let content = "a".repeat(3 * 1024 * 1024);
        fs::write(temp.path().join("large.log"), &content).unwrap();

        let file = read("workspace", temp.path().to_str().unwrap(), "large.log").unwrap();

        assert_eq!(file.size, content.len() as u64);
        assert_eq!(file.content.len(), content.len());
        assert!(!file.read_only);
    }

    #[test]
    fn shows_git_metadata_build_output_and_symlinks() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join(".git"),
            "gitdir: H:/repo/.git/worktrees/abc\n",
        )
        .unwrap();
        fs::write(temp.path().join(".gitignore"), "dist\n").unwrap();
        fs::create_dir(temp.path().join("node_modules")).unwrap();
        fs::create_dir(temp.path().join("dist")).unwrap();
        fs::write(temp.path().join("README.md"), "ok").unwrap();
        let outside = temp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("pkg.js"), "module.exports = 1\n").unwrap();
        symlink_to(&outside, &temp.path().join("linked_modules"), true);
        symlink_to(
            &temp.path().join("README.md"),
            &temp.path().join("readme-link"),
            false,
        );

        let root = list_directory("workspace", temp.path().to_str().unwrap(), None).unwrap();
        let names: Vec<_> = root.iter().map(|entry| entry.name.as_str()).collect();
        assert!(!names.contains(&".git"));
        assert!(names.contains(&".gitignore"));
        assert!(names.contains(&"README.md"));
        assert!(names.contains(&"node_modules"));
        assert!(names.contains(&"dist"));
        assert!(names.contains(&"linked_modules"));
        assert!(names.contains(&"readme-link"));
        let linked_modules = root
            .iter()
            .find(|entry| entry.name == "linked_modules")
            .unwrap();
        assert!(linked_modules.is_directory);
        assert!(linked_modules
            .symlink_target
            .as_deref()
            .unwrap()
            .ends_with("/outside"));
        let readme_link = root
            .iter()
            .find(|entry| entry.name == "readme-link")
            .unwrap();
        assert!(!readme_link.is_directory);
        assert!(readme_link
            .symlink_target
            .as_deref()
            .unwrap()
            .ends_with("/README.md"));
        assert!(root
            .iter()
            .find(|entry| entry.name == "README.md")
            .unwrap()
            .symlink_target
            .is_none());

        let linked = list_directory(
            "workspace",
            temp.path().to_str().unwrap(),
            Some("linked_modules"),
        )
        .unwrap();
        assert_eq!(linked[0].path, "linked_modules/pkg.js");

        let tree = list_tree("workspace", temp.path().to_str().unwrap()).unwrap();
        assert!(!tree.iter().any(|entry| entry.name == ".git"));

        fs::remove_file(temp.path().join(".git")).unwrap();
        fs::create_dir(temp.path().join(".git")).unwrap();
        fs::write(temp.path().join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        // `.git` 在 EXPLORER_EXCLUDE 里，目录形态也不出现在列表中。
        let with_directory =
            list_directory("workspace", temp.path().to_str().unwrap(), None).unwrap();
        assert!(!with_directory.iter().any(|entry| entry.name == ".git"));
    }

    fn symlink_to(source: &Path, dest: &Path, directory: bool) {
        #[cfg(unix)]
        {
            let _ = directory;
            std::os::unix::fs::symlink(source, dest).unwrap();
        }
        #[cfg(windows)]
        {
            let result = if directory {
                std::os::windows::fs::symlink_dir(source, dest)
            } else {
                std::os::windows::fs::symlink_file(source, dest)
            };
            result.unwrap();
        }
    }

    #[test]
    fn creates_renames_and_deletes_entries() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_str().unwrap();
        create_directory(root, "docs").unwrap();
        create_file(root, "docs/note.txt", "hello").unwrap();
        assert_eq!(
            fs::read_to_string(temp.path().join("docs/note.txt")).unwrap(),
            "hello"
        );
        rename_entry(root, "docs/note.txt", "docs/readme.txt").unwrap();
        assert!(!temp.path().join("docs/note.txt").exists());
        delete_entry(root, "docs/readme.txt").unwrap();
        delete_entry(root, "docs").unwrap();
        assert!(!temp.path().join("docs").exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_pointing_outside_cannot_be_used_to_escape_the_root() {
        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("keep.txt"), "keep").unwrap();
        symlink_to(outside.path(), &workspace.path().join("link"), true);
        let root = workspace.path().to_str().unwrap();

        assert!(write(root, "link/keep.txt", "overwritten").is_err());
        assert!(write(root, "link/new/file.txt", "x").is_err());
        assert!(create_file(root, "link/created.txt", "x").is_err());
        assert!(create_directory(root, "link/dir").is_err());
        assert!(rename_entry(root, "link/keep.txt", "moved.txt").is_err());
        assert!(!outside.path().join("new").exists());
        assert!(!outside.path().join("created.txt").exists());
        let linked = read("workspace", root, "link/keep.txt").unwrap();
        assert!(linked.read_only);

        delete_entry(root, "link").unwrap();
        assert!(fs::symlink_metadata(workspace.path().join("link")).is_err());
        assert_eq!(
            fs::read_to_string(outside.path().join("keep.txt")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn keeps_utf8_chinese_when_a_few_bytes_are_invalid() {
        let mut bytes = "安装文档\n请先安装".as_bytes().to_vec();
        bytes.push(0x80);
        let text = decode_text(&bytes).unwrap();
        assert!(text.contains("安装文档"));
        assert!(text.contains("请先安装"));
    }

    #[test]
    fn reads_gbk_and_utf16_text_files() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_str().unwrap();
        let (gbk, _, _) = encoding_rs::GB18030.encode("你好\n");
        fs::write(temp.path().join("notes.txt"), gbk.as_ref()).unwrap();
        let file = read("workspace", root, "notes.txt").unwrap();
        assert_eq!(file.content, "你好\n");

        let mut utf16 = vec![0xFF, 0xFE];
        for unit in "hi".encode_utf16() {
            utf16.extend_from_slice(&unit.to_le_bytes());
        }
        fs::write(temp.path().join("wide.txt"), utf16).unwrap();
        let wide = read("workspace", root, "wide.txt").unwrap();
        assert_eq!(wide.content, "hi");
    }

    #[test]
    fn reads_absolute_paths_outside_any_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("spec.md");
        fs::write(&file, "# 外部文档\n").unwrap();
        fs::create_dir(temp.path().join("docs")).unwrap();
        fs::write(temp.path().join("docs/a.md"), "a").unwrap();

        let loaded = read_absolute(file.to_str().unwrap()).unwrap();
        assert_eq!(loaded.content, "# 外部文档\n");
        assert!(loaded.workspace_id.is_empty());
        assert!(loaded.read_only);

        let listing = list_absolute(temp.path().join("docs").to_str().unwrap()).unwrap();
        assert_eq!(listing.len(), 1);
        assert_eq!(listing[0].name, "a.md");
    }

    #[test]
    fn previews_png_instead_of_rejecting_it() {
        let temp = tempfile::tempdir().unwrap();
        let bytes = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        fs::write(temp.path().join("status_closed_black.png"), bytes).unwrap();
        let file = read(
            "workspace",
            temp.path().to_str().unwrap(),
            "status_closed_black.png",
        )
        .unwrap();
        assert_eq!(file.media, "image");
        assert!(file.read_only);
        assert!(file.content.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn rejects_nul_binary_files() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("blob.bin"), [0_u8, 1, 2, 3]).unwrap();
        let error = match read("workspace", temp.path().to_str().unwrap(), "blob.bin") {
            Ok(_) => panic!("expected binary files to be rejected"),
            Err(message) => message,
        };
        assert!(error.contains("二进制"));
    }
}
