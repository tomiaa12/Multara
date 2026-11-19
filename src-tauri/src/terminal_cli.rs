//! `multara terminal ...`：读快照直连本机；open/run/write/focus 走 `MULTARA_CTL`。

use std::{
    collections::HashMap,
    env,
    path::{Path, PathBuf},
    time::Duration,
};

use serde_json::{json, Value};

use crate::{canvas_cli, terminal, terminal_ctl::TerminalCtlError};

pub struct CliOutput {
    pub stdout: String,
    pub stderr: Vec<String>,
    pub code: i32,
}

pub fn help() -> &'static str {
    "multara terminal list
multara terminal read [id] [--start N] [--end N]
multara terminal open [--cwd <path>] [--workspace <id>] [--title ...] [--new]
multara terminal run --command \"...\" [--cwd <path>] [--workspace <id>] [--title ...] [--new]
multara terminal write <id> --text \"...\"
multara terminal focus [id] [--cwd <path>] [--workspace <id>]"
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

fn fail(error: TerminalCtlError) -> CliOutput {
    CliOutput {
        stdout: error.to_json().to_string(),
        stderr: vec![error.message.clone()],
        code: error.exit_code(),
    }
}

fn finish(value: Value) -> CliOutput {
    let ok = value.get("ok").and_then(Value::as_bool) == Some(true);
    let mut stderr = Vec::new();
    let code = if ok {
        0
    } else {
        let code = value
            .get("code")
            .and_then(Value::as_str)
            .unwrap_or("terminal.failed");
        if let Some(message) = value.get("message").and_then(Value::as_str) {
            stderr.push(message.to_string());
        }
        TerminalCtlError::new(static_code(code), "").exit_code()
    };
    CliOutput {
        stdout: value.to_string(),
        stderr,
        code,
    }
}

fn static_code(code: &str) -> &'static str {
    const KNOWN: &[&str] = &[
        "terminal.invalid_args",
        "terminal.app_not_running",
        "terminal.no_workspace",
        "terminal.ui_timeout",
        "terminal.not_found",
        "terminal.unknown_method",
        "terminal.failed",
    ];
    KNOWN
        .iter()
        .copied()
        .find(|known| *known == code)
        .unwrap_or("terminal.failed")
}

fn app_data_dir() -> Result<PathBuf, TerminalCtlError> {
    env::var("MULTARA_APP_DATA")
        .ok()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            env::var("MULTARA_DB")
                .ok()
                .and_then(|db| PathBuf::from(db).parent().map(Path::to_path_buf))
        })
        .ok_or_else(|| {
            TerminalCtlError::new(
                "terminal.app_not_running",
                tr!("无法确定应用数据目录"),
            )
        })
}

fn absolute(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    }
}

fn parse_line(value: &str, name: &str) -> Result<usize, TerminalCtlError> {
    value.parse::<usize>().map_err(|_| {
        TerminalCtlError::new(
            "terminal.invalid_args",
            tr!("--{name} 要是行号", name = name),
        )
    })
}

fn request_body(verb: &str, args: &Args) -> Result<Value, TerminalCtlError> {
    Ok(match verb {
        "open" | "run" | "focus" => {
            let mut body = json!({
                "cwd": args.flags.get("cwd").map(|cwd| absolute(Path::new(cwd)).to_string_lossy().to_string()),
                "workspace": args.flags.get("workspace"),
                "title": args.flags.get("title"),
                "new": args.flags.contains_key("new"),
            });
            if verb == "run" {
                let command = args
                    .flags
                    .get("command")
                    .cloned()
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| {
                        TerminalCtlError::new("terminal.invalid_args", tr!("缺少 --command"))
                    })?;
                body["command"] = json!(command);
            }
            if verb == "focus" {
                if let Some(id) = args.positional.get(2) {
                    body["id"] = json!(id);
                }
            }
            body
        }
        "write" => {
            let id = args.positional.get(2).cloned().ok_or_else(|| {
                TerminalCtlError::new("terminal.invalid_args", tr!("缺少终端 id"))
            })?;
            let text = args
                .flags
                .get("text")
                .cloned()
                .or_else(|| args.flags.get("data").cloned())
                .ok_or_else(|| {
                    TerminalCtlError::new("terminal.invalid_args", tr!("缺少 --text"))
                })?;
            json!({ "id": id, "text": text })
        }
        _ => {
            return Err(TerminalCtlError::new(
                "terminal.invalid_args",
                tr!("不认识的子命令：terminal {verb}", verb = verb),
            ))
        }
    })
}

fn via_ctl(url: &str, verb: &str, body: &Value) -> CliOutput {
    match canvas_cli::post(
        url,
        &format!("terminal.{verb}"),
        body,
        Duration::from_secs(20),
    ) {
        Ok(value) => finish(value),
        Err(message) => fail(TerminalCtlError::new(
            "terminal.app_not_running",
            tr!("连不上 Multara：{message}", message = message),
        )),
    }
}

fn read_local(args: &Args) -> CliOutput {
    let app_data = match app_data_dir() {
        Ok(dir) => dir,
        Err(error) => return fail(error),
    };
    let id = args.positional.get(2).cloned();
    let start = match args
        .flags
        .get("start")
        .map(|value| parse_line(value, "start"))
        .transpose()
    {
        Ok(value) => value,
        Err(error) => return fail(error),
    };
    let end = match args
        .flags
        .get("end")
        .map(|value| parse_line(value, "end"))
        .transpose()
    {
        Ok(value) => value,
        Err(error) => return fail(error),
    };
    match terminal::read_snapshot(&app_data, id.as_deref(), start, end) {
        // 与旧版 CLI 一致：成功时 stdout 直接是快照对象
        Ok(value) => match serde_json::to_string_pretty(&value) {
            Ok(stdout) => CliOutput {
                stdout,
                stderr: Vec::new(),
                code: 0,
            },
            Err(error) => fail(TerminalCtlError::new("terminal.failed", error.to_string())),
        },
        Err(message) => fail(TerminalCtlError::new("terminal.not_found", message)),
    }
}

fn list_local() -> CliOutput {
    let app_data = match app_data_dir() {
        Ok(dir) => dir,
        Err(error) => return fail(error),
    };
    match terminal::list_snapshots(&app_data) {
        // 与旧版 CLI 一致：成功时 stdout 直接是数组
        Ok(value) => match serde_json::to_string_pretty(&value) {
            Ok(stdout) => CliOutput {
                stdout,
                stderr: Vec::new(),
                code: 0,
            },
            Err(error) => fail(TerminalCtlError::new("terminal.failed", error.to_string())),
        },
        Err(message) => fail(TerminalCtlError::new("terminal.failed", message)),
    }
}

pub fn run(args: &[String]) -> CliOutput {
    let parsed = parse(args);
    let verb = parsed.positional.get(1).map(String::as_str).unwrap_or("");
    if verb.is_empty() || parsed.flags.contains_key("help") {
        return CliOutput {
            stdout: json!({ "ok": true, "value": help() }).to_string(),
            stderr: vec![help().to_string()],
            code: 0,
        };
    }
    match verb {
        "list" => list_local(),
        "read" => read_local(&parsed),
        "open" | "run" | "write" | "focus" => {
            let body = match request_body(verb, &parsed) {
                Ok(body) => body,
                Err(error) => return fail(error),
            };
            match env::var("MULTARA_CTL").ok().filter(|url| !url.is_empty()) {
                Some(url) => via_ctl(&url, verb, &body),
                None => fail(TerminalCtlError::new(
                    "terminal.app_not_running",
                    tr!("需要 Multara 应用在运行（缺少 MULTARA_CTL）"),
                )),
            }
        }
        _ => fail(TerminalCtlError::new(
            "terminal.invalid_args",
            tr!("不认识的子命令：terminal {verb}", verb = verb),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_string()).collect()
    }

    #[test]
    fn run_requires_command() {
        let parsed = parse(&args(&["terminal", "run", "--cwd", "/tmp"]));
        let error = request_body("run", &parsed).unwrap_err();
        assert_eq!(error.code, "terminal.invalid_args");
    }

    #[test]
    fn write_body_includes_id_and_text() {
        let parsed = parse(&args(&[
            "terminal",
            "write",
            "abc",
            "--text",
            "echo hi\n",
        ]));
        let body = request_body("write", &parsed).unwrap();
        assert_eq!(body["id"], "abc");
        assert_eq!(body["text"], "echo hi\n");
    }
}
