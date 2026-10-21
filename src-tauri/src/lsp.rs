use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Stdio},
    sync::{Arc, LazyLock, Mutex},
    thread,
};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

use crate::database::AppState;

struct ServerSpec {
    command: &'static str,
    args: &'static [&'static str],
}

const RUST: &[ServerSpec] = &[ServerSpec { command: "rust-analyzer", args: &[] }];
const PYTHON: &[ServerSpec] = &[
    ServerSpec { command: "pyright-langserver", args: &["--stdio"] },
    ServerSpec { command: "basedpyright-langserver", args: &["--stdio"] },
    ServerSpec { command: "pylsp", args: &[] },
];
const GO: &[ServerSpec] = &[ServerSpec { command: "gopls", args: &[] }];
const CPP: &[ServerSpec] = &[ServerSpec { command: "clangd", args: &[] }];
const VUE: &[ServerSpec] = &[ServerSpec { command: "vue-language-server", args: &["--stdio"] }];
const PHP: &[ServerSpec] = &[ServerSpec { command: "intelephense", args: &["--stdio"] }];
const RUBY: &[ServerSpec] = &[ServerSpec { command: "solargraph", args: &["stdio"] }];

fn specs(language: &str) -> &'static [ServerSpec] {
    match language {
        "rust" => RUST,
        "python" => PYTHON,
        "go" => GO,
        "cpp" => CPP,
        "vue" => VUE,
        "php" => PHP,
        "ruby" => RUBY,
        _ => &[],
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LspServerInfo {
    pub id: String,
    pub command: String,
    pub root_path: String,
    pub initialization_options: Option<Value>,
}

#[derive(Clone, Serialize)]
struct LspMessage {
    id: String,
    message: String,
}

#[derive(Clone, Serialize)]
struct LspExit {
    id: String,
}

struct Server {
    key: String,
    info: LspServerInfo,
    stdin: Mutex<ChildStdin>,
    child: Mutex<Child>,
}

static SERVERS: LazyLock<Mutex<HashMap<String, Arc<Server>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

fn servers() -> std::sync::MutexGuard<'static, HashMap<String, Arc<Server>>> {
    SERVERS.lock().unwrap_or_else(|error| error.into_inner())
}

/// 同一工作区同一语言只起一个进程；没有安装对应语言服务器时返回 `None`。
#[tauri::command]
pub async fn lsp_start(
    workspace_id: String,
    language: String,
    app: AppHandle,
) -> Result<Option<LspServerInfo>, String> {
    let root = {
        let state = app.state::<AppState>();
        crate::commands::workspace_root(&state, &workspace_id)?
    };
    tauri::async_runtime::spawn_blocking(move || start(app, &workspace_id, &root, &language))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub fn lsp_send(id: String, message: String) -> Result<(), String> {
    let server = servers()
        .get(&id)
        .cloned()
        .ok_or_else(|| "语言服务器已退出".to_string())?;
    let mut stdin = server.stdin.lock().unwrap_or_else(|error| error.into_inner());
    write_message(&mut *stdin, &message).map_err(|error| format!("发送给语言服务器失败：{error}"))
}

#[tauri::command]
pub fn lsp_stop(id: String) {
    let server = servers().remove(&id);
    if let Some(server) = server {
        let _ = server.child.lock().unwrap_or_else(|error| error.into_inner()).kill();
    }
}

fn start(app: AppHandle, workspace_id: &str, root: &str, language: &str) -> Result<Option<LspServerInfo>, String> {
    let key = format!("{workspace_id}\0{language}");
    if let Some(existing) = servers().values().find(|server| server.key == key) {
        return Ok(Some(existing.info.clone()));
    }
    let root_path = PathBuf::from(root);
    for spec in specs(language) {
        let Some(program) = resolve_program(&root_path, spec.command) else {
            continue;
        };
        let mut command = crate::process::command(&program);
        command
            .args(spec.args)
            .current_dir(&root_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|error| format!("启动 {} 失败：{error}", spec.command))?;
        let (Some(stdin), Some(stdout), Some(stderr)) = (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            let _ = child.kill();
            return Err(format!("{} 没有可用的标准输入输出", spec.command));
        };
        let info = LspServerInfo {
            id: Uuid::new_v4().to_string(),
            command: spec.command.to_string(),
            root_path: root.to_string(),
            initialization_options: initialization_options(language, &root_path),
        };
        let server = Arc::new(Server {
            key,
            info: info.clone(),
            stdin: Mutex::new(stdin),
            child: Mutex::new(child),
        });
        servers().insert(info.id.clone(), server);
        let id = info.id.clone();
        thread::spawn(move || read_loop(app, id, stdout));
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                if line.is_err() {
                    break;
                }
            }
        });
        return Ok(Some(info));
    }
    Ok(None)
}

/// 优先用项目里安装的版本（如 `node_modules/.bin/vue-language-server`），版本和项目依赖一致。
fn resolve_program(root: &Path, command: &str) -> Option<PathBuf> {
    let local = root.join("node_modules").join(".bin");
    let candidates: &[&str] = if cfg!(windows) { &[".cmd", ".exe", ""] } else { &[""] };
    for suffix in candidates {
        let path = local.join(format!("{command}{suffix}"));
        if path.is_file() {
            return Some(path);
        }
    }
    crate::runtime::resolve_runtime_command(command, "")
}

fn initialization_options(language: &str, root: &Path) -> Option<Value> {
    if language != "vue" {
        return None;
    }
    let tsdk = root.join("node_modules").join("typescript").join("lib");
    let mut options = json!({ "vue": { "hybridMode": false } });
    if tsdk.is_dir() {
        options["typescript"] = json!({ "tsdk": tsdk.to_string_lossy() });
    }
    Some(options)
}

fn read_loop(app: AppHandle, id: String, stdout: impl Read) {
    let mut reader = BufReader::new(stdout);
    while let Some(message) = read_message(&mut reader) {
        let _ = app.emit("lsp-message", LspMessage { id: id.clone(), message });
    }
    let removed = servers().remove(&id);
    if let Some(server) = removed {
        let _ = server.child.lock().unwrap_or_else(|error| error.into_inner()).wait();
    }
    let _ = app.emit("lsp-exit", LspExit { id });
}

fn read_message(reader: &mut impl BufRead) -> Option<String> {
    loop {
        let mut length = None;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).ok()? == 0 {
                return None;
            }
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            if let Some((name, value)) = line.split_once(':') {
                if name.trim().eq_ignore_ascii_case("content-length") {
                    length = value.trim().parse::<usize>().ok();
                }
            }
        }
        let Some(length) = length else {
            continue;
        };
        let mut body = vec![0; length];
        reader.read_exact(&mut body).ok()?;
        return Some(String::from_utf8_lossy(&body).into_owned());
    }
}

fn write_message(writer: &mut impl Write, message: &str) -> std::io::Result<()> {
    write!(writer, "Content-Length: {}\r\n\r\n", message.len())?;
    writer.write_all(message.as_bytes())?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn frames_round_trip_with_multibyte_bodies() {
        let mut buffer = Vec::new();
        write_message(&mut buffer, r#"{"a":"跨端"}"#).unwrap();
        write_message(&mut buffer, "{}").unwrap();
        let mut reader = Cursor::new(buffer);
        assert_eq!(read_message(&mut reader).as_deref(), Some(r#"{"a":"跨端"}"#));
        assert_eq!(read_message(&mut reader).as_deref(), Some("{}"));
        assert_eq!(read_message(&mut reader), None);
    }

    #[test]
    fn headers_are_case_insensitive_and_extra_headers_are_ignored() {
        let raw = "content-length: 2\r\nContent-Type: application/vscode-jsonrpc\r\n\r\n{}";
        let mut reader = Cursor::new(raw.as_bytes().to_vec());
        assert_eq!(read_message(&mut reader).as_deref(), Some("{}"));
    }

    #[test]
    fn unknown_languages_have_no_servers() {
        assert!(specs("typescript").is_empty());
        assert!(!specs("rust").is_empty());
    }
}
