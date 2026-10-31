use std::{
    collections::HashMap,
    io::{self, BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Stdio},
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc, Arc, LazyLock, Mutex,
    },
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

const RUST: &[ServerSpec] = &[ServerSpec {
    command: "rust-analyzer",
    args: &[],
}];
const PYTHON: &[ServerSpec] = &[
    ServerSpec {
        command: "pyright-langserver",
        args: &["--stdio"],
    },
    ServerSpec {
        command: "basedpyright-langserver",
        args: &["--stdio"],
    },
    ServerSpec {
        command: "pylsp",
        args: &[],
    },
];
const GO: &[ServerSpec] = &[ServerSpec {
    command: "gopls",
    args: &[],
}];
const CPP: &[ServerSpec] = &[ServerSpec {
    command: "clangd",
    args: &[],
}];
const VUE: &[ServerSpec] = &[ServerSpec {
    command: "vue-language-server",
    args: &["--stdio"],
}];
const PHP: &[ServerSpec] = &[ServerSpec {
    command: "intelephense",
    args: &["--stdio"],
}];
const RUBY: &[ServerSpec] = &[ServerSpec {
    command: "solargraph",
    args: &["stdio"],
}];

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

/// 单条消息体上限；超过视为协议损坏，结束该服务器。
const MAX_MESSAGE_BYTES: usize = 64 * 1024 * 1024;
const MAX_HEADER_LINE_BYTES: u64 = 8 * 1024;
/// 服务器不读 stdin 时排队待写的总字节上限，超过就报错而不是无限堆积。
const MAX_PENDING_BYTES: usize = 64 * 1024 * 1024;

struct Server {
    key: String,
    info: LspServerInfo,
    /// 交给该服务器自己的写线程，`lsp_send` 只入队不阻塞；同步命令按调用顺序入队，消息不会乱序。
    outbox: mpsc::Sender<String>,
    pending: Arc<AtomicUsize>,
    child: Mutex<Child>,
}

static SERVERS: LazyLock<Mutex<HashMap<String, Arc<Server>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
/// 让“是否已在运行”的检查和插入成为一步，避免并发 `lsp_start` 各起一个进程。
static START_LOCK: Mutex<()> = Mutex::new(());

fn servers() -> std::sync::MutexGuard<'static, HashMap<String, Arc<Server>>> {
    SERVERS.lock().unwrap_or_else(|error| error.into_inner())
}

fn reap(server: &Server) {
    let mut child = server
        .child
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let _ = child.kill();
    let _ = child.wait();
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
        .ok_or_else(|| tr!("语言服务器已退出"))?;
    let size = message.len();
    if server.pending.fetch_add(size, Ordering::AcqRel) + size > MAX_PENDING_BYTES {
        server.pending.fetch_sub(size, Ordering::AcqRel);
        return Err(tr!("发送给语言服务器失败：服务器长时间没有读取输入"));
    }
    server
        .outbox
        .send(message)
        .map_err(|_| tr!("语言服务器已退出"))
}

#[tauri::command]
pub fn lsp_stop(id: String) {
    let server = servers().remove(&id);
    if let Some(server) = server {
        thread::spawn(move || reap(&server));
    }
}

/// 应用退出时停掉所有语言服务器；进程退出不会跑 static 析构。
pub fn stop_all() {
    let servers: Vec<_> = servers().drain().map(|(_, server)| server).collect();
    for server in servers {
        reap(&server);
    }
}

fn start(
    app: AppHandle,
    workspace_id: &str,
    root: &str,
    language: &str,
) -> Result<Option<LspServerInfo>, String> {
    let _starting = START_LOCK.lock().unwrap_or_else(|error| error.into_inner());
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
            .map_err(|error| {
                tr!(
                    "启动 {command} 失败：{error}",
                    command = spec.command,
                    error = error
                )
            })?;
        let (Some(mut stdin), Some(stdout), Some(mut stderr)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(tr!(
                "{command} 没有可用的标准输入输出",
                command = spec.command
            ));
        };
        let info = LspServerInfo {
            id: Uuid::new_v4().to_string(),
            command: spec.command.to_string(),
            root_path: root.to_string(),
            initialization_options: initialization_options(language, &root_path),
        };
        let (outbox, inbox) = mpsc::channel::<String>();
        let pending = Arc::new(AtomicUsize::new(0));
        let writer_pending = Arc::clone(&pending);
        let server = Arc::new(Server {
            key,
            info: info.clone(),
            outbox,
            pending,
            child: Mutex::new(child),
        });
        servers().insert(info.id.clone(), server);
        let id = info.id.clone();
        thread::spawn(move || {
            for message in inbox {
                let written = write_message(&mut stdin, &message);
                writer_pending.fetch_sub(message.len(), Ordering::AcqRel);
                if written.is_err() {
                    break;
                }
            }
        });
        thread::spawn(move || read_loop(app, id, stdout));
        thread::spawn(move || {
            let _ = io::copy(&mut stderr, &mut io::sink());
        });
        return Ok(Some(info));
    }
    Ok(None)
}

/// 优先用项目里安装的版本（如 `node_modules/.bin/vue-language-server`），版本和项目依赖一致。
fn resolve_program(root: &Path, command: &str) -> Option<PathBuf> {
    let local = root.join("node_modules").join(".bin");
    let candidates: &[&str] = if cfg!(windows) {
        &[".cmd", ".exe", ""]
    } else {
        &[""]
    };
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
        let _ = app.emit(
            "lsp-message",
            LspMessage {
                id: id.clone(),
                message,
            },
        );
    }
    // 读失败也可能是协议损坏而进程还活着，先 kill 再 wait，否则 wait 会永远阻塞。
    let removed = servers().remove(&id);
    if let Some(server) = removed {
        reap(&server);
    }
    let _ = app.emit("lsp-exit", LspExit { id });
}

fn read_message(reader: &mut impl BufRead) -> Option<String> {
    loop {
        let mut length = None;
        loop {
            let mut line = String::new();
            let read = reader
                .by_ref()
                .take(MAX_HEADER_LINE_BYTES)
                .read_line(&mut line)
                .ok()?;
            if read == 0 || (!line.ends_with('\n') && read as u64 >= MAX_HEADER_LINE_BYTES) {
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
        if length > MAX_MESSAGE_BYTES {
            return None;
        }
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
        assert_eq!(
            read_message(&mut reader).as_deref(),
            Some(r#"{"a":"跨端"}"#)
        );
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
    fn oversized_frames_and_header_lines_are_rejected() {
        let huge = format!("Content-Length: {}\r\n\r\n{{}}", MAX_MESSAGE_BYTES + 1);
        assert_eq!(read_message(&mut Cursor::new(huge.into_bytes())), None);
        let long_header = format!(
            "X-Junk: {}\r\n\r\n",
            "a".repeat(MAX_HEADER_LINE_BYTES as usize * 2)
        );
        assert_eq!(
            read_message(&mut Cursor::new(long_header.into_bytes())),
            None
        );
    }

    #[test]
    fn unknown_languages_have_no_servers() {
        assert!(specs("typescript").is_empty());
        assert!(!specs("rust").is_empty());
    }
}
