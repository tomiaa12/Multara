//! 一次聊天会话的诊断包：消息和时间线、运行记录、以及进程日志里和这些运行有关的行。
//! 工具输出会截断，方便把文件发出去排查卡住、报错和停在思考中的情况。

use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use rusqlite::OptionalExtension;
use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::{commands::list_chat_messages_blocking, database::AppState, models::ChatMessage};

const CONTENT_LIMIT: usize = 16_000;
const DETAIL_LIMIT: usize = 8_000;
const SHORT_LIMIT: usize = 4_000;
const LOG_LINE_LIMIT: usize = 2_000;
const EXPORT_BYTE_LIMIT: usize = 32 * 1024 * 1024;
const NEEDLE_MIN_CHARS: usize = 8;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionDiagnostic {
    kind: &'static str,
    schema_version: u32,
    exported_at: String,
    app_version: &'static str,
    note: &'static str,
    clipped: bool,
    session: Option<SessionRow>,
    signals: Signals,
    runs: Vec<RunRow>,
    messages: Vec<ChatMessage>,
    process_log: ProcessLog,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionRow {
    id: String,
    title: String,
    project_id: Option<String>,
    target_type: String,
    target_id: String,
    default_mode: String,
    created_at: String,
    updated_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RunRow {
    id: String,
    agent_id: String,
    runtime_id: String,
    parent_run_id: Option<String>,
    status: String,
    mode: Option<String>,
    effort: Option<String>,
    model_override: Option<String>,
    turn_id: Option<String>,
    created_at: String,
    started_at: Option<String>,
    completed_at: Option<String>,
    assignment: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Signals {
    open_runs: Vec<OpenRun>,
    last_message_at: Option<String>,
    last_event_type: Option<String>,
    last_event_at: Option<String>,
    failure_reasons: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OpenRun {
    id: String,
    status: String,
    runtime_id: String,
    created_at: String,
    started_at: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProcessLog {
    matched_lines: Vec<String>,
    dropped: usize,
    missing: Vec<String>,
}

#[tauri::command]
pub async fn export_session_diagnostics(
    session_id: String,
    app: AppHandle,
) -> Result<serde_json::Value, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    let diagnostic = crate::commands::off_main(app, move |state| {
        build_diagnostic(state, &data_dir, &session_id)
    })
    .await?;
    serde_json::to_value(diagnostic).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn save_diagnostic_file(path: String, contents: String) -> Result<(), String> {
    let path = PathBuf::from(&path);
    if !path.is_absolute() {
        return Err(tr!("诊断文件必须保存到绝对路径"));
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    if !extension.eq_ignore_ascii_case("json") {
        return Err(tr!("诊断文件必须是 JSON"));
    }
    if contents.len() > EXPORT_BYTE_LIMIT {
        return Err(tr!("诊断文件过大"));
    }
    if !contents.contains("\"kind\": \"multara-chat-diagnostics\"")
        && !contents.contains("\"kind\":\"multara-chat-diagnostics\"")
    {
        return Err(tr!("诊断文件内容不正确"));
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
    }
    fs::write(path, contents).map_err(|error| error.to_string())
}

fn build_diagnostic(
    state: &AppState,
    data_dir: &Path,
    session_id: &str,
) -> Result<SessionDiagnostic, String> {
    let mut messages = list_chat_messages_blocking(state, session_id.to_string())?;
    let (session, mut runs) = {
        let db = state.lock_db()?;
        let session = load_session(&db, session_id)?;
        if session.is_none() && messages.is_empty() {
            return Err(tr!("会话不存在"));
        }
        let runs = load_runs(&db, session_id)?;
        (session, runs)
    };
    let mut clipped = false;
    for message in &mut messages {
        clip_message(message, &mut clipped);
    }
    for run in &mut runs {
        run.assignment = clip_chars(
            std::mem::take(&mut run.assignment),
            SHORT_LIMIT,
            &mut clipped,
        );
    }
    let needles = needles_for(session_id, &runs);
    let process_log = read_process_log(data_dir, &needles);
    let signals = signals_from(&runs, &messages);
    Ok(SessionDiagnostic {
        kind: "multara-chat-diagnostics",
        schema_version: 1,
        exported_at: chrono::Utc::now().to_rfc3339(),
        app_version: env!("CARGO_PKG_VERSION"),
        note: "One chat. Messages include timelines. Tool outputs longer than the limit are clipped. processLog keeps lines that mention this session or its runs, plus WAIT, PANIC, REAP, and SLOW lines.",
        clipped,
        session,
        signals,
        runs,
        messages,
        process_log,
    })
}

fn load_session(db: &rusqlite::Connection, session_id: &str) -> Result<Option<SessionRow>, String> {
    db.query_row(
        "SELECT id, title, project_id, target_type, target_id, default_mode, created_at, updated_at
         FROM chat_sessions WHERE id=?1",
        [session_id],
        |row| {
            Ok(SessionRow {
                id: row.get(0)?,
                title: row.get(1)?,
                project_id: row.get(2)?,
                target_type: row.get(3)?,
                target_id: row.get(4)?,
                default_mode: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        },
    )
    .optional()
    .map_err(|error| error.to_string())
}

fn load_runs(db: &rusqlite::Connection, session_id: &str) -> Result<Vec<RunRow>, String> {
    let mut statement = db
        .prepare(
            "SELECT id, agent_id, runtime_id, parent_run_id, status, mode, effort,
                    model_override, turn_id, created_at, started_at, completed_at, assignment
             FROM agent_runs
             WHERE session_id=?1
             ORDER BY created_at, rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([session_id], |row| {
            Ok(RunRow {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                runtime_id: row.get(2)?,
                parent_run_id: row.get(3)?,
                status: row.get(4)?,
                mode: row.get(5)?,
                effort: row.get(6)?,
                model_override: row.get(7)?,
                turn_id: row.get(8)?,
                created_at: row.get(9)?,
                started_at: row.get(10)?,
                completed_at: row.get(11)?,
                assignment: row.get(12)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

fn signals_from(runs: &[RunRow], messages: &[ChatMessage]) -> Signals {
    let open_runs = runs
        .iter()
        .filter(|run| run.status == "queued" || run.status == "running")
        .map(|run| OpenRun {
            id: run.id.clone(),
            status: run.status.clone(),
            runtime_id: run.runtime_id.clone(),
            created_at: run.created_at.clone(),
            started_at: run.started_at.clone(),
        })
        .collect();
    let mut failure_reasons = Vec::new();
    for message in messages {
        if let Some(reason) = message.failure_reason.as_deref() {
            let reason = reason.trim();
            if !reason.is_empty() && !failure_reasons.iter().any(|item: &String| item == reason) {
                failure_reasons.push(reason.to_string());
            }
        }
    }
    failure_reasons.truncate(20);
    let last_message_at = messages.last().map(|message| message.created_at.clone());
    let mut last_event_type = None;
    let mut last_event_at = None;
    for message in messages {
        for item in &message.timeline {
            last_event_type = Some(item.event_type.to_string());
            last_event_at = Some(item.created_at.clone());
        }
    }
    Signals {
        open_runs,
        last_message_at,
        last_event_type,
        last_event_at,
        failure_reasons,
    }
}

fn needles_for(session_id: &str, runs: &[RunRow]) -> Vec<String> {
    let mut needles = Vec::new();
    if session_id.chars().count() >= NEEDLE_MIN_CHARS {
        needles.push(session_id.to_string());
    }
    for run in runs {
        if run.id.chars().count() >= NEEDLE_MIN_CHARS {
            needles.push(run.id.clone());
        }
    }
    needles
}

fn clip_message(message: &mut ChatMessage, clipped: &mut bool) {
    message.content = clip_chars(std::mem::take(&mut message.content), CONTENT_LIMIT, clipped);
    if let Some(reason) = message.failure_reason.take() {
        message.failure_reason = Some(clip_chars(reason, SHORT_LIMIT, clipped));
    }
    if let Some(meta) = message.source_meta.as_mut() {
        clip_json(meta, DETAIL_LIMIT, clipped);
    }
    for item in &mut message.timeline {
        if let Some(content) = item.content.take() {
            item.content = Some(clip_chars(content, DETAIL_LIMIT, clipped));
        }
        if let Some(output) = item.output.take() {
            item.output = Some(clip_chars(output, DETAIL_LIMIT, clipped));
        }
        if let Some(input) = item.input.as_mut() {
            clip_json(input, DETAIL_LIMIT, clipped);
        }
        if let Some(output) = item.output_json.as_mut() {
            clip_json(output, DETAIL_LIMIT, clipped);
        }
    }
}

fn clip_json(value: &mut Value, limit: usize, clipped: &mut bool) {
    let text = value.to_string();
    if text.chars().count() <= limit {
        return;
    }
    *clipped = true;
    *value = json!({
        "clipped": true,
        "chars": text.chars().count(),
    });
}

pub(crate) fn clip_chars(text: String, limit: usize, clipped: &mut bool) -> String {
    let count = text.chars().count();
    if count <= limit {
        return text;
    }
    *clipped = true;
    let head: String = text.chars().take(limit).collect();
    format!("{head}\n…[clipped {} chars]", count - limit)
}

pub(crate) fn select_log_lines<'a>(
    lines: impl Iterator<Item = &'a str>,
    needles: &[String],
    limit: usize,
) -> (Vec<String>, usize) {
    let mut matched = Vec::new();
    for line in lines {
        if log_line_matches(line, needles) {
            matched.push(line.to_string());
        }
    }
    let total = matched.len();
    if limit == 0 || total <= limit {
        return (matched, 0);
    }
    let tail = matched.split_off(total - limit);
    (tail, total - limit)
}

fn log_line_matches(line: &str, needles: &[String]) -> bool {
    let message = line.split_once("] ").map(|(_, rest)| rest).unwrap_or(line);
    if message.starts_with("PANIC ")
        || message.starts_with("WAIT ")
        || message.starts_with("POISONED ")
        || message.starts_with("REAP ")
        || message.contains(" SLOW")
    {
        return true;
    }
    needles
        .iter()
        .any(|needle| !needle.is_empty() && line.contains(needle))
}

fn read_process_log(data_dir: &Path, needles: &[String]) -> ProcessLog {
    let current = data_dir.join("multara.log");
    let previous = current.with_extension("log.old");
    let mut lines = Vec::new();
    let mut missing = Vec::new();
    for path in [&previous, &current] {
        match fs::read(path) {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                lines.extend(text.lines().map(str::to_string));
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => missing.push(format!("{}: {error}", path.display())),
        }
    }
    let (matched_lines, dropped) =
        select_log_lines(lines.iter().map(String::as_str), needles, LOG_LINE_LIMIT);
    ProcessLog {
        matched_lines,
        dropped,
        missing,
    }
}

#[cfg(test)]
mod tests {
    use super::{clip_chars, log_line_matches, select_log_lines};

    #[test]
    fn clip_marks_the_omitted_tail() {
        let mut clipped = false;
        let text = clip_chars("abcdef".into(), 3, &mut clipped);
        assert!(clipped);
        assert_eq!(text, "abc\n…[clipped 3 chars]");
    }

    #[test]
    fn log_selection_keeps_run_lines_and_stalls_from_the_tail() {
        let needles = vec!["run-12345678".to_string()];
        let lines = [
            "2026-01-01 [1] HEARTBEAT",
            "2026-01-01 [1] RUN start run-12345678",
            "2026-01-01 [1] WAIT db 2.50s at commands.rs:10",
            "2026-01-01 [1] LEAVE finish_run 0.01s",
            "2026-01-01 [1] LEAVE list_chat_messages 3.20s SLOW",
            "2026-01-01 [1] RUN end other-run completed",
        ];
        let (kept, dropped) = select_log_lines(lines.into_iter(), &needles, 2);
        assert_eq!(dropped, 1);
        assert_eq!(kept.len(), 2);
        assert!(kept[0].contains("WAIT"));
        assert!(kept[1].contains("SLOW"));
        assert!(log_line_matches(lines[1], &needles));
        assert!(!log_line_matches(lines[0], &needles));
    }
}
