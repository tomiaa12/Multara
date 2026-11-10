//! 把智能体已开启的 MCP（`{"mcpServers": ...}`）交到每个 CLI 自己认的入口。
//! 通道对齐 Multica：没有统一参数，空列表不改该 CLI 自己的配置。

use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

pub struct McpLaunch {
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    /// ACP `session/new` 和 dsh `mcp_servers` 用的服务器列表。
    pub protocol_servers: Vec<Value>,
    temps: Vec<PathBuf>,
}

impl Drop for McpLaunch {
    fn drop(&mut self) {
        for dir in self.temps.drain(..) {
            let _ = fs::remove_dir_all(dir);
        }
    }
}

pub fn prepare(
    provider: &str,
    cwd: &Path,
    config_file: Option<&Path>,
) -> Result<McpLaunch, String> {
    let mut launch = McpLaunch {
        args: Vec::new(),
        env: Vec::new(),
        protocol_servers: Vec::new(),
        temps: Vec::new(),
    };
    let Some(path) = config_file else {
        return Ok(launch);
    };
    let raw = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let config: Value = serde_json::from_str(&raw).map_err(|error| error.to_string())?;
    let servers = config
        .get("mcpServers")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if servers.is_empty() {
        return Ok(launch);
    }
    launch.protocol_servers = protocol_servers(&servers);
    match provider {
        "claude" => {}
        "codebuddy" | "qwen" => {
            launch.args.push("--mcp-config".into());
            launch.args.push(path.display().to_string());
        }
        "cursor" => prepare_cursor(&mut launch, cwd, &servers)?,
        "codex" => prepare_codex(&mut launch, &servers)?,
        "opencode" | "codearts" => {
            let content = opencode_config(&servers)?;
            launch.env.push(("OPENCODE_CONFIG_CONTENT".into(), content));
        }
        "pi" => prepare_pi(&mut launch, &servers)?,
        // OMP 会读附加工作区里的 .cursor/mcp.json，不写进用户仓库。
        "omp" => {
            let root = mcp_root(&servers)?;
            launch.args.push("--add-dir".into());
            launch.args.push(root.display().to_string());
            launch.temps.push(root);
        }
        _ => {}
    }
    Ok(launch)
}

fn protocol_servers(servers: &Map<String, Value>) -> Vec<Value> {
    let mut names: Vec<&String> = servers.keys().collect();
    names.sort();
    names
        .into_iter()
        .filter_map(|name| acp_server(name, servers.get(name)?))
        .collect()
}

fn acp_server(name: &str, entry: &Value) -> Option<Value> {
    let object = entry.as_object()?;
    let command = object.get("command").and_then(Value::as_str).unwrap_or("").trim();
    if !command.is_empty() {
        let args = object.get("args").cloned().unwrap_or_else(|| Value::Array(Vec::new()));
        let env = env_pairs(object.get("env"));
        return Some(serde_json::json!({
            "name": name,
            "command": command,
            "args": args,
            "env": env,
        }));
    }
    let url = object.get("url").and_then(Value::as_str).unwrap_or("").trim();
    if url.is_empty() {
        return None;
    }
    let kind = object.get("type").and_then(Value::as_str).unwrap_or("");
    let kind = if kind.eq_ignore_ascii_case("sse") { "sse" } else { "http" };
    Some(serde_json::json!({
        "type": kind,
        "name": name,
        "url": url,
        "headers": env_pairs(object.get("headers")),
    }))
}

fn env_pairs(value: Option<&Value>) -> Vec<Value> {
    let Some(object) = value.and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut keys: Vec<&String> = object.keys().collect();
    keys.sort();
    keys.into_iter()
        .filter_map(|key| {
            let text = object.get(key)?.as_str()?;
            Some(serde_json::json!({ "name": key, "value": text }))
        })
        .collect()
}

fn prepare_cursor(launch: &mut McpLaunch, cwd: &Path, servers: &Map<String, Value>) -> Result<(), String> {
    // cursor-agent 只从各个 workspace root 的 .cursor/mcp.json 和 ~/.cursor/mcp.json 读配置。
    // 把配置放在临时目录，用 --add-dir 挂进去，避免写进用户仓库。
    let root = mcp_root(servers)?;
    launch.temps.push(root.clone());
    launch.args.push("--add-dir".into());
    launch.args.push(root.display().to_string());
    launch.args.push("--approve-mcps".into());

    let project = project_root(cwd);
    let data_dir = tempfile::tempdir().map_err(|error| error.to_string())?;
    let cursor_data = data_dir.path().join("cursor-data");
    let mut approvals = approval_keys(&project, servers)?;
    approvals.extend(approval_keys(&root, servers)?);
    let project_data = cursor_data.join("projects").join(slugify(&project));
    fs::create_dir_all(&project_data).map_err(|error| error.to_string())?;
    write_private(
        &project_data.join("mcp-approvals.json"),
        &serde_json::to_vec_pretty(&approvals).map_err(|error| error.to_string())?,
    )?;
    let trust = serde_json::json!({
        "trustedAt": "1970-01-01T00:00:00Z",
        "workspacePath": project.display().to_string(),
        "trustMethod": "multara-managed",
    });
    write_private(
        &project_data.join(".workspace-trusted"),
        &serde_json::to_vec_pretty(&trust).map_err(|error| error.to_string())?,
    )?;
    launch.env.push((
        "CURSOR_DATA_DIR".into(),
        cursor_data.display().to_string(),
    ));
    launch.temps.push(data_dir.keep());
    Ok(())
}

/// 临时目录里放一份 `.cursor/mcp.json`，给会扫描附加工作区的 CLI 用。
fn mcp_root(servers: &Map<String, Value>) -> Result<PathBuf, String> {
    let dir = tempfile::tempdir().map_err(|error| error.to_string())?;
    let cursor_dir = dir.path().join(".cursor");
    fs::create_dir_all(&cursor_dir).map_err(|error| error.to_string())?;
    write_private(
        &cursor_dir.join("mcp.json"),
        &serde_json::to_vec_pretty(&serde_json::json!({ "mcpServers": servers }))
            .map_err(|error| error.to_string())?,
    )?;
    Ok(dir.keep())
}

fn prepare_codex(launch: &mut McpLaunch, servers: &Map<String, Value>) -> Result<(), String> {
    let source = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs_home().map(|home| home.join(".codex")));
    let home = tempfile::tempdir().map_err(|error| error.to_string())?;
    if let Some(source) = source.as_ref().filter(|path| path.is_dir()) {
        for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
            let Ok(entry) = entry else { continue };
            let file_type = entry.file_type().ok();
            if !file_type.is_some_and(|kind| kind.is_file()) {
                continue;
            }
            if entry.file_name() == "config.toml" {
                continue;
            }
            let _ = fs::copy(entry.path(), home.path().join(entry.file_name()));
        }
    }
    let mut config = source
        .as_ref()
        .and_then(|path| fs::read_to_string(path.join("config.toml")).ok())
        .unwrap_or_default();
    config = strip_mcp_tables(&config);
    config.push_str(&codex_mcp_block(servers)?);
    write_private(&home.path().join("config.toml"), config.as_bytes())?;
    launch.env.push(("CODEX_HOME".into(), home.path().display().to_string()));
    launch.temps.push(home.keep());
    Ok(())
}

fn prepare_pi(launch: &mut McpLaunch, servers: &Map<String, Value>) -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|error| error.to_string())?;
    let encoded = serde_json::to_string(servers).map_err(|error| error.to_string())?;
    let literal = serde_json::to_string(&encoded).map_err(|error| error.to_string())?;
    let script = format!(
        "const servers = JSON.parse({literal});\n\
         export default function (pi) {{\n\
           if (typeof pi.registerMcpServer !== \"function\") {{\n\
             throw new Error(\"Multara MCP delivery requires Pi 0.99 or newer\");\n\
           }}\n\
           for (const [name, config] of Object.entries(servers)) {{\n\
             try {{\n\
               pi.registerMcpServer(name, config);\n\
             }} catch (error) {{\n\
               const reason = error instanceof Error ? error.message : String(error);\n\
               console.error(\"[multara] could not register MCP server \" + JSON.stringify(name) + \": \" + reason);\n\
             }}\n\
           }}\n\
         }}\n"
    );
    let path = dir.path().join("mcp-extension.mjs");
    write_private(&path, script.as_bytes())?;
    launch.args.push("--extension".into());
    launch.args.push(path.display().to_string());
    launch.temps.push(dir.keep());
    Ok(())
}

fn opencode_config(servers: &Map<String, Value>) -> Result<String, String> {
    let mut mcp = Map::new();
    for (name, entry) in servers {
        let object = entry.as_object().ok_or_else(|| format!("MCP {name} 配置无效"))?;
        let url = object.get("url").and_then(Value::as_str).unwrap_or("").trim();
        if !url.is_empty() {
            let mut remote = Map::new();
            remote.insert("type".into(), Value::String("remote".into()));
            remote.insert("url".into(), Value::String(url.to_string()));
            if let Some(headers) = object.get("headers") {
                remote.insert("headers".into(), headers.clone());
            }
            mcp.insert(name.clone(), Value::Object(remote));
            continue;
        }
        let command = object.get("command").and_then(Value::as_str).unwrap_or("").trim();
        if command.is_empty() {
            return Err(format!("MCP {name} 缺少 command 或 url"));
        }
        let mut argv = vec![Value::String(command.to_string())];
        if let Some(args) = object.get("args").and_then(Value::as_array) {
            argv.extend(args.iter().cloned());
        }
        let mut local = Map::new();
        local.insert("type".into(), Value::String("local".into()));
        local.insert("command".into(), Value::Array(argv));
        if let Some(env) = object.get("env") {
            local.insert("environment".into(), env.clone());
        }
        mcp.insert(name.clone(), Value::Object(local));
    }
    serde_json::to_string(&serde_json::json!({ "mcp": mcp })).map_err(|error| error.to_string())
}

fn codex_mcp_block(servers: &Map<String, Value>) -> Result<String, String> {
    let mut names: Vec<&String> = servers.keys().collect();
    names.sort();
    let mut block = String::from("\n# BEGIN multara-managed mcp_servers\n");
    for name in names {
        if !name.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-') {
            return Err(format!("MCP 名称 {name} 不能写入 Codex 配置"));
        }
        let entry = servers.get(name).and_then(Value::as_object);
        let Some(entry) = entry else {
            return Err(format!("MCP {name} 配置无效"));
        };
        block.push_str(&format!("[mcp_servers.{name}]\n"));
        let remote = entry.get("url").and_then(Value::as_str).is_some_and(|url| !url.is_empty())
            && entry.get("command").and_then(Value::as_str).unwrap_or("").is_empty();
        let mut fields = BTreeMap::new();
        for (key, value) in entry {
            if matches!(key.as_str(), "tools" | "prompts" | "resources" | "type" | "enabled" | "disabled") {
                continue;
            }
            if remote && key == "headers" {
                fields.insert("http_headers".to_string(), value.clone());
                continue;
            }
            fields.insert(key.clone(), value.clone());
        }
        if remote {
            fields.insert("experimental_use_rmcp_client".into(), Value::Bool(true));
        }
        for (key, value) in fields {
            block.push_str(&format!("{key} = {}\n", toml_inline(&value)));
        }
        block.push('\n');
    }
    block.push_str("# END multara-managed mcp_servers\n");
    Ok(block)
}

fn toml_inline(value: &Value) -> String {
    match value {
        Value::String(text) => serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into()),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::Array(items) => {
            let inner = items.iter().map(toml_inline).collect::<Vec<_>>().join(", ");
            format!("[{inner}]")
        }
        Value::Object(object) => {
            let inner = object
                .iter()
                .map(|(key, item)| format!("{key} = {}", toml_inline(item)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{ {inner} }}")
        }
        Value::Null => "\"\"".into(),
    }
}

fn strip_mcp_tables(content: &str) -> String {
    let mut out = Vec::new();
    let mut skipping = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("[mcp_servers") {
            skipping = true;
            continue;
        }
        if skipping && trimmed.starts_with('[') {
            skipping = false;
        }
        if !skipping {
            out.push(line);
        }
    }
    let mut text = out.join("\n");
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

fn approval_keys(project: &Path, servers: &Map<String, Value>) -> Result<Vec<String>, String> {
    let mut names: Vec<&String> = servers.keys().collect();
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let server = approval_server(servers.get(name).unwrap_or(&Value::Null))?;
            let mut payload = br#"{"path":"#.to_vec();
            payload.extend(serde_json::to_vec(&project.display().to_string()).map_err(|error| error.to_string())?);
            payload.extend(br#","server":"#);
            payload.extend(server);
            payload.push(b'}');
            let digest = Sha256::digest(&payload);
            let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
            Ok(format!("{name}-{}", &hex[..16]))
        })
        .collect()
}

fn approval_server(entry: &Value) -> Result<Vec<u8>, String> {
    let object = entry.as_object().ok_or_else(|| "MCP 配置无效".to_string())?;
    let keys: &[&str] = if object.contains_key("command") {
        &["type", "command", "args", "env", "cwd"]
    } else if object.contains_key("url") {
        &["type", "url", "headers"]
    } else {
        return serde_json::to_vec(entry).map_err(|error| error.to_string());
    };
    let mut normalized = Map::new();
    for key in keys {
        if let Some(value) = object.get(*key) {
            normalized.insert((*key).to_string(), value.clone());
        }
    }
    serde_json::to_vec(&Value::Object(normalized)).map_err(|error| error.to_string())
}

fn project_root(cwd: &Path) -> PathBuf {
    let start = fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    let mut dir = start.clone();
    loop {
        if dir.join(".git").exists() {
            return dir;
        }
        let Some(parent) = dir.parent() else {
            return start;
        };
        if parent == dir {
            return start;
        }
        dir = parent.to_path_buf();
    }
}

fn slugify(path: &Path) -> String {
    let mut slug = String::new();
    let mut dash = false;
    for ch in path.display().to_string().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            dash = false;
        } else if !dash {
            slug.push('-');
            dash = true;
        }
    }
    slug.trim_matches('-').to_string()
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut file = fs::File::create(path).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = file.set_permissions(fs::Permissions::from_mode(0o600));
    }
    file.write_all(bytes).map_err(|error| error.to_string())?;
    Ok(())
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::approval_keys;
    use serde_json::json;

    fn keys(path: &str, server: serde_json::Value) -> Vec<String> {
        let servers = serde_json::Map::from_iter([("fetch".into(), server.clone())]);
        let name = if server.get("command").and_then(|value| value.as_str()) == Some("uvx")
            && server.get("cwd").is_some()
        {
            "withtypecwd"
        } else if server.get("url").is_some() {
            "remote"
        } else {
            "fetch"
        };
        let mut named = serde_json::Map::new();
        named.insert(name.into(), server);
        let _ = servers;
        approval_keys(std::path::Path::new(path), &named).unwrap()
    }

    #[test]
    fn cursor_approval_matches_stdio() {
        let got = keys("/tmp/work", json!({"command":"uvx","args":["mcp-server-fetch"]}));
        assert_eq!(got, vec!["fetch-b3a6127d3cbd8e52".to_string()]);
    }

    #[test]
    fn cursor_approval_reorders_stdio_fields() {
        let got = keys(
            "/tmp/work",
            json!({"args":["mcp-server-fetch"],"command":"uvx","env":{"TOKEN":"x"}}),
        );
        assert_eq!(got, vec!["fetch-4c7232835349773b".to_string()]);
    }

    #[test]
    fn cursor_approval_does_not_escape_html() {
        let got = keys(
            "/tmp/work",
            json!({"command":"uvx","args":["mcp"],"env":{"URL":"https://x.test?a=1&b=2"}}),
        );
        assert_eq!(got, vec!["fetch-37ab92a3e088d73a".to_string()]);
    }

    #[test]
    fn cursor_approval_keeps_stdio_type_and_cwd() {
        let got = keys(
            "/private/tmp/work",
            json!({"enabled":true,"cwd":"/tmp/probe","timeout":30,"args":["mcp"],"command":"uvx","type":"stdio"}),
        );
        assert_eq!(got, vec!["withtypecwd-ba71d2f70913528e".to_string()]);
    }

    #[test]
    fn cursor_approval_matches_remote() {
        let got = keys(
            "/private/tmp/work",
            json!({"enabled":true,"headers":{"Authorization":"Bearer x"},"timeout":30,"url":"https://mcp.example.com","type":"http"}),
        );
        assert_eq!(got, vec!["remote-abc2ffe78532b37e".to_string()]);
    }
}
