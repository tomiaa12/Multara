use std::{fs, path::Path};

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use tauri::{AppHandle, Manager};

use crate::database::AppState;

const ROLLBACK_NOTE: &str = "回退说明：用户刚刚确认把工作区文件恢复到这条提问之前。那些改动是有意丢弃的。不要从 agent-transcripts、旧对话、git 历史或其他日志里把它们找回来再套用。只根据当前工作区文件和本轮任务重新实现。";

pub const CONTINUE_ASSIGNMENT: &str =
    "上一次运行中断了。从停下的地方继续完成原来的任务，不要重头再做一遍。";

const HISTORY_CHAR_LIMIT: usize = 60_000;
const ENTRY_CHAR_LIMIT: usize = 8_000;

pub fn resumes_via_cli(runtime_id: &str) -> bool {
    matches!(runtime_id, "cursor" | "claude" | "copilot" | "opencode")
}

pub fn id_from_line(line: &str) -> Option<String> {
    let value: Value = serde_json::from_str(line).ok()?;
    ["session_id", "sessionId", "sessionID"]
        .iter()
        .find_map(|key| {
            value
                .get(*key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(str::to_string)
        })
}

pub fn apply_resume(
    mut args: Vec<String>,
    runtime_id: &str,
    conversation_id: Option<&str>,
) -> Vec<String> {
    let Some(id) = conversation_id.map(str::trim).filter(|id| !id.is_empty()) else {
        return args;
    };
    match runtime_id {
        "opencode" => {
            args.push("--session".into());
            args.push(id.into());
        }
        "cursor" | "claude" | "copilot" => {
            args.push("--resume".into());
            args.push(id.into());
        }
        _ => {}
    }
    args
}

pub fn build_prompt(
    brief: &str,
    history: &str,
    assignment: &str,
    resume_id: Option<&str>,
    runtime_id: &str,
) -> String {
    if resume_id.is_some() && resumes_via_cli(runtime_id) {
        return assignment.to_string();
    }
    format!("{brief}\n\n--- 以下为本轮 ---\n{history}用户任务：{assignment}")
}

pub fn load_id(
    db: &Connection,
    session_id: &str,
    agent_id: &str,
    runtime_id: &str,
) -> Result<Option<String>, String> {
    db.query_row(
        "SELECT conversation_id FROM session_runtime_chats
         WHERE session_id=?1 AND agent_id=?2 AND runtime_id=?3",
        params![session_id, agent_id, runtime_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(|error| error.to_string())
}

pub fn save_id(
    db: &Connection,
    session_id: &str,
    agent_id: &str,
    runtime_id: &str,
    conversation_id: &str,
) -> Result<(), String> {
    if session_id.starts_with("automation-run:") || session_id == "commit-message" {
        return Ok(());
    }
    let id = conversation_id.trim();
    if id.is_empty() {
        return Ok(());
    }
    db.execute(
        "INSERT INTO session_runtime_chats
         (session_id, agent_id, runtime_id, conversation_id, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(session_id, agent_id, runtime_id) DO UPDATE SET
           conversation_id=excluded.conversation_id,
           updated_at=excluded.updated_at",
        params![
            session_id,
            agent_id,
            runtime_id,
            id,
            chrono::Utc::now().to_rfc3339()
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn clear_session(db: &Connection, session_id: &str) -> Result<(), String> {
    db.execute(
        "DELETE FROM session_runtime_chats WHERE session_id=?1",
        [session_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn conversation_ids(db: &Connection, session_id: &str) -> Result<Vec<String>, String> {
    let mut statement = db
        .prepare("SELECT conversation_id FROM session_runtime_chats WHERE session_id=?1")
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([session_id], |row| row.get(0))
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

pub fn mark_rollback_reask(db: &Connection, session_id: &str) -> Result<(), String> {
    db.execute(
        "INSERT INTO app_settings (key, value) VALUES (?1, '1')
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        [rollback_key(session_id)],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn take_rollback_note(db: &Connection, session_id: &str) -> Result<Option<String>, String> {
    let key = rollback_key(session_id);
    let found: Option<String> = db
        .query_row(
            "SELECT value FROM app_settings WHERE key=?1",
            [&key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if found.is_none() {
        return Ok(None);
    }
    db.execute("DELETE FROM app_settings WHERE key=?1", [&key])
        .map_err(|error| error.to_string())?;
    Ok(Some(ROLLBACK_NOTE.to_string()))
}

pub fn cursor_project_slug(path: &str) -> String {
    let trimmed = path.trim().trim_end_matches(['/', '\\']);
    let without_root = trimmed.trim_start_matches(['/', '\\']);
    without_root
        .chars()
        .filter(|c| *c != '.')
        .map(|c| if c == '/' || c == '\\' { '-' } else { c })
        .collect()
}

pub fn clear_runtime_transcripts(
    home: &Path,
    work_paths: &[String],
    source_paths: &[String],
    conversation_ids: &[String],
) -> usize {
    let mut removed = 0usize;
    for path in work_paths {
        if !is_multara_worktree(path) {
            continue;
        }
        let transcripts = cursor_project_dir(home, path).join("agent-transcripts");
        if transcripts.is_dir() && fs::remove_dir_all(&transcripts).is_ok() {
            removed += 1;
        }
    }
    let ids: Vec<&str> = conversation_ids
        .iter()
        .map(|id| id.trim())
        .filter(|id| !id.is_empty())
        .collect();
    if ids.is_empty() {
        return removed;
    }
    for path in source_paths.iter().chain(work_paths.iter()) {
        if is_multara_worktree(path) {
            continue;
        }
        let transcripts = cursor_project_dir(home, path).join("agent-transcripts");
        for id in &ids {
            let dir = transcripts.join(id);
            if dir.is_dir() && fs::remove_dir_all(&dir).is_ok() {
                removed += 1;
            }
        }
    }
    removed
}

fn rollback_key(session_id: &str) -> String {
    format!("reask_rollback:{session_id}")
}

fn is_multara_worktree(path: &str) -> bool {
    let mut parts = Path::new(path).components();
    while let Some(part) = parts.next() {
        if part.as_os_str() == ".multara"
            && parts
                .clone()
                .next()
                .is_some_and(|next| next.as_os_str() == "workspaces")
        {
            return true;
        }
    }
    false
}

fn cursor_project_dir(home: &Path, path: &str) -> std::path::PathBuf {
    home.join(".cursor")
        .join("projects")
        .join(cursor_project_slug(path))
}

pub fn persist_from_line(
    app: &AppHandle,
    session_id: &str,
    agent_id: &str,
    runtime_id: &str,
    line: &str,
) {
    let Some(id) = id_from_line(line) else {
        return;
    };
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let Ok(db) = state.lock_db() else {
        return;
    };
    let _ = save_id(&db, session_id, agent_id, runtime_id, &id);
}

pub fn prepare(
    app: &AppHandle,
    session_id: &str,
    agent_id: &str,
    runtime_id: &str,
    provider: &str,
) -> Result<(Option<String>, String), String> {
    let state = app.state::<AppState>();
    let db = state.lock_db()?;
    let mut resume_id = load_id(&db, session_id, agent_id, runtime_id)?;
    let rollback_note = take_rollback_note(&db, session_id)?;
    if rollback_note.is_some() {
        resume_id = None;
    }
    let mut history = if resume_id.is_some() && resumes_via_cli(provider) {
        String::new()
    } else {
        history_block(&db, session_id)?
    };
    if let Some(note) = rollback_note {
        if !history.is_empty() && !history.ends_with('\n') {
            history.push('\n');
        }
        history.push_str(&note);
        history.push_str("\n\n");
    }
    Ok((resume_id, history))
}

pub fn history_block(db: &Connection, session_id: &str) -> Result<String, String> {
    let mut statement = db
        .prepare(
            "SELECT role, content FROM messages
             WHERE session_id=?1
               AND role IN ('user', 'assistant')
               AND COALESCE(message_kind, 'message') = 'message'
               AND COALESCE(audience_type, 'user') = 'user'
               AND id != COALESCE((
                 SELECT id FROM messages
                 WHERE session_id=?1 AND role='user'
                 ORDER BY created_at DESC, rowid DESC
                 LIMIT 1
               ), '')
             ORDER BY created_at, rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([session_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    let entries = rows
        .into_iter()
        .filter_map(|(role, content)| {
            let text = clip(&content, ENTRY_CHAR_LIMIT);
            if text.is_empty() {
                return None;
            }
            let label = if role == "assistant" {
                "助手"
            } else {
                "用户"
            };
            Some(format!("{label}：\n{text}"))
        })
        .collect::<Vec<_>>();
    let compact: Option<String> = db
        .query_row(
            "SELECT compact_summary FROM chat_sessions WHERE id=?1",
            [session_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .flatten()
        .filter(|value| !value.trim().is_empty());
    if entries.is_empty() {
        return Ok(compact
            .map(|summary| format!("对话摘要：\n{}\n\n", summary.trim()))
            .unwrap_or_default());
    }
    let mut selected = Vec::new();
    let mut used = 0usize;
    for entry in entries.into_iter().rev() {
        let size = entry.chars().count();
        if used + size > HISTORY_CHAR_LIMIT && !selected.is_empty() {
            break;
        }
        used += size;
        selected.push(entry);
    }
    selected.reverse();
    let history = format!("对话历史：\n{}\n\n", selected.join("\n\n"));
    Ok(match compact {
        Some(summary) => format!("对话摘要：\n{}\n\n{history}", summary.trim()),
        None => history,
    })
}

fn clip(value: &str, max_chars: usize) -> String {
    let trimmed = value.trim();
    let chars = trimmed.chars().collect::<Vec<_>>();
    if chars.len() <= max_chars {
        return trimmed.to_string();
    }
    format!("{}…", chars[..max_chars].iter().collect::<String>())
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::{
        apply_resume, build_prompt, clear_runtime_transcripts, clear_session, cursor_project_slug,
        history_block, id_from_line, load_id, mark_rollback_reask, save_id, take_rollback_note,
    };

    fn db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        crate::database::migrate(&db).unwrap();
        db.execute(
            "INSERT INTO chat_sessions (id, title, target_type, target_id, created_at, updated_at)
             VALUES ('sess', '测试', 'agent', 'agent-1', 't0', 't0')",
            [],
        )
        .unwrap();
        db
    }

    #[test]
    fn reads_cursor_session_id_from_system_init() {
        assert_eq!(
            id_from_line(
                r#"{"type":"system","subtype":"init","session_id":"chat-abc","cwd":"/tmp"}"#
            )
            .as_deref(),
            Some("chat-abc")
        );
        assert_eq!(id_from_line(r#"{"type":"assistant","text":"hi"}"#), None);
    }

    #[test]
    fn resume_flag_only_for_cursor_and_claude() {
        assert_eq!(
            apply_resume(vec!["-p".into(), "--yolo".into()], "cursor", Some("chat-1")),
            ["-p", "--yolo", "--resume", "chat-1"]
        );
        assert_eq!(
            apply_resume(vec!["-p".into()], "claude", Some("sess-9")),
            ["-p", "--resume", "sess-9"]
        );
        assert_eq!(
            apply_resume(vec!["exec".into()], "codex", Some("chat-1")),
            ["exec"]
        );
        assert_eq!(
            apply_resume(vec!["run".into()], "opencode", Some("ses-1")),
            ["run", "--session", "ses-1"]
        );
        assert_eq!(
            apply_resume(vec!["-p".into()], "copilot", Some("cp-1")),
            ["-p", "--resume", "cp-1"]
        );
        assert_eq!(apply_resume(vec!["-p".into()], "cursor", None), ["-p"]);
    }

    #[test]
    fn resume_sends_only_the_new_user_turn() {
        let prompt = build_prompt(
            "你是助手\n\n工作区：/repo\n\n无 Skills",
            "对话历史：\n用户：上一轮\n\n",
            "帮我修复",
            Some("chat-1"),
            "cursor",
        );
        assert_eq!(prompt, "帮我修复");
        assert!(!prompt.contains("上一轮"));
    }

    #[test]
    fn first_turn_keeps_wrapper_and_history() {
        let prompt = build_prompt(
            "你是助手\n\n工作区：/repo\n\n无 Skills",
            "对话历史：\n用户：为什么扫不到\n\n助手：只扫 Program Files\n\n",
            "帮我修复",
            None,
            "cursor",
        );
        assert!(prompt.contains("你是助手"));
        assert!(prompt.contains("为什么扫不到"));
        assert!(prompt.contains("只扫 Program Files"));
        assert!(prompt.contains("用户任务：帮我修复"));
    }

    #[test]
    fn history_skips_the_latest_user_message() {
        let db = db();
        db.execute(
            "INSERT INTO messages (id, session_id, role, content, created_at)
             VALUES ('u1', 'sess', 'user', '为什么扫不到商店应用', 't1'),
                    ('a1', 'sess', 'assistant', 'WindowsApps 被跳过了', 't2'),
                    ('u2', 'sess', 'user', '帮我修复', 't3')",
            [],
        )
        .unwrap();
        let history = history_block(&db, "sess").unwrap();
        assert!(history.contains("为什么扫不到商店应用"));
        assert!(history.contains("WindowsApps 被跳过了"));
        assert!(!history.contains("帮我修复"));
    }

    #[test]
    fn stores_and_clears_runtime_chat_ids() {
        let db = db();
        save_id(&db, "sess", "agent-1", "cursor", "chat-1").unwrap();
        assert_eq!(
            load_id(&db, "sess", "agent-1", "cursor")
                .unwrap()
                .as_deref(),
            Some("chat-1")
        );
        clear_session(&db, "sess").unwrap();
        assert_eq!(load_id(&db, "sess", "agent-1", "cursor").unwrap(), None);
    }

    #[test]
    fn rollback_note_is_one_shot() {
        let db = db();
        assert_eq!(take_rollback_note(&db, "sess").unwrap(), None);
        mark_rollback_reask(&db, "sess").unwrap();
        let note = take_rollback_note(&db, "sess").unwrap().unwrap();
        assert!(note.contains("有意丢弃"));
        assert!(note.contains("agent-transcripts"));
        assert_eq!(take_rollback_note(&db, "sess").unwrap(), None);
    }

    #[test]
    fn cursor_slug_matches_worktree_project_dir() {
        assert_eq!(
            cursor_project_slug(
                "/Users/tomiaa/.multara/workspaces/a5362779-9a24-4e0c-92fc-43e3536ca9e1/c9174a13-bed4-444e-adc7-f19ab0dbeb1e"
            ),
            "Users-tomiaa-multara-workspaces-a5362779-9a24-4e0c-92fc-43e3536ca9e1-c9174a13-bed4-444e-adc7-f19ab0dbeb1e"
        );
    }

    #[test]
    fn reask_clears_worktree_transcripts_and_only_known_source_chats() {
        let home = tempfile::tempdir().unwrap();
        let work = "/Users/tomiaa/.multara/workspaces/proj/ws";
        let source = "/Users/tomiaa/Documents/github/client";
        let work_slug = cursor_project_slug(work);
        let source_slug = cursor_project_slug(source);
        let work_transcripts = home
            .path()
            .join(".cursor/projects")
            .join(&work_slug)
            .join("agent-transcripts");
        let source_transcripts = home
            .path()
            .join(".cursor/projects")
            .join(&source_slug)
            .join("agent-transcripts");
        std::fs::create_dir_all(work_transcripts.join("old-chat")).unwrap();
        std::fs::write(work_transcripts.join("old-chat/old-chat.jsonl"), "{}").unwrap();
        std::fs::create_dir_all(
            home.path()
                .join(".cursor/projects")
                .join(&work_slug)
                .join("agent-tools"),
        )
        .unwrap();
        std::fs::create_dir_all(source_transcripts.join("chat-old")).unwrap();
        std::fs::create_dir_all(source_transcripts.join("other-chat")).unwrap();

        let removed = clear_runtime_transcripts(
            home.path(),
            &[work.to_string()],
            &[source.to_string()],
            &["chat-old".to_string()],
        );

        assert_eq!(removed, 2);
        assert!(!work_transcripts.exists());
        assert!(home
            .path()
            .join(".cursor/projects")
            .join(&work_slug)
            .join("agent-tools")
            .exists());
        assert!(!source_transcripts.join("chat-old").exists());
        assert!(source_transcripts.join("other-chat").exists());
    }
}
