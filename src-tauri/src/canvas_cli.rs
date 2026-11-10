//! `multara canvas ...` 子命令（方案 §5.6）。
//!
//! - 有 `MULTARA_CTL`（智能体运行中）：POST 给应用的控制通道，应用负责写库、托管、推送界面。
//! - 没有（应用没开，或用户在自己终端里跑）：直接写库。`open` / `close` / `status` 只写库，
//!   stderr 提示下次显示时生效；`action` 返回 `canvas.app_not_running`。
//! - stdout 一律 JSON：`{ ok, value }` / `{ ok:false, code, message }`；提示走 stderr。
//!   退出码：1 参数错误，2 运行环境问题，3 Canvas 错误。

use std::{
    collections::HashMap,
    env,
    io::{Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    time::Duration,
};

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::ops::canvas::{self as ops, CanvasError, Caller, OpenRequest};

pub struct CliOutput {
    pub stdout: String,
    pub stderr: Vec<String>,
    pub code: i32,
}

pub fn help() -> &'static str {
    "multara canvas list
multara canvas open --dir <path> [--instance <id>] [--title ...] [--input-json '{}']
multara canvas open --id <canvasId> [--instance <id>] [--input-json '{}']
multara canvas action <instanceId> <name> [--input-json '{}'] [--timeout 15]
multara canvas context <instanceId>
multara canvas status <instanceId> --text \"...\"
multara canvas close <instanceId>"
}

struct Args {
    positional: Vec<String>,
    flags: HashMap<String, String>,
}

fn parse(args: &[String]) -> Args {
    let mut positional = Vec::new();
    let mut flags = HashMap::new();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if let Some(name) = arg.strip_prefix("--").filter(|name| !name.is_empty()) {
            if let Some((name, value)) = name.split_once('=') {
                flags.insert(name.to_string(), value.to_string());
                index += 1;
                continue;
            }
            match args.get(index + 1) {
                Some(value) if !value.starts_with("--") => {
                    flags.insert(name.to_string(), value.clone());
                    index += 2;
                }
                _ => {
                    flags.insert(name.to_string(), "true".to_string());
                    index += 1;
                }
            }
            continue;
        }
        positional.push(arg.clone());
        index += 1;
    }
    Args { positional, flags }
}

fn fail(error: CanvasError) -> CliOutput {
    CliOutput {
        stdout: error.to_json().to_string(),
        stderr: vec![error.message.clone()],
        code: error.exit_code(),
    }
}

fn input_flag(args: &Args) -> Result<Option<Value>, CanvasError> {
    args.flags
        .get("input-json")
        .map(|raw| {
            serde_json::from_str(raw)
                .map_err(|error| CanvasError::new("canvas.invalid_args", tr!("--input-json 不是合法 JSON：{error}", error = error)))
        })
        .transpose()
}

/// 把命令行参数整理成控制通道的请求体
fn request_body(verb: &str, args: &Args) -> Result<Value, CanvasError> {
    let instance = args.positional.get(2).cloned();
    let need_instance = || {
        instance
            .clone()
            .ok_or_else(|| CanvasError::new("canvas.invalid_args", tr!("缺少实例 id")))
    };
    Ok(match verb {
        "list" => json!({}),
        "open" => {
            let dir = args.flags.get("dir").map(|dir| absolute(Path::new(dir)));
            let id = args.flags.get("id").cloned();
            if dir.is_none() && id.is_none() {
                return Err(CanvasError::new("canvas.invalid_args", tr!("需要 --dir 或 --id")));
            }
            json!({
                "dir": dir.map(|dir| dir.to_string_lossy().to_string()),
                "id": id,
                "instance": args.flags.get("instance"),
                "title": args.flags.get("title"),
                "input": input_flag(args)?,
            })
        }
        "close" | "context" => json!({ "instance": need_instance()? }),
        "status" => json!({
            "instance": need_instance()?,
            "text": args.flags.get("text").cloned().unwrap_or_default(),
        }),
        "action" => {
            let name = args
                .positional
                .get(3)
                .cloned()
                .ok_or_else(|| CanvasError::new("canvas.invalid_args", tr!("缺少动作名")))?;
            let timeout = args
                .flags
                .get("timeout")
                .map(|value| {
                    value
                        .parse::<u64>()
                        .map_err(|_| CanvasError::new("canvas.invalid_args", tr!("--timeout 要是秒数")))
                })
                .transpose()?;
            json!({
                "instance": need_instance()?,
                "name": name,
                "input": input_flag(args)?.unwrap_or(Value::Null),
                "timeout": timeout,
            })
        }
        _ => {
            return Err(CanvasError::new(
                "canvas.invalid_args",
                tr!("不认识的子命令：canvas {verb}", verb = verb),
            ))
        }
    })
}

fn absolute(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir().map(|cwd| cwd.join(path)).unwrap_or_else(|_| path.to_path_buf())
    }
}

pub fn run(args: &[String], db: &mut Connection) -> CliOutput {
    let parsed = parse(args);
    let verb = parsed.positional.get(1).map(String::as_str).unwrap_or("");
    if verb.is_empty() || parsed.flags.contains_key("help") {
        return CliOutput {
            stdout: json!({ "ok": true, "value": help() }).to_string(),
            stderr: vec![help().to_string()],
            code: 0,
        };
    }
    let body = match request_body(verb, &parsed) {
        Ok(body) => body,
        Err(error) => return fail(error),
    };
    match env::var("MULTARA_CTL").ok().filter(|url| !url.is_empty()) {
        Some(url) => via_ctl(&url, verb, &body),
        None => direct(db, verb, &body),
    }
}

fn finish(value: Value) -> CliOutput {
    let ok = value.get("ok").and_then(Value::as_bool) == Some(true);
    let mut stderr: Vec<String> = value
        .get("warnings")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    let code = if ok {
        0
    } else {
        let code = value.get("code").and_then(Value::as_str).unwrap_or("canvas.internal");
        if let Some(message) = value.get("message").and_then(Value::as_str) {
            stderr.push(message.to_string());
        }
        // 用同一张表算退出码
        CanvasError::new(static_code(code), "").exit_code()
    };
    CliOutput {
        stdout: value.to_string(),
        stderr,
        code,
    }
}

/// 控制通道返回的错误码 → 已知的静态码（未知的按 Canvas 错误处理）
fn static_code(code: &str) -> &'static str {
    const KNOWN: &[&str] = &[
        "canvas.invalid_args",
        "canvas.invalid_manifest",
        "canvas.invalid_input",
        "canvas.dir_not_allowed",
        "canvas.unknown_action",
        "canvas.disabled",
        "canvas.app_not_running",
        "canvas.no_session",
    ];
    KNOWN.iter().copied().find(|known| *known == code).unwrap_or("canvas.error")
}

fn via_ctl(url: &str, verb: &str, body: &Value) -> CliOutput {
    let timeout = body
        .get("timeout")
        .and_then(Value::as_u64)
        .unwrap_or(ops::DEFAULT_ACTION_TIMEOUT_SECS)
        + 10;
    match post(url, &format!("canvas.{verb}"), body, Duration::from_secs(timeout)) {
        Ok(value) => finish(value),
        Err(message) => fail(CanvasError::new(
            "canvas.app_not_running",
            tr!("连不上 Multara：{message}", message = message),
        )),
    }
}

/// 极简 HTTP/1.1 POST（只连本机控制端点）
pub fn post(url: &str, method: &str, body: &Value, timeout: Duration) -> Result<Value, String> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| "MULTARA_CTL".to_string())?;
    let (host, token) = rest.split_once('/').ok_or_else(|| "MULTARA_CTL".to_string())?;
    if !host.starts_with("127.0.0.1:") {
        return Err("MULTARA_CTL".to_string());
    }
    let mut stream = TcpStream::connect(host).map_err(|error| error.to_string())?;
    let _ = stream.set_read_timeout(Some(timeout));
    let payload = body.to_string();
    let request = format!(
        "POST /{token}/{method} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    );
    stream.write_all(request.as_bytes()).map_err(|error| error.to_string())?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).map_err(|error| error.to_string())?;
    let text = String::from_utf8_lossy(&raw);
    let (head, body) = text.split_once("\r\n\r\n").ok_or_else(|| "bad response".to_string())?;
    let body = if head.to_ascii_lowercase().contains("transfer-encoding: chunked") {
        dechunk(body)
    } else {
        body.to_string()
    };
    serde_json::from_str(&body).map_err(|error| error.to_string())
}

fn dechunk(body: &str) -> String {
    let mut out = String::new();
    let mut rest = body;
    while let Some((size, after)) = rest.split_once("\r\n") {
        let Ok(size) = usize::from_str_radix(size.trim(), 16) else { break };
        if size == 0 || after.len() < size {
            break;
        }
        out.push_str(&after[..size]);
        rest = after[size..].trim_start_matches("\r\n");
    }
    out
}

fn direct(db: &mut Connection, verb: &str, body: &Value) -> CliOutput {
    if !ops::canvases_enabled(db) {
        return fail(CanvasError::new("canvas.disabled", tr!("Canvas 未启用（设置 → 实验功能）")));
    }
    let session_id = env::var("MULTARA_SESSION_ID").unwrap_or_default();
    if session_id.is_empty() {
        return fail(CanvasError::new("canvas.no_session", tr!("不知道当前是哪个会话（没有 MULTARA_SESSION_ID）")));
    }
    let caller = Caller {
        session_id: session_id.clone(),
        run_id: env::var("MULTARA_RUN_ID").ok(),
        agent_id: env::var("MULTARA_AGENT_ID").ok(),
    };
    let instance = body.get("instance").and_then(Value::as_str).unwrap_or("").to_string();
    let not_running = tr!("应用未运行，下次显示时生效");
    let result: Result<(Value, Vec<String>), CanvasError> = match verb {
        "list" => ops::list(db, &session_id, false).map(|value| (value, Vec::new())),
        "open" => {
            let app_data = app_data_dir();
            match app_data {
                None => Err(CanvasError::new("canvas.app_not_running", tr!("无法确定应用数据目录"))),
                Some(app_data) => ops::open(
                    db,
                    &app_data,
                    &caller,
                    &OpenRequest {
                        dir: body.get("dir").and_then(Value::as_str).map(str::to_string),
                        id: body.get("id").and_then(Value::as_str).map(str::to_string),
                        instance: body.get("instance").and_then(Value::as_str).map(str::to_string),
                        title: body.get("title").and_then(Value::as_str).map(str::to_string),
                        input: body.get("input").cloned().filter(|value| !value.is_null()),
                    },
                )
                .map(|opened| {
                    let mut hints = opened.warnings;
                    hints.push(not_running.clone());
                    (opened.instance, hints)
                }),
            }
        }
        "close" => ops::close(db, &session_id, &instance).map(|()| (Value::Null, vec![not_running.clone()])),
        "status" => ops::set_status(
            db,
            &session_id,
            &instance,
            body.get("text").and_then(Value::as_str).unwrap_or(""),
        )
        .map(|value| (value, vec![not_running.clone()])),
        "context" => ops::get_context(db, &session_id, &instance).map(|value| (value, Vec::new())),
        "action" => Err(CanvasError::new(
            "canvas.app_not_running",
            tr!("应用未运行，无法驱动页面"),
        )),
        _ => Err(CanvasError::new("canvas.invalid_args", verb.to_string())),
    };
    match result {
        Ok((value, hints)) => CliOutput {
            stdout: json!({ "ok": true, "value": value }).to_string(),
            stderr: hints,
            code: 0,
        },
        Err(error) => fail(error),
    }
}

fn app_data_dir() -> Option<PathBuf> {
    env::var("MULTARA_APP_DATA")
        .ok()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            env::var("MULTARA_DB")
                .ok()
                .and_then(|db| PathBuf::from(db).parent().map(Path::to_path_buf))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|item| item.to_string()).collect()
    }

    #[test]
    fn builds_request_bodies() {
        let parsed = parse(&args(&["canvas", "action", "board", "set", "--input-json", r#"{"ok":true}"#, "--timeout", "5"]));
        let body = request_body("action", &parsed).unwrap();
        assert_eq!(body["instance"], "board");
        assert_eq!(body["name"], "set");
        assert_eq!(body["input"]["ok"], true);
        assert_eq!(body["timeout"], 5);
        let parsed = parse(&args(&["canvas", "open", "--dir", "rel/board"]));
        let body = request_body("open", &parsed).unwrap();
        assert!(Path::new(body["dir"].as_str().unwrap()).is_absolute());
        assert_eq!(request_body("open", &parse(&args(&["canvas", "open"]))).unwrap_err().code, "canvas.invalid_args");
        let bad = parse(&args(&["canvas", "open", "--id", "x", "--input-json", "{oops"]));
        assert_eq!(request_body("open", &bad).unwrap_err().exit_code(), 1);
        assert_eq!(request_body("close", &parse(&args(&["canvas", "close"]))).unwrap_err().code, "canvas.invalid_args");
    }

    #[test]
    fn maps_ctl_errors_to_exit_codes() {
        assert_eq!(finish(json!({ "ok": true, "value": 1 })).code, 0);
        assert_eq!(finish(json!({ "ok": false, "code": "canvas.disabled", "message": "x" })).code, 2);
        assert_eq!(finish(json!({ "ok": false, "code": "canvas.timeout", "message": "x" })).code, 3);
        assert_eq!(finish(json!({ "ok": false, "code": "canvas.invalid_input", "message": "x" })).code, 1);
        let out = finish(json!({ "ok": true, "value": 1, "warnings": ["w"] }));
        assert_eq!(out.stderr, vec!["w".to_string()]);
    }

    #[test]
    fn dechunks_bodies() {
        assert_eq!(dechunk("3\r\nabc\r\n2\r\nde\r\n0\r\n\r\n"), "abcde");
    }

    #[test]
    fn direct_mode_requires_the_setting() {
        let mut db = Connection::open_in_memory().unwrap();
        crate::database::migrate(&db).unwrap();
        let out = direct(&mut db, "list", &json!({}));
        assert_eq!(out.code, 2);
        assert!(out.stdout.contains("canvas.disabled"));
    }
}
