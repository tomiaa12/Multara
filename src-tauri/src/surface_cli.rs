//! `multara surface ...`：让智能体操作当前聊天界面上已有的能力。

use std::{collections::HashMap, env, time::Duration};

use serde_json::{json, Value};

use crate::{canvas_cli, surface_ctl::SurfaceCtlError};

pub struct CliOutput {
    pub stdout: String,
    pub stderr: Vec<String>,
    pub code: i32,
}

pub fn help() -> &'static str {
    "multara surface list
multara surface open-file --path <path>
multara surface browser open --url <url>
multara surface browser list
multara surface browser back|forward|reload"
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

fn fail(error: SurfaceCtlError) -> CliOutput {
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
        if let Some(message) = value.get("message").and_then(Value::as_str) {
            stderr.push(message.to_string());
        }
        2
    };
    CliOutput {
        stdout: value.to_string(),
        stderr,
        code,
    }
}

fn request_body(verb: &str, args: &Args) -> Result<Value, SurfaceCtlError> {
    match verb {
        "list" => Ok(json!({})),
        "open-file" => {
            let path = args.flags.get("path").cloned().filter(|value| !value.is_empty()).ok_or_else(|| {
                SurfaceCtlError::new("surface.invalid_args", tr!("缺少 --path"))
            })?;
            Ok(json!({ "path": path, "line": args.flags.get("line").and_then(|value| value.parse::<u64>().ok()) }))
        }
        "browser-open" => {
            let url = args.flags.get("url").cloned().filter(|value| !value.is_empty()).ok_or_else(|| {
                SurfaceCtlError::new("surface.invalid_args", tr!("缺少 --url"))
            })?;
            Ok(json!({ "url": url }))
        }
        "browser-list" | "browser-back" | "browser-forward" | "browser-reload" => Ok(json!({})),
        _ => Err(SurfaceCtlError::new(
            "surface.invalid_args",
            tr!("不认识的子命令：surface {verb}", verb = verb),
        )),
    }
}

pub fn run(args: &[String]) -> CliOutput {
    let parsed = parse(args);
    let verb = match parsed.positional.get(1).map(String::as_str).unwrap_or("") {
        "browser" => match parsed.positional.get(2).map(String::as_str).unwrap_or("") {
            "open" => "browser-open",
            "list" => "browser-list",
            "back" => "browser-back",
            "forward" => "browser-forward",
            "reload" => "browser-reload",
            other => other,
        },
        other => other,
    };
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
        Some(url) => match canvas_cli::post(url.as_str(), &format!("surface.{verb}"), &body, Duration::from_secs(20)) {
            Ok(value) => finish(value),
            Err(message) => fail(SurfaceCtlError::new(
                "surface.app_not_running",
                tr!("连不上 Multara：{message}", message = message),
            )),
        },
        None => fail(SurfaceCtlError::new(
            "surface.app_not_running",
            tr!("需要 Multara 应用在运行（缺少 MULTARA_CTL）"),
        )),
    }
}
