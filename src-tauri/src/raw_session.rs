//! 会话原始数据查看：记录每次运行实际执行的命令行与完整 prompt，并按 run 懒加载回看。
//! 列表接口只返回元信息；prompt、消息和 run_events 走单独的分页接口，避免一次 IPC 传输过大。

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::Value;
use tauri::AppHandle;

pub(crate) const PROMPT_LIMIT_CHARS: usize = 256 * 1024;
const EVENT_PAGE_DEFAULT: u32 = 200;
const EVENT_PAGE_MAX: u32 = 500;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RawSessionData {
    kind: &'static str,
    schema_version: u32,
    generated_at: String,
    session: Option<RawSession>,
    runs: Vec<RawRunSummary>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RawSession {
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
struct RawRunSummary {
    id: String,
    agent_id: String,
    runtime_id: String,
    parent_run_id: Option<String>,
    status: String,
    mode: Option<String>,
    effort: Option<String>,
    model_override: Option<String>,
    usage_model: Option<String>,
    turn_id: Option<String>,
    created_at: String,
    started_at: Option<String>,
    completed_at: Option<String>,
    assignment: String,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    cache_read_tokens: Option<i64>,
    cache_write_tokens: Option<i64>,
    cost_usd: Option<f64>,
    event_count: i64,
    invocation: Option<RawInvocationMeta>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RawInvocationMeta {
    program: String,
    args: Vec<String>,
    cwd: String,
    delivery: String,
    resume_id: Option<String>,
    prompt_chars: i64,
    prompt_truncated: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RawRunDetail {
    run: RawRunSummary,
    invocation: Option<RawInvocationFull>,
    user_message: Option<RawMessage>,
    assistant_messages: Vec<RawMessage>,
    events: Vec<RawEvent>,
    event_count: i64,
    next_after_seq: Option<i64>,
    has_more: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RawInvocationFull {
    program: String,
    args: Vec<String>,
    cwd: String,
    delivery: String,
    resume_id: Option<String>,
    prompt: Option<String>,
    prompt_chars: i64,
    prompt_truncated: bool,
    created_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RawMessage {
    id: String,
    role: String,
    content: String,
    created_at: String,
    failure_reason: Option<String>,
    elapsed_ms: Option<i64>,
    message_kind: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RawEvent {
    seq: i64,
    event_type: String,
    call_id: Option<String>,
    tool: Option<String>,
    content: Option<String>,
    input_json: Option<String>,
    output: Option<String>,
    output_json: Option<String>,
    created_at: String,
    parent_call_id: Option<String>,
    status: Option<String>,
}

/// spawn 前调用：记录实际执行的程序、参数、cwd、投递方式与完整 prompt。
/// 参数里的具体 prompt 会换成 `<prompt>`，避免和 prompt 正文重复存储。
pub(crate) fn record_invocation(
    db: &Connection,
    run_id: &str,
    session_id: &str,
    runtime_id: &str,
    program: &str,
    args: &[String],
    cwd: &str,
    delivery: &str,
    resume_id: Option<&str>,
    user_message_id: Option<&str>,
    prompt: &str,
    now: &str,
) -> Result<(), String> {
    let masked = mask_args(args, prompt);
    let args_json = serde_json::to_string(&masked).map_err(|error| error.to_string())?;
    let (prompt_text, prompt_chars, truncated) = clip_prompt(prompt);
    db.execute(
        "INSERT OR REPLACE INTO run_invocations
         (run_id, session_id, runtime_id, program, args_json, cwd, delivery, resume_id,
          user_message_id, prompt_text, prompt_chars, prompt_truncated, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            run_id,
            session_id,
            runtime_id,
            program,
            args_json,
            cwd,
            delivery,
            resume_id,
            user_message_id,
            prompt_text,
            prompt_chars as i64,
            truncated as i64,
            now
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) fn mask_args(args: &[String], prompt: &str) -> Vec<String> {
    let mut masked = Vec::with_capacity(args.len());
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == prompt && !prompt.trim().is_empty() {
            masked.push("<prompt>".to_string());
            index += 1;
            continue;
        }
        if let Some((flag, _)) = arg.split_once('=') {
            if is_secret_flag(flag) {
                masked.push(format!("{flag}=***"));
                index += 1;
                continue;
            }
        } else if is_secret_flag(arg) && index + 1 < args.len() {
            masked.push(arg.clone());
            masked.push("***".to_string());
            index += 2;
            continue;
        }
        masked.push(mask_bare_secret(arg));
        index += 1;
    }
    masked
}

fn is_secret_flag(arg: &str) -> bool {
    if !arg.starts_with('-') {
        return false;
    }
    let name = arg.trim_start_matches('-').to_ascii_lowercase();
    ["key", "token", "secret", "password", "passwd", "auth", "credential", "cookie"]
        .iter()
        .any(|needle| name.contains(needle))
}

fn mask_bare_secret(arg: &str) -> String {
    let lower = arg.to_ascii_lowercase();
    if lower.starts_with("sk-") && arg.len() > 12 {
        return "***".to_string();
    }
    if let Some(scheme) = arg.find("://") {
        let rest = &arg[scheme + 3..];
        if let Some(at) = rest.find('@') {
            let slash = rest.find('/').unwrap_or(rest.len());
            if at < slash {
                return format!("{}://***@{}", &arg[..scheme], &rest[at + 1..]);
            }
        }
    }
    arg.to_string()
}

pub(crate) fn clip_prompt(prompt: &str) -> (String, usize, bool) {
    let chars = prompt.chars().count();
    if chars <= PROMPT_LIMIT_CHARS {
        return (prompt.to_string(), chars, false);
    }
    (
        prompt.chars().take(PROMPT_LIMIT_CHARS).collect(),
        chars,
        true,
    )
}

#[tauri::command]
pub async fn list_session_raw_runs(
    session_id: String,
    app: AppHandle,
) -> Result<Value, String> {
    let data = crate::commands::off_main(app, move |state| {
        let db = state.lock_db()?;
        build_session_raw(&db, &session_id)
    })
    .await?;
    serde_json::to_value(data).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn get_run_raw_detail(
    run_id: String,
    after_seq: Option<i64>,
    limit: Option<u32>,
    app: AppHandle,
) -> Result<Value, String> {
    let data = crate::commands::off_main(app, move |state| {
        let db = state.lock_db()?;
        build_run_detail(&db, &run_id, after_seq.unwrap_or(-1), limit)
    })
    .await?;
    serde_json::to_value(data).map_err(|error| error.to_string())
}

fn build_session_raw(db: &Connection, session_id: &str) -> Result<RawSessionData, String> {
    let session = load_session(db, session_id)?;
    let runs = load_run_summaries(db, session_id)?;
    if session.is_none() && runs.is_empty() {
        return Err(tr!("会话不存在"));
    }
    Ok(RawSessionData {
        kind: "multara-session-raw",
        schema_version: 1,
        generated_at: chrono::Utc::now().to_rfc3339(),
        session,
        runs,
    })
}

fn build_run_detail(
    db: &Connection,
    run_id: &str,
    after_seq: i64,
    limit: Option<u32>,
) -> Result<RawRunDetail, String> {
    let run = load_run_summaries_for(db, run_id)?
        .into_iter()
        .next()
        .ok_or_else(|| tr!("运行不存在"))?;
    let session_id = db
        .query_row("SELECT session_id FROM agent_runs WHERE id=?1", [run_id], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|_| tr!("运行不存在"))?;
    let (invocation, user_message_id) = load_invocation(db, run_id)?;
    let user_message = load_user_message(db, &session_id, run.turn_id.as_deref(), user_message_id.as_deref())?;
    let assistant_messages = load_run_messages(db, run_id)?;
    let event_count =
        db.query_row("SELECT COUNT(*) FROM run_events WHERE run_id=?1", [run_id], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap_or(0);
    let page_limit = limit
        .unwrap_or(EVENT_PAGE_DEFAULT)
        .clamp(1, EVENT_PAGE_MAX) as usize;
    let (events, has_more, next_after_seq) =
        load_events_page(db, run_id, after_seq, page_limit)?;
    Ok(RawRunDetail {
        run,
        invocation,
        user_message,
        assistant_messages,
        events,
        event_count,
        next_after_seq,
        has_more,
    })
}

fn load_session(db: &Connection, session_id: &str) -> Result<Option<RawSession>, String> {
    db.query_row(
        "SELECT id, title, project_id, target_type, target_id, default_mode, created_at, updated_at
         FROM chat_sessions WHERE id=?1",
        [session_id],
        |row| {
            Ok(RawSession {
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

const RUN_SUMMARY_SQL: &str = "SELECT r.id, r.agent_id, r.runtime_id, r.parent_run_id, r.status,
        r.mode, r.effort, r.model_override, r.usage_model, r.turn_id, r.created_at,
        r.started_at, r.completed_at, r.assignment, r.input_tokens, r.output_tokens,
        r.cache_read_tokens, r.cache_write_tokens, r.cost_usd,
        (SELECT COUNT(*) FROM run_events e WHERE e.run_id=r.id),
        i.program, i.args_json, i.cwd, i.delivery, i.resume_id, i.prompt_chars, i.prompt_truncated
 FROM agent_runs r
 LEFT JOIN run_invocations i ON i.run_id=r.id";

fn load_run_summaries(db: &Connection, session_id: &str) -> Result<Vec<RawRunSummary>, String> {
    let sql = format!("{RUN_SUMMARY_SQL} WHERE r.session_id=?1 ORDER BY r.created_at, r.rowid");
    let mut statement = db.prepare(&sql).map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([session_id], map_run_summary)
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

fn load_run_summaries_for(db: &Connection, run_id: &str) -> Result<Vec<RawRunSummary>, String> {
    let sql = format!("{RUN_SUMMARY_SQL} WHERE r.id=?1");
    let mut statement = db.prepare(&sql).map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([run_id], map_run_summary)
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

fn map_run_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawRunSummary> {
    let program: Option<String> = row.get(20)?;
    let args_raw: Option<String> = row.get(21)?;
    let cwd: Option<String> = row.get(22)?;
    let delivery: Option<String> = row.get(23)?;
    let resume_id: Option<String> = row.get(24)?;
    let prompt_chars: Option<i64> = row.get(25)?;
    let prompt_truncated: Option<i64> = row.get(26)?;
    let invocation = program.map(|program| RawInvocationMeta {
        program,
        args: args_raw
            .as_deref()
            .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
            .unwrap_or_default(),
        cwd: cwd.unwrap_or_default(),
        delivery: delivery.unwrap_or_default(),
        resume_id,
        prompt_chars: prompt_chars.unwrap_or(0),
        prompt_truncated: prompt_truncated.unwrap_or(0) != 0,
    });
    Ok(RawRunSummary {
        id: row.get(0)?,
        agent_id: row.get(1)?,
        runtime_id: row.get(2)?,
        parent_run_id: row.get(3)?,
        status: row.get(4)?,
        mode: row.get(5)?,
        effort: row.get(6)?,
        model_override: row.get(7)?,
        usage_model: row.get(8)?,
        turn_id: row.get(9)?,
        created_at: row.get(10)?,
        started_at: row.get(11)?,
        completed_at: row.get(12)?,
        assignment: row.get(13)?,
        input_tokens: row.get(14)?,
        output_tokens: row.get(15)?,
        cache_read_tokens: row.get(16)?,
        cache_write_tokens: row.get(17)?,
        cost_usd: row.get(18)?,
        event_count: row.get(19)?,
        invocation,
    })
}

fn load_invocation(
    db: &Connection,
    run_id: &str,
) -> Result<(Option<RawInvocationFull>, Option<String>), String> {
    db.query_row(
        "SELECT program, args_json, cwd, delivery, resume_id, user_message_id, prompt_text,
                prompt_chars, prompt_truncated, created_at
         FROM run_invocations WHERE run_id=?1",
        [run_id],
        |row| {
            let args_raw: String = row.get(1)?;
            let invocation = RawInvocationFull {
                program: row.get(0)?,
                args: serde_json::from_str::<Vec<String>>(&args_raw).unwrap_or_default(),
                cwd: row.get(2)?,
                delivery: row.get(3)?,
                resume_id: row.get(4)?,
                prompt: row.get(6)?,
                prompt_chars: row.get::<_, i64>(7).unwrap_or(0),
                prompt_truncated: row.get::<_, i64>(8).unwrap_or(0) != 0,
                created_at: row.get(9)?,
            };
            Ok((invocation, row.get::<_, Option<String>>(5)?))
        },
    )
    .optional()
    .map(|row| row.map_or((None, None), |(invocation, user)| (Some(invocation), user)))
    .map_err(|error| error.to_string())
}

fn load_user_message(
    db: &Connection,
    session_id: &str,
    turn_id: Option<&str>,
    user_message_id: Option<&str>,
) -> Result<Option<RawMessage>, String> {
    if let Some(message_id) = user_message_id {
        let message = db
            .query_row(
                "SELECT id, role, content, created_at, failure_reason, elapsed_ms,
                        COALESCE(message_kind, 'message')
                 FROM messages WHERE id=?1",
                [message_id],
                map_message,
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if message.is_some() {
            return Ok(message);
        }
    }
    let Some(turn_id) = turn_id.filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    db.query_row(
        "SELECT id, role, content, created_at, failure_reason, elapsed_ms,
                COALESCE(message_kind, 'message')
         FROM messages
         WHERE session_id=?1 AND turn_id=?2 AND role='user'
         ORDER BY created_at, rowid LIMIT 1",
        params![session_id, turn_id],
        map_message,
    )
    .optional()
    .map_err(|error| error.to_string())
}

fn load_run_messages(db: &Connection, run_id: &str) -> Result<Vec<RawMessage>, String> {
    let mut statement = db
        .prepare(
            "SELECT id, role, content, created_at, failure_reason, elapsed_ms,
                    COALESCE(message_kind, 'message')
             FROM messages
             WHERE run_id=?1 AND role IN ('assistant', 'system')
             ORDER BY created_at, rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([run_id], map_message)
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

fn map_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawMessage> {
    Ok(RawMessage {
        id: row.get(0)?,
        role: row.get(1)?,
        content: row.get(2)?,
        created_at: row.get(3)?,
        failure_reason: row.get(4)?,
        elapsed_ms: row.get(5)?,
        message_kind: row.get(6)?,
    })
}

fn load_events_page(
    db: &Connection,
    run_id: &str,
    after_seq: i64,
    limit: usize,
) -> Result<(Vec<RawEvent>, bool, Option<i64>), String> {
    let mut statement = db
        .prepare(
            "SELECT seq, event_type, call_id, tool, content, input_json, output,
                    output_json, created_at, parent_call_id, status
             FROM run_events
             WHERE run_id=?1 AND seq>?2
             ORDER BY seq
             LIMIT ?3",
        )
        .map_err(|error| error.to_string())?;
    let mut events = statement
        .query_map(params![run_id, after_seq, (limit + 1) as i64], |row| {
            Ok(RawEvent {
                seq: row.get(0)?,
                event_type: row.get(1)?,
                call_id: row.get(2)?,
                tool: row.get(3)?,
                content: row.get(4)?,
                input_json: row.get(5)?,
                output: row.get(6)?,
                output_json: row.get(7)?,
                created_at: row.get(8)?,
                parent_call_id: row.get(9)?,
                status: row.get(10)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    let has_more = events.len() > limit;
    if has_more {
        events.truncate(limit);
    }
    let next_after_seq = events.last().map(|event| event.seq).or(Some(after_seq).filter(|seq| *seq >= 0));
    Ok((events, has_more, next_after_seq))
}

#[cfg(test)]
mod tests {
    use super::{clip_prompt, mask_args, PROMPT_LIMIT_CHARS};
    use rusqlite::Connection;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn masks_secret_flags_and_prompt_argument() {
        let prompt = "完整 prompt 正文";
        let masked = mask_args(
            &strings(&[
                "--model",
                "sonnet",
                "--api-key",
                "sk-live-1234567890",
                "--token=abc",
                "https://user:pass@example.com/path",
                prompt,
            ]),
            prompt,
        );
        assert_eq!(
            masked,
            strings(&[
                "--model",
                "sonnet",
                "--api-key",
                "***",
                "--token=***",
                "https://***@example.com/path",
                "<prompt>",
            ])
        );
    }

    #[test]
    fn clips_prompt_on_char_boundary() {
        let long = "汉".repeat(PROMPT_LIMIT_CHARS + 5);
        let (clipped, chars, truncated) = clip_prompt(&long);
        assert!(truncated);
        assert_eq!(chars, PROMPT_LIMIT_CHARS + 5);
        assert_eq!(clipped.chars().count(), PROMPT_LIMIT_CHARS);
    }

    #[test]
    fn records_and_reads_invocation() {
        let db = Connection::open_in_memory().unwrap();
        crate::database::migrate(&db).unwrap();
        db.execute_batch(
            "INSERT INTO agents (id, name, avatar, description, instructions, runtime_id, model, max_concurrent_tasks)
             VALUES ('a', 'A', '', '', '', 'cursor', 'auto', 1);
             INSERT INTO chat_sessions (id, title, target_type, target_id, created_at, updated_at)
             VALUES ('s', '会话', 'agent', 'a', 'now', 'now');
             INSERT INTO agent_runs (id, session_id, agent_id, runtime_id, assignment, status, created_at)
             VALUES ('r', 's', 'a', 'cursor', '查一下', 'running', 'now');",
        )
        .unwrap();
        super::record_invocation(
            &db,
            "r",
            "s",
            "cursor",
            "cursor-agent",
            &strings(&["-p", "查一下", "--token", "secret"]),
            "/tmp/work",
            "arg",
            Some("resume-1"),
            None,
            "完整 prompt",
            "now",
        )
        .unwrap();
        let data = super::build_session_raw(&db, "s").unwrap();
        assert_eq!(data.runs.len(), 1);
        let invocation = data.runs[0].invocation.as_ref().expect("invocation");
        assert_eq!(invocation.program, "cursor-agent");
        assert_eq!(invocation.args, strings(&["-p", "查一下", "--token", "***"]));
        assert_eq!(invocation.prompt_chars, "完整 prompt".chars().count() as i64);
        let detail = super::build_run_detail(&db, "r", -1, None).unwrap();
        assert_eq!(
            detail.invocation.as_ref().and_then(|item| item.prompt.as_deref()),
            Some("完整 prompt")
        );
    }
}
