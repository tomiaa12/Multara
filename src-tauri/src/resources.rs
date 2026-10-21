use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use serde_json::{Map, Value};

use crate::cli_catalog;
use crate::models::{GlobalResource, McpForm, McpInput, PromptFile, ResourceSource};

/// Skills 只扫用户目录，不扫工作区；MCP 仍可挂工作区配置。
/// 全局 CLI / 路径见 `shared/cli-catalog.json`。
const WORKSPACE_MCP_KEYS: &[(&str, &str)] = &[
    ("mcp", ".mcp.json"),
    ("cursor", ".cursor/mcp.json"),
    ("vscode", ".vscode/mcp.json"),
];

struct WorkspaceRoot {
    id: String,
    name: String,
    path: PathBuf,
}

static WORKSPACES: Mutex<Vec<WorkspaceRoot>> = Mutex::new(Vec::new());

/// 工作区来源的 owner 写成 `ws/<workspaceId>/<key>`，不含冒号，资源 ID 仍可按冒号拆分。
pub fn set_workspace_roots(roots: Vec<(String, String, PathBuf)>) {
    let mut seen = HashSet::new();
    let list = roots
        .into_iter()
        .filter(|(_, _, path)| path.is_dir() && seen.insert(path.clone()))
        .map(|(id, name, path)| WorkspaceRoot { id, name, path })
        .collect();
    if let Ok(mut current) = WORKSPACES.lock() {
        *current = list;
    }
}

fn workspace_owners(keys: &[(&str, &str)]) -> Vec<String> {
    let Ok(list) = WORKSPACES.lock() else {
        return Vec::new();
    };
    list.iter()
        .flat_map(|root| keys.iter().map(move |(key, _)| format!("ws/{}/{key}", root.id)))
        .collect()
}

fn skill_owners() -> Vec<String> {
    cli_catalog::skill_owners().into_iter().map(str::to_string).collect()
}

fn mcp_owners() -> Vec<String> {
    let mut owners: Vec<String> = cli_catalog::mcp_owners().into_iter().map(str::to_string).collect();
    owners.extend(workspace_owners(WORKSPACE_MCP_KEYS));
    owners
}

/// 解析 `ws/<id>/<key>`，返回工作区名称、根目录和 key。
fn workspace_part(owner: &str) -> Option<(String, PathBuf, String)> {
    let rest = owner.strip_prefix("ws/")?;
    let (id, key) = rest.split_once('/')?;
    let list = WORKSPACES.lock().ok()?;
    let root = list.iter().find(|root| root.id == id)?;
    Some((root.name.clone(), root.path.clone(), key.to_string()))
}

fn owner_label(owner: &str, _kind: &str) -> String {
    if let Some((name, _, key)) = workspace_part(owner) {
        let relative = WORKSPACE_MCP_KEYS
            .iter()
            .find(|(item, _)| *item == key)
            .map(|(_, path)| *path)
            .unwrap_or("");
        return format!("{name} · {relative}");
    }
    cli_catalog::label(owner).unwrap_or(owner).to_string()
}

fn owner_scope(owner: &str) -> &'static str {
    if owner.starts_with("ws/") {
        "workspace"
    } else {
        "global"
    }
}

pub fn sources() -> Vec<ResourceSource> {
    let mut output = Vec::new();
    for (kind, owners) in [("skill", skill_owners()), ("mcp", mcp_owners())] {
        for owner in owners {
            let path = if kind == "skill" { skill_root(&owner) } else { mcp_path(&owner) };
            let Some(path) = path else { continue };
            output.push(ResourceSource {
                kind,
                label: owner_label(&owner, kind),
                scope: owner_scope(&owner),
                exists: path.exists(),
                path: path.to_string_lossy().to_string(),
                owner,
            });
        }
    }
    output
}

pub fn scan() -> Vec<GlobalResource> {
    let mut resources = Vec::new();
    for owner in skill_owners() {
        scan_skills(&owner, &mut resources);
    }
    for owner in mcp_owners() {
        scan_mcp(&owner, &mut resources);
    }
    resources.sort_by(|left, right| {
        (&left.kind, &left.owner, &left.name).cmp(&(&right.kind, &right.owner, &right.name))
    });
    resources
}

pub fn transfer(resource_id: &str, target_owner: &str, copy: bool) -> Result<(), String> {
    let known = cli_catalog::entry(target_owner).is_some() || workspace_part(target_owner).is_some();
    if !known {
        return Err("目标工具不受支持".to_string());
    }
    let resource = scan()
        .into_iter()
        .find(|resource| resource.id == resource_id)
        .ok_or_else(|| "资源不存在，请重新扫描".to_string())?;
    if resource.owner == target_owner {
        return Err("来源与目标相同".to_string());
    }
    match resource.kind {
        "skill" => transfer_skill(&resource, target_owner, copy),
        "mcp" => transfer_mcp(&resource, target_owner, copy),
        _ => Err("未知资源类型".to_string()),
    }
}

pub fn selected_mcp_config(resource_ids: &[String]) -> Result<Value, String> {
    let mut servers = Map::new();
    for resource_id in resource_ids {
        let mut parts = resource_id.splitn(3, ':');
        if parts.next() != Some("mcp") {
            continue;
        }
        let owner = parts.next().ok_or_else(|| "MCP 资源 ID 无效".to_string())?;
        let name = parts.next().ok_or_else(|| "MCP 资源 ID 无效".to_string())?;
        let entry = read_mcp_entry(owner, name)?;
        if !mcp_profile_for(owner, name, &entry).enabled {
            continue;
        }
        let prepared = crate::mcp_vars::prepare(name, &entry);
        if !prepared.missing.is_empty() {
            eprintln!("[mcp] 跳过 {name}：缺少输入 {}", prepared.missing.join(", "));
            continue;
        }
        servers.insert(name.to_string(), prepared.entry);
    }
    Ok(Value::Object(Map::from_iter([(
        "mcpServers".to_string(),
        Value::Object(servers),
    )])))
}

fn scan_skills(owner: &str, output: &mut Vec<GlobalResource>) {
    let Some(root) = skill_root(owner) else {
        return;
    };
    let Ok(entries) = fs::read_dir(&root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() || !path.join("SKILL.md").is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if let Ok(resource) = skill_resource_at(owner, &name, &path) {
            output.push(resource);
        }
    }
}

fn scan_mcp(owner: &str, output: &mut Vec<GlobalResource>) {
    let Some(path) = mcp_path(owner) else {
        return;
    };
    let entries = mcp_entries(owner, &path);
    for (name, entry) in entries {
        let profile = mcp_profile_for(owner, &name, &entry);
        let config_status = if profile.config_status != "ready" {
            profile.config_status
        } else if entry.get("oauth").is_some() && !crate::mcp_oauth::has_token(&name) {
            "needs_auth".to_string()
        } else if missing_inputs(&name, &entry).is_empty() {
            "ready".to_string()
        } else {
            "needs_input".to_string()
        };
        output.push(GlobalResource {
            scope: owner_scope(owner),
            source_label: owner_label(owner, "mcp"),
            id: format!("mcp:{owner}:{name}"),
            kind: "mcp",
            owner: owner.to_string(),
            name: name.clone(),
            description: profile.summary.clone(),
            source_path: path.to_string_lossy().to_string(),
            enabled: profile.enabled,
            updated_at: modified_at(&path),
            transport: profile.transport,
            summary: profile.summary,
            needs_auth: profile.needs_auth,
            config_status,
        });
    }
}

struct McpProfile {
    transport: String,
    summary: String,
    needs_auth: bool,
    enabled: bool,
    config_status: String,
}

fn missing_inputs(name: &str, entry: &Value) -> Vec<String> {
    let stored = crate::mcp_vars::stored_inputs(name);
    crate::mcp_vars::referenced_inputs(entry)
        .into_iter()
        .filter(|id| !stored.contains_key(id))
        .collect()
}

/// VS Code 的 mcp.json 用 `servers`，其余工具用 `mcpServers`。
fn servers_key(owner: &str) -> &'static str {
    let vscode = owner == "vscode" || workspace_part(owner).is_some_and(|(_, _, key)| key == "vscode");
    if vscode {
        "servers"
    } else {
        "mcpServers"
    }
}

/// Codex / Grok / TRAE CLI 2：TOML 里 `mcp_servers` 表。
fn uses_toml_mcp_servers_map(owner: &str) -> bool {
    matches!(owner, "codex" | "grok" | "traecli")
}

/// Reasonix：全局 `config.toml` 里 `[[plugins]]`。
fn uses_toml_plugins_array(owner: &str) -> bool {
    owner == "reasonix"
}

/// OpenCode / CodeArts / DevEco / OpenClaw：配置嵌在 `mcp` / `mcp.servers`。
fn uses_nested_mcp_object(owner: &str) -> bool {
    matches!(owner, "opencode" | "codearts" | "deveco" | "openclaw")
}

fn mcp_entries(owner: &str, path: &Path) -> Vec<(String, Value)> {
    if uses_toml_mcp_servers_map(owner) || owner == "zeroclaw" || uses_toml_plugins_array(owner) {
        return mcp_entries_from_toml(owner, path);
    }
    read_json(path)
        .map(|value| json_mcp_server_map(&value, owner))
        .unwrap_or_default()
}

fn mcp_entries_from_toml(owner: &str, path: &Path) -> Vec<(String, Value)> {
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(table) = content.parse::<toml::Table>() else {
        return Vec::new();
    };
    if uses_toml_mcp_servers_map(owner) {
        return toml_table_to_entries(table.get("mcp_servers").and_then(toml::Value::as_table));
    }
    if uses_toml_plugins_array(owner) {
        return table
            .get("plugins")
            .and_then(toml::Value::as_array)
            .map(|array| {
                array
                    .iter()
                    .filter_map(|item| {
                        let plugin = item.as_table()?;
                        let name = plugin.get("name")?.as_str()?.to_string();
                        serde_json::to_value(item).ok().map(|entry| (name, entry))
                    })
                    .collect()
            })
            .unwrap_or_default();
    }
    // ZeroClaw：`[mcp.servers.name]` 或 `[[mcp.servers]]` + name 字段
    let Some(mcp) = table.get("mcp").and_then(toml::Value::as_table) else {
        return Vec::new();
    };
    let Some(servers) = mcp.get("servers") else {
        return Vec::new();
    };
    if let Some(map) = servers.as_table() {
        return toml_table_to_entries(Some(map));
    }
    if let Some(array) = servers.as_array() {
        return array
            .iter()
            .filter_map(|item| {
                let table = item.as_table()?;
                let name = table.get("name")?.as_str()?.to_string();
                serde_json::to_value(item).ok().map(|entry| (name, entry))
            })
            .collect();
    }
    Vec::new()
}

fn toml_table_to_entries(table: Option<&toml::Table>) -> Vec<(String, Value)> {
    table
        .map(|table| {
            table
                .iter()
                .filter_map(|(name, value)| {
                    serde_json::to_value(value).ok().map(|entry| (name.clone(), entry))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 从 JSON 根取出服务器表：`mcpServers` / `servers` / `mcp.servers` / 扁平 `mcp`。
fn json_mcp_server_map(value: &Value, owner: &str) -> Vec<(String, Value)> {
    if let Some(servers) = value.get(servers_key(owner)).and_then(Value::as_object) {
        return servers.iter().map(|(name, entry)| (name.clone(), entry.clone())).collect();
    }
    if let Some(mcp) = value.get("mcp") {
        if let Some(servers) = mcp.get("servers").and_then(Value::as_object) {
            return servers.iter().map(|(name, entry)| (name.clone(), entry.clone())).collect();
        }
        // QwenPaw：`mcp.clients`
        if let Some(clients) = mcp.get("clients").and_then(Value::as_object) {
            return clients.iter().map(|(name, entry)| (name.clone(), entry.clone())).collect();
        }
        if let Some(servers) = mcp.as_object() {
            return servers
                .iter()
                .filter(|(key, entry)| *key != "servers" && *key != "clients" && entry.is_object())
                .map(|(name, entry)| (name.clone(), entry.clone()))
                .collect();
        }
    }
    // Antigravity 等可能直接把服务器写在根上
    if owner == "antigravity" {
        if let Some(root) = value.as_object() {
            let entries: Vec<_> = root
                .iter()
                .filter(|(key, entry)| {
                    *key != "mcpServers"
                        && *key != "servers"
                        && *key != "mcp"
                        && entry.is_object()
                })
                .map(|(name, entry)| (name.clone(), entry.clone()))
                .collect();
            if !entries.is_empty() {
                return entries;
            }
        }
    }
    Vec::new()
}

/// 对照 VS Code 的 MCP 配置：stdio 看 command，远程看 url / type，授权看 oauth。
/// 停用：JSON 用 `disabled: true`，Codex 用 `enabled = false`。
/// Cursor 全局启停在 state.vscdb，见 `mcp_profile_for`。
fn mcp_profile(entry: &Value) -> McpProfile {
    let disabled = entry_config_disabled(entry);
    let url = entry
        .get("url")
        .or_else(|| entry.get("serverUrl"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let (command, command_args) = mcp_command_parts(entry);
    let type_name = entry.get("type").and_then(Value::as_str).unwrap_or("");
    let http = !url.is_empty()
        || matches!(type_name, "http" | "sse" | "streamable-http" | "remote");
    let transport = if type_name == "sse" {
        "sse"
    } else if http {
        "http"
    } else {
        "stdio"
    };
    let args = entry
        .get("args")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .filter(|arg| !looks_secret(arg))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|text| !text.is_empty())
        .unwrap_or(command_args);
    let summary = if http {
        url.to_string()
    } else if args.is_empty() {
        command.clone()
    } else {
        format!("{command} {args}")
    };
    let needs_auth = entry.get("oauth").is_some() || entry.get("authentication").is_some();
    let incomplete = if http { url.is_empty() } else { command.is_empty() };
    let config_status = if disabled {
        "disabled"
    } else if incomplete {
        "incomplete"
    } else {
        "ready"
    };
    McpProfile {
        transport: transport.to_string(),
        summary,
        needs_auth,
        enabled: !disabled,
        config_status: config_status.to_string(),
    }
}

fn entry_config_disabled(entry: &Value) -> bool {
    entry.get("disabled").and_then(Value::as_bool) == Some(true)
        || entry.get("enabled").and_then(Value::as_bool) == Some(false)
}

fn entry_incomplete(entry: &Value) -> bool {
    let url = entry
        .get("url")
        .or_else(|| entry.get("serverUrl"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let (command, _) = mcp_command_parts(entry);
    let type_name = entry.get("type").and_then(Value::as_str).unwrap_or("");
    let http = !url.is_empty()
        || matches!(type_name, "http" | "sse" | "streamable-http" | "remote");
    if http {
        url.is_empty()
    } else {
        command.is_empty()
    }
}

/// Cursor 的启用状态读 `cursor/disabledGlobalMcpServers`；其它来源看配置文件字段。
fn mcp_profile_for(owner: &str, name: &str, entry: &Value) -> McpProfile {
    let mut profile = mcp_profile(entry);
    if owner != "cursor" {
        return profile;
    }
    let Some(home_dir) = home() else {
        return profile;
    };
    let disabled = crate::cursor_mcp_state::is_user_server_disabled(&home_dir, name);
    profile.enabled = !disabled;
    if disabled {
        profile.config_status = "disabled".to_string();
    } else if profile.config_status == "disabled" {
        // mcp.json 里的 disabled 对 Cursor UI 无效，恢复为 incomplete/ready
        profile.config_status = if entry_incomplete(entry) {
            "incomplete".to_string()
        } else {
            "ready".to_string()
        };
    }
    profile
}

fn looks_secret(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    ["token", "secret", "password", "api_key", "apikey", "authorization"]
        .iter()
        .any(|part| lower.contains(part))
}

/// OpenCode / CodeArts 等把 command 写成字符串数组：`["npx", "-y", "pkg"]`。
fn mcp_command_parts(entry: &Value) -> (String, String) {
    if let Some(command) = entry.get("command").and_then(Value::as_str) {
        return (command.to_string(), String::new());
    }
    let Some(items) = entry.get("command").and_then(Value::as_array) else {
        return (String::new(), String::new());
    };
    let parts: Vec<&str> = items.iter().filter_map(Value::as_str).collect();
    let Some((first, rest)) = parts.split_first() else {
        return (String::new(), String::new());
    };
    (
        (*first).to_string(),
        rest.iter()
            .copied()
            .filter(|arg| !looks_secret(arg))
            .collect::<Vec<_>>()
            .join(" "),
    )
}

pub fn create_skill(owner: &str, name: &str, description: &str, body: &str) -> Result<GlobalResource, String> {
    let root = ensure_skill_root(owner)?;
    let folder = unique_skill_folder(&root, &skill_folder_name(name)?)?;
    let target = root.join(&folder);
    fs::create_dir_all(&target).map_err(|error| error.to_string())?;
    let description = description.trim();
    let body = body.trim();
    let markdown = format!(
        "---\nname: {folder}\ndescription: {description}\n---\n\n# {name}\n\n{body}\n",
        name = name.trim(),
        description = if description.is_empty() {
            "用户创建的 Skill。".to_string()
        } else {
            yaml_single_line(description)
        },
        body = if body.is_empty() { "在这里写具体步骤。" } else { body },
    );
    fs::write(target.join("SKILL.md"), markdown).map_err(|error| error.to_string())?;
    skill_resource_at(owner, &folder, &target)
}

/// 把完整的 SKILL.md 内容（例如 AI 生成的）写进新 Skill，名称取 frontmatter 的 name。
pub fn create_skill_from_markdown(owner: &str, fallback_name: &str, markdown: &str) -> Result<GlobalResource, String> {
    let root = ensure_skill_root(owner)?;
    let name = parse_skill_name(markdown).unwrap_or_else(|| fallback_name.to_string());
    let folder = unique_skill_folder(&root, &skill_folder_name(&name)?)?;
    let target = root.join(&folder);
    fs::create_dir_all(&target).map_err(|error| error.to_string())?;
    let content = if markdown.trim_start().starts_with("---") {
        markdown.trim().to_string()
    } else {
        format!("---\nname: {folder}\ndescription: {}\n---\n\n{}", yaml_single_line(fallback_name), markdown.trim())
    };
    fs::write(target.join("SKILL.md"), format!("{content}\n")).map_err(|error| error.to_string())?;
    skill_resource_at(owner, &folder, &target)
}

pub fn delete_skill(resource_id: &str) -> Result<(), String> {
    let path = skill_directory(resource_id)?;
    trash::delete(&path).map_err(|error| format!("移到回收站失败：{error}"))?;
    let mut state = read_skill_state();
    state.remove(resource_id);
    write_skill_state(&state)
}

pub fn set_skill_enabled(resource_id: &str, enabled: bool) -> Result<(), String> {
    skill_directory(resource_id)?;
    let mut state = read_skill_state();
    if enabled {
        state.remove(resource_id);
    } else {
        state.insert(resource_id.to_string());
    }
    write_skill_state(&state)
}

pub fn skill_enabled(resource_id: &str) -> bool {
    !read_skill_state().contains(resource_id)
}

/// 停用的 Skill 记在 `~/.multara/skill-state.json`，不改动各工具自己的目录。
fn read_skill_state() -> HashSet<String> {
    home()
        .map(|home| home.join(".multara/skill-state.json"))
        .and_then(|path| read_json(&path))
        .and_then(|value| value.get("disabled").and_then(Value::as_array).cloned())
        .map(|items| items.iter().filter_map(|item| item.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

fn write_skill_state(state: &HashSet<String>) -> Result<(), String> {
    let path = home().ok_or_else(|| "无法定位用户目录".to_string())?.join(".multara/skill-state.json");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut disabled: Vec<&String> = state.iter().collect();
    disabled.sort();
    atomic_write(&path, &serde_json::to_string_pretty(&serde_json::json!({ "disabled": disabled })).map_err(|error| error.to_string())?)
}

/// 找出可以转换成 Skill 的提示文件：工作区 `.github/prompts` 和 VS Code 用户目录的 prompts。
pub fn prompt_files() -> Vec<PromptFile> {
    let mut folders: Vec<(String, PathBuf)> = Vec::new();
    if let Ok(list) = WORKSPACES.lock() {
        for root in list.iter() {
            folders.push((format!("{} · .github/prompts", root.name), root.path.join(".github/prompts")));
        }
    }
    if let Some(user) = vscode_user_dir() {
        folders.push(("VS Code".to_string(), user.join("prompts")));
    }
    let mut output = Vec::new();
    for (label, folder) in folders {
        let Ok(entries) = fs::read_dir(&folder) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = entry.file_name().to_string_lossy().to_string();
            let Some(name) = file_name.strip_suffix(".prompt.md") else { continue };
            let content = fs::read_to_string(&path).unwrap_or_default();
            output.push(PromptFile {
                path: path.to_string_lossy().to_string(),
                name: name.to_string(),
                description: parse_skill_description(&content).unwrap_or_default(),
                source_label: label.clone(),
            });
        }
    }
    output.sort_by(|left, right| left.name.cmp(&right.name));
    output
}

pub fn convert_prompt_to_skill(prompt_path: &str, owner: &str) -> Result<GlobalResource, String> {
    let path = Path::new(prompt_path);
    let file_name = path.file_name().and_then(|name| name.to_str()).unwrap_or("");
    let name = file_name.strip_suffix(".prompt.md").ok_or_else(|| "只能转换 .prompt.md 文件".to_string())?;
    let content = fs::read_to_string(path).map_err(|error| format!("读取提示文件失败：{error}"))?;
    let description = parse_skill_description(&content).unwrap_or_else(|| format!("由提示文件 {name} 转换"));
    let body = strip_frontmatter(&content);
    let root = ensure_skill_root(owner)?;
    let folder = unique_skill_folder(&root, &skill_folder_name(name)?)?;
    let target = root.join(&folder);
    fs::create_dir_all(&target).map_err(|error| error.to_string())?;
    let markdown = format!(
        "---\nname: {folder}\ndescription: {}\n---\n\n{}\n",
        yaml_single_line(&description),
        body.trim()
    );
    fs::write(target.join("SKILL.md"), markdown).map_err(|error| error.to_string())?;
    skill_resource_at(owner, &folder, &target)
}

fn strip_frontmatter(content: &str) -> String {
    if !content.trim_start().starts_with("---") {
        return content.to_string();
    }
    let mut lines = content.trim_start().lines();
    lines.next();
    let mut closed = false;
    let mut body = Vec::new();
    for line in lines {
        if !closed {
            closed = line.trim() == "---";
            continue;
        }
        body.push(line);
    }
    body.join("\n")
}

pub fn import_skill_from_folder(path: &str, owner: &str) -> Result<GlobalResource, String> {
    let source = find_skill_dir(Path::new(path), None)?;
    install_skill_dir(&source, owner)
}

pub fn import_skill_from_url(url: &str, git_command: &str, owner: &str) -> Result<GlobalResource, String> {
    let source = parse_skill_source_url(url)?;
    let tmp = tempfile::tempdir().map_err(|error| error.to_string())?;
    clone_github(
        git_command,
        &source.owner,
        &source.repo,
        &source.branch,
        tmp.path(),
    )?;
    let skill_dir = find_skill_dir(tmp.path(), source.subdir.as_deref())?;
    install_skill_dir(&skill_dir, owner)
}

#[derive(Debug, PartialEq, Eq)]
pub struct SkillSourceUrl {
    pub owner: String,
    pub repo: String,
    pub branch: String,
    pub subdir: Option<String>,
}

pub fn parse_skill_source_url(raw: &str) -> Result<SkillSourceUrl, String> {
    let (host, parts) = host_and_path(raw)?;
    if matches!(host.as_str(), "skills.sh" | "www.skills.sh") {
        if parts.len() < 3 {
            return Err("Skills.sh 地址应为 skills.sh/{owner}/{repo}/{skill}".to_string());
        }
        return Ok(SkillSourceUrl {
            owner: parts[0].clone(),
            repo: parts[1].clone(),
            branch: "main".into(),
            subdir: Some(parts[2].clone()),
        });
    }
    if matches!(host.as_str(), "github.com" | "www.github.com") {
        if parts.len() < 2 {
            return Err("GitHub 地址应为 github.com/{owner}/{repo}".to_string());
        }
        let owner = parts[0].clone();
        let repo = parts[1].trim_end_matches(".git").to_string();
        if parts.get(2).is_some_and(|item| item == "tree" || item == "blob") && parts.len() >= 4 {
            return Ok(SkillSourceUrl {
                owner,
                repo,
                branch: parts[3].clone(),
                subdir: skill_subdir_from_segments(&parts[4..]),
            });
        }
        return Ok(SkillSourceUrl {
            owner,
            repo,
            branch: "main".into(),
            subdir: None,
        });
    }
    Err("目前支持 GitHub 和 Skills.sh 地址，例如 github.com/anthropics/skills 或 skills.sh/owner/repo/skill".to_string())
}

fn host_and_path(raw: &str) -> Result<(String, Vec<String>), String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Skill 地址无效".to_string());
    }
    let rest = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .unwrap_or(trimmed);
    let rest = rest.split(['?', '#']).next().unwrap_or(rest);
    let mut parts = rest.split('/');
    let host = parts.next().unwrap_or("").trim().to_ascii_lowercase();
    if host.is_empty() {
        return Err("Skill 地址无效".to_string());
    }
    Ok((
        host,
        parts
            .filter(|part| !part.is_empty())
            .map(percent_decode)
            .collect(),
    ))
}

fn skill_subdir_from_segments(parts: &[String]) -> Option<String> {
    if parts.is_empty() {
        return None;
    }
    let mut segments = parts.to_vec();
    if segments.last().is_some_and(|item| item.eq_ignore_ascii_case("SKILL.md")) {
        segments.pop();
    }
    if segments.is_empty() {
        None
    } else {
        Some(segments.join("/"))
    }
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&value[index + 1..index + 3], 16) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| value.to_string())
}

fn yaml_single_line(value: &str) -> String {
    value.replace('\n', " ").replace(':', "：")
}

fn skill_folder_name(name: &str) -> Result<String, String> {
    let mut out = String::new();
    for ch in name.trim().chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || !ch.is_ascii() {
            out.push(ch);
        } else if ch.is_whitespace() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        return Err("Skill 名称无效".to_string());
    }
    Ok(out)
}

fn ensure_skill_root(owner: &str) -> Result<PathBuf, String> {
    let root = skill_root(owner).ok_or_else(|| "这个来源不支持 Skills".to_string())?;
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    Ok(root)
}

fn unique_skill_folder(root: &Path, name: &str) -> Result<String, String> {
    if !root.join(name).exists() {
        return Ok(name.to_string());
    }
    for index in 2..100 {
        let candidate = format!("{name}-{index}");
        if !root.join(&candidate).exists() {
            return Ok(candidate);
        }
    }
    Err("同名 Skill 太多，请换一个名称".to_string())
}

fn install_skill_dir(source: &Path, owner: &str) -> Result<GlobalResource, String> {
    if !source.join("SKILL.md").is_file() {
        return Err("所选目录没有 SKILL.md".to_string());
    }
    let fallback = source
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("skill");
    let parsed = parse_skill_name(&fs::read_to_string(source.join("SKILL.md")).unwrap_or_default());
    let root = ensure_skill_root(owner)?;
    let folder = unique_skill_folder(&root, &skill_folder_name(parsed.as_deref().unwrap_or(fallback))?)?;
    let target = root.join(&folder);
    copy_directory(source, &target)?;
    skill_resource_at(owner, &folder, &target)
}

fn skill_resource_at(owner: &str, name: &str, path: &Path) -> Result<GlobalResource, String> {
    let id = format!("skill:{owner}:{name}");
    Ok(GlobalResource {
        enabled: skill_enabled(&id),
        id,
        kind: "skill",
        owner: owner.to_string(),
        scope: owner_scope(owner),
        source_label: owner_label(owner, "skill"),
        name: name.to_string(),
        description: skill_description(&path.join("SKILL.md")),
        source_path: path.to_string_lossy().to_string(),
        updated_at: modified_at(&path.join("SKILL.md")),
        transport: String::new(),
        summary: String::new(),
        needs_auth: false,
        config_status: String::new(),
    })
}

fn parse_skill_name(content: &str) -> Option<String> {
    let mut lines = content.lines();
    if !content.trim_start().starts_with("---") {
        return None;
    }
    lines.find(|line| line.trim() == "---");
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        if let Some(value) = line.trim().strip_prefix("name:") {
            let text = value.trim().trim_matches(['"', '\'']);
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }
    }
    None
}

fn find_skill_dir(root: &Path, preferred: Option<&str>) -> Result<PathBuf, String> {
    if let Some(name) = preferred {
        for candidate in [
            root.join(name),
            root.join("skills").join(name),
            root.join(".claude").join("skills").join(name),
        ] {
            if candidate.join("SKILL.md").is_file() {
                return Ok(candidate);
            }
        }
    }
    if root.join("SKILL.md").is_file() {
        return Ok(root.to_path_buf());
    }
    let mut found = Vec::new();
    collect_skill_dirs(root, 0, &mut found)?;
    if let Some(name) = preferred {
        if let Some(match_dir) = found.iter().find(|path| {
            path.file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value == name)
        }) {
            return Ok(match_dir.clone());
        }
    }
    if found.len() == 1 {
        return Ok(found.remove(0));
    }
    if found.is_empty() {
        return Err("没有找到 SKILL.md，确认这是一个 Agent Skill 目录".to_string());
    }
    Err("仓库里有多个 Skill，请把地址指到具体目录，例如 .../tree/main/pdf".to_string())
}

fn collect_skill_dirs(directory: &Path, depth: usize, output: &mut Vec<PathBuf>) -> Result<(), String> {
    if depth > 3 {
        return Ok(());
    }
    for entry in fs::read_dir(directory).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if path.file_name().is_some_and(|name| name == ".git") {
            continue;
        }
        if path.join("SKILL.md").is_file() {
            output.push(path);
        } else {
            collect_skill_dirs(&path, depth + 1, output)?;
        }
    }
    Ok(())
}

fn clone_github(git: &str, owner: &str, repo: &str, branch: &str, dest: &Path) -> Result<(), String> {
    let url = format!("https://github.com/{owner}/{repo}.git");
    let dest_text = dest.to_string_lossy().to_string();
    let mut args = vec![
        "clone".into(),
        "--depth".into(),
        "1".into(),
        "--branch".into(),
        branch.to_string(),
        "--".into(),
        url.clone(),
        dest_text.clone(),
    ];
    let first = crate::process::command(git)
        .args(&args)
        .output()
        .map_err(|error| format!("无法调用 git：{error}"))?;
    if first.status.success() {
        return Ok(());
    }
    if branch != "main" && branch != "master" {
        return Err(format!(
            "克隆 GitHub 仓库失败：{}",
            String::from_utf8_lossy(&first.stderr).trim()
        ));
    }
    let _ = fs::remove_dir_all(dest);
    fs::create_dir_all(dest).map_err(|error| error.to_string())?;
    args.retain(|item| item != "--branch" && item != branch);
    let fallback = crate::process::command(git)
        .args(&args)
        .output()
        .map_err(|error| format!("无法调用 git：{error}"))?;
    if fallback.status.success() {
        return Ok(());
    }
    Err(format!(
        "克隆 GitHub 仓库失败：{}",
        String::from_utf8_lossy(&fallback.stderr).trim()
    ))
}

fn transfer_skill(resource: &GlobalResource, target_owner: &str, copy: bool) -> Result<(), String> {
    let root = skill_root(target_owner).ok_or_else(|| "目标不支持 Skills".to_string())?;
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let target = root.join(&resource.name);
    if target.exists() {
        return Err(format!("目标已存在同名 Skill：{}", resource.name));
    }
    let temporary = root.join(format!(".{}.multara-import", resource.name));
    if temporary.exists() {
        fs::remove_dir_all(&temporary).map_err(|error| error.to_string())?;
    }
    copy_directory(Path::new(&resource.source_path), &temporary)?;
    fs::rename(&temporary, &target).map_err(|error| {
        let _ = fs::remove_dir_all(&temporary);
        format!("写入目标 Skill 失败：{error}")
    })?;
    if !copy {
        fs::remove_dir_all(&resource.source_path).map_err(|error| {
            let _ = fs::remove_dir_all(&target);
            format!("移动后清理来源失败，已回滚目标：{error}")
        })?;
    }
    Ok(())
}

fn transfer_mcp(resource: &GlobalResource, target_owner: &str, copy: bool) -> Result<(), String> {
    let entry = read_mcp_entry(&resource.owner, &resource.name)?;
    write_mcp_entry(target_owner, &resource.name, entry)?;
    if !copy {
        if let Err(error) = remove_mcp_entry(&resource.owner, &resource.name) {
            let _ = remove_mcp_entry(target_owner, &resource.name);
            return Err(format!("清理来源 MCP 失败，已回滚目标：{error}"));
        }
    }
    Ok(())
}

fn read_mcp_entry(owner: &str, name: &str) -> Result<Value, String> {
    let path = mcp_path(owner).ok_or_else(|| "来源不支持 MCP".to_string())?;
    mcp_entries(owner, &path)
        .into_iter()
        .find(|(existing, _)| existing == name)
        .map(|(_, entry)| entry)
        .ok_or_else(|| "MCP 配置不存在".to_string())
}

/// 读出某个来源的全部服务器交给 `change` 修改，再原子写回。Codex 是 TOML，其余是 JSON。
/// 配置文件存在但解析失败时直接报错，不能当成空文件覆盖掉用户的其它配置。
fn update_servers(
    owner: &str,
    change: impl FnOnce(&mut Map<String, Value>) -> Result<(), String>,
) -> Result<(), String> {
    let path = mcp_path(owner).ok_or_else(|| "这个来源不支持 MCP".to_string())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    if uses_toml_mcp_servers_map(owner) {
        let mut table = if path.exists() {
            fs::read_to_string(&path)
                .map_err(|error| error.to_string())?
                .parse::<toml::Table>()
                .map_err(|error| format!("配置不是有效的 TOML：{error}"))?
        } else {
            toml::Table::new()
        };
        let mut servers = match table.get("mcp_servers") {
            Some(value) => serde_json::to_value(value)
                .map_err(|error| error.to_string())?
                .as_object()
                .cloned()
                .ok_or_else(|| "mcp_servers 配置格式无效".to_string())?,
            None => Map::new(),
        };
        change(&mut servers)?;
        table.insert(
            "mcp_servers".to_string(),
            toml::Value::try_from(Value::Object(servers)).map_err(|error| error.to_string())?,
        );
        return atomic_write(&path, &toml::to_string_pretty(&table).map_err(|error| error.to_string())?);
    }
    let mut root = if path.exists() {
        read_json(&path).ok_or_else(|| format!("{} 不是有效的 JSON，请先修好再保存", path.display()))?
    } else {
        Value::Object(Map::new())
    };
    let servers = json_mcp_servers_mut(&mut root, owner)?;
    change(servers)?;
    atomic_write(&path, &serde_json::to_string_pretty(&root).map_err(|error| error.to_string())?)
}

fn json_mcp_servers_mut<'a>(root: &'a mut Value, owner: &str) -> Result<&'a mut Map<String, Value>, String> {
    let object = root
        .as_object_mut()
        .ok_or_else(|| "MCP 配置不是 JSON 对象".to_string())?;
    if !uses_nested_mcp_object(owner) {
        return object
            .entry(servers_key(owner).to_string())
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .ok_or_else(|| "MCP 服务器列表格式无效".to_string());
    }
    let mcp = object
        .entry("mcp".to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| "mcp 配置格式无效".to_string())?;
    // 已有扁平 mcp 服务时继续写扁平；否则写 mcp.servers
    let has_flat = mcp
        .iter()
        .any(|(key, value)| key != "servers" && value.is_object());
    if has_flat {
        return Ok(mcp);
    }
    mcp.entry("servers".to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| "mcp.servers 配置格式无效".to_string())
}

fn write_mcp_entry(owner: &str, name: &str, entry: Value) -> Result<(), String> {
    update_servers(owner, |servers| {
        if servers.contains_key(name) {
            return Err(format!("目标已存在同名 MCP：{name}"));
        }
        servers.insert(name.to_string(), entry);
        Ok(())
    })
}

fn remove_mcp_entry(owner: &str, name: &str) -> Result<(), String> {
    update_servers(owner, |servers| {
        servers
            .remove(name)
            .map(|_| ())
            .ok_or_else(|| "MCP 不存在".to_string())
    })
}

/// 表单管理的字段。保存时只替换这些，其余字段（timeout、自定义扩展等）原样保留。
const MANAGED_MCP_KEYS: &[&str] = &[
    "type", "url", "serverUrl", "headers", "http_headers", "oauth", "command", "args", "env", "cwd",
    "envFile", "disabled", "enabled",
];

fn input_owner(owner: Option<&str>) -> String {
    owner.map(str::trim).filter(|value| !value.is_empty()).unwrap_or("multara").to_string()
}

pub fn save_mcp(input: McpInput) -> Result<(), String> {
    let owner = input_owner(input.owner.as_deref());
    let name = input.name.trim().to_string();
    if name.is_empty() || name.contains(':') {
        return Err("MCP 名称不能为空，也不能包含冒号".to_string());
    }
    let previous = input
        .previous_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let fields = mcp_fields_from_input(&input, &owner)?;
    let codex = owner == "codex";
    update_servers(&owner, |servers| {
        if servers.contains_key(&name) && previous.as_deref() != Some(name.as_str()) {
            return Err(format!("已存在同名 MCP：{name}"));
        }
        let base = previous.as_deref().and_then(|previous| servers.remove(previous));
        let mut entry = base.and_then(|value| value.as_object().cloned()).unwrap_or_default();
        for key in MANAGED_MCP_KEYS {
            entry.remove(*key);
        }
        entry.extend(fields);
        // Cursor 启停写 state.vscdb，不写 mcp.json
        if owner != "cursor" && !input.enabled {
            if codex {
                entry.insert("enabled".to_string(), Value::Bool(false));
            } else {
                entry.insert("disabled".to_string(), Value::Bool(true));
            }
        }
        servers.insert(name.clone(), Value::Object(entry));
        Ok(())
    })?;
    if owner == "cursor" {
        let home_dir = home().ok_or_else(|| "找不到用户目录".to_string())?;
        crate::cursor_mcp_state::set_user_server_enabled(&home_dir, &name, input.enabled)?;
    }
    if let Some(previous) = previous.filter(|previous| *previous != name) {
        crate::mcp_vars::rename_inputs(&previous, &name)?;
        if owner == "cursor" {
            let home_dir = home().ok_or_else(|| "找不到用户目录".to_string())?;
            let _ = crate::cursor_mcp_state::set_user_server_enabled(&home_dir, &previous, true);
        }
    }
    Ok(())
}

/// 从目录（如 MCP Registry）安装：写入完整配置，不允许覆盖同名服务器。
pub fn install_mcp(owner: &str, name: &str, entry: Value) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() || name.contains(':') {
        return Err("MCP 名称不能为空，也不能包含冒号".to_string());
    }
    if !entry.is_object() {
        return Err("MCP 配置必须是对象".to_string());
    }
    write_mcp_entry(&input_owner(Some(owner)), name, entry)
}

pub fn delete_mcp(owner: &str, name: &str) -> Result<(), String> {
    remove_mcp_entry(&input_owner(Some(owner)), name)
}

pub fn authorize_mcp(owner: &str, name: &str) -> Result<(), String> {
    let entry = read_mcp_entry(&input_owner(Some(owner)), name)?;
    let url = entry
        .get("url")
        .or_else(|| entry.get("serverUrl"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "这个 MCP 没有 HTTP 地址，不能授权".to_string())?;
    crate::mcp_oauth::authorize(name, url)
}

pub fn set_mcp_enabled(owner: &str, name: &str, enabled: bool) -> Result<(), String> {
    let owner = input_owner(Some(owner));
    if owner == "cursor" {
        let home_dir = home().ok_or_else(|| "找不到用户目录".to_string())?;
        // 确认配置里存在该服务器
        let _ = read_mcp_entry(&owner, name)?;
        return crate::cursor_mcp_state::set_user_server_enabled(&home_dir, name, enabled);
    }
    let codex = owner == "codex";
    update_servers(&owner, |servers| {
        let object = servers
            .get_mut(name)
            .and_then(Value::as_object_mut)
            .ok_or_else(|| "MCP 不存在".to_string())?;
        object.remove("disabled");
        object.remove("enabled");
        if !enabled {
            if codex {
                object.insert("enabled".to_string(), Value::Bool(false));
            } else {
                object.insert("disabled".to_string(), Value::Bool(true));
            }
        }
        Ok(())
    })
}

pub fn mcp_form(owner: &str, name: &str) -> Result<McpForm, String> {
    let owner = input_owner(Some(owner));
    let entry = read_mcp_entry(&owner, name)?;
    let profile = mcp_profile_for(&owner, name, &entry);
    let text = |key: &str| entry.get(key).and_then(Value::as_str).unwrap_or("").to_string();
    Ok(McpForm {
        name: name.to_string(),
        owner,
        transport: profile.transport,
        command: text("command"),
        cwd: text("cwd"),
        env_file: text("envFile"),
        args: entry
            .get("args")
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(|item| item.as_str().map(str::to_string)).collect())
            .unwrap_or_default(),
        url: entry
            .get("url")
            .or_else(|| entry.get("serverUrl"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        env_text: pairs_text(entry.get("env"), "="),
        headers_text: pairs_text(entry.get("headers").or_else(|| entry.get("http_headers")), ": "),
        needs_auth: entry.get("oauth").is_some(),
        enabled: profile.enabled,
    })
}

pub fn mcp_entry_json(owner: &str, name: &str) -> Result<String, String> {
    let entry = read_mcp_entry(&input_owner(Some(owner)), name)?;
    let wrapped = serde_json::json!({ name: entry });
    serde_json::to_string_pretty(&wrapped).map_err(|error| error.to_string())
}

pub fn probe_mcp(owner: &str, name: &str) -> Result<crate::mcp_probe::McpProbe, String> {
    let owner = input_owner(Some(owner));
    let entry = read_mcp_entry(&owner, name)?;
    if !mcp_profile_for(&owner, name, &entry).enabled {
        let mut result = crate::mcp_probe::McpProbe::default();
        result.status = "disabled".into();
        result.error = "已禁用".into();
        return Ok(result);
    }
    Ok(crate::mcp_probe::probe(name, &entry))
}

/// 供后台缓存队列判断是否还需要探测。
pub fn mcp_is_enabled(owner: &str, name: &str) -> bool {
    let owner = input_owner(Some(owner));
    let Ok(entry) = read_mcp_entry(&owner, name) else {
        return false;
    };
    mcp_profile_for(&owner, name, &entry).enabled
}

pub fn mcp_inputs(owner: &str, name: &str) -> Result<crate::models::McpInputs, String> {
    let entry = read_mcp_entry(&input_owner(Some(owner)), name)?;
    Ok(crate::models::McpInputs {
        ids: crate::mcp_vars::referenced_inputs(&entry),
        values: crate::mcp_vars::stored_inputs(name),
    })
}

fn mcp_fields_from_input(input: &McpInput, owner: &str) -> Result<Map<String, Value>, String> {
    let codex = owner == "codex";
    let mut object = Map::new();
    if matches!(input.transport.as_str(), "http" | "sse") {
        let url = input.url.trim();
        if url.is_empty() {
            return Err("远程 MCP 需要填写 URL".to_string());
        }
        if codex && input.transport == "sse" {
            return Err("Codex 不支持旧版 SSE 传输，请改用 HTTP".to_string());
        }
        if !codex {
            object.insert("type".to_string(), Value::String(input.transport.clone()));
        }
        object.insert("url".to_string(), Value::String(url.to_string()));
        let headers = parse_pairs(&input.headers_text, ":")?;
        if !headers.is_empty() {
            let key = if codex { "http_headers" } else { "headers" };
            object.insert(key.to_string(), Value::Object(headers));
        }
        if input.needs_auth && !codex {
            object.insert("oauth".to_string(), Value::Object(Map::new()));
        }
    } else {
        let command = input.command.trim();
        if command.is_empty() {
            return Err("本地 MCP 需要填写命令".to_string());
        }
        object.insert("command".to_string(), Value::String(command.to_string()));
        let args: Vec<Value> = input
            .args
            .iter()
            .map(|arg| arg.trim())
            .filter(|arg| !arg.is_empty())
            .map(|arg| Value::String(arg.to_string()))
            .collect();
        if !args.is_empty() {
            object.insert("args".to_string(), Value::Array(args));
        }
        let env = parse_pairs(&input.env_text, "=")?;
        if !env.is_empty() {
            object.insert("env".to_string(), Value::Object(env));
        }
        if !input.cwd.trim().is_empty() {
            object.insert("cwd".to_string(), Value::String(input.cwd.trim().to_string()));
        }
        if !input.env_file.trim().is_empty() && !codex {
            object.insert("envFile".to_string(), Value::String(input.env_file.trim().to_string()));
        }
    }
    Ok(object)
}

fn parse_pairs(text: &str, separator: &str) -> Result<Map<String, Value>, String> {
    let mut pairs = Map::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once(separator) else {
            return Err(format!("无法解析这一行：{line}"));
        };
        let key = key.trim();
        if key.is_empty() {
            return Err(format!("无法解析这一行：{line}"));
        }
        pairs.insert(key.to_string(), Value::String(value.trim().to_string()));
    }
    Ok(pairs)
}

fn pairs_text(value: Option<&Value>, separator: &str) -> String {
    value
        .and_then(Value::as_object)
        .map(|object| {
            object
                .iter()
                .filter_map(|(key, item)| item.as_str().map(|text| format!("{key}{separator}{text}")))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

pub fn resource_path(resource_id: &str) -> Result<PathBuf, String> {
    let resource = scan()
        .into_iter()
        .find(|item| item.id == resource_id)
        .ok_or_else(|| "资源不存在，请重新扫描".to_string())?;
    let path = PathBuf::from(&resource.source_path);
    Ok(if resource.kind == "skill" { path.join("SKILL.md") } else { path })
}

pub fn skill_directory(resource_id: &str) -> Result<PathBuf, String> {
    let resource = scan()
        .into_iter()
        .find(|item| item.id == resource_id && item.kind == "skill")
        .ok_or_else(|| "技能不存在，请重新扫描".to_string())?;
    let path = PathBuf::from(&resource.source_path);
    if !path.is_dir() {
        return Err("技能目录不存在".to_string());
    }
    Ok(path)
}

fn modified_at(path: &Path) -> String {
    let Ok(metadata) = fs::metadata(path) else {
        return String::new();
    };
    let Ok(modified) = metadata.modified() else {
        return String::new();
    };
    let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) else {
        return String::new();
    };
    chrono::DateTime::from_timestamp(duration.as_secs() as i64, duration.subsec_nanos())
        .map(|value| value.to_rfc3339())
        .unwrap_or_default()
}

fn skill_root(owner: &str) -> Option<PathBuf> {
    if owner.starts_with("ws/") {
        return None;
    }
    cli_catalog::skill_root(&home()?, owner)
}

fn mcp_path(owner: &str) -> Option<PathBuf> {
    if let Some((_, root, key)) = workspace_part(owner) {
        let relative = WORKSPACE_MCP_KEYS.iter().find(|(item, _)| *item == key)?.1;
        return Some(root.join(relative));
    }
    cli_catalog::mcp_path(&home()?, owner)
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn vscode_user_dir() -> Option<PathBuf> {
    cli_catalog::mcp_path(&home()?, "vscode").and_then(|path| path.parent().map(Path::to_path_buf))
}

fn skill_description(path: &Path) -> String {
    let content = fs::read_to_string(path).unwrap_or_default();
    parse_skill_description(&content).unwrap_or_else(|| "全局 Skill".to_string())
}

/// 从 SKILL.md 的 frontmatter 里取 description。
/// 支持单行写法和 YAML 块标量：`>` / `>-` 折叠成一行，`|` / `|-` 保留换行。
fn parse_skill_description(content: &str) -> Option<String> {
    let mut lines = content.lines();
    // 有 frontmatter 时只在 `---` 之间找，避免误读正文里的 description:
    let in_frontmatter = content.trim_start().starts_with("---");
    if in_frontmatter {
        lines.find(|line| line.trim() == "---");
    }
    let mut lines = lines.peekable();
    while let Some(line) = lines.next() {
        if in_frontmatter && line.trim() == "---" {
            break;
        }
        let Some(value) = line.trim().strip_prefix("description:") else { continue };
        let value = value.trim();
        let block = value.chars().next().filter(|c| *c == '>' || *c == '|');
        let Some(style) = block else {
            let text = value.trim_matches(['"', '\'']).to_string();
            return (!text.is_empty()).then_some(text);
        };
        // 块标量正文：后面缩进的行（空行也算块内），直到遇到不缩进的下一个键
        let mut parts: Vec<&str> = Vec::new();
        while let Some(next) = lines.peek() {
            if next.trim().is_empty() {
                parts.push("");
                lines.next();
                continue;
            }
            if !next.starts_with([' ', '\t']) {
                break;
            }
            parts.push(next.trim());
            lines.next();
        }
        while parts.last() == Some(&"") {
            parts.pop();
        }
        let text = if style == '|' {
            parts.join("\n")
        } else {
            // 折叠：段内换行变空格，空行保留为段落换行
            parts
                .split(|part| part.is_empty())
                .map(|paragraph| fold_lines(paragraph))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let text = text.trim().to_string();
        return (!text.is_empty()).then_some(text);
    }
    None
}

/// YAML 折叠用空格连接；两侧都是中文（非 ASCII）时不加空格，避免「中文。 适用于」
fn fold_lines(lines: &[&str]) -> String {
    let mut text = String::new();
    for line in lines {
        let cjk_join = text.chars().last().is_some_and(|c| !c.is_ascii())
            && line.chars().next().is_some_and(|c| !c.is_ascii());
        if !text.is_empty() && !cjk_join {
            text.push(' ');
        }
        text.push_str(line);
    }
    text
}

fn read_json(path: &Path) -> Option<Value> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text)
        .ok()
        .or_else(|| serde_json::from_str(&strip_jsonc(&text)).ok())
}

/// VS Code 的 mcp.json 允许注释和尾逗号，去掉后再按 JSON 解析。
fn strip_jsonc(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    let mut in_string = false;
    while index < chars.len() {
        let ch = chars[index];
        if in_string {
            out.push(ch);
            if ch == '\\' && index + 1 < chars.len() {
                out.push(chars[index + 1]);
                index += 1;
            } else if ch == '"' {
                in_string = false;
            }
        } else if ch == '"' {
            in_string = true;
            out.push(ch);
        } else if ch == '/' && chars.get(index + 1) == Some(&'/') {
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
            continue;
        } else if ch == '/' && chars.get(index + 1) == Some(&'*') {
            index += 2;
            while index + 1 < chars.len() && !(chars[index] == '*' && chars[index + 1] == '/') {
                index += 1;
            }
            index += 2;
            continue;
        } else if ch == ',' {
            let next = chars[index + 1..].iter().find(|item| !item.is_whitespace());
            if !matches!(next, Some('}') | Some(']')) {
                out.push(ch);
            }
        } else {
            out.push(ch);
        }
        index += 1;
    }
    out
}

fn atomic_write(path: &Path, content: &str) -> Result<(), String> {
    let temporary = path.with_extension("multara.tmp");
    let backup = path.with_extension("multara.backup");
    fs::write(&temporary, content).map_err(|error| error.to_string())?;
    if backup.exists() {
        fs::remove_file(&backup).map_err(|error| error.to_string())?;
    }
    if path.exists() {
        fs::rename(path, &backup).map_err(|error| error.to_string())?;
    }
    fs::rename(&temporary, path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        if backup.exists() {
            let _ = fs::rename(&backup, path);
        }
        error.to_string()
    })?;
    if backup.exists() {
        fs::remove_file(backup).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn copy_directory(source: &Path, target: &Path) -> Result<(), String> {
    fs::create_dir_all(target).map_err(|error| error.to_string())?;
    for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        if file_type.is_symlink() {
            return Err("Skill 包含符号链接，已阻止迁移".to_string());
        }
        let destination = target.join(entry.file_name());
        if file_type.is_dir() {
            copy_directory(&entry.path(), &destination)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), destination).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{atomic_write, copy_directory, parse_skill_description};

    #[test]
    fn reads_folded_and_literal_skill_descriptions() {
        let folded = "---\nname: x\ndescription: >-\n  新建 Agent 时，标题必须使用简体中文。适用于 Task 工具的\n  description、New Agent。每次创建或恢复\n  Agent 时都要应用。\n---\n# x\n";
        assert_eq!(
            parse_skill_description(folded).as_deref(),
            Some("新建 Agent 时，标题必须使用简体中文。适用于 Task 工具的 description、New Agent。每次创建或恢复 Agent 时都要应用。"),
        );
        let literal = "---\ndescription: |\n  第一行\n  第二行\nname: x\n---\n";
        assert_eq!(parse_skill_description(literal).as_deref(), Some("第一行\n第二行"));
        let inline = "---\nname: x\ndescription: \"单行描述\"\n---\n";
        assert_eq!(parse_skill_description(inline).as_deref(), Some("单行描述"));
        assert_eq!(parse_skill_description("---\nname: x\n---\ndescription: 正文\n"), None);
    }

    #[test]
    fn atomic_write_replaces_existing_config_and_cleans_backup() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("mcp.json");
        fs::write(&path, "before").unwrap();
        atomic_write(&path, "after").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "after");
        assert!(!path.with_extension("multara.tmp").exists());
        assert!(!path.with_extension("multara.backup").exists());
    }

    #[test]
    fn copies_complete_skill_directory() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let target = temp.path().join("target");
        fs::create_dir_all(source.join("references")).unwrap();
        fs::write(source.join("SKILL.md"), "# Test").unwrap();
        fs::write(source.join("references/guide.md"), "guide").unwrap();
        copy_directory(&source, &target).unwrap();
        assert_eq!(
            fs::read_to_string(target.join("references/guide.md")).unwrap(),
            "guide"
        );
    }

    #[test]
    fn parses_github_and_skills_sh_urls() {
        assert_eq!(
            super::parse_skill_source_url("https://github.com/anthropics/skills").unwrap(),
            super::SkillSourceUrl {
                owner: "anthropics".into(),
                repo: "skills".into(),
                branch: "main".into(),
                subdir: None,
            }
        );
        assert_eq!(
            super::parse_skill_source_url("github.com/foo/bar.git/tree/dev/skills/pdf").unwrap(),
            super::SkillSourceUrl {
                owner: "foo".into(),
                repo: "bar".into(),
                branch: "dev".into(),
                subdir: Some("skills/pdf".into()),
            }
        );
        assert_eq!(
            super::parse_skill_source_url(
                "https://github.com/foo/bar/blob/main/skills/pdf/SKILL.md"
            )
            .unwrap(),
            super::SkillSourceUrl {
                owner: "foo".into(),
                repo: "bar".into(),
                branch: "main".into(),
                subdir: Some("skills/pdf".into()),
            }
        );
        assert_eq!(
            super::parse_skill_source_url("https://skills.sh/owner/repo/my-skill").unwrap(),
            super::SkillSourceUrl {
                owner: "owner".into(),
                repo: "repo".into(),
                branch: "main".into(),
                subdir: Some("my-skill".into()),
            }
        );
        assert!(super::parse_skill_source_url("https://clawhub.com/x").is_err());
    }

    #[test]
    fn slugs_skill_folder_names() {
        assert_eq!(super::skill_folder_name("Code Review").unwrap(), "Code-Review");
        assert_eq!(super::skill_folder_name("代码审查").unwrap(), "代码审查");
        assert!(super::skill_folder_name("   ").is_err());
    }

    #[test]
    fn finds_nested_or_direct_skill_dir() {
        let temp = tempfile::tempdir().unwrap();
        let skill = temp.path().join("skills/pdf");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "---\nname: pdf\n---\n").unwrap();
        assert_eq!(
            super::find_skill_dir(temp.path(), Some("pdf")).unwrap(),
            skill
        );
        assert_eq!(super::find_skill_dir(&skill, None).unwrap(), skill);
        assert!(super::find_skill_dir(temp.path(), None).is_ok());
    }
}
