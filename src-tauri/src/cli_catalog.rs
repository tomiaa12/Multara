use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::Deserialize;

const RAW: &str = include_str!("../../shared/cli-catalog.json");

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliEntry {
    pub id: String,
    pub label: String,
    pub command: Option<String>,
    pub protocol: Option<String>,
    pub gateway: Option<bool>,
    pub url_suffix: Option<String>,
    pub runtime: Option<bool>,
    pub skills_home: Option<String>,
    pub mcp_home: Option<String>,
    pub mcp_special: Option<String>,
    pub supported: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct CatalogFile {
    entries: Vec<CliEntry>,
}

fn catalog() -> &'static [CliEntry] {
    static CATALOG: OnceLock<Vec<CliEntry>> = OnceLock::new();
    CATALOG
        .get_or_init(|| {
            serde_json::from_str::<CatalogFile>(RAW)
                .map(|file| file.entries)
                .unwrap_or_default()
        })
        .as_slice()
}

pub fn entry(id: &str) -> Option<&'static CliEntry> {
    catalog().iter().find(|item| item.id == id)
}

/// 「添加运行时」弹窗里的基础 CLI：与前端 `runtimeEntries()` 同一过滤条件。
pub fn runtime_entries() -> Vec<&'static CliEntry> {
    catalog()
        .iter()
        .filter(|item| {
            item.runtime == Some(true)
                && item
                    .command
                    .as_deref()
                    .is_some_and(|command| !command.trim().is_empty())
                && item
                    .protocol
                    .as_deref()
                    .is_some_and(|protocol| !protocol.trim().is_empty())
        })
        .collect()
}

pub fn label(id: &str) -> Option<&'static str> {
    entry(id).map(|item| item.label.as_str())
}

pub fn skill_owners() -> Vec<&'static str> {
    catalog()
        .iter()
        .filter(|item| item.skills_home.is_some())
        .map(|item| item.id.as_str())
        .collect()
}

pub fn mcp_owners() -> Vec<&'static str> {
    catalog()
        .iter()
        .filter(|item| item.mcp_home.is_some() || item.mcp_special.as_deref() == Some("vscode"))
        .map(|item| item.id.as_str())
        .collect()
}

pub fn supports_skill_target(id: &str) -> bool {
    entry(id).is_some_and(|item| item.skills_home.is_some())
}

pub fn skill_root(home: &Path, owner: &str) -> Option<PathBuf> {
    let rel = entry(owner)?.skills_home.as_ref()?;
    Some(home.join(rel))
}

pub fn mcp_path(home: &Path, owner: &str) -> Option<PathBuf> {
    let item = entry(owner)?;
    if item.mcp_special.as_deref() == Some("vscode") {
        return vscode_user_dir(home).map(|dir| dir.join("mcp.json"));
    }
    let rel = item.mcp_home.as_ref()?;
    Some(home.join(rel))
}

fn vscode_user_dir(home: &Path) -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(|path| PathBuf::from(path).join("Code/User"))
    } else if cfg!(target_os = "macos") {
        Some(home.join("Library/Application Support/Code/User"))
    } else {
        Some(home.join(".config/Code/User"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_shared_catalog() {
        assert!(entry("cursor").is_some());
        assert!(entry("dsh").is_some());
        assert_eq!(entry("dsh").and_then(|item| item.skills_home.as_deref()), Some(".dsh/skills"));
        assert!(skill_owners().contains(&"opencode"));
        assert!(skill_owners().contains(&"dsh"));
        assert!(skill_owners().contains(&"kimi"));
        assert!(mcp_owners().contains(&"vscode"));
        assert!(mcp_owners().contains(&"copilot"));
        assert!(mcp_owners().contains(&"opencode"));
        assert!(mcp_owners().contains(&"kiro"));
    }

    #[test]
    fn resolves_paths_under_home() {
        let home = Path::new("/Users/demo");
        assert_eq!(
            skill_root(home, "opencode"),
            Some(PathBuf::from("/Users/demo/.config/opencode/skills"))
        );
        assert_eq!(
            mcp_path(home, "cursor"),
            Some(PathBuf::from("/Users/demo/.cursor/mcp.json"))
        );
    }
}
