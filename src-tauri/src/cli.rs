use std::{
    collections::HashMap,
    env, fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process,
};

use rusqlite::{Connection, OptionalExtension};

use crate::{
    builtin_agents, database,
    models::{AgentInput, SquadInput},
    ops, runtime,
};

fn help() -> String {
    tr!("\
multara — Multara 内置 CLI

用法：
  multara <resource> <verb> [id] [flags]

资源：
  task         list|get|create|update|status|assign|comment|runs
  agent        list|get|create|update|skills
  mcp          list
  squad        list|get|create|update|member
  project      list|get|create|update
  workspace    list|add
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
")
}

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
            | "workspace"
            | "automation"
            | "channel"
            | "skill"
            | "mcp"
            | "runtime"
            | "session"
            | "terminal"
            | "canvas"
    )
}

pub fn run_args(
    args: &[String],
    mut stdout: impl io::Write,
    mut stderr: impl io::Write,
) -> Result<(), i32> {
    // `-h` 只在开头才算求助，否则恰好等于 `-h` 的描述或评论会被吞掉。
    let canvas = args.first().map(String::as_str) == Some("canvas");
    if args.is_empty() || args[0] == "-h" || (!canvas && args.iter().any(|arg| arg == "--help")) {
        let _ = writeln!(stderr, "{}", help());
        return Ok(());
    }
    let mut db = match open_db() {
        Ok(db) => db,
        Err(error) => {
            let _ = writeln!(stderr, "{error}");
            return Err(1);
        }
    };
    // canvas 子命令有自己的退出码约定（1 参数 / 2 环境 / 3 Canvas 错误），stdout 失败时也是 JSON
    if args.first().map(String::as_str) == Some("canvas") {
        let output = crate::canvas_cli::run(args, &mut db);
        for line in output.stderr {
            let _ = writeln!(stderr, "{line}");
        }
        let _ = writeln!(stdout, "{}", output.stdout);
        return if output.code == 0 { Ok(()) } else { Err(output.code) };
    }
    match execute(args, &mut db) {
        Ok(json) => {
            let json = with_ui(args, json);
            let _ = writeln!(stdout, "{json}");
            Ok(())
        }
        Err(error) => {
            let _ = writeln!(stderr, "{error}");
            Err(1)
        }
    }
}

/// 列表类命令带 `--ui` 时，输出改成 `{ items, _ui }`：`_ui` 是一个 `multara-ui` table 块，
/// 聊天里直接显示成表格，模型不用再转述（方案 §4.4）。不带 `--ui` 时输出不变，免得记账类调用刷屏（R8）。
fn with_ui(args: &[String], json: String) -> String {
    if !args.iter().any(|arg| arg == "--ui") {
        return json;
    }
    let parsed = parse_args(args);
    let resource = parsed.positional.first().map(String::as_str).unwrap_or("");
    let verb = parsed.positional.get(1).map(String::as_str).unwrap_or("");
    let columns: &[(&str, &str)] = match (resource, verb) {
        ("task", "list") => &[
            ("title", "标题"),
            ("status", "状态"),
            ("agentId", "负责人"),
            ("priority", "优先级"),
            ("updatedAt", "更新时间"),
        ],
        ("task", "runs") => &[
            ("agentId", "智能体"),
            ("runtimeId", "运行时"),
            ("status", "状态"),
            ("createdAt", "开始时间"),
        ],
        ("agent", "list") => &[("name", "名称"), ("id", "ID"), ("runtimeId", "运行时"), ("model", "模型")],
        _ => return json,
    };
    let Ok(serde_json::Value::Array(items)) = serde_json::from_str::<serde_json::Value>(&json) else {
        return json;
    };
    let rows: Vec<serde_json::Value> = items
        .iter()
        .take(200)
        .map(|item| {
            serde_json::Value::Object(
                columns
                    .iter()
                    .map(|(key, _)| {
                        let value = match item.get(*key) {
                            Some(serde_json::Value::String(text)) => serde_json::Value::String(text.clone()),
                            Some(serde_json::Value::Null) | None => serde_json::Value::Null,
                            Some(other) => serde_json::Value::String(other.to_string()),
                        };
                        (key.to_string(), value)
                    })
                    .collect(),
            )
        })
        .collect();
    let format = |key: &str| if key.ends_with("At") { "date" } else { "text" };
    let ui = serde_json::json!({
        "type": "table",
        "title": format!("multara {resource} {verb}"),
        "props": {
            "columns": columns.iter().map(|(key, label)| serde_json::json!({
                "key": key,
                "label": crate::i18n::tr(label),
                "format": format(key),
            })).collect::<Vec<_>>(),
            "rows": rows,
        },
    });
    serde_json::to_string_pretty(&serde_json::json!({ "items": items, "_ui": ui })).unwrap_or(json)
}

pub fn execute(args: &[String], db: &mut Connection) -> Result<String, String> {
    let parsed = parse_args(args);
    let resource = parsed.positional.first().map(String::as_str).unwrap_or("");
    let verb = parsed.positional.get(1).map(String::as_str).unwrap_or("");
    let id = parsed.positional.get(2).cloned();
    ensure_write_allowed(
        db,
        env::var("MULTARA_RUN_ID").ok().as_deref(),
        resource,
        verb,
    )?;
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
            let id = id.ok_or_else(|| tr!("缺少任务 id"))?;
            ops::json(&ops::task::get(db, &id)?)
        }
        ("task", "create") => {
            let title = parsed
                .flag(&["title"])
                .ok_or_else(|| tr!("缺少 --title"))?;
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
                    requested_mode: crate::models::RunMode::Auto,
                },
            )?;
            ops::json(&created.task)
        }
        ("task", "update") => {
            let id = id.ok_or_else(|| tr!("缺少任务 id"))?;
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
            let id = id.ok_or_else(|| tr!("缺少任务 id"))?;
            let status = parsed
                .positional
                .get(3)
                .map(String::as_str)
                .or_else(|| parsed.flag(&["status"]))
                .ok_or_else(|| tr!("缺少状态"))?;
            ops::json(&ops::task::update_status(db, &id, status)?)
        }
        ("task", "assign") => {
            let id = id.ok_or_else(|| tr!("缺少任务 id"))?;
            let to = parsed
                .flag(&["to", "assignee", "assignee-id"])
                .ok_or_else(|| tr!("缺少 --to"))?;
            ops::json(&ops::task::assign(db, &id, to)?)
        }
        ("task", "comment") => {
            let id = id.ok_or_else(|| tr!("缺少任务 id"))?;
            let text = parsed
                .flag(&["comment"])
                .ok_or_else(|| tr!("缺少 --comment"))?;
            ops::task::comment(db, &id, text)?;
            ops::json(&serde_json::json!({ "ok": true }))
        }
        ("task", "runs") => {
            let id = id.ok_or_else(|| tr!("缺少任务 id"))?;
            ops::json(&ops::task::runs(db, &id)?)
        }
        ("agent", "list") => ops::json(&ops::agent::list(db)?),
        ("agent", "get") => {
            let id = id.ok_or_else(|| tr!("缺少智能体 id"))?;
            ops::json(&ops::agent::get(db, &id)?)
        }
        ("agent", "create") => {
            let name = parsed
                .flag(&["name"])
                .ok_or_else(|| tr!("缺少 --name"))?;
            let runtime_id = parsed
                .flag(&["runtime-id"])
                .ok_or_else(|| tr!("缺少 --runtime-id"))?;
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
                resolve_resources("skill", &parsed.list_flag("skills"))?,
                resolve_resources("mcp", &parsed.list_flag("mcps"))?,
            )?)
        }
        ("agent", "update") => {
            let id = id.ok_or_else(|| tr!("缺少智能体 id"))?;
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
                skill_ids: changed_resources(
                    &current.skill_ids,
                    "skill",
                    parsed.flags.contains_key("skills"),
                    &parsed.list_flag("skills"),
                    parsed.flags.contains_key("remove-skills"),
                    &parsed.list_flag("remove-skills"),
                )?,
                mcp_ids: changed_resources(
                    &current.mcp_ids,
                    "mcp",
                    parsed.flags.contains_key("mcps"),
                    &parsed.list_flag("mcps"),
                    parsed.flags.contains_key("remove-mcps"),
                    &parsed.list_flag("remove-mcps"),
                )?,
            };
            ops::json(&ops::agent::save(db, input, false)?)
        }
        ("agent", "skills") => {
            let locale = crate::app_locale::from_db(db);
            let genui = crate::commands::genui_enabled(db);
            ops::json(&ops::skill::builtin(locale, genui))
        }
        ("squad", "list") => ops::json(&ops::squad::list(db)?),
        ("squad", "get") => {
            let id = id.ok_or_else(|| tr!("缺少小队 id"))?;
            ops::json(&ops::squad::get(db, &id)?)
        }
        ("squad", "create") => {
            let name = parsed
                .flag(&["name"])
                .ok_or_else(|| tr!("缺少 --name"))?;
            let lead = parsed
                .flag(&["lead-id", "leader-id"])
                .ok_or_else(|| tr!("缺少 --lead-id"))?;
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
            let id = id.ok_or_else(|| tr!("缺少小队 id"))?;
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
            let id = id.ok_or_else(|| tr!("缺少小队 id"))?;
            ops::json(&ops::squad::set_members(
                db,
                &id,
                parsed.list_flag("member-ids"),
            )?)
        }
        ("project", "list") => ops::json(&ops::project::list(db)?),
        ("project", "get") => {
            let id = id.ok_or_else(|| tr!("缺少需求 id"))?;
            ops::json(&ops::project::get(db, &id)?)
        }
        ("project", "create") => ops::json(&ops::project::create(
            db,
            parsed
                .flag(&["name"])
                .ok_or_else(|| tr!("缺少 --name"))?,
            parsed.flag(&["description", "note"]).unwrap_or(""),
        )?),
        ("project", "update") => {
            let id = id.ok_or_else(|| tr!("缺少需求 id"))?;
            ops::json(&ops::project::update(
                db,
                &id,
                parsed.flag(&["name"]),
                parsed.flag(&["description", "note"]),
            )?)
        }
        ("workspace", "list") => {
            let rows = match parsed.flag(&["project", "project-id"]) {
                Some(project_id) => {
                    ops::require_project(db, project_id)?;
                    crate::commands::list_project_workspaces(db, project_id)?
                }
                None => crate::commands::list_workspaces(db)?,
            };
            ops::json(&rows)
        }
        ("workspace", "add") => add_workspace_cli(db, &parsed),
        ("automation", "list") => ops::json(&ops::automation::list(db)?),
        ("automation", "get") => {
            let id = id.ok_or_else(|| tr!("缺少自动化 id"))?;
            ops::json(&ops::automation::get(db, &id)?)
        }
        ("automation", "create") => create_automation(db, &parsed),
        ("channel", "list") => {
            let rows = crate::channel::store::list_installations(db)?;
            ops::json(&rows.iter().map(channel_json).collect::<Vec<_>>())
        }
        ("channel", "get") => {
            let id = id.ok_or_else(|| tr!("缺少渠道 id"))?;
            let installation = crate::channel::store::get_installation(db, &id)?
                .ok_or_else(|| tr!("渠道不存在"))?;
            ops::json(&channel_json(&installation))
        }
        ("channel", "enable") | ("channel", "disable") => {
            let id = id.ok_or_else(|| tr!("缺少渠道 id"))?;
            let enabled = verb == "enable";
            db.execute(
                "UPDATE channel_installations SET enabled=?1, updated_at=?2 WHERE id=?3",
                rusqlite::params![enabled as i64, chrono::Utc::now().to_rfc3339(), id],
            )
            .map_err(|error| error.to_string())?;
            ops::json(&serde_json::json!({ "id": id, "enabled": enabled }))
        }
        ("skill", "list") => ops::json(&ops::skill::list_scanned()),
        ("mcp", "list") => ops::json(&ops::skill::list_mcp()),
        ("skill", "get") => {
            let id = id.ok_or_else(|| tr!("缺少 skill id"))?;
            ops::json(&ops::skill::get(&id)?)
        }
        ("skill", "builtin") => {
            let locale = crate::app_locale::from_db(db);
            let genui = crate::commands::genui_enabled(db);
            ops::json(&ops::skill::builtin(locale, genui))
        }
        ("skill", "import") => {
            Err(tr!("第一期不支持 URL 导入。用 `multara skill list` 查看本机扫描结果。"))
        }
        ("runtime", "list") => ops::json(&runtime::detect()),
        ("session", "history") => {
            let session_id = id
                .or_else(|| env::var("MULTARA_SESSION_ID").ok())
                .ok_or_else(|| tr!("缺少会话 id 或 MULTARA_SESSION_ID"))?;
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
                .ok_or_else(|| tr!("缺少会话 id 或 MULTARA_SESSION_ID"))?;
            ops::json(&ops::session::resume(
                db,
                &session_id,
                parsed.flag(&["prompt"]),
            )?)
        }
        ("", _) => Err(help()),
        (resource, "") => Err(tr!(
            "缺少动词。试试 `multara {resource} --help`",
            resource = resource
        )),
        (resource, verb) => Err(tr!(
            "未知命令：multara {resource} {verb}",
            resource = resource,
            verb = verb
        )),
    }
}

fn ensure_write_allowed(
    db: &Connection,
    run_id: Option<&str>,
    resource: &str,
    verb: &str,
) -> Result<(), String> {
    const WRITE_COMMANDS: &[(&str, &str)] = &[
        ("task", "create"),
        ("task", "update"),
        ("task", "status"),
        ("task", "assign"),
        ("task", "comment"),
        ("agent", "create"),
        ("agent", "update"),
        ("squad", "create"),
        ("squad", "update"),
        ("squad", "member"),
        ("project", "create"),
        ("project", "update"),
        ("workspace", "add"),
        ("automation", "create"),
        ("channel", "enable"),
        ("channel", "disable"),
        ("session", "resume"),
    ];
    if !WRITE_COMMANDS.contains(&(resource, verb)) {
        return Ok(());
    }
    let Some(run_id) = run_id else {
        return Ok(());
    };
    let mode: Option<String> = db
        .query_row("SELECT mode FROM agent_runs WHERE id=?1", [run_id], |row| {
            row.get(0)
        })
        .optional()
        .map_err(|error| error.to_string())?;
    if mode.as_deref() == Some("plan") {
        return Err(tr!(
            "规划模式下不能执行写命令：multara {resource} {verb}",
            resource = resource,
            verb = verb
        ));
    }
    Ok(())
}

fn add_workspace_cli(db: &Connection, parsed: &ParsedArgs) -> Result<String, String> {
    let project_id = parsed
        .flag(&["project-id", "project"])
        .ok_or_else(|| tr!("缺少 --project"))?;
    let path = parsed
        .flag(&["path"])
        .ok_or_else(|| tr!("缺少 --path"))?;
    let mode = parsed
        .flag(&["mode"])
        .unwrap_or("worktree-new");
    ops::require_project(db, project_id)?;
    let git_command = crate::commands::setting_value(db, "gitExecutable", "git");
    let candidates = crate::workspace::discover(path, &git_command)?;
    if candidates.len() != 1 {
        let listed = candidates
            .iter()
            .map(|item| item.path.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        return Err(tr!(
            "这个目录里有多个仓库，请分别传入其中一个路径：\n{listed}",
            listed = listed
        ));
    }
    let candidate = &candidates[0];
    let (storage_mode, create_branch, checkout_name, base_ref) = match mode {
        "current" | "direct" => ("direct", false, None, None),
        "worktree-existing" | "existing" => {
            if candidate.kind != "git" {
                return Err(tr!("普通文件夹不能迁出 Worktree，请改用 --mode current"));
            }
            let branch = parsed
                .flag(&["branch"])
                .filter(|name| !name.trim().is_empty())
                .ok_or_else(|| tr!("迁出已有分支需要 --branch"))?;
            ("isolated", false, Some(branch), Some(branch))
        }
        "worktree-new" | "new" => {
            if candidate.kind != "git" {
                return Err(tr!("普通文件夹不能新建 Worktree，请改用 --mode current"));
            }
            let branch = parsed
                .flag(&["branch"])
                .filter(|name| !name.trim().is_empty())
                .ok_or_else(|| tr!("新建 Worktree 需要 --branch"))?;
            let base = parsed.flag(&["base", "base-ref"]).unwrap_or("HEAD");
            ("isolated", true, Some(branch), Some(base))
        }
        _ => return Err(tr!("--mode 只能是 worktree-new、worktree-existing 或 current")),
    };
    let configured = db
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'isolationRoot'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap_or_default();
    let prepared = crate::workspace::prepare(
        &app_data_dir()?,
        project_id,
        &candidate.path,
        storage_mode,
        crate::commands::expand_user_path(&configured),
        &git_command,
        base_ref,
        checkout_name,
        create_branch,
    )?;
    let workspace = crate::commands::commit_workspace(
        db,
        prepared,
        &git_command,
        create_branch,
        base_ref,
    )?;
    ops::json(&workspace)
}

fn create_automation(db: &mut Connection, parsed: &ParsedArgs) -> Result<String, String> {
    if parsed.flag(&["mode"]) == Some("plan") {
        return Err(tr!("自动化不支持规划模式"));
    }
    let name = parsed
        .flag(&["name"])
        .ok_or_else(|| tr!("缺少 --name"))?;
    let agent_id = parsed
        .flag(&["agent-id", "assignee", "assignee-id"])
        .ok_or_else(|| tr!("缺少 --agent-id"))?;
    let prompt = parsed
        .flag(&["prompt"])
        .ok_or_else(|| tr!("缺少 --prompt"))?;
    let project_id = parsed
        .flag(&["project-id", "project"])
        .ok_or_else(|| tr!("缺少 --project-id"))?;
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
    let trigger_type = if schedule.is_some() {
        "interval"
    } else {
        "manual"
    };
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
        chrono_tz::Tz::from_str(timezone).map_err(|_| tr!("--timezone 需要 IANA 时区"))?;
    let schedule = cron::Schedule::from_str(&format!("0 {}", expression.trim()))
        .map_err(|error| tr!("--schedule 需要 5 字段 Cron 表达式：{error}", error = error))?;
    schedule
        .after(&chrono::Utc::now().with_timezone(&timezone))
        .next()
        .map(|next| next.with_timezone(&chrono::Utc).to_rfc3339())
        .ok_or_else(|| tr!("Cron 无法计算下一次运行时间"))
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
        .ok_or_else(|| tr!("缺少 --description 或 --description-file"))?;
    ensure_description_file_allowed(path)?;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| {
            file.take(MAX_DESCRIPTION_FILE_BYTES + 1)
                .read_to_end(&mut bytes)
        })
        .map_err(|error| tr!("读取 description-file 失败：{error}", error = error))?;
    if bytes.len() as u64 > MAX_DESCRIPTION_FILE_BYTES {
        return Err(tr!(
            "description-file 超过 {size} KB",
            size = MAX_DESCRIPTION_FILE_BYTES / 1024
        ));
    }
    String::from_utf8(bytes)
        .map_err(|error| tr!("读取 description-file 失败：{error}", error = error))
}

const MAX_DESCRIPTION_FILE_BYTES: u64 = 1024 * 1024;

/// 描述内容之后能通过 `task get` 读出来，所以只允许读当前目录（智能体的工作区）和系统临时目录里的文件，
/// 不能拿它去读 `~/.ssh/*` 之类的任意路径。
fn ensure_description_file_allowed(path: &str) -> Result<(), String> {
    let target =
        fs::canonicalize(path).map_err(|error| tr!("读取 description-file 失败：{error}", error = error))?;
    let allowed = [env::current_dir().ok(), Some(env::temp_dir())]
        .into_iter()
        .flatten()
        .filter_map(|root| fs::canonicalize(root).ok())
        .any(|root| target.starts_with(root));
    if allowed {
        Ok(())
    } else {
        Err(tr!("description-file 只能位于当前目录或系统临时目录内"))
    }
}

fn open_db() -> Result<Connection, String> {
    let path = db_path()?;
    let db = database::open_file_light(&path)?;
    crate::app_locale::set_current(crate::app_locale::from_db(&db));
    Ok(db)
}

fn parse_line(value: &str, name: &str) -> Result<usize, String> {
    value
        .parse()
        .ok()
        .filter(|line| *line >= 1)
        .ok_or_else(|| tr!("--{name} 需要从 1 开始的行号", name = name))
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
        .ok_or_else(|| tr!("无法确定主目录，请设置 MULTARA_DB"))?;
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

fn changed_resources(
    current: &[String],
    kind: &str,
    add: bool,
    add_tokens: &[String],
    remove: bool,
    remove_tokens: &[String],
) -> Result<Vec<String>, String> {
    let mut ids = current.to_vec();
    if add {
        for id in resolve_resources(kind, add_tokens)? {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    }
    if remove {
        let gone = resolve_resources(kind, remove_tokens)?;
        ids.retain(|id| !gone.contains(id));
    }
    Ok(ids)
}

fn resolve_resources(kind: &str, tokens: &[String]) -> Result<Vec<String>, String> {
    if tokens.is_empty() {
        return Ok(Vec::new());
    }
    let scanned = crate::resources::scan();
    let mut ids = Vec::new();
    for token in tokens {
        let id = resolve_resource(&scanned, kind, token)?;
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    Ok(ids)
}

fn resolve_resource(
    resources: &[crate::models::GlobalResource],
    kind: &str,
    token: &str,
) -> Result<String, String> {
    let token = token.trim();
    let pool: Vec<_> = resources.iter().filter(|item| item.kind == kind).collect();
    if let Some(found) = pool.iter().find(|item| item.id == token) {
        return Ok(found.id.clone());
    }
    if let Some((owner, name)) = token.rsplit_once(':') {
        let matched: Vec<_> = pool
            .iter()
            .copied()
            .filter(|item| item.owner == owner && item.name == name)
            .collect();
        if matched.len() == 1 {
            return Ok(matched[0].id.clone());
        }
        if matched.len() > 1 {
            return Err(ambiguous_resource(kind, token, &matched));
        }
    }
    let matched: Vec<_> = pool.iter().copied().filter(|item| item.name == token).collect();
    if matched.len() == 1 {
        return Ok(matched[0].id.clone());
    }
    if matched.is_empty() {
        return Err(tr!("找不到 {kind}：{name}", kind = kind, name = token));
    }
    Err(ambiguous_resource(kind, token, &matched))
}

fn ambiguous_resource(
    kind: &str,
    token: &str,
    matched: &[&crate::models::GlobalResource],
) -> String {
    let ids = matched
        .iter()
        .map(|item| item.id.as_str())
        .collect::<Vec<_>>()
        .join("、");
    tr!(
        "找到多个 {kind}「{name}」，请改用完整 id：{ids}",
        kind = kind,
        name = token,
        ids = ids
    )
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

    use super::{execute, resolve_resource};
    use crate::database;
    use crate::models::GlobalResource;

    fn resource(kind: &'static str, owner: &str, name: &str) -> GlobalResource {
        GlobalResource {
            id: format!("{kind}:{owner}:{name}"),
            kind,
            owner: owner.into(),
            scope: "global",
            source_label: owner.into(),
            name: name.into(),
            description: String::new(),
            source_path: String::new(),
            enabled: true,
            updated_at: String::new(),
            transport: String::new(),
            summary: String::new(),
            needs_auth: false,
            config_status: String::new(),
        }
    }

    fn db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        database::migrate(&db).unwrap();
        database::seed(&db).unwrap();
        db
    }

    #[test]
    fn resolve_resource_by_name_owner_or_id() {
        let resources = vec![
            resource("skill", "cursor", "fsw-repo-catalog"),
            resource("skill", "multara", "shared"),
            resource("skill", "cursor", "shared"),
            resource("mcp", "cursor", "feishu-project"),
        ];
        assert_eq!(
            resolve_resource(&resources, "skill", "fsw-repo-catalog").unwrap(),
            "skill:cursor:fsw-repo-catalog"
        );
        assert_eq!(
            resolve_resource(&resources, "mcp", "cursor:feishu-project").unwrap(),
            "mcp:cursor:feishu-project"
        );
        assert_eq!(
            resolve_resource(&resources, "skill", "skill:cursor:shared").unwrap(),
            "skill:cursor:shared"
        );
        assert!(resolve_resource(&resources, "skill", "shared").is_err());
        assert!(resolve_resource(&resources, "mcp", "missing").is_err());
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
    fn workspace_add_keeps_a_plain_folder_on_its_real_path() {
        let mut db = db();
        let created = execute(
            &[
                "project".into(),
                "create".into(),
                "--name".into(),
                "需求".into(),
            ],
            &mut db,
        )
        .unwrap();
        let project_id = serde_json::from_str::<serde_json::Value>(&created).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let dir = tempfile::tempdir().unwrap();
        let added = execute(
            &[
                "workspace".into(),
                "add".into(),
                "--project".into(),
                project_id.clone(),
                "--path".into(),
                dir.path().display().to_string(),
                "--mode".into(),
                "current".into(),
            ],
            &mut db,
        )
        .unwrap();
        let workspace: serde_json::Value = serde_json::from_str(&added).unwrap();
        assert_eq!(workspace["projectId"], project_id);
        assert_eq!(workspace["isolationMode"], "direct");
        assert_eq!(workspace["kind"], "folder");
        let listed = execute(
            &[
                "workspace".into(),
                "list".into(),
                "--project".into(),
                project_id,
            ],
            &mut db,
        )
        .unwrap();
        let rows: serde_json::Value = serde_json::from_str(&listed).unwrap();
        assert_eq!(rows.as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn workspace_add_rejects_a_new_worktree_without_a_branch_name() {
        let mut db = db();
        let dir = tempfile::tempdir().unwrap();
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .arg("init")
            .status()
            .unwrap();
        assert!(status.success());
        db.execute(
            "INSERT INTO projects (id, name, created_at, opened_at) VALUES ('p', 'P', 'now', 'now')",
            [],
        )
        .unwrap();
        let error = execute(
            &[
                "workspace".into(),
                "add".into(),
                "--project".into(),
                "p".into(),
                "--path".into(),
                dir.path().display().to_string(),
                "--mode".into(),
                "worktree-new".into(),
            ],
            &mut db,
        )
        .unwrap_err();
        assert!(error.contains("--branch"), "{error}");
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
                "automation",
                "create",
                "--name",
                "日报",
                "--agent-id",
                "miku",
                "--prompt",
                "写日报",
                "--project-id",
                "p",
                "--schedule",
                schedule,
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
    fn automation_rejects_plan_mode() {
        let mut db = db();
        let error = execute(
            &[
                "automation".into(),
                "create".into(),
                "--mode".into(),
                "plan".into(),
            ],
            &mut db,
        )
        .unwrap_err();
        assert_eq!(error, "自动化不支持规划模式");
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

    #[test]
    fn ui_flag_wraps_lists_in_a_table_block() {
        let args = |list: &[&str]| list.iter().map(|item| item.to_string()).collect::<Vec<_>>();
        let raw = r#"[{"title":"修登录","status":"todo","agentId":"miku","priority":"high","updatedAt":"2026-11-01T00:00:00Z"}]"#.to_string();
        // 不带 --ui：原样
        assert_eq!(super::with_ui(&args(&["task", "list"]), raw.clone()), raw);
        let wrapped: serde_json::Value =
            serde_json::from_str(&super::with_ui(&args(&["task", "list", "--ui"]), raw.clone())).unwrap();
        assert_eq!(wrapped["items"][0]["title"], "修登录");
        assert_eq!(wrapped["_ui"]["type"], "table");
        assert_eq!(wrapped["_ui"]["props"]["rows"][0]["status"], "todo");
        assert_eq!(wrapped["_ui"]["props"]["columns"][4]["format"], "date");
        // 不支持的命令带 --ui 也原样
        assert_eq!(super::with_ui(&args(&["squad", "list", "--ui"]), raw.clone()), raw);
    }

    #[test]
    fn plan_run_cannot_execute_write_commands() {
        let db = db();
        db.execute(
            "INSERT INTO agent_runs
             (id, session_id, agent_id, runtime_id, assignment, status, created_at, mode)
             VALUES ('plan-run', 'welcome', 'miku', 'claude', 'plan', 'running', 'now', 'plan')",
            [],
        )
        .unwrap();
        assert!(
            super::ensure_write_allowed(&db, Some("plan-run"), "task", "create")
                .unwrap_err()
                .contains("规划模式")
        );
        assert!(super::ensure_write_allowed(&db, Some("plan-run"), "task", "list").is_ok());
    }

    #[test]
    fn description_file_rejects_paths_outside_cwd_and_temp() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        assert!(super::ensure_description_file_allowed(temp.path().to_str().unwrap()).is_ok());
        let home = std::env::var("HOME").unwrap_or_else(|_| "/etc".into());
        let outside = if std::path::Path::new("/etc/hosts").exists() {
            "/etc/hosts".to_string()
        } else {
            home
        };
        assert!(super::ensure_description_file_allowed(&outside).is_err());
    }
}
