//! MCP 配置里的变量：`${input:id}` 取已保存的输入，`${env:NAME}` 取环境变量，
//! `${userHome}` 取用户目录。下发给运行时和探测前都要先解析，运行时自己不认识这些占位符。

use std::{fs, path::PathBuf};

use serde_json::{Map, Value};

pub struct Prepared {
    pub entry: Value,
    pub missing: Vec<String>,
}

pub fn referenced_inputs(entry: &Value) -> Vec<String> {
    let mut found = Vec::new();
    visit_strings(entry, &mut |text| {
        for id in placeholders(text, "input:") {
            if !found.contains(&id) {
                found.push(id);
            }
        }
    });
    found
}

pub fn stored_inputs(server: &str) -> Map<String, Value> {
    read_store()
        .get(server)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

pub fn save_inputs(server: &str, values: Map<String, Value>) -> Result<(), String> {
    let mut store = read_store();
    let cleaned: Map<String, Value> = values
        .into_iter()
        .filter(|(_, value)| value.as_str().is_some_and(|text| !text.is_empty()))
        .collect();
    if cleaned.is_empty() {
        store.remove(server);
    } else {
        store.insert(server.to_string(), Value::Object(cleaned));
    }
    write_store(&store)
}

pub fn rename_inputs(previous: &str, next: &str) -> Result<(), String> {
    let mut store = read_store();
    if let Some(values) = store.remove(previous) {
        store.insert(next.to_string(), values);
        write_store(&store)?;
    }
    Ok(())
}

/// 解析变量、合并 envFile、补上 OAuth token，去掉运行时不认识的字段。
pub fn prepare(server: &str, entry: &Value) -> Prepared {
    let inputs = stored_inputs(server);
    let home = home().map(|path| path.to_string_lossy().to_string()).unwrap_or_default();
    let mut missing = Vec::new();
    let mut resolved = entry.clone();
    map_strings(&mut resolved, &mut |text| {
        let mut out = text.replace("${userHome}", &home);
        for id in placeholders(text, "input:") {
            match inputs.get(&id).and_then(Value::as_str) {
                Some(value) => out = out.replace(&format!("${{input:{id}}}"), value),
                None if !missing.contains(&id) => missing.push(id),
                None => {}
            }
        }
        for name in placeholders(text, "env:") {
            match std::env::var(&name) {
                Ok(value) => out = out.replace(&format!("${{env:{name}}}"), &value),
                Err(_) => {
                    let key = format!("env:{name}");
                    if !missing.contains(&key) {
                        missing.push(key);
                    }
                }
            }
        }
        out
    });
    if let Some(object) = resolved.as_object_mut() {
        object.remove("disabled");
        object.remove("enabled");
        if !object.contains_key("headers") {
            if let Some(headers) = object.remove("http_headers") {
                object.insert("headers".into(), headers);
            }
        }
        if object.get("oauth").and_then(Value::as_object).is_some_and(Map::is_empty) {
            object.remove("oauth");
        }
        if let Some(file) = object.remove("envFile").and_then(|value| value.as_str().map(str::to_string)) {
            let env = object
                .entry("env")
                .or_insert_with(|| Value::Object(Map::new()));
            if let Some(env) = env.as_object_mut() {
                for (key, value) in read_env_file(&file) {
                    env.entry(key).or_insert(Value::String(value));
                }
            }
        }
        let remote = object.get("url").and_then(Value::as_str).is_some_and(|url| !url.is_empty());
        if remote && !has_authorization(object.get("headers")) {
            if let Some(token) = crate::mcp_oauth::access_token(server) {
                let headers = object
                    .entry("headers")
                    .or_insert_with(|| Value::Object(Map::new()));
                if let Some(headers) = headers.as_object_mut() {
                    headers.insert("Authorization".into(), Value::String(format!("Bearer {token}")));
                }
            }
        }
    }
    Prepared { entry: resolved, missing }
}

fn has_authorization(headers: Option<&Value>) -> bool {
    headers
        .and_then(Value::as_object)
        .is_some_and(|object| object.keys().any(|key| key.eq_ignore_ascii_case("authorization")))
}

fn read_env_file(path: &str) -> Vec<(String, String)> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let line = line.strip_prefix("export ").unwrap_or(line);
            let (key, value) = line.split_once('=')?;
            Some((key.trim().to_string(), value.trim().trim_matches(['"', '\'']).to_string()))
        })
        .collect()
}

fn placeholders(text: &str, prefix: &str) -> Vec<String> {
    let marker = format!("${{{prefix}");
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(&marker) {
        let after = &rest[start + marker.len()..];
        let Some(end) = after.find('}') else { break };
        let id = after[..end].trim();
        if !id.is_empty() {
            found.push(id.to_string());
        }
        rest = &after[end + 1..];
    }
    found
}

fn visit_strings(value: &Value, visit: &mut impl FnMut(&str)) {
    match value {
        Value::String(text) => visit(text),
        Value::Array(items) => items.iter().for_each(|item| visit_strings(item, visit)),
        Value::Object(object) => object.values().for_each(|item| visit_strings(item, visit)),
        _ => {}
    }
}

fn map_strings(value: &mut Value, map: &mut impl FnMut(&str) -> String) {
    match value {
        Value::String(text) => *text = map(text),
        Value::Array(items) => items.iter_mut().for_each(|item| map_strings(item, map)),
        Value::Object(object) => object.values_mut().for_each(|item| map_strings(item, map)),
        _ => {}
    }
}

fn store_path() -> Option<PathBuf> {
    Some(home()?.join(".multara/mcp-inputs.json"))
}

fn read_store() -> Map<String, Value> {
    store_path()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
}

fn write_store(store: &Map<String, Value>) -> Result<(), String> {
    let path = store_path().ok_or_else(|| "无法定位用户目录".to_string())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(&path, serde_json::to_string_pretty(store).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn finds_input_placeholders() {
        let entry = json!({ "headers": { "Authorization": "Bearer ${input:token}" }, "env": { "A": "${input:a} ${input:token}" } });
        let mut ids = super::referenced_inputs(&entry);
        ids.sort();
        assert_eq!(ids, vec!["a".to_string(), "token".to_string()]);
    }

    #[test]
    fn prepare_strips_host_only_fields() {
        let prepared = super::prepare("__multara_test__", &json!({ "command": "x", "disabled": false, "oauth": {} }));
        assert_eq!(prepared.entry, json!({ "command": "x" }));
        assert!(prepared.missing.is_empty());
    }
}
