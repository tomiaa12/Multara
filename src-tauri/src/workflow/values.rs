//! 端口值：类型兼容、运行时转换、读写 `workflow_values`（大值落盘）。

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{Map, Value};

use super::model::{PortDef, PortType};

pub const INLINE_LIMIT: usize = 64 * 1024;

const TYPES: [PortType; 9] = [
    PortType::Text,
    PortType::Markdown,
    PortType::Number,
    PortType::Boolean,
    PortType::Json,
    PortType::Url,
    PortType::File,
    PortType::Files,
    PortType::Any,
];

/// 技术方案 2.2 的兼容表；`shared/workflow-port-compat.json` 由同一张表生成，前端读它
pub fn compatible(from: PortType, to: PortType) -> bool {
    use PortType::*;
    if from == to || to == Any || from == Any {
        return true;
    }
    match to {
        Text | Markdown => matches!(from, Text | Markdown | Number | Boolean | Json | Url | File),
        Json => matches!(from, Number | Boolean | Files),
        Files => matches!(from, File),
        _ => false,
    }
}

pub fn compat_table() -> Value {
    let mut table = Map::new();
    for from in TYPES {
        let targets: Vec<Value> = TYPES
            .iter()
            .filter(|to| compatible(from, **to))
            .map(|to| Value::String(to.as_str().into()))
            .collect();
        table.insert(from.as_str().into(), Value::Array(targets));
    }
    Value::Object(table)
}

pub fn type_ok(value: &Value, port_type: PortType) -> bool {
    match port_type {
        PortType::Text | PortType::Markdown | PortType::Url | PortType::File => {
            value.is_string() || value.is_null()
        }
        PortType::Number => value.is_number() || value.is_null(),
        PortType::Boolean => value.is_boolean() || value.is_null(),
        PortType::Files => {
            value.is_null()
                || value
                    .as_array()
                    .is_some_and(|items| items.iter().all(Value::is_string))
        }
        PortType::Json | PortType::Any => true,
    }
}

/// 把来源值转成目标端口要的形状（2.2 表注 1~5），并按目标 schema 校验
pub fn coerce(value: Value, to: &PortDef) -> Result<Value, String> {
    if value.is_null() {
        return Ok(value);
    }
    let converted = match to.port_type {
        PortType::Text | PortType::Markdown => match value {
            Value::String(_) => value,
            Value::Number(_) | Value::Bool(_) => Value::String(value.to_string()),
            other => Value::String(serde_json::to_string_pretty(&other).unwrap_or_default()),
        },
        PortType::Files => match value {
            Value::String(path) => Value::Array(vec![Value::String(path)]),
            other => other,
        },
        _ => value,
    };
    if !type_ok(&converted, to.port_type) {
        return Err(tr!(
            "端口 {port} 需要 {expected} 类型的值",
            port = to.name,
            expected = to.port_type.as_str()
        ));
    }
    if let Some(schema) = &to.schema {
        if to.port_type == PortType::Json || to.port_type == PortType::Any {
            super::schema::validate(schema, &converted).map_err(|errors| {
                tr!(
                    "端口 {port} 的值不符合 schema：{errors}",
                    port = to.name,
                    errors = errors.join("; ")
                )
            })?;
        }
    }
    Ok(converted)
}

/// 输出端口校验：类型 + schema，返回问题列表
pub fn check_output(value: &Value, port: &PortDef) -> Vec<String> {
    let mut problems = Vec::new();
    if !type_ok(value, port.port_type) {
        problems.push(format!(
            "{}: expected {}",
            port.name,
            port.port_type.as_str()
        ));
        return problems;
    }
    if let (Some(schema), false) = (&port.schema, value.is_null()) {
        if let Err(errors) = super::schema::validate(schema, value) {
            problems.extend(errors.into_iter().map(|e| format!("{}{}", port.name, e.trim_start_matches('$'))));
        }
    }
    problems
}

pub struct PutValue<'a> {
    pub run_id: &'a str,
    pub node_run_id: &'a str,
    pub node_id: &'a str,
    pub port: &'a str,
    pub port_type: PortType,
    pub value: &'a Value,
    pub source: &'a str,
    pub now: &'a str,
}

pub fn artifact_dir(root: &Path, run_id: &str, node_run_id: &str, kind: &str) -> PathBuf {
    root.join(run_id).join(node_run_id).join(kind)
}

pub fn put(db: &Connection, artifacts: Option<&Path>, input: PutValue<'_>) -> Result<(), String> {
    let raw = serde_json::to_string(input.value).map_err(|error| error.to_string())?;
    let size = raw.len() as i64;
    let (inline, path) = match artifacts {
        Some(root) if raw.len() > INLINE_LIMIT => {
            let dir = artifact_dir(root, input.run_id, input.node_run_id, "outputs");
            std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
            let file = dir.join(format!("{}.json", sanitize(input.port)));
            std::fs::write(&file, &raw).map_err(|error| error.to_string())?;
            (None, Some(file.to_string_lossy().to_string()))
        }
        _ => (Some(raw), None),
    };
    db.execute(
        "INSERT INTO workflow_values
         (id, run_id, node_run_id, node_id, port, type, value_json, artifact_path, size, source, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT(node_run_id, port) DO UPDATE SET
           type=excluded.type, value_json=excluded.value_json, artifact_path=excluded.artifact_path,
           size=excluded.size, source=excluded.source, created_at=excluded.created_at",
        params![
            uuid::Uuid::new_v4().to_string(),
            input.run_id,
            input.node_run_id,
            input.node_id,
            input.port,
            input.port_type.as_str(),
            inline,
            path,
            size,
            input.source,
            input.now
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .collect()
}

fn load(inline: Option<String>, path: Option<String>) -> Value {
    let raw = match (inline, path) {
        (Some(raw), _) => raw,
        (None, Some(path)) => std::fs::read_to_string(path).unwrap_or_default(),
        _ => return Value::Null,
    };
    serde_json::from_str(&raw).unwrap_or(Value::Null)
}

pub fn outputs(db: &Connection, node_run_id: &str) -> Result<Map<String, Value>, String> {
    let mut statement = db
        .prepare(
            "SELECT port, value_json, artifact_path FROM workflow_values
             WHERE node_run_id=?1 ORDER BY created_at",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([node_run_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    let mut map = Map::new();
    for row in rows {
        let (port, inline, path) = row.map_err(|error| error.to_string())?;
        map.insert(port, load(inline, path));
    }
    Ok(map)
}

pub fn output(db: &Connection, node_run_id: &str, port: &str) -> Result<Option<Value>, String> {
    let row: Option<(Option<String>, Option<String>)> = db
        .query_row(
            "SELECT value_json, artifact_path FROM workflow_values WHERE node_run_id=?1 AND port=?2",
            params![node_run_id, port],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    Ok(row.map(|(inline, path)| load(inline, path)))
}

pub fn has_any_submitted(db: &Connection, node_run_id: &str) -> Result<bool, String> {
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM workflow_values WHERE node_run_id=?1 AND source='agent_cli')",
        [node_run_id],
        |row| row.get(0),
    )
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn compat_matches_shared_table() {
        let shared: Value =
            serde_json::from_str(include_str!("../../../shared/workflow-port-compat.json")).unwrap();
        assert_eq!(shared, compat_table());
    }

    #[test]
    fn compat_rules() {
        assert!(compatible(PortType::Json, PortType::Text));
        assert!(compatible(PortType::File, PortType::Files));
        assert!(!compatible(PortType::Text, PortType::Number));
        assert!(!compatible(PortType::Files, PortType::Text));
    }

    #[test]
    fn coerce_and_schema() {
        let mut port = PortDef::new("spec", PortType::Text);
        assert_eq!(coerce(json!({ "a": 1 }), &port).unwrap(), json!("{\n  \"a\": 1\n}"));
        port.port_type = PortType::Json;
        port.schema = Some(json!({ "type": "object", "required": ["a"] }));
        assert!(coerce(json!({ "b": 1 }), &port).is_err());
        let files = PortDef::new("f", PortType::Files);
        assert_eq!(coerce(json!("/a"), &files).unwrap(), json!(["/a"]));
        let number = PortDef::new("n", PortType::Number);
        assert!(coerce(json!("1"), &number).is_err());
    }
}
