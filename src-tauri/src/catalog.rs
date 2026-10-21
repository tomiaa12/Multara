//! Skills / MCP 商店：从 GitHub 官方 Skills 仓库和 MCP Registry 搜索，安装走现有导入接口。

use serde::Serialize;
use serde_json::Value;

use crate::http_client;

const SKILL_TREE_URL: &str = "https://api.github.com/repos/anthropics/skills/git/trees/main?recursive=1";
const MCP_REGISTRY_URL: &str = "https://registry.modelcontextprotocol.io/v0/servers";
const SKILL_DOCS: &str = "https://github.com/anthropics/skills";

#[derive(Clone, Serialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CatalogItem {
    pub kind: String,
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: String,
    pub url: String,
    /// Skill 是 GitHub 目录地址；MCP 是可写入 mcp.json 的服务对象 JSON。
    pub install: String,
}

pub fn search(kind: &str, query: &str) -> Result<Vec<CatalogItem>, String> {
    match kind {
        "skill" => search_skills(query),
        "mcp" => search_mcp(query),
        _ => Err("商店类型无效".to_string()),
    }
}

fn search_skills(query: &str) -> Result<Vec<CatalogItem>, String> {
    match http_client::get(SKILL_TREE_URL, "application/vnd.github+json") {
        Ok(response) if (200..300).contains(&response.status) => {
            let value: Value = serde_json::from_str(&response.body).map_err(|error| error.to_string())?;
            Ok(filter_items(parse_skill_tree(&value), query))
        }
        Ok(response) => Err(format!("Skills 商店 HTTP {}", response.status)),
        Err(_) => Ok(filter_items(fallback_skills(), query)),
    }
}

fn search_mcp(query: &str) -> Result<Vec<CatalogItem>, String> {
    let url = if query.trim().is_empty() {
        format!("{MCP_REGISTRY_URL}?limit=40")
    } else {
        format!(
            "{MCP_REGISTRY_URL}?search={}&limit=40",
            urlencoding(query.trim())
        )
    };
    match http_client::get(&url, "application/json") {
        Ok(response) if (200..300).contains(&response.status) => {
            let value: Value = serde_json::from_str(&response.body).map_err(|error| error.to_string())?;
            Ok(filter_items(parse_mcp_registry(&value), query))
        }
        Ok(response) => Err(format!("MCP 商店 HTTP {}", response.status)),
        Err(error) => Err(error),
    }
}

pub fn parse_skill_tree(value: &Value) -> Vec<CatalogItem> {
    let Some(tree) = value.get("tree").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut items = Vec::new();
    for entry in tree {
        if entry.get("type").and_then(Value::as_str) != Some("blob") {
            continue;
        }
        let Some(path) = entry.get("path").and_then(Value::as_str) else {
            continue;
        };
        if !path.ends_with("/SKILL.md") && path != "SKILL.md" {
            continue;
        }
        let folder = path.trim_end_matches("/SKILL.md").trim_end_matches("SKILL.md");
        let name = folder
            .rsplit('/')
            .next()
            .filter(|part| !part.is_empty())
            .unwrap_or(folder)
            .to_string();
        if name.is_empty() {
            continue;
        }
        let url = format!("https://github.com/anthropics/skills/tree/main/{folder}");
        items.push(CatalogItem {
            kind: "skill".into(),
            id: format!("github:anthropics/skills:{folder}"),
            name,
            description: folder.replace('/', " · "),
            source: "Anthropic Skills".into(),
            url,
            install: format!("https://github.com/anthropics/skills/tree/main/{folder}"),
        });
    }
    items.sort_by(|left, right| left.name.cmp(&right.name));
    items
}

pub fn parse_mcp_registry(value: &Value) -> Vec<CatalogItem> {
    let servers = value
        .get("servers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut items = Vec::new();
    for row in servers {
        let server = row.get("server").cloned().unwrap_or(row);
        let name = server
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if name.is_empty() {
            continue;
        }
        let short = name.rsplit('/').next().unwrap_or(&name).to_string();
        let description = server
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let Some(install) = mcp_install_json(&server, &short) else {
            continue;
        };
        items.push(CatalogItem {
            kind: "mcp".into(),
            id: name.clone(),
            name: short,
            description,
            source: "MCP Registry".into(),
            url: format!("https://registry.modelcontextprotocol.io/servers/{name}"),
            install,
        });
    }
    items
}

fn mcp_install_json(server: &Value, fallback_name: &str) -> Option<String> {
    if let Some(remote) = server
        .get("remotes")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
    {
        let url = remote.get("url").and_then(Value::as_str)?;
        let type_name = match remote.get("type").and_then(Value::as_str).unwrap_or("http") {
            "sse" => "sse",
            _ => "http",
        };
        return serde_json::to_string(&serde_json::json!({
            fallback_name: { "type": type_name, "url": url }
        }))
        .ok();
    }
    let package = server.get("packages").and_then(Value::as_array)?.first()?;
    let identifier = package.get("identifier").and_then(Value::as_str)?;
    let registry = package.get("registryType").and_then(Value::as_str).unwrap_or("npm");
    let (command, args): (&str, Vec<&str>) = match registry {
        "pypi" | "python" => ("uvx", vec![identifier]),
        _ => ("npx", vec!["-y", identifier]),
    };
    serde_json::to_string(&serde_json::json!({
        fallback_name: { "command": command, "args": args }
    }))
    .ok()
}

fn filter_items(items: Vec<CatalogItem>, query: &str) -> Vec<CatalogItem> {
    let text = query.trim().to_ascii_lowercase();
    if text.is_empty() {
        return items.into_iter().take(80).collect();
    }
    items
        .into_iter()
        .filter(|item| {
            item.name.to_ascii_lowercase().contains(&text)
                || item.description.to_ascii_lowercase().contains(&text)
                || item.source.to_ascii_lowercase().contains(&text)
        })
        .take(80)
        .collect()
}

fn fallback_skills() -> Vec<CatalogItem> {
    [
        ("pdf", "PDF 处理：提取、创建和编辑 PDF。"),
        ("docx", "Word 文档读写。"),
        ("xlsx", "电子表格读写。"),
        ("pptx", "演示文稿创建。"),
        ("skill-creator", "编写新的 Agent Skill。"),
    ]
    .into_iter()
    .map(|(name, description)| CatalogItem {
        kind: "skill".into(),
        id: format!("github:anthropics/skills:{name}"),
        name: name.into(),
        description: description.into(),
        source: "Anthropic Skills".into(),
        url: format!("{SKILL_DOCS}/tree/main/{name}"),
        install: format!("{SKILL_DOCS}/tree/main/{name}"),
    })
    .collect()
}

fn urlencoding(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{filter_items, parse_mcp_registry, parse_skill_tree};
    use serde_json::json;

    #[test]
    fn parses_github_skill_tree() {
        let items = parse_skill_tree(&json!({
            "tree": [
                { "path": "document-skills/pdf/SKILL.md", "type": "blob" },
                { "path": "README.md", "type": "blob" },
                { "path": "skills", "type": "tree" }
            ]
        }));
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "pdf");
        assert!(items[0].install.contains("document-skills/pdf"));
    }

    #[test]
    fn parses_registry_stdio_and_http() {
        let items = parse_mcp_registry(&json!({
            "servers": [
                { "server": {
                    "name": "io.github.example/github",
                    "description": "GitHub tools",
                    "packages": [{ "registryType": "npm", "identifier": "@modelcontextprotocol/server-github" }]
                }},
                { "server": {
                    "name": "io.github.example/remote",
                    "description": "Remote",
                    "remotes": [{ "type": "streamable-http", "url": "https://example.com/mcp" }]
                }}
            ]
        }));
        assert_eq!(items.len(), 2);
        assert!(items[0].install.contains("npx"));
        assert!(items[1].install.contains("https://example.com/mcp"));
    }

    #[test]
    fn filters_catalog_by_query() {
        let items = parse_skill_tree(&json!({
            "tree": [
                { "path": "pdf/SKILL.md", "type": "blob" },
                { "path": "xlsx/SKILL.md", "type": "blob" }
            ]
        }));
        assert_eq!(filter_items(items, "pdf").len(), 1);
    }
}
