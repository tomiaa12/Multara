use std::{
    collections::HashMap,
    env, fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process,
};

use rusqlite::Connection;

use crate::{
    builtin_agents, database,
    models::{AgentInput, SquadInput},
    ops, runtime,
};

const HELP: &str = "\
multara — Multara 内置 CLI

用法：
  multara <resource> <verb> [id] [flags]

资源：
  task         list|get|create|update|status|assign|comment|runs
  agent        list|get|create|update|skills
  squad        list|get|create|update|member
  project      list|get|create|update
  automation   list|get|create
  channel      list|get|enable|disable
  skill        list|get|builtin|import
  runtime      list
  session      history|resume
  terminal     list|read [id] [--start N] [--end N]

stdout 为 JSON。提示与 --help 走 stderr。解析 stdout 时不要 2>&1。

环境变量：
  MULTARA_DB          sqlite 路径
  MULTARA_APP_DATA    应用数据目录（默认其中的 multara.sqlite3）
  MULTARA_SESSION_ID  当前聊天会话，用于继承需求
";

pub fn run() {
    let args: Vec<String> = env::args().skip(1).collect();
    let args = args
        .into_iter()
        .filter(|arg| arg != "--cli")
        .collect::<Vec<_>>();
    match run_args(&args, io::stdout(), io::stderr()) {
        Ok(()) => {}
        Err(code) => process::exit(code),
    }
}

pub fn should_run_as_cli(args: &[String]) -> bool {
    let Some(first) = args.iter().find(|arg| *arg != "--") else {
        return false;
    };
    matches!(
        first.as_str(),
        "--help"
            | "-h"
            | "--cli"
            | "task"
            | "agent"
            | "squad"
            | "project"
            | "automation"
            | "channel"
            | "skill"
            | "runtime"
            | "session"
            | "terminal"
    )
}

pub fn run_args(
    args: &[String],
    mut stdout: impl io::Write,
    mut stderr: impl io::Write,
) -> Result<(), i32> {
    // `-h` 只在开头才算求助，否则恰好等于 `-h` 的描述或评论会被吞掉。
    if args.is_empty() || args[0] == "-h" || args.iter().any(|arg| arg == "--help") {
        let _ = writeln!(stderr, "{HELP}");
        return Ok(());
    }
    let mut db = match open_db() {
        Ok(db) => db,
        Err(error) => {
            let _ = writeln!(stderr, "{error}");
            return Err(1);
        }
    };
    match execute(args, &mut db) {
        Ok(json) => {
            let _ = writeln!(stdout, "{json}");
            Ok(())
        }
        Err(error) => {
            let _ = writeln!(stderr, "{error}");
            Err(1)
        }
    }
}

pub fn execute(args: &[String], db: &mut Connection) -> Result<String, String> {
    let parsed = parse_args(args);
    let resource = parsed.positional.first().map(String::as_str).unwrap_or("");
    let verb = parsed.positional.get(1).map(String::as_str).unwrap_or("");
    let id = parsed.positional.get(2).cloned();
    match (resource, verb) {
        ("task", "list") => {
            ops::task::settle_finished(db)?;
            ops::json(&ops::task::list(
                db,
                parsed.flag(&["status"]),
                parsed.flag(&["assignee", "assignee-id"]),
                parsed.flag(&["project", "project-id"]),
            )?)
        }
        ("task", "get") => {
            let id = id.ok_or_else(|| "缺少任务 id".to_string())?;
            ops::json(&ops::task::get(db, &id)?)
        }
        ("task", "create") => {
            let title = parsed
                .flag(&["title"])
                .ok_or_else(|| "缺少 --title".to_string())?;
            let description = description_from(&parsed)?;
            let assignee = parsed
                .flag(&["assignee", "assignee-id"])
                .map(str::to_string)
                .or_else(|| env::var("MULTARA_AGENT_ID").ok())
                .unwrap_or_else(|| builtin_agents::MIKU_ID.to_string());
            let source_session = env::var("MULTARA_SESSION_ID").ok();
            // 在某个任务的 run 里建子任务时，未传 --parent 则挂到当前任务下，详情右侧才能看见。
            let parent_task_id = parsed
                .flag(&["parent", "parent-id"])
                .map(str::to_string)
                .or_else(|| env::var("MULTARA_TASK_ID").ok());
            let created = ops::task::create(
                db,
                ops::task::CreateTaskInput {
                    title,
                    description: &description,
                    agent_id: &assignee,
                    project_id: parsed.flag(&["project", "project-id"]),
                    project_ids: &[],
                    source_session_id: source_session.as_deref(),
                    status: parsed.flag(&["status"]),
                    parent_task_id: parent_task_id.as_deref(),
                },
            )?;
            ops::json(&created.task)
        }
        ("task", "update") => {
            let id = id.ok_or_else(|| "缺少任务 id".to_string())?;
            let description = parsed
                .flag(&["description"])
                .map(str::to_string)
                .or_else(|| description_file(&parsed).ok());
            ops::json(&ops::task::update(
                db,
                &id,
                parsed.flag(&["title"]),
                description.as_deref(),
                parsed.flag(&["status"]),
                parsed.flag(&["assignee", "assignee-id"]),
                parsed.flag(&["comment"]),
            )?)
        }
        ("task", "status") => {
            let id = id.ok_or_else(|| "缺少任务 id".to_string())?;
            let status = parsed
                .positional
                .get(3)
                .map(String::as_str)
                .or_else(|| parsed.flag(&["status"]))
                .ok_or_else(|| "缺少状态".to_string())?;
            ops::json(&ops::task::update_status(db, &id, status)?)
        }
        ("task", "assign") => {
            let id = id.ok_or_else(|| "缺少任务 id".to_string())?;
            let to = parsed
                .flag(&["to", "assignee", "assignee-id"])
                .ok_or_else(|| "缺少 --to".to_string())?;
            ops::json(&ops::task::assign(db, &id, to)?)
        }
        ("task", "comment") => {
            let id = id.ok_or_else(|| "缺少任务 id".to_string())?;
            let text = parsed
                .flag(&["comment"])
                .ok_or_else(|| "缺少 --comment".to_string())?;
            ops::task::comment(db, &id, text)?;
            ops::json(&serde_json::json!({ "ok": true }))
        }
        ("task", "runs") => {
            let id = id.ok_or_else(|| "缺少任务 id".to_string())?;
            ops::json(&ops::task::runs(db, &id)?)
        }
        ("agent", "list") => ops::json(&ops::agent::list(db)?),
        ("agent", "get") => {
            let id = id.ok_or_else(|| "缺少智能体 id".to_string())?;
            ops::json(&ops::agent::get(db, &id)?)
        }
        ("agent", "create") => {
            let name = parsed
                .flag(&["name"])
                .ok_or_else(|| "缺少 --name".to_string())?;
            let runtime_id = parsed
                .flag(&["runtime-id"])
                .ok_or_else(|| "缺少 --runtime-id".to_string())?;
            ops::json(&ops::agent::create(
                db,
                name,
                parsed.flag(&["description"]).unwrap_or(""),
                parsed.flag(&["instructions"]).unwrap_or(""),
                runtime_id,
                parsed.flag(&["avatar"]).unwrap_or("🤖"),
                parsed
                    .flag(&["max-concurrent-tasks"])
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(1),
            )?)
        }
        ("agent", "update") => {
            let id = id.ok_or_else(|| "缺少智能体 id".to_string())?;
            let current = ops::agent::get(db, &id)?;
            let input = AgentInput {
                id: Some(id),
                name: parsed.flag(&["name"]).unwrap_or(&current.name).to_string(),
                avatar: parsed
                    .flag(&["avatar"])
                    .unwrap_or(&current.avatar)
                    .to_string(),
                description: parsed
                    .flag(&["description"])
                    .unwrap_or(&current.description)
                    .to_string(),
                instructions: parsed
                    .flag(&["instructions"])
                    .unwrap_or(&current.instructions)
                    .to_string(),
                runtime_id: parsed
                    .flag(&["runtime-id"])
                    .unwrap_or(&current.runtime_id)
                    .to_string(),
                model: current.model,
                max_concurrent_tasks: parsed
                    .flag(&["max-concurrent-tasks"])
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(current.max_concurrent_tasks),
                skill_ids: current.skill_ids,
                mcp_ids: current.mcp_ids,
            };
            ops::json(&ops::agent::save(db, input, false)?)
        }
        ("agent", "skills") => {
            let locale = crate::app_locale::from_db(db);
            ops::json(&ops::skill::builtin(locale))
        }
        ("squad", "list") => ops::json(&ops::squad::list(db)?),
        ("squad", "get") => {
            let id = id.ok_or_else(|| "缺少小队 id".to_string())?;
            ops::json(&ops::squad::get(db, &id)?)
        }
        ("squad", "create") => {
            let name = parsed
                .flag(&["name"])
                .ok_or_else(|| "缺少 --name".to_string())?;
            let lead = parsed
                .flag(&["lead-id", "leader-id"])
                .ok_or_else(|| "缺少 --lead-id".to_string())?;
            let members = parsed.list_flag("member-ids");
            ops::json(&ops::squad::create(
                db,
                name,
                parsed.flag(&["description"]).unwrap_or(""),
                lead,
                members,
            )?)
        }
        ("squad", "update") => {
            let id = id.ok_or_else(|| "缺少小队 id".to_string())?;
            let current = ops::squad::get(db, &id)?;
            ops::json(&ops::squad::save(
                db,
                SquadInput {
                    id: Some(id),
                    name: parsed.flag(&["name"]).unwrap_or(&current.name).to_string(),
                    description: parsed
                        .flag(&["description"])
                        .unwrap_or(&current.description)
                        .to_string(),
                    instructions: parsed
                        .flag(&["instructions"])
                        .unwrap_or(&current.instructions)
                        .to_string(),
                    leader_agent_id: parsed
                        .flag(&["lead-id", "leader-id"])
                        .unwrap_or(&current.leader_agent_id)
                        .to_string(),
                    member_agent_ids: if parsed.flags.contains_key("member-ids") {
                        parsed.list_flag("member-ids")
                    } else {
                        current.member_agent_ids
                    },
                },
            )?)
        }
        ("squad", "member") => {
            let id = id.ok_or_else(|| "缺少小队 id".to_string())?;
            ops::json(&ops::squad::set_members(
                db,
                &id,
                parsed.list_flag("member-ids"),
            )?)
        }
        ("project", "list") => ops::json(&ops::project::list(db)?),
        ("project", "get") => {
            let id = id.ok_or_else(|| "缺少需求 id".to_string())?;
            ops::json(&ops::project::get(db, &id)?)
        }
        ("project", "create") => ops::json(&ops::project::create(
            db,
            parsed
                .flag(&["name"])
                .ok_or_else(|| "缺少 --name".to_string())?,
            parsed.flag(&["description", "note"]).unwrap_or(""),
        )?),
        ("project", "update") => {
            let id = id.ok_or_else(|| "缺少需求 id".to_string())?;
            ops::json(&ops::project::update(
                db,
                &id,
                parsed.flag(&["name"]),
                parsed.flag(&["description", "note"]),
            )?)
        }
        ("automation", "list") => ops::json(&ops::automation::list(db)?),
        ("automation", "get") => {
            let id = id.ok_or_else(|| "缺少自动化 id".to_string())?;
            ops::json(&ops::automation::get(db, &id)?)
        }
        ("automation", "create") => create_automation(db, &parsed),
        ("channel", "list") => {
            let rows = crate::channel::store::list_installations(db)?;
            ops::json(&rows.iter().map(channel_json).collect::<Vec<_>>())
        }
        ("channel", "get") => {
            let id = id.ok_or_else(|| "缺少渠道 id".to_string())?;
            let installation = crate::channel::store::get_installation(db, &id)?
                .ok_or_else(|| "渠道不存在".to_string())?;
            ops::json(&channel_json(&installation))
        }
        ("channel", "enable") | ("channel", "disable") => {
            let id = id.ok_or_else(|| "缺少渠道 id".to_string())?;
            let enabled = verb == "enable";
            db.execute(
                "UPDATE channel_installations SET enabled=?1, updated_at=?2 WHERE id=?3",
                rusqlite::params![enabled as i64, chrono::Utc::now().to_rfc3339(), id],
            )
            .map_err(|error| error.to_string())?;
            ops::json(&serde_json::json!({ "id": id, "enabled": enabled }))
        }
        ("skill", "list") => ops::json(&ops::skill::list_scanned()),
        ("skill", "get") => {
            let id = id.ok_or_else(|| "缺少 skill id".to_string())?;
            ops::json(&ops::skill::get(&id)?)
        }
        ("skill", "builtin") => {
            let locale = crate::app_locale::from_db(db);
            ops::json(&ops::skill::builtin(locale))
        }
        ("skill", "import") => {
            Err("第一期不支持 URL 导入。用 `multara skill list` 查看本机扫描结果。".into())
        }
        ("runtime", "list") => ops::json(&runtime::detect()),
        ("session", "history") => {
            let session_id = id
                .or_else(|| env::var("MULTARA_SESSION_ID").ok())
                .ok_or_else(|| "缺少会话 id 或 MULTARA_SESSION_ID".to_string())?;
            ops::json(&ops::skill::comment_history(db, &session_id)?)
        }
        ("terminal", "list") => ops::json(&crate::terminal::list_snapshots(&app_data_dir()?)?),
        ("terminal", "read") => {
            let start = parsed
                .flag(&["start"])
                .map(|value| parse_line(value, "start"))
                .transpose()?;
            let end = parsed
                .flag(&["end"])
                .map(|value| parse_line(value, "end"))
                .transpose()?;
            ops::json(&crate::terminal::read_snapshot(
                &app_data_dir()?,
                id.as_deref(),
                start,
                end,
            )?)
        }
        ("session", "resume") => {
            let session_id = id
                .or_else(|| env::var("MULTARA_SESSION_ID").ok())
                .ok_or_else(|| "缺少会话 id 或 MULTARA_SESSION_ID".to_string())?;
            ops::json(&ops::session::resume(
                db,
                &session_id,
                parsed.flag(&["prompt"]),
            )?)
        }
        ("", _) => Err(HELP.to_string()),
        (resource, "") => Err(format!("缺少动词。试试 `multara {resource} --help`")),
        (resource, verb) => Err(format!("未知命令：multara {resource} {verb}")),
    }
}

fn create_automation(db: &mut Connection, parsed: &ParsedArgs) -> Result<String, String> {
    let name = parsed
        .flag(&["name"])
        .ok_or_else(|| "缺少 --name".to_string())?;
    let agent_id = parsed
        .flag(&["agent-id", "assignee", "assignee-id"])
        .ok_or_else(|| "缺少 --agent-id".to_string())?;
    let prompt = parsed
        .flag(&["prompt"])
        .ok_or_else(|| "缺少 --prompt".to_string())?;
    let project_id = parsed
        .flag(&["project-id", "project"])
        .ok_or_else(|| "缺少 --project-id".to_string())?;
    ops::require_agent(db, agent_id)?;
    ops::require_project(db, project_id)?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let enabled = parsed
        .flag(&["enabled"])
        .map(|value| value != "false")
        .unwrap_or(true);
    let schedule = parsed
        .flag(&["schedule"])
        .map(|expression| {
            let timezone = parsed.flag(&["timezone"]).unwrap_or(DEFAULT_TIMEZONE);
            next_schedule_run(expression, timezone)
                .map(|next_run_at| (expression, timezone, next_run_at))
        })
        .transpose()?;
    // 与界面保存的计划自动化一致：automations 记 interval，真正的计划在触发器行里。
    let trigger_type = if schedule.is_some() { "interval" } else { "manual" };
    let transaction = db.transaction().map_err(|error| error.to_string())?;
    transaction
        .execute(
            "INSERT INTO automations
             (id, name, project_id, target_type, target_id, prompt, trigger_type,
              interval_minutes, event_name, enabled, last_run_at, created_at)
             VALUES (?1, ?2, ?3, 'agent', ?4, ?5, ?6, NULL, NULL, ?7, NULL, ?8)",
            rusqlite::params![
                id,
                name,
                project_id,
                agent_id,
                prompt,
                trigger_type,
                enabled,
                now
            ],
        )
        .map_err(|error| error.to_string())?;
    if let Some((expression, timezone, next_run_at)) = schedule {
        transaction
            .execute(
                "INSERT INTO automation_triggers
                 (id, automation_id, kind, enabled, cron_expression, timezone, next_run_at, created_at)
                 VALUES (?1, ?2, 'schedule', ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    uuid::Uuid::new_v4().to_string(),
                    id,
                    enabled,
                    expression.trim(),
                    timezone,
                    next_run_at,
                    now
                ],
            )
            .map_err(|error| error.to_string())?;
    }
    transaction.commit().map_err(|error| error.to_string())?;
    ops::json(&ops::automation::get(db, &id)?)
}

const DEFAULT_TIMEZONE: &str = "UTC";

/// `--schedule` 是 5 字段 Cron（分 时 日 月 周），按 `--timezone`（IANA，默认 UTC）推算首次运行。
fn next_schedule_run(expression: &str, timezone: &str) -> Result<String, String> {
    use std::str::FromStr;
    let timezone =
        chrono_tz::Tz::from_str(timezone).map_err(|_| "--timezone 需要 IANA 时区".to_string())?;
    let schedule = cron::Schedule::from_str(&format!("0 {}", expression.trim()))
        .map_err(|error| format!("--schedule 需要 5 字段 Cron 表达式：{error}"))?;
    schedule
        .after(&chrono::Utc::now().with_timezone(&timezone))
        .next()
        .map(|next| next.with_timezone(&chrono::Utc).to_rfc3339())
        .ok_or_else(|| "Cron 无法计算下一次运行时间".to_string())
}

fn description_from(parsed: &ParsedArgs) -> Result<String, String> {
    if let Some(text) = parsed.flag(&["description"]) {
        return Ok(text.to_string());
    }
    description_file(parsed)
}

fn description_file(parsed: &ParsedArgs) -> Result<String, String> {
    let path = parsed
        .flag(&["description-file"])
        .ok_or_else(|| "缺少 --description 或 --description-file".to_string())?;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| file.take(MAX_DESCRIPTION_FILE_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|error| format!("读取 description-file 失败：{error}"))?;
    if bytes.len() as u64 > MAX_DESCRIPTION_FILE_BYTES {
        return Err(format!(
            "description-file 超过 {} KB",
            MAX_DESCRIPTION_FILE_BYTES / 1024
        ));
    }
    String::from_utf8(bytes).map_err(|error| format!("读取 description-file 失败：{error}"))
}

const MAX_DESCRIPTION_FILE_BYTES: u64 = 1024 * 1024;

fn open_db() -> Result<Connection, String> {
    let path = db_path()?;
    database::open_file_light(&path)
}

fn parse_line(value: &str, name: &str) -> Result<usize, String> {
    value
        .parse()
        .ok()
        .filter(|line| *line >= 1)
        .ok_or_else(|| format!("--{name} 需要从 1 开始的行号"))
}

fn app_data_dir() -> Result<PathBuf, String> {
    Ok(db_path()?
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf())
}

pub fn db_path() -> Result<PathBuf, String> {
    if let Ok(path) = env::var("MULTARA_DB") {
        return Ok(PathBuf::from(path));
    }
    if let Ok(dir) = env::var("MULTARA_APP_DATA") {
        return Ok(PathBuf::from(dir).join("multara.sqlite3"));
    }
    let home = env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .ok_or_else(|| "无法确定主目录，请设置 MULTARA_DB".to_string())?;
    let mut path = PathBuf::from(home);
    if cfg!(target_os = "macos") {
        path.push("Library/Application Support/com.multara.desktop/multara.sqlite3");
    } else if cfg!(windows) {
        path.push("AppData/Roaming/com.multara.desktop/multara.sqlite3");
    } else {
        path.push(".local/share/com.multara.desktop/multara.sqlite3");
    }
    Ok(path)
}

pub fn ensure_shim(app_data_dir: &std::path::Path) -> PathBuf {
    let bin_dir = app_data_dir.join("bin");
    let _ = fs::create_dir_all(&bin_dir);
    let dest = bin_dir.join(if cfg!(windows) {
        "multara.exe"
    } else {
        "multara"
    });
    if let Some(source) = cli_binary_source() {
        install_cli_binary(&source, &dest);
    }
    bin_dir
}

/// 给智能体用的 CLI。独立的 `multara-cli` 比当前进程旧时，不能再拿去覆盖。
/// 旧二进制打开数据库时会把正在运行的任务标成失败。
fn cli_binary_source() -> Option<PathBuf> {
    let exe = env::current_exe().ok()?;
    match sidecar_path() {
        Some(sidecar) if !binary_is_older(&sidecar, &exe) => Some(sidecar),
        _ => Some(exe),
    }
}

fn binary_is_older(candidate: &std::path::Path, than: &std::path::Path) -> bool {
    let candidate_mtime = fs::metadata(candidate)
        .and_then(|meta| meta.modified())
        .ok();
    let than_mtime = fs::metadata(than).and_then(|meta| meta.modified()).ok();
    match (candidate_mtime, than_mtime) {
        (Some(candidate_mtime), Some(than_mtime)) => candidate_mtime < than_mtime,
        _ => true,
    }
}

fn install_cli_binary(source: &std::path::Path, dest: &std::path::Path) {
    let Ok(meta) = fs::metadata(source) else {
        return;
    };
    let modified = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|time| time.as_secs())
        .unwrap_or(0);
    let stamp = dest.with_extension("stamp");
    let key = format!("{}:{}:{modified}", source.display(), meta.len());
    if dest.is_file() && fs::read_to_string(&stamp).ok().as_deref() == Some(key.as_str()) {
        return;
    }
    // 先写同目录临时文件再改名：正在执行的旧 CLI 保留原 inode，并发 exec 不会撞上半截文件。
    let staging = dest.with_extension(format!("{}.tmp", process::id()));
    if fs::copy(source, &staging).is_err() {
        let _ = fs::remove_file(&staging);
        return;
    }
    if fs::rename(&staging, dest).is_err() {
        let _ = fs::remove_file(&staging);
        return;
    }
    let _ = fs::write(stamp, key);
}

pub fn sidecar_path() -> Option<PathBuf> {
    let exe = env::current_exe().ok()?;
    let dir = exe.parent()?;
    for name in ["multara-cli", "multara-cli.exe"] {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

pub fn path_env(bin_dir: &std::path::Path) -> String {
    let prefix = bin_dir.to_string_lossy();
    let rest = runtime::augmented_path_env();
    let rest = rest.to_string_lossy();
    if rest.is_empty() {
        return prefix.into_owned();
    }
    if cfg!(windows) {
        format!("{prefix};{rest}")
    } else {
        format!("{prefix}:{rest}")
    }
}

struct ParsedArgs {
    positional: Vec<String>,
    flags: HashMap<String, String>,
}

impl ParsedArgs {
    fn flag(&self, names: &[&str]) -> Option<&str> {
        names
            .iter()
            .find_map(|name| self.flags.get(*name).map(String::as_str))
    }

    fn list_flag(&self, name: &str) -> Vec<String> {
        self.flags
            .get(name)
            .map(|value| {
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }
}

fn parse_args(args: &[String]) -> ParsedArgs {
    let mut positional = Vec::new();
    let mut flags = HashMap::new();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if let Some(name) = arg.strip_prefix("--").filter(|_| is_flag(arg)) {
            if let Some(value) = args.get(index + 1).filter(|item| !is_flag(item)) {
                flags.insert(name.to_string(), value.clone());
                index += 2;
                continue;
            }
            flags.insert(name.to_string(), "true".to_string());
            index += 1;
            continue;
        }
        positional.push(arg.clone());
        index += 1;
    }
    ParsedArgs { positional, flags }
}

/// 只有 `--名字` 才是标志；`-x`、`- 列表项`、`---` 这类都是值。
fn is_flag(arg: &str) -> bool {
    arg.strip_prefix("--")
        .and_then(|name| name.chars().next())
        .is_some_and(|first| first.is_ascii_alphabetic())
}

fn channel_json(installation: &crate::channel::Installation) -> serde_json::Value {
    serde_json::json!({
        "id": installation.id,
        "channelType": installation.channel_type,
        "name": installation.name,
        "targetType": installation.target_type,
        "targetId": installation.target_id,
        "projectId": installation.project_id,
        "appId": installation.config.app_id,
        "region": installation.config.region(),
        "botName": installation.config.bot_name,
        "groupEnabled": installation.config.group_enabled,
        "enabled": installation.enabled,
        "status": installation.status,
        "lastError": installation.last_error,
    })
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::execute;
    use crate::database;

    fn db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        database::migrate(&db).unwrap();
        database::seed(&db).unwrap();
        db
    }

    #[test]
    fn task_create_todo_json_contains_id_and_run() {
        let mut db = db();
        let json = execute(
            &[
                "task".into(),
                "create".into(),
                "--title".into(),
                "整理目标".into(),
                "--description".into(),
                "列出当前目标并建成任务".into(),
                "--assignee".into(),
                "miku".into(),
                "--status".into(),
                "todo".into(),
            ],
            &mut db,
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let id = value["id"].as_str().expect("id");
        assert!(!id.is_empty());
        let count: u32 = db
            .query_row("SELECT COUNT(*) FROM agent_runs", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn opening_the_cli_database_keeps_live_runs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("multara.sqlite3");
        {
            let db = database::open_file(&path).unwrap();
            db.execute(
                "INSERT INTO agent_runs
                 (id, session_id, agent_id, runtime_id, assignment, status, created_at, started_at)
                 VALUES ('live', 'welcome', 'miku', 'cursor', 'task', 'running',
                         '2026-09-23T00:00:00Z', '2026-09-23T00:00:00Z')",
                [],
            )
            .unwrap();
        }
        let db = database::open_file(&path).unwrap();
        let (status, completed_at): (String, Option<String>) = db
            .query_row(
                "SELECT status, completed_at FROM agent_runs WHERE id='live'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(status, "running");
        assert!(completed_at.is_none());
    }

    #[test]
    fn task_create_backlog_does_not_enqueue() {
        let mut db = db();
        execute(
            &[
                "task".into(),
                "create".into(),
                "--title".into(),
                "以后再说".into(),
                "--description".into(),
                "先记下来".into(),
                "--status".into(),
                "backlog".into(),
            ],
            &mut db,
        )
        .unwrap();
        let count: u32 = db
            .query_row("SELECT COUNT(*) FROM agent_runs", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn session_resume_enqueues_the_saved_cursor_chat() {
        let mut db = db();
        db.execute("UPDATE agents SET runtime_id='cursor' WHERE id='miku'", [])
            .unwrap();
        crate::conversation::save_id(&db, "welcome", "miku", "cursor", "chat-old").unwrap();
        let json = execute(
            &["session".into(), "resume".into(), "welcome".into()],
            &mut db,
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["conversationId"], "chat-old");
        assert_eq!(value["runtimeId"], "cursor");
        assert!(value["assignment"]
            .as_str()
            .unwrap()
            .contains("从停下的地方继续"));
        let status: String = db
            .query_row("SELECT status FROM agent_runs", [], |row| row.get(0))
            .unwrap();
        assert_eq!(status, "queued");
        assert_eq!(
            crate::conversation::load_id(&db, "welcome", "miku", "cursor")
                .unwrap()
                .as_deref(),
            Some("chat-old")
        );
    }

    #[test]
    fn flag_values_may_start_with_a_dash() {
        let parsed = super::parse_args(&[
            "task".into(),
            "create".into(),
            "--description".into(),
            "- 第一项".into(),
            "--title".into(),
            "-h".into(),
            "--prompt".into(),
            "---".into(),
            "--enabled".into(),
            "--name".into(),
            "x".into(),
        ]);
        assert_eq!(parsed.flag(&["description"]), Some("- 第一项"));
        assert_eq!(parsed.flag(&["title"]), Some("-h"));
        assert_eq!(parsed.flag(&["prompt"]), Some("---"));
        assert_eq!(parsed.flag(&["enabled"]), Some("true"));
        assert_eq!(parsed.flag(&["name"]), Some("x"));
        assert_eq!(parsed.positional, ["task", "create"]);
    }

    #[test]
    fn short_help_only_counts_as_the_first_argument() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        super::run_args(&["-h".into()], &mut stdout, &mut stderr).unwrap();
        assert!(String::from_utf8_lossy(&stderr).contains("用法"));
        assert!(stdout.is_empty());
    }

    #[test]
    fn automation_schedule_creates_a_due_trigger() {
        let mut db = db();
        db.execute(
            "INSERT INTO projects (id, name, created_at, opened_at) VALUES ('p', 'P', 'now', 'now')",
            [],
        )
        .unwrap();
        let args = |schedule: &str| -> Vec<String> {
            [
                "automation", "create", "--name", "日报", "--agent-id", "miku", "--prompt",
                "写日报", "--project-id", "p", "--schedule", schedule,
            ]
            .map(String::from)
            .to_vec()
        };
        execute(&args("0 9 * * *"), &mut db).unwrap();
        let (cron, next): (String, String) = db
            .query_row(
                "SELECT cron_expression, next_run_at FROM automation_triggers WHERE kind='schedule'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(cron, "0 9 * * *");
        assert!(chrono::DateTime::parse_from_rfc3339(&next).is_ok());
        assert!(execute(&args("每天"), &mut db).is_err());
    }

    #[test]
    fn session_resume_requires_a_saved_conversation() {
        let mut db = db();
        db.execute("UPDATE agents SET runtime_id='cursor' WHERE id='miku'", [])
            .unwrap();
        let error = execute(
            &["session".into(), "resume".into(), "welcome".into()],
            &mut db,
        )
        .unwrap_err();
        assert!(error.contains("conversation id"));
    }
}
