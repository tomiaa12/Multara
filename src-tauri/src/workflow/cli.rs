//! `multara node …` / `multara workflow …`（技术方案第 8 节）。CLI 直接打开 SQLite，
//! 通过 MULTARA_RUN_ID → agent_runs.session_id → workflow_node_runs 找到当前节点。

use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::Read;

use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Value};

use super::engine;
use super::model::{find_port, PortType};
use super::store;

const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;

struct Args {
    positional: Vec<String>,
    flags: HashMap<String, Vec<String>>,
}

impl Args {
    fn parse(args: &[String]) -> Args {
        let mut positional = Vec::new();
        let mut flags: HashMap<String, Vec<String>> = HashMap::new();
        let mut current: Option<String> = None;
        for arg in args {
            if let Some(name) = arg
                .strip_prefix("--")
                .filter(|name| !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
            {
                flags.entry(name.to_string()).or_default();
                current = Some(name.to_string());
                continue;
            }
            match &current {
                Some(name) => {
                    let list = flags.entry(name.clone()).or_default();
                    list.push(arg.clone());
                    // 只有 --files / --input 接多个值
                    if name != "files" && name != "input" {
                        current = None;
                    }
                }
                None => positional.push(arg.clone()),
            }
        }
        Args { positional, flags }
    }

    fn one(&self, name: &str) -> Option<&str> {
        self.flags.get(name).and_then(|list| list.first()).map(String::as_str)
    }

    fn has(&self, name: &str) -> bool {
        self.flags.contains_key(name)
    }

    fn many(&self, name: &str) -> Vec<String> {
        self.flags.get(name).cloned().unwrap_or_default()
    }
}

/// 只允许读当前目录（agent 的工作区）和系统临时目录里的文件
fn ensure_file_allowed(path: &str) -> Result<std::path::PathBuf, String> {
    let target = fs::canonicalize(path).map_err(|error| tr!("读取文件失败：{error}", error = error))?;
    let allowed = [env::current_dir().ok(), Some(env::temp_dir())]
        .into_iter()
        .flatten()
        .filter_map(|root| fs::canonicalize(root).ok())
        .any(|root| target.starts_with(root));
    if allowed {
        Ok(target)
    } else {
        Err(tr!("文件只能位于当前目录或系统临时目录内"))
    }
}

fn read_file(path: &str) -> Result<String, String> {
    let target = ensure_file_allowed(path)?;
    let mut bytes = Vec::new();
    fs::File::open(&target)
        .and_then(|file| file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|error| tr!("读取文件失败：{error}", error = error))?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(tr!("文件超过 {size} MB", size = MAX_FILE_BYTES / 1024 / 1024));
    }
    String::from_utf8(bytes).map_err(|error| tr!("读取文件失败：{error}", error = error))
}

fn current_node_run(db: &Connection) -> Result<store::NodeRun, String> {
    if let Ok(id) = env::var("MULTARA_NODE_RUN_ID") {
        if let Ok(item) = store::node_run(db, &id) {
            return Ok(item);
        }
    }
    let run_id = env::var("MULTARA_RUN_ID").map_err(|_| tr!("当前运行不属于工作流节点"))?;
    let session: Option<String> = db
        .query_row("SELECT session_id FROM agent_runs WHERE id=?1", [&run_id], |row| row.get(0))
        .optional()
        .map_err(|error| error.to_string())?;
    let session = session.ok_or_else(|| tr!("当前运行不属于工作流节点"))?;
    store::node_run_by_session(db, &session)?
        .filter(|item| matches!(item.status.as_str(), "running" | "waiting_human"))
        .ok_or_else(|| tr!("当前运行不属于工作流节点"))
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

pub fn execute(args: &[String], db: &mut Connection) -> Result<String, String> {
    let parsed = Args::parse(args);
    let resource = parsed.positional.first().map(String::as_str).unwrap_or("");
    let verb = parsed.positional.get(1).map(String::as_str).unwrap_or("");
    let now = chrono::Utc::now().to_rfc3339();
    match (resource, verb) {
        ("node", "inputs") => {
            let nr = current_node_run(db)?;
            Ok(pretty(&nr.inputs))
        }
        ("node", "status") => {
            let nr = current_node_run(db)?;
            let outputs = super::values::outputs(db, &nr.id)?;
            Ok(pretty(&json!({
                "nodeId": nr.node_id,
                "iteration": nr.iteration,
                "attempt": nr.attempt,
                "status": nr.status,
                "submitted": outputs.keys().collect::<Vec<_>>(),
            })))
        }
        ("node", "outputs") => {
            let nr = current_node_run(db)?;
            let run = store::run(db, &nr.run_id)?;
            let def = store::definition(db, &run.workflow_id, run.version)?;
            let node = def.node(&nr.node_id).ok_or_else(|| tr!("节点不存在"))?;
            let submitted = super::values::outputs(db, &nr.id)?;
            let list: Vec<Value> = node
                .outputs
                .iter()
                .map(|port| {
                    json!({
                        "name": port.name,
                        "type": port.port_type.as_str(),
                        "required": port.required,
                        "description": port.description,
                        "schema": port.schema.as_ref().map(super::schema::resolve_builtin),
                        "submitted": submitted.contains_key(&port.name),
                    })
                })
                .collect();
            Ok(pretty(&Value::Array(list)))
        }
        ("node", "output") => {
            let port_name = parsed.positional.get(2).ok_or_else(|| tr!("缺少输出端口名"))?.clone();
            let nr = current_node_run(db)?;
            let run = store::run(db, &nr.run_id)?;
            let def = store::definition(db, &run.workflow_id, run.version)?;
            let node = def.node(&nr.node_id).ok_or_else(|| tr!("节点不存在"))?;
            let port_type = find_port(&node.outputs, &port_name).map(|port| port.port_type).unwrap_or(PortType::Any);
            let value = if let Some(text) = parsed.one("text") {
                json!(text)
            } else if let Some(path) = parsed.one("text-file") {
                json!(read_file(path)?)
            } else if let Some(raw) = parsed.one("json") {
                serde_json::from_str(raw).map_err(|error| tr!("--json 不是合法的 JSON：{error}", error = error))?
            } else if let Some(path) = parsed.one("json-file") {
                let raw = read_file(path)?;
                serde_json::from_str(&raw).map_err(|error| tr!("--json-file 不是合法的 JSON：{error}", error = error))?
            } else if let Some(path) = parsed.one("file") {
                json!(ensure_file_allowed(path)?.to_string_lossy())
            } else if parsed.has("files") {
                let mut list = Vec::new();
                for path in parsed.many("files") {
                    list.push(json!(ensure_file_allowed(&path)?.to_string_lossy()));
                }
                Value::Array(list)
            } else {
                return Err(tr!("用 --text、--text-file、--json、--json-file、--file 或 --files 提供值"));
            };
            let value = if port_type == PortType::Json && value.is_string() && parsed.has("text") {
                serde_json::from_str(value.as_str().unwrap_or("")).unwrap_or(value)
            } else {
                value
            };
            engine::submit_output(db, &nr.id, &port_name, value, &now)?;
            Ok(tr!("已提交输出 {port}", port = port_name))
        }
        ("node", "fail") => {
            let reason = parsed.one("reason").ok_or_else(|| tr!("缺少 --reason"))?;
            let nr = current_node_run(db)?;
            engine::declare_failure(db, &nr.id, reason)?;
            Ok(tr!("已标记节点失败，结束本轮回答即可"))
        }
        ("workflow", "list") => {
            let items: Vec<Value> = store::list_workflows(db)?
                .into_iter()
                .map(|wf| json!({ "id": wf.id, "name": wf.name, "publishedVersion": wf.published_version, "enabled": wf.enabled, "description": wf.description }))
                .collect();
            Ok(pretty(&Value::Array(items)))
        }
        ("workflow", "get") => {
            let id = parsed.positional.get(2).ok_or_else(|| tr!("缺少工作流 id"))?;
            let workflow = store::workflow(db, id)?;
            let def = match workflow.published_version {
                Some(version) => store::definition(db, id, version)?,
                None => store::draft(db, id)?,
            };
            Ok(pretty(&json!({
                "id": workflow.id,
                "name": workflow.name,
                "description": workflow.description,
                "publishedVersion": workflow.published_version,
                "inputs": def.inputs,
                "outputs": def.outputs,
                "nodes": def.nodes.iter().map(|node| json!({ "id": node.id, "kind": node.kind.as_str(), "title": node.title })).collect::<Vec<_>>(),
            })))
        }
        ("workflow", "runs") => {
            let id = parsed.positional.get(2).map(String::as_str);
            let runs: Vec<Value> = store::list_runs(db, id, parsed.one("status"), 20)?
                .into_iter()
                .map(|run| json!({ "id": run.id, "workflowId": run.workflow_id, "status": run.status, "createdAt": run.created_at, "error": run.error, "outputs": run.outputs }))
                .collect();
            Ok(pretty(&Value::Array(runs)))
        }
        ("workflow", "run") => {
            let id = parsed.positional.get(2).ok_or_else(|| tr!("缺少工作流 id"))?;
            let mut inputs = match parsed.one("input-json") {
                Some(raw) => serde_json::from_str::<Value>(raw).map_err(|error| tr!("--input-json 不是合法的 JSON：{error}", error = error))?,
                None => json!({}),
            };
            for pair in parsed.many("input") {
                let (key, value) = pair.split_once('=').ok_or_else(|| tr!("--input 的格式是 key=value"))?;
                inputs[key.trim()] = serde_json::from_str(value).unwrap_or_else(|_| json!(value));
            }
            let run_id = engine::enqueue_run(db, id, inputs, "cli", &now)?;
            Ok(pretty(&json!({ "runId": run_id, "status": "queued", "note": tr!("Multara 会在 30 秒内开始这次运行") })))
        }
        _ => Err(tr!(
            "用法：multara node inputs|outputs|status|output <port> …|fail --reason …；multara workflow list|get <id>|runs [id]|run <id> --input k=v"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multi_value_flags() {
        let args: Vec<String> = ["node", "output", "shots", "--files", "a.png", "b.png"].iter().map(|s| s.to_string()).collect();
        let parsed = Args::parse(&args);
        assert_eq!(parsed.positional, vec!["node", "output", "shots"]);
        assert_eq!(parsed.many("files"), vec!["a.png", "b.png"]);
        let args: Vec<String> = ["node", "output", "summary", "--text", "-- 列表", "x"].iter().map(|s| s.to_string()).collect();
        let parsed = Args::parse(&args);
        assert_eq!(parsed.one("text"), Some("-- 列表"));
        assert_eq!(parsed.positional.last().map(String::as_str), Some("x"));
    }
}
