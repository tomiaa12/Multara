use std::{
    fs,
    path::{Component, Path},
};

use rusqlite::Connection;
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    files,
    models::Workspace,
    snapshot_db::{self, TurnFileSnapshot},
};

const MAX_FILE_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedSnapshot {
    pub workspace_id: String,
    pub file_path: String,
    pub content: String,
}

const FILE_MUTATION_TOOLS: &[&str] = &[
    "Edit",
    "Write",
    "Create",
    "EditFile",
    "WriteFile",
    "MultiEdit",
    "NotebookEdit",
    "ApplyPatch",
    "apply_patch",
];

const EDITOR_TOOLS_WITH_COMMAND: &[&str] = &["str_replace_editor", "StrReplaceEditor"];

pub fn is_file_mutation_tool(tool: &str, input: Option<&Value>) -> bool {
    let name = tool.trim();
    if FILE_MUTATION_TOOLS.iter().any(|item| *item == name) {
        return true;
    }
    let lower = name.to_ascii_lowercase();
    if lower.contains("str_replace") {
        return true;
    }
    if EDITOR_TOOLS_WITH_COMMAND
        .iter()
        .any(|item| item.eq_ignore_ascii_case(name))
    {
        if let Some(command) = input
            .and_then(|value| value.get("command"))
            .and_then(Value::as_str)
        {
            return matches!(command, "edit" | "create" | "write");
        }
        return true;
    }
    false
}

pub fn extract_file_path(input: Option<&Value>) -> Option<String> {
    let input = input?;
    let raw = ["file_path", "path", "target_file", "targetFile", "filePath"]
        .into_iter()
        .find_map(|key| input.get(key).and_then(Value::as_str))
        .filter(|value| !value.trim().is_empty())?;
    Some(normalize_rel_path(raw))
}

pub fn resolve_workspace_file<'a>(
    workspaces: &'a [Workspace],
    raw_path: &str,
) -> Option<(&'a Workspace, String)> {
    let normalized = normalize_rel_path(raw_path);
    if normalized.is_empty() || path_has_parent_dir(&normalized) {
        return None;
    }
    let is_absolute = Path::new(&normalized).is_absolute() || normalized.starts_with('/');
    if is_absolute {
        for workspace in workspaces {
            let root = normalize_rel_path(&workspace.work_path)
                .trim_end_matches('/')
                .to_string();
            if root.is_empty() {
                continue;
            }
            if normalized == root {
                return None;
            }
            if normalized.starts_with(&format!("{root}/")) {
                let rel = normalized[root.len() + 1..].to_string();
                files::normalize_relative(&rel).ok()?;
                return Some((workspace, rel));
            }
        }
        return None;
    }
    let rel = files::normalize_relative(&normalized).ok()?;
    workspaces.first().map(|workspace| (workspace, rel))
}

pub fn capture_file_before_edit(
    db: &Connection,
    message_id: &str,
    session_id: &str,
    workspace: &Workspace,
    rel_file_path: &str,
) -> Result<Option<CapturedSnapshot>, String> {
    let rel_file_path = files::normalize_relative(&normalize_rel_path(rel_file_path))?;
    if rel_file_path.is_empty() {
        return Ok(None);
    }
    if snapshot_db::snapshot_exists(db, message_id, &workspace.id, &rel_file_path)? {
        return Ok(None);
    }

    let abs_path = files::resolve_existing_or_new(&workspace.work_path, &rel_file_path)?;
    let raw_content = if abs_path.exists() {
        fs::read(&abs_path).map_err(|error| error.to_string())?
    } else {
        Vec::new()
    };

    if raw_content.len() > MAX_FILE_BYTES || is_binary(&raw_content) {
        return Ok(None);
    }

    let content = String::from_utf8_lossy(&raw_content).into_owned();
    let hash = sha256_hex(&raw_content);
    let compressed = if raw_content.is_empty() {
        Vec::new()
    } else {
        zstd::bulk::compress(&raw_content, 3).map_err(|error| error.to_string())?
    };

    snapshot_db::insert_turn_snapshot(
        db,
        &TurnFileSnapshot {
            id: Uuid::new_v4().to_string(),
            message_id: message_id.into(),
            session_id: session_id.into(),
            workspace_id: workspace.id.clone(),
            file_path: rel_file_path.clone(),
            content: compressed,
            content_hash: hash,
            snapshot_at: chrono::Utc::now().timestamp(),
        },
    )?;
    snapshot_db::ensure_review_baseline(db, &workspace.id, &rel_file_path, &content)?;

    Ok(Some(CapturedSnapshot {
        workspace_id: workspace.id.clone(),
        file_path: rel_file_path,
        content,
    }))
}

fn normalize_rel_path(path: &str) -> String {
    path.replace('\\', "/")
        .trim()
        .trim_start_matches("./")
        .to_string()
}

fn path_has_parent_dir(path: &str) -> bool {
    Path::new(path)
        .components()
        .any(|component| matches!(component, Component::ParentDir))
}

fn is_binary(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }
    if bytes.contains(&0) {
        return true;
    }
    std::str::from_utf8(bytes).is_err()
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::{
        capture_file_before_edit, extract_file_path, is_binary, is_file_mutation_tool,
        resolve_workspace_file, CapturedSnapshot,
    };
    use crate::{models::Workspace, snapshot_db};
    use rusqlite::Connection;
    use std::fs;
    use std::path::Path;

    fn test_db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        crate::database::migrate(&db).unwrap();
        db
    }

    fn test_workspace(work_path: &Path) -> Workspace {
        Workspace {
            id: "ws".into(),
            project_id: "proj".into(),
            session_id: Some("sess".into()),
            source_workspace_id: None,
            name: "test".into(),
            kind: "folder",
            source_path: work_path.to_string_lossy().into(),
            work_path: work_path.to_string_lossy().into(),
            branch: None,
            isolation_mode: "direct",
            created_at: "now".into(),
        }
    }

    fn snapshot_paths(db: &Connection, message_id: &str) -> Vec<String> {
        let mut statement = db
            .prepare(
                "SELECT file_path FROM turn_file_snapshots
                 WHERE message_id=?1 AND file_path != ''
                 ORDER BY file_path",
            )
            .unwrap();
        statement
            .query_map([message_id], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    }

    #[test]
    fn recognizes_file_mutation_tools() {
        assert!(is_file_mutation_tool("Edit", None));
        assert!(is_file_mutation_tool("Write", None));
        assert!(is_file_mutation_tool("Create", None));
        assert!(is_file_mutation_tool("EditFile", None));
        assert!(is_file_mutation_tool("WriteFile", None));
        assert!(is_file_mutation_tool("MultiEdit", None));
        assert!(is_file_mutation_tool("NotebookEdit", None));
        assert!(is_file_mutation_tool("apply_patch", None));
        assert!(is_file_mutation_tool("str_replace_editor", None));
        assert!(is_file_mutation_tool(
            "str_replace_editor",
            Some(&serde_json::json!({"command": "create", "file_path": "a.ts"})),
        ));
        assert!(!is_file_mutation_tool("Read", None));
        assert!(!is_file_mutation_tool("Bash", None));
        assert!(!is_file_mutation_tool(
            "Bash",
            Some(&serde_json::json!({"command": "write"})),
        ));
    }

    #[test]
    fn extracts_relative_file_path() {
        assert_eq!(
            extract_file_path(Some(&serde_json::json!({"file_path": "src/a.ts"}))).as_deref(),
            Some("src/a.ts")
        );
        assert_eq!(
            extract_file_path(Some(&serde_json::json!({"path": "src\\b.ts"}))).as_deref(),
            Some("src/b.ts")
        );
        assert_eq!(
            extract_file_path(Some(&serde_json::json!({"query": "x"}))),
            None
        );
    }

    #[test]
    fn t_s01_snapshots_only_the_edited_file() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("src")).unwrap();
        fs::write(temp.path().join("src/a.ts"), "v0").unwrap();
        fs::write(temp.path().join("src/b.ts"), "u0").unwrap();
        let db = test_db();
        let workspace = test_workspace(temp.path());
        let captured = capture_file_before_edit(&db, "msg1", "sess", &workspace, "src/a.ts")
            .unwrap()
            .unwrap();

        assert_eq!(
            captured,
            CapturedSnapshot {
                workspace_id: "ws".into(),
                file_path: "src/a.ts".into(),
                content: "v0".into(),
            }
        );
        assert_eq!(snapshot_paths(&db, "msg1"), vec!["src/a.ts".to_string()]);
        assert_eq!(
            snapshot_db::get_snapshot_content(&db, "msg1", "ws", "src/a.ts").unwrap(),
            "v0"
        );
        let snapshot_at: i64 = db
            .query_row(
                "SELECT snapshot_at FROM turn_file_snapshots
                 WHERE message_id='msg1' AND file_path='src/a.ts'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(snapshot_at <= chrono::Utc::now().timestamp());
    }

    #[test]
    fn t_s02_same_file_is_snapshotted_only_once() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("src")).unwrap();
        fs::write(temp.path().join("src/a.ts"), "v0").unwrap();
        let db = test_db();
        let workspace = test_workspace(temp.path());
        assert!(
            capture_file_before_edit(&db, "msg1", "sess", &workspace, "src/a.ts")
                .unwrap()
                .is_some()
        );
        fs::write(temp.path().join("src/a.ts"), "v1").unwrap();
        assert!(
            capture_file_before_edit(&db, "msg1", "sess", &workspace, "src/a.ts")
                .unwrap()
                .is_none()
        );
        assert_eq!(snapshot_paths(&db, "msg1"), vec!["src/a.ts".to_string()]);
        assert_eq!(
            snapshot_db::get_snapshot_content(&db, "msg1", "ws", "src/a.ts").unwrap(),
            "v0"
        );
        let baselines = snapshot_db::load_review_baselines(&db).unwrap();
        assert_eq!(baselines.len(), 1);
        assert_eq!(baselines[0].content, "v0");
    }

    #[test]
    fn later_turn_does_not_replace_review_baseline() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("src")).unwrap();
        fs::write(temp.path().join("src/a.ts"), "v0").unwrap();
        let db = test_db();
        let workspace = test_workspace(temp.path());
        capture_file_before_edit(&db, "msg1", "sess", &workspace, "src/a.ts").unwrap();
        fs::write(temp.path().join("src/a.ts"), "v1").unwrap();
        capture_file_before_edit(&db, "msg2", "sess", &workspace, "src/a.ts").unwrap();
        let baselines = snapshot_db::load_review_baselines(&db).unwrap();
        assert_eq!(baselines.len(), 1);
        assert_eq!(baselines[0].content, "v0");
    }

    #[test]
    fn t_s03_different_files_are_snapshotted_independently() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("src")).unwrap();
        fs::write(temp.path().join("src/a.ts"), "v0").unwrap();
        fs::write(temp.path().join("src/b.ts"), "u0").unwrap();
        let db = test_db();
        let workspace = test_workspace(temp.path());
        capture_file_before_edit(&db, "msg1", "sess", &workspace, "src/a.ts").unwrap();
        capture_file_before_edit(&db, "msg1", "sess", &workspace, "src/b.ts").unwrap();
        assert_eq!(
            snapshot_paths(&db, "msg1"),
            vec!["src/a.ts".to_string(), "src/b.ts".to_string()]
        );
    }

    #[test]
    fn t_s04_missing_file_uses_empty_baseline() {
        let temp = tempfile::tempdir().unwrap();
        let db = test_db();
        let workspace = test_workspace(temp.path());
        let captured = capture_file_before_edit(&db, "msg1", "sess", &workspace, "src/new.ts")
            .unwrap()
            .unwrap();
        assert_eq!(captured.content, "");
        assert_eq!(
            snapshot_db::get_snapshot_content(&db, "msg1", "ws", "src/new.ts").unwrap(),
            ""
        );
    }

    #[test]
    fn t_s06_skips_files_over_two_megabytes() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("huge.sql"), vec![b'x'; 3 * 1024 * 1024]).unwrap();
        let db = test_db();
        let workspace = test_workspace(temp.path());
        assert!(
            capture_file_before_edit(&db, "msg1", "sess", &workspace, "huge.sql")
                .unwrap()
                .is_none()
        );
        assert!(snapshot_paths(&db, "msg1").is_empty());
    }

    #[test]
    fn t_s07_skips_binary_files() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("asset.png"),
            [0x89, 0x50, 0x4E, 0x47, 0, 0x0A, 0x1A, 0x0A],
        )
        .unwrap();
        let db = test_db();
        let workspace = test_workspace(temp.path());
        assert!(is_binary(&[0x89, 0x50, 0, 0x47]));
        assert!(
            capture_file_before_edit(&db, "msg1", "sess", &workspace, "asset.png")
                .unwrap()
                .is_none()
        );
        assert!(snapshot_paths(&db, "msg1").is_empty());
    }

    #[test]
    fn t_e06_roundtrips_unicode_after_zstd() {
        let temp = tempfile::tempdir().unwrap();
        let original = "hello 你好 こんにちは ←→ ≥ ≤ ★";
        fs::write(temp.path().join("mixed.txt"), original).unwrap();
        let db = test_db();
        capture_file_before_edit(
            &db,
            "msg1",
            "sess",
            &test_workspace(temp.path()),
            "mixed.txt",
        )
        .unwrap();
        let stored: Vec<u8> = db
            .query_row(
                "SELECT content FROM turn_file_snapshots
                 WHERE message_id='msg1' AND file_path='mixed.txt'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_ne!(stored, original.as_bytes());
        assert_eq!(
            snapshot_db::get_snapshot_content(&db, "msg1", "ws", "mixed.txt").unwrap(),
            original
        );
    }

    #[test]
    fn resolves_absolute_paths_against_workspace() {
        let workspace = test_workspace(Path::new("/repo"));
        let workspaces = [workspace];
        let resolved = resolve_workspace_file(&workspaces, "/repo/src/a.ts").unwrap();
        assert_eq!(resolved.1, "src/a.ts");
    }

    #[test]
    fn rejects_path_escape_and_unmatched_absolute_paths() {
        let workspace = test_workspace(Path::new("/repo"));
        let workspaces = [workspace];
        assert!(resolve_workspace_file(&workspaces, "../../secret").is_none());
        assert!(resolve_workspace_file(&workspaces, "/etc/hosts").is_none());
        assert!(resolve_workspace_file(&workspaces, "src/../secret").is_none());
    }

    #[test]
    fn capture_rejects_path_escape() {
        let temp = tempfile::tempdir().unwrap();
        let db = test_db();
        let workspace = test_workspace(temp.path());
        assert!(
            capture_file_before_edit(&db, "msg1", "sess", &workspace, "../outside.ts").is_err()
        );
    }
}
