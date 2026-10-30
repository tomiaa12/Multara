use std::{
    collections::HashMap,
    io::{BufReader, Read, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc, Arc, Mutex, OnceLock,
    },
    thread,
    time::{Duration, Instant},
};

use chrono::Utc;
use regex::Regex;
use rusqlite::params;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::{
    builtin_agents, builtin_skills, cli, conversation,
    database::{ActiveRun, AppState},
    models::{RunEvent, TimelineItem},
    resources, runtime, runtime_brief, session, snapshot,
    usage::{TokenUsage, UsageAccumulator},
};

/// `spawn_run` 退出时把监督标记清掉。panic 也会走 Drop，轮询线程据此收尸。
struct SupervisorGuard(Arc<AtomicBool>);

impl Drop for SupervisorGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

struct QueuedRun {
    id: String,
    session_id: String,
    turn_id: Option<String>,
    agent_id: String,
    runtime_id: String,
    assignment: String,
    instructions: String,
    model: String,
    agent_limit: u8,
    user_message_id: Option<String>,
    agent_name: String,
    system_key: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
struct NormalizedEvent {
    event_type: &'static str,
    call_id: Option<String>,
    tool: Option<String>,
    content: Option<String>,
    input: Option<Value>,
    output: Option<String>,
    replaces_final_text: bool,
    parent_call_id: Option<String>,
    status: Option<String>,
}

pub fn dispatch(app: AppHandle) {
    thread::spawn(move || dispatch_pending(app));
}

pub fn start_poller(app: AppHandle) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(2));
        reap_orphaned_runs(&app);
        dispatch_pending(app.clone());
    });
}

/// 监督线程已经不在，但子进程还挂在 `active_runs` 里。此时不能再等它自己 `finish`。
fn reap_orphaned_runs(app: &AppHandle) {
    let orphans: Vec<(String, Arc<Mutex<Child>>)> = {
        let state = app.state::<AppState>();
        let Ok(active) = state.active_runs.lock() else {
            return;
        };
        active
            .iter()
            .filter(|(_, run)| !run.supervisor_alive.load(Ordering::SeqCst))
            .map(|(id, run)| (id.clone(), run.child.clone()))
            .collect()
    };
    for (id, child) in orphans {
        let exit = child
            .lock()
            .ok()
            .and_then(|mut process| match process.try_wait() {
                Ok(Some(status)) => status.code(),
                Ok(None) => {
                    kill_process(&mut process);
                    process.wait().ok().and_then(|status| status.code())
                }
                Err(_) => None,
            });
        if let Ok(mut active) = app.state::<AppState>().active_runs.lock() {
            active.remove(&id);
        }
        let Ok((_, run)) = load_run(app, &id) else {
            continue;
        };
        let detail = match exit {
            Some(code) => format!("运行时已退出，退出码 {code}"),
            None => "运行时已退出".to_string(),
        };
        crate::diag::write(&format!("REAP orphan {id} {detail}"));
        finish(app, &run, "failed", "", Some(detail));
    }
}

pub fn generate_chat_title(app: &AppHandle, prompt: &str) -> Result<String, String> {
    let clipped: String = prompt.chars().take(800).collect();
    let instruction = format!(
        "根据用户的第一句提问，生成一个简短对话标题。\
         只输出标题本身，不要引号、句号、解释或 Markdown。\
         中文不超过 16 个字，英文不超过 6 个单词。\n\n{clipped}"
    );
    let title = generate_plain_line(app, &instruction, "对话标题", Duration::from_secs(8), None)?;
    let title = title
        .trim_matches(|ch: char| {
            matches!(ch, '"' | '\'' | '“' | '”' | '‘' | '’' | '。' | '！' | '!')
        })
        .trim();
    if title.is_empty() {
        return Err("运行时没有生成对话标题".to_string());
    }
    Ok(title.chars().take(24).collect())
}

/// 分支名 / Commit 生成：智能体或自定义 Chat Completions。
pub struct HelperSource {
    pub mode: String,
    pub agent_id: String,
    pub api_url: String,
    pub api_model: String,
    pub api_key: String,
}

pub fn generate_commit_message(
    app: &AppHandle,
    diff: &str,
    extra_prompt: &str,
    source: &HelperSource,
    locale: &str,
) -> Result<String, String> {
    let rules = extra_prompt.trim();
    let using_default = rules.is_empty();
    let rules_text = if using_default {
        crate::app_locale::default_commit_prompt(locale)
    } else {
        rules
    };
    let prompt = format!(
        "{rules_text}\n\n{}{diff}",
        crate::app_locale::commit_diff_wrapper(locale),
    );
    let message = generate_plain_line(
        app,
        &prompt,
        "提交信息",
        Duration::from_secs(60),
        Some(source),
    )?;
    let conventional = [
        "feat", "fix", "refactor", "docs", "test", "chore", "perf", "build", "ci", "style",
        "revert",
    ]
    .iter()
    .any(|prefix| {
        message.starts_with(&format!("{prefix}:")) || message.starts_with(&format!("{prefix}("))
    });
    // 默认规则要求 Conventional Commit；自定义提示词时不再强行加 chore:
    let message = if using_default && !conventional {
        format!("chore: {message}")
    } else {
        message.to_string()
    };
    Ok(message.chars().take(120).collect())
}

/// 用自定义提示词让运行时起一个分支名。空提示词不走这里，由前端按前缀本地拼接。
pub fn generate_branch_name(
    app: &AppHandle,
    requirement: &str,
    prefix: &str,
    prompt: &str,
    source: &HelperSource,
    locale: &str,
) -> Result<String, String> {
    let instruction = format!(
        "{prompt}{}",
        crate::app_locale::branch_name_wrapper(locale, prefix, requirement)
    );
    generate_plain_line(
        app,
        &instruction,
        "分支名",
        Duration::from_secs(45),
        Some(source),
    )
}

fn generate_plain_line(
    app: &AppHandle,
    prompt: &str,
    label: &str,
    timeout: Duration,
    source: Option<&HelperSource>,
) -> Result<String, String> {
    let text = match source {
        Some(src) if src.mode.trim() == "api" => {
            generate_text_via_api(prompt, label, timeout, src)?
        }
        Some(src) => {
            let agent_id = src.agent_id.trim();
            generate_text(
                app,
                prompt,
                label,
                timeout,
                if agent_id.is_empty() {
                    None
                } else {
                    Some(agent_id)
                },
            )?
        }
        None => generate_text(app, prompt, label, timeout, None)?,
    };
    let line = text
        .trim_matches('`')
        .trim_matches('"')
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    if line.is_empty() {
        return Err(format!("运行时没有生成{label}"));
    }
    Ok(line)
}

fn chat_completions_url(raw: &str) -> String {
    let url = raw.trim().trim_end_matches('/');
    if url.ends_with("/chat/completions") {
        url.to_string()
    } else {
        format!("{url}/chat/completions")
    }
}

fn message_content_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(parts) => {
            let mut out = String::new();
            for part in parts {
                if let Some(text) = part.get("text").and_then(Value::as_str) {
                    out.push_str(text);
                }
            }
            if out.is_empty() {
                None
            } else {
                Some(out)
            }
        }
        _ => None,
    }
}

fn generate_text_via_api(
    prompt: &str,
    label: &str,
    timeout: Duration,
    source: &HelperSource,
) -> Result<String, String> {
    let url = chat_completions_url(&source.api_url);
    if url.is_empty() || url == "/chat/completions" {
        return Err("请先填写 API URL".to_string());
    }
    let model = source.api_model.trim();
    if model.is_empty() {
        return Err("请先填写模型".to_string());
    }
    let api_key = source.api_key.trim();
    if api_key.is_empty() {
        return Err("请先填写 API Key".to_string());
    }
    let body = json!({
        "model": model,
        "messages": [{ "role": "user", "content": prompt }],
        "temperature": 0.2,
    })
    .to_string();
    let response = crate::http_client::request(
        "POST",
        &url,
        &[
            ("Content-Type".into(), "application/json".into()),
            ("Authorization".into(), format!("Bearer {api_key}")),
            ("Accept".into(), "application/json".into()),
        ],
        Some(&body),
        timeout.as_secs().max(1) as u32,
    )?;
    if !(200..300).contains(&response.status) {
        return Err(format!(
            "生成{label}失败（HTTP {}）：{}",
            response.status,
            truncate_error(response.body.trim(), 1_000)
        ));
    }
    let parsed: Value = serde_json::from_str(&response.body)
        .map_err(|error| format!("接口返回无法解析：{error}"))?;
    let content = parsed
        .pointer("/choices/0/message/content")
        .and_then(message_content_text)
        .or_else(|| {
            parsed
                .pointer("/choices/0/text")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default();
    let content = content.trim();
    if content.is_empty() {
        return Err(format!("接口没有生成{label}"));
    }
    Ok(content.to_string())
}

const SKILL_GENERATION_PROMPT: &str = "\
根据下面的需求写一个 Agent Skill 的完整 SKILL.md。要求：\n\
1. 以 YAML frontmatter 开头，包含 name（小写英文、数字和连字符）和 description（一句话说明做什么、什么时候用）。\n\
2. 正文用 Markdown 写清楚适用场景、具体步骤和注意事项，语言与需求一致。\n\
3. 只输出 SKILL.md 的内容，不要用代码块包裹，不要解释，不要调用工具。";

/// 让运行时根据需求写一份 SKILL.md。
pub fn generate_skill_markdown(app: &AppHandle, requirement: &str) -> Result<String, String> {
    let prompt = format!("{SKILL_GENERATION_PROMPT}\n\n需求：{}", requirement.trim());
    let text = generate_text(app, &prompt, "Skill", Duration::from_secs(180), None)?;
    let text = text.trim();
    let text = text
        .strip_prefix("```markdown")
        .or_else(|| text.strip_prefix("```md"))
        .or_else(|| text.strip_prefix("```"))
        .map(|rest| rest.trim_end().trim_end_matches("```").trim())
        .unwrap_or(text);
    let start = text.find("---").unwrap_or(0);
    Ok(text[start..].trim().to_string())
}

fn generate_text(
    app: &AppHandle,
    prompt: &str,
    label: &str,
    timeout: Duration,
    preferred_agent_id: Option<&str>,
) -> Result<String, String> {
    let agents = {
        let state = app.state::<AppState>();
        let db = state.lock_db()?;
        let mut statement = db
            .prepare(
                "SELECT id, runtime_id, instructions, model, max_concurrent_tasks, name, system_key
                 FROM agents ORDER BY id",
            )
            .map_err(|error| error.to_string())?;
        let agents = statement
            .query_map([], |row| {
                Ok(QueuedRun {
                    id: "commit-message".to_string(),
                    session_id: "commit-message".to_string(),
                    turn_id: None,
                    agent_id: row.get(0)?,
                    runtime_id: row.get(1)?,
                    assignment: String::new(),
                    instructions: row.get(2)?,
                    model: row.get(3)?,
                    agent_limit: row.get(4)?,
                    user_message_id: None,
                    agent_name: row.get(5)?,
                    system_key: row.get(6)?,
                })
            })
            .map_err(|error| error.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())?;
        agents
    };
    let preferred = preferred_agent_id
        .map(str::trim)
        .filter(|id| !id.is_empty());
    let mut selected = None;
    if let Some(id) = preferred {
        let mut agent = agents
            .into_iter()
            .find(|agent| agent.agent_id == id)
            .ok_or_else(|| format!("找不到指定的智能体：{id}"))?;
        agent.assignment = prompt.to_string();
        let execution = execution_for(app, &agent, prompt)
            .map_err(|_| format!("指定的智能体暂时不可用：{id}"))?;
        selected = Some((agent, execution));
    } else {
        for mut agent in agents {
            agent.assignment = prompt.to_string();
            if let Ok(execution) = execution_for(app, &agent, prompt) {
                selected = Some((agent, execution));
                break;
            }
        }
    }
    let (agent, (program, args, _)) =
        selected.ok_or_else(|| format!("没有可用的智能体运行时来生成{label}"))?;
    let provider = provider_for(app, &agent.runtime_id);
    let final_args = apply_options(
        replace_assignment(args, &prompt, &prompt),
        &provider,
        &agent.model,
        &[],
        None,
    );
    // 保留运行时的免确认参数。去掉之后 CLI 会弹出权限确认，提交信息就生成不出来。
    let prompt_via_stdin = runtime::prompt_via_stdin(&provider);
    let mut command = crate::process::command(program);
    command
        .args(final_args)
        .current_dir(std::env::temp_dir())
        .env("CI", "1")
        .stdin(if prompt_via_stdin {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("启动{label}生成器失败：{error}"))?;
    // 先起读线程再写 stdin：大提示词时子进程边读边写，先写后读会互相卡死。
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_reader = thread::spawn(move || read_capped(stdout));
    let stderr_reader = thread::spawn(move || read_capped(stderr));
    let stdin_writer = prompt_via_stdin.then(|| {
        let stdin = child.stdin.take();
        let payload = runtime::stdin_prompt(&provider, prompt).unwrap_or_default();
        thread::spawn(move || {
            stdin
                .ok_or_else(|| "运行时标准输入不可用".to_string())?
                .write_all(&payload)
                .map_err(|error| error.to_string())
        })
    });
    let started = Instant::now();
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            break status;
        }
        if started.elapsed() > timeout {
            kill_process(&mut child);
            timed_out = true;
            break child.wait().map_err(|error| error.to_string())?;
        }
        thread::sleep(GENERATE_POLL_INTERVAL);
    };
    let deadline = Instant::now() + READER_JOIN_TIMEOUT;
    let drained = wait_until_finished(&stdout_reader, deadline)
        && wait_until_finished(&stderr_reader, deadline);
    // 进程组里残留的孙进程（握着管道、或单纯没退出）一并清掉。
    kill_process_group(child.id());
    if !drained {
        crate::diag::write(&format!("GENERATE {label} readers stuck, process group killed"));
    }
    let grace = Instant::now() + READER_KILL_GRACE;
    let stdout = finish_reader(stdout_reader, grace);
    let stderr = finish_reader(stderr_reader, grace);
    if timed_out {
        return Err(format!("生成{label}超时"));
    }
    let stdin_error = stdin_writer
        .filter(|writer| writer.is_finished())
        .and_then(|writer| writer.join().ok())
        .and_then(Result::err);
    if let Some(error) = stdin_error {
        return Err(format!("发送{label}提示失败：{error}"));
    }
    if !status.success() {
        return Err(format!(
            "生成{label}失败：{}",
            truncate_error(stderr.trim(), 1_000)
        ));
    }
    let mut message = String::new();
    for line in stdout.lines() {
        for event in normalize_runtime_line(&provider, line, false) {
            if event.event_type != "text" {
                continue;
            }
            if event.replaces_final_text {
                message.clear();
            }
            if let Some(content) = event.content {
                message.push_str(&content);
            }
        }
    }
    let message = message.trim();
    if message.is_empty() {
        return Err(format!("运行时没有生成{label}"));
    }
    Ok(message.to_string())
}

/// 读到 EOF，最多保留 `MAX_LINE_BYTES`，其余丢弃但继续排空管道；非法 UTF-8 按 lossy 转换。
fn read_capped<R: Read>(stream: Option<R>) -> String {
    let Some(mut stream) = stream else {
        return String::new();
    };
    let mut bytes = Vec::new();
    let _ = stream
        .by_ref()
        .take(crate::process::MAX_LINE_BYTES as u64)
        .read_to_end(&mut bytes);
    let _ = std::io::copy(&mut stream, &mut std::io::sink());
    String::from_utf8_lossy(&bytes).into_owned()
}

pub fn notify_run(
    app: &AppHandle,
    run_id: &str,
    session_id: &str,
    agent_id: &str,
    status: &'static str,
) {
    let _ = app.emit("run-event", run_event(run_id, session_id, agent_id, status));
}

fn permission_prompt(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    let Ok(db) = state.lock_db() else {
        return false;
    };
    let value: String = db
        .query_row(
            "SELECT value FROM app_settings WHERE key='permissionMode'",
            [],
            |row| row.get(0),
        )
        .unwrap_or_default();
    matches!(value.trim(), "prompt" | "ask")
}

type StdinSlot = Arc<Mutex<Option<ChildStdin>>>;

/// 只在 `active_runs` 锁里拿出 stdin 槽位，写入和判断都在锁外做，慢 stdin 不会卡住整个执行器。
fn session_stdin_slots(app: &AppHandle, session_id: &str) -> Vec<(StdinSlot, String)> {
    let state = app.state::<AppState>();
    let Ok(active) = state.active_runs.lock() else {
        return Vec::new();
    };
    active
        .values()
        .filter(|run| run.session_id == session_id)
        .map(|run| (run.stdin.clone(), run.provider.clone()))
        .collect()
}

pub fn can_steer(app: &AppHandle, session_id: &str) -> bool {
    session_stdin_slots(app, session_id)
        .iter()
        .any(|(slot, _)| match slot.try_lock() {
            Ok(stdin) => stdin.is_some(),
            // 别的线程正在往里写，说明 stdin 还开着。
            Err(std::sync::TryLockError::WouldBlock) => true,
            Err(std::sync::TryLockError::Poisoned(_)) => false,
        })
}

pub fn steer(app: &AppHandle, session_id: &str, text: &str) -> Result<bool, String> {
    let Some((slot, provider)) = session_stdin_slots(app, session_id).into_iter().next() else {
        return Ok(false);
    };
    let mut stdin = slot
        .lock()
        .map_err(|_| "无法写入运行中的任务".to_string())?;
    let Some(stdin) = stdin.as_mut() else {
        return Ok(false);
    };
    let payload = runtime::stdin_prompt(&provider, text).unwrap_or_else(|| {
        let mut bytes = text.as_bytes().to_vec();
        bytes.push(b'\n');
        bytes
    });
    stdin
        .write_all(&payload)
        .and_then(|_| stdin.flush())
        .map_err(|error| format!("无法把后续消息发给运行中的智能体：{error}"))?;
    Ok(true)
}

/// 兼容旧调用方：不知道 run_id 时跨所有运行按 request_id 找。
pub fn resolve_permission(
    app: &AppHandle,
    request_id: &str,
    option_id: &str,
) -> Result<(), String> {
    resolve_run_permission(app, None, request_id, option_id)
}

/// ACP 的 request_id 是每个运行从小整数开始的，并发运行会撞号；有 run_id 时只在那个运行里找。
pub fn resolve_run_permission(
    app: &AppHandle,
    run_id: Option<&str>,
    request_id: &str,
    option_id: &str,
) -> Result<(), String> {
    let permissions: Vec<_> = {
        let state = app.state::<AppState>();
        let active = state
            .active_runs
            .lock()
            .map_err(|_| "无法访问运行中的任务".to_string())?;
        active
            .iter()
            .filter(|(id, _)| run_id.is_none_or(|run_id| run_id == id.as_str()))
            .map(|(_, run)| run.permissions.clone())
            .collect()
    };
    let sender = permissions.iter().find_map(|pending| {
        pending
            .lock()
            .ok()
            .and_then(|mut pending| pending.remove(request_id))
    });
    match sender {
        Some(sender) => {
            let _ = sender.send(option_id.to_string());
            Ok(())
        }
        None => Err("没有等待确认的权限请求".to_string()),
    }
}

pub fn compact_session(app: &AppHandle, session_id: &str) -> Result<String, String> {
    let prompt = {
        let state = app.state::<AppState>();
        let db = state.lock_db()?;
        crate::ops::chat::compact_history_text(&db, session_id)?
    };
    let summary = generate_text(app, &prompt, "对话摘要", Duration::from_secs(90), None)?;
    let state = app.state::<AppState>();
    let db = state.lock_db()?;
    let last = crate::ops::chat::last_message_id(&db, session_id)?;
    crate::ops::chat::save_compact_summary(&db, session_id, &summary, last.as_deref())?;
    Ok(summary)
}

pub fn stop(app: &AppHandle, run_id: &str) -> Result<(), String> {
    let (status, run) = load_run(app, run_id)?;
    if status != "queued" && status != "running" {
        return Err("任务已经结束".to_string());
    }
    // 先标 stopped，让 spawn 收尾时带着已流式输出的正文 finish；不要在这里用空内容覆盖气泡。
    {
        let state = app.state::<AppState>();
        let db = state.lock_db()?;
        let _ = db.execute(
            "UPDATE agent_runs SET status = 'stopped'
             WHERE id = ?1 AND status IN ('queued', 'running')",
            [&run.id],
        );
    }
    let child = {
        let state = app.state::<AppState>();
        state
            .active_runs
            .lock()
            .ok()
            .and_then(|mut active| active.remove(run_id))
    };
    if let Some(active) = &child {
        // steer 正卡在写 stdin 时拿不到锁：跳过中断消息直接杀进程，写入随之失败返回。
        if let Ok(mut stdin) = active.stdin.try_lock() {
            if let Some(stdin) = stdin.as_mut() {
                let _ = writeln!(
                    stdin,
                    r#"{{"type":"control_request","request":{{"subtype":"interrupt"}}}}"#
                );
                let _ = stdin.flush();
            }
            *stdin = None;
        }
        kill_child(&active.child);
    }
    if status == "queued" || child.is_none() {
        finish(app, &run, "stopped", "", None);
    }
    Ok(())
}

fn load_run(app: &AppHandle, run_id: &str) -> Result<(String, QueuedRun), String> {
    let state = app.state::<AppState>();
    let db = state.lock_db()?;
    db.query_row(
        "SELECT r.status, r.id, r.session_id, r.turn_id, r.agent_id, r.runtime_id, r.assignment,
                a.instructions, a.model, a.max_concurrent_tasks, a.name, a.system_key
         FROM agent_runs r
         JOIN agents a ON a.id = r.agent_id
         WHERE r.id = ?1",
        [run_id],
        |row| {
            Ok((
                row.get(0)?,
                QueuedRun {
                    id: row.get(1)?,
                    session_id: row.get(2)?,
                    turn_id: row.get(3)?,
                    agent_id: row.get(4)?,
                    runtime_id: row.get(5)?,
                    assignment: row.get(6)?,
                    instructions: row.get(7)?,
                    model: row.get(8)?,
                    agent_limit: row.get::<_, u8>(9)?.min(6),
                    user_message_id: None,
                    agent_name: row.get(10)?,
                    system_key: row.get(11)?,
                },
            ))
        },
    )
    .map_err(|_| "任务不存在".to_string())
}

const STOP_CHECK_INTERVAL: Duration = Duration::from_millis(500);
/// supervisor 轮询子进程很密，但查库确认停止不用这么勤；`stop()` 自己会直接杀进程。
const SUPERVISOR_STOP_CHECK_INTERVAL: Duration = Duration::from_secs(1);
const SUPERVISOR_POLL_INTERVAL: Duration = Duration::from_millis(120);
const READER_JOIN_TIMEOUT: Duration = Duration::from_secs(5);
/// 杀掉进程组之后再给读线程一点时间把最后的输出落库。
const READER_KILL_GRACE: Duration = Duration::from_secs(1);
const READER_POLL_INTERVAL: Duration = Duration::from_millis(20);
const GENERATE_POLL_INTERVAL: Duration = Duration::from_millis(80);
/// 用量写库至少间隔这么久；结束时总会再写一次最终值。
const USAGE_PERSIST_INTERVAL: Duration = Duration::from_secs(2);
/// stderr 的 error 事件按时间窗口或行数合并成一条。
const STDERR_BATCH_INTERVAL: Duration = Duration::from_millis(200);
const STDERR_BATCH_MAX_LINES: usize = 50;
/// 失败详情只用 stderr 末尾一段（`truncate_error` 取最后 4000 字），缓冲留够余量即可。
const STDERR_TAIL_BYTES: usize = 64 * 1024;
const STDERR_LINE_CHARS: usize = 32_000;
/// 模型连续收到这么多次工具报错还不改正，基本是在死循环，直接停掉，避免事件无限增长。
const MAX_CONSECUTIVE_TOOL_ERRORS: usize = 25;

/// 上游把工具参数切碎后，CLI 会产出没有工具名、input 只有 `__unparsedToolInput` 的残片。
fn is_garbage_tool_use(event: &NormalizedEvent) -> bool {
    if event.event_type != "tool_use" || event.tool.as_deref().is_some_and(|t| !t.trim().is_empty()) {
        return false;
    }
    match &event.input {
        Some(Value::Object(map)) => {
            !map.is_empty() && map.keys().all(|key| key == "__unparsedToolInput")
        }
        _ => false,
    }
}

fn is_tool_use_error(event: &NormalizedEvent) -> bool {
    event
        .output
        .as_deref()
        .or(event.content.as_deref())
        .is_some_and(|text| text.contains("<tool_use_error>"))
}

fn abort_tool_error_loop(
    app: &AppHandle,
    run_id: &str,
    session_id: &str,
    agent_id: &str,
    event_seq: &AtomicU32,
) {
    let message = format!("工具调用连续失败 {MAX_CONSECUTIVE_TOOL_ERRORS} 次，已自动停止。请检查模型或网关对工具参数的流式输出。");
    let seq = event_seq.fetch_add(1, Ordering::SeqCst) + 1;
    let item = timeline_item(
        seq,
        NormalizedEvent {
            event_type: "error",
            call_id: None,
            tool: None,
            content: Some(message.clone()),
            input: None,
            output: None,
            replaces_final_text: false,
            parent_call_id: None,
            status: None,
        },
    );
    persist_timeline_item(app, run_id, session_id, &item);
    let mut outgoing = run_event(run_id, session_id, agent_id, "running");
    outgoing.error = Some(message);
    outgoing.timeline_item = Some(item);
    let _ = app.emit("run-event", outgoing);
    let _ = stop(app, run_id);
}

fn is_stopped(app: &AppHandle, run_id: &str) -> bool {
    matches!(
        current_status(app, run_id).as_deref(),
        Some("stopped" | "failed")
    )
}

/// 子进程用 `process_group(0)` 起在独立进程组里，组 id 就是它的 pid。Windows 上没有进程组，什么都不做。
#[cfg_attr(not(unix), allow(unused_variables))]
fn kill_process_group(pid: u32) {
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .args(["-KILL", &format!("-{pid}")])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn kill_process(process: &mut Child) {
    let pid = process.id();
    kill_process_group(pid);
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = process.kill();
}

fn kill_child(child: &Arc<Mutex<Child>>) {
    if let Ok(mut process) = child.lock() {
        kill_process(&mut process);
        let _ = process.try_wait();
    }
}

/// 子进程退出后等读线程收尾。孙进程继承了管道时 EOF 永远不来，到截止时间就不再等。
fn wait_until_finished<T>(handle: &thread::JoinHandle<T>, deadline: Instant) -> bool {
    while !handle.is_finished() {
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(READER_POLL_INTERVAL);
    }
    true
}

fn finish_reader<T: Default>(handle: thread::JoinHandle<T>, deadline: Instant) -> T {
    if wait_until_finished(&handle, deadline) {
        handle.join().unwrap_or_default()
    } else {
        T::default()
    }
}

fn forget_child(app: &AppHandle, run_id: &str) {
    if let Ok(mut active) = app.state::<AppState>().active_runs.lock() {
        active.remove(run_id);
    }
}

fn abort_stopped(app: &AppHandle, run: &QueuedRun, child: Option<&Arc<Mutex<Child>>>) -> bool {
    if !is_stopped(app, &run.id) {
        return false;
    }
    if let Some(process) = child {
        kill_child(process);
        forget_child(app, &run.id);
    }
    true
}

fn dispatch_pending(app: AppHandle) {
    let _span = crate::diag::span("dispatch_pending");
    loop {
        let next = {
            let state = app.state::<AppState>();
            let db = match state.lock_db() {
                Ok(db) => db,
                Err(_) => return,
            };
            let mut statement = match db.prepare(
                "SELECT r.id, r.session_id, r.turn_id, r.agent_id, r.runtime_id, r.assignment,
                        a.instructions, a.model, a.max_concurrent_tasks, a.name, a.system_key
                 FROM agent_runs r
                 JOIN agents a ON a.id = r.agent_id
                 WHERE r.status = 'queued'
                   AND (SELECT COUNT(*) FROM agent_runs running
                        WHERE running.runtime_id = r.runtime_id AND running.status = 'running')
                       < COALESCE(
                         (SELECT max_concurrent FROM runtime_settings settings
                          WHERE settings.runtime_id = r.runtime_id),
                         1
                       )
                   AND (SELECT COUNT(*) FROM agent_runs running
                        WHERE running.agent_id = r.agent_id AND running.status = 'running')
                       < a.max_concurrent_tasks
                 ORDER BY r.created_at, r.id
                 LIMIT 1",
            ) {
                Ok(statement) => statement,
                Err(_) => return,
            };
            statement
                .query_row([], |row| {
                    Ok(QueuedRun {
                        id: row.get(0)?,
                        session_id: row.get(1)?,
                        turn_id: row.get(2)?,
                        agent_id: row.get(3)?,
                        runtime_id: row.get(4)?,
                        assignment: row.get(5)?,
                        instructions: row.get(6)?,
                        model: row.get(7)?,
                        agent_limit: row.get::<_, u8>(8)?.min(6),
                        user_message_id: None,
                        agent_name: row.get(9)?,
                        system_key: row.get(10)?,
                    })
                })
                .ok()
        };
        let Some(run) = next else {
            return;
        };

        let runtime_limit = match execution_for(&app, &run, &run.assignment) {
            Ok((_, _, limit)) => limit,
            Err(error) => {
                fail_without_process(&app, &run, &error);
                continue;
            }
        };
        if !reserve_slot(&app, &run, runtime_limit) {
            return;
        }
        spawn_run(app.clone(), run);
    }
}

fn reserve_slot(app: &AppHandle, run: &QueuedRun, runtime_limit: u8) -> bool {
    let state = app.state::<AppState>();
    let db = match state.lock_db() {
        Ok(db) => db,
        Err(_) => return false,
    };
    let configured_limit: u8 = db
        .query_row(
            "SELECT max_concurrent FROM runtime_settings WHERE runtime_id = ?1",
            [&run.runtime_id],
            |row| row.get(0),
        )
        .unwrap_or(runtime_limit)
        .clamp(1, 6);
    let runtime_running: u8 = db
        .query_row(
            "SELECT COUNT(*) FROM agent_runs WHERE runtime_id = ?1 AND status = 'running'",
            [&run.runtime_id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    let agent_running: u8 = db
        .query_row(
            "SELECT COUNT(*) FROM agent_runs WHERE agent_id = ?1 AND status = 'running'",
            [&run.agent_id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    if runtime_running >= configured_limit || agent_running >= run.agent_limit {
        return false;
    }
    db.execute(
        "UPDATE agent_runs SET status = 'running', started_at = ?1
         WHERE id = ?2 AND status = 'queued'",
        params![Utc::now().to_rfc3339(), run.id],
    )
    .is_ok_and(|affected| affected == 1)
}

fn spawn_run(app: AppHandle, mut run: QueuedRun) {
    thread::spawn(move || {
        crate::diag::write(&format!("RUN start {}", run.id));
        run.user_message_id = user_message_id_for_run(&app, &run.id, run.turn_id.as_deref());
        if abort_stopped(&app, &run, None) {
            return;
        }
        emit(&app, &run, "running", None, None, None);
        let (program, args, _) = match execution_for(&app, &run, &run.assignment) {
            Ok(execution) => execution,
            Err(error) => {
                finish(&app, &run, "failed", "", Some(error));
                return;
            }
        };
        let (cwd, workspace_context) = match workspace_context(&app, &run.session_id) {
            Ok(context) => context,
            Err(error) => {
                finish(&app, &run, "failed", "", Some(error));
                return;
            }
        };
        let launch_provider = provider_for(&app, &run.runtime_id);
        let (resource_context, mcp_config) =
            match resource_context(&app, &run.agent_id, &launch_provider, &run.assignment) {
                Ok(context) => context,
                Err(error) => {
                    finish(&app, &run, "failed", "", Some(error));
                    return;
                }
            };
        if abort_stopped(&app, &run, None) {
            return;
        }
        let (resume_id, history) = match conversation::prepare(
            &app,
            &run.session_id,
            &run.agent_id,
            &run.runtime_id,
            &launch_provider,
        ) {
            Ok(value) => value,
            Err(error) => {
                finish(&app, &run, "failed", "", Some(error));
                return;
            }
        };
        let prompt = {
            let locale = app
                .state::<AppState>()
                .lock_db()
                .ok()
                .map(|db| crate::app_locale::from_db(&db))
                .unwrap_or("zh-CN");
            let identity = if builtin_agents::is_miku_key(run.system_key.as_deref()) {
                builtin_agents::compose_miku_instructions(
                    &run.agent_name,
                    &run.instructions,
                    locale,
                )
            } else if run.instructions.trim().is_empty() {
                builtin_agents::generic_agent_identity(&run.agent_name, locale)
            } else {
                run.instructions.clone()
            };
            let skill_root = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::env::temp_dir())
                .join("builtin-skills");
            let _ = builtin_skills::materialize(&skill_root, locale);
            let skill_root_text = skill_root.to_string_lossy().to_string();
            let skill_names = builtin_skills::names();
            let brief = runtime_brief::build(runtime_brief::RuntimeBrief {
                agent_id: &run.agent_id,
                agent_name: &run.agent_name,
                identity_block: &identity,
                workspace: &workspace_context.0,
                skill_names: &skill_names,
                skill_root: &skill_root_text,
                user_skill_context: &resource_context,
                has_project_workspace: workspace_context.2,
            });
            conversation::build_prompt(
                &brief,
                &history,
                &run.assignment,
                resume_id.as_deref(),
                &launch_provider,
            )
        };
        let mcp_path = mcp_config
            .as_ref()
            .map(|file| file.path().to_string_lossy().to_string());
        let custom_launch = custom_runtime_launch(&app, &run.runtime_id).ok();
        let gateway = match custom_launch
            .as_ref()
            .filter(|launch| launch.kind == "gateway")
            .map(|launch| prepare_gateway(launch, &run.model))
            .transpose()
        {
            Ok(gateway) => gateway,
            Err(error) => {
                finish(&app, &run, "failed", "", Some(error));
                return;
            }
        };
        let launch_provider = custom_launch
            .as_ref()
            .map(|item| item.provider.as_str())
            .unwrap_or(run.runtime_id.as_str())
            .to_string();
        let mut launch_args = args;
        if let Some(gateway) = &gateway {
            let insertion = launch_args
                .iter()
                .position(|arg| arg == &run.assignment)
                .unwrap_or(launch_args.len());
            launch_args.splice(insertion..insertion, gateway.args.clone());
        }
        let cli_model = gateway
            .as_ref()
            .map(|gateway| gateway.cli_model.as_str())
            .unwrap_or(run.model.as_str());
        let final_args = conversation::apply_resume(
            apply_options(
                replace_assignment(launch_args, &run.assignment, &prompt),
                &launch_provider,
                cli_model,
                &workspace_context.1,
                mcp_path.as_deref(),
            ),
            &launch_provider,
            resume_id.as_deref(),
        );
        let protocol = runtime::protocol_driver(&launch_provider);
        let prompt_via_stdin = runtime::prompt_via_stdin(&launch_provider) || protocol.is_some();
        let stdin_payload = if protocol.is_some() {
            None
        } else {
            runtime::stdin_prompt(&launch_provider, &prompt)
        };
        let parser_id = if protocol.is_some() {
            "wire".to_string()
        } else {
            launch_provider.clone()
        };
        let mut child = match {
            let mut command = crate::process::command(program);
            command
                .args(final_args)
                .current_dir(&cwd)
                .env("CI", "1")
                .env("MULTARA_SESSION_ID", &run.session_id)
                .env("MULTARA_AGENT_ID", &run.agent_id);
            if let Ok(db) = app.state::<AppState>().lock_db() {
                if let Ok(task_id) = db.query_row(
                    "SELECT id FROM tasks WHERE session_id = ?1 LIMIT 1",
                    [&run.session_id],
                    |row| row.get::<_, String>(0),
                ) {
                    command.env("MULTARA_TASK_ID", task_id);
                }
            }
            if let Ok(data_dir) = app.path().app_data_dir() {
                let bin_dir = cli::ensure_shim(&data_dir);
                command.env("PATH", cli::path_env(&bin_dir));
                command.env("MULTARA_APP_DATA", &data_dir);
                command.env("MULTARA_DB", data_dir.join("multara.sqlite3"));
                let shim_name = if cfg!(windows) {
                    "multara.exe"
                } else {
                    "multara"
                };
                command.env("MULTARA_CLI", bin_dir.join(shim_name));
            }
            if let Some(launch) = &custom_launch {
                if launch.kind == "gateway" {
                    for key in runtime::managed_env_keys() {
                        command.env_remove(key);
                    }
                }
                for (key, value) in &launch.env {
                    command.env(key, value);
                }
            }
            if let Some(gateway) = &gateway {
                for (key, value) in &gateway.env {
                    command.env(key, value);
                }
            }
            #[cfg(unix)]
            {
                use std::os::unix::process::CommandExt;
                command.process_group(0);
            }
            command
                .stdin(if prompt_via_stdin {
                    Stdio::piped()
                } else {
                    Stdio::null()
                })
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
        } {
            Ok(child) => child,
            Err(error) => {
                finish(
                    &app,
                    &run,
                    "failed",
                    "",
                    Some(format!("启动运行时失败：{error}")),
                );
                return;
            }
        };
        let permissions = Arc::new(Mutex::new(HashMap::<String, mpsc::Sender<String>>::new()));
        let stdin_slot = Arc::new(Mutex::new(None));
        let protocol_stdout = if let Some(kind) = protocol {
            let stdout = child.stdout.take();
            let stdin = child.stdin.take();
            let prompt = prompt.clone();
            let workdir = cwd.clone();
            let (tx, rx) = mpsc::channel::<Vec<u8>>();
            let gate = crate::protocol::PermissionGate {
                prompt: permission_prompt(&app),
                pending: permissions.clone(),
                notify: {
                    let app = app.clone();
                    let session_id = run.session_id.clone();
                    let run_id = run.id.clone();
                    Some(Arc::new(move |request_id: &str, params: Option<&Value>| {
                        crate::channel::outbound::on_permission_request(
                            &app,
                            &session_id,
                            &run_id,
                            request_id,
                            params,
                        );
                    }))
                },
            };
            thread::spawn(move || {
                let (Some(stdout), Some(mut stdin)) = (stdout, stdin) else {
                    return;
                };
                let mut writer = crate::protocol::ByteSender(tx);
                crate::protocol::drive(
                    kind,
                    stdout,
                    &mut stdin,
                    &prompt,
                    &workdir,
                    &mut writer,
                    Some(gate),
                );
            });
            Some(crate::protocol::ByteReceiver(rx, Vec::new()))
        } else {
            None
        };
        let mut stdin_writer = stdin_payload.map(|payload| {
            let keep_open = matches!(launch_provider.as_str(), "claude" | "codebuddy");
            let stdin = child
                .stdin
                .take()
                .ok_or_else(|| "运行时标准输入不可用".to_string());
            thread::spawn(move || {
                let mut stdin = stdin?;
                stdin
                    .write_all(&payload)
                    .map_err(|error| format!("发送任务到运行时失败：{error}"))?;
                Ok(keep_open.then_some(stdin))
            })
        });
        let stdout = protocol_stdout
            .map(|stream| Box::new(stream) as Box<dyn Read + Send>)
            .or_else(|| {
                child
                    .stdout
                    .take()
                    .map(|stream| Box::new(stream) as Box<dyn Read + Send>)
            });
        let stderr = child.stderr.take();
        let child = Arc::new(Mutex::new(child));
        let supervisor_alive = Arc::new(AtomicBool::new(true));
        let _supervisor_guard = SupervisorGuard(supervisor_alive.clone());
        if let Ok(mut active) = app.state::<AppState>().active_runs.lock() {
            active.insert(
                run.id.clone(),
                ActiveRun {
                    child: child.clone(),
                    supervisor_alive,
                    session_id: run.session_id.clone(),
                    provider: launch_provider.clone(),
                    stdin: stdin_slot.clone(),
                    permissions: permissions.clone(),
                },
            );
        }
        let output = Arc::new(Mutex::new(String::new()));
        let stderr_output = Arc::new(Mutex::new(String::new()));
        let event_seq = Arc::new(AtomicU32::new(0));
        let usage = Arc::new(Mutex::new(UsageAccumulator::new(
            run.model.clone(),
            run.runtime_id.clone(),
        )));
        let stdout_reader = stdout.map(|stream| {
            read_stream(
                app.clone(),
                run.id.clone(),
                run.session_id.clone(),
                run.agent_id.clone(),
                run.runtime_id.clone(),
                parser_id.clone(),
                stream,
                output.clone(),
                event_seq.clone(),
                run.user_message_id.clone(),
                usage.clone(),
            )
        });
        let stderr_reader = stderr.map(|stream| {
            read_error_stream(
                app.clone(),
                run.id.clone(),
                run.session_id.clone(),
                run.agent_id.clone(),
                stream,
                stderr_output.clone(),
                event_seq,
            )
        });

        let timeout = run_timeout(&app);
        let started = Instant::now();
        let mut timed_out = false;
        // kill 只发一次，之后等 try_wait 回收，不要每个 tick 都 fork 一个 kill。
        let mut killed = false;
        let mut stdin_error: Option<String> = None;
        let mut last_stop_check = Instant::now();
        let pid = child.lock().map(|process| process.id()).ok();
        let status = loop {
            // stdin 写线程不在循环外 join：子进程不读大载荷时写会一直阻塞，超时和停止都得照常生效。
            if stdin_writer.as_ref().is_some_and(thread::JoinHandle::is_finished) {
                stdin_error = stdin_writer
                    .take()
                    .and_then(|writer| settle_stdin_writer(writer, &stdin_slot));
            }
            let stop_due = !killed && last_stop_check.elapsed() >= SUPERVISOR_STOP_CHECK_INTERVAL;
            if stop_due {
                last_stop_check = Instant::now();
            }
            let timeout_due = !killed && timeout.is_some_and(|limit| started.elapsed() >= limit);
            timed_out |= timeout_due;
            // 用户点停止、超时或发送任务失败：杀进程，等 try_wait，带着已有正文 finish。
            if !killed
                && (stdin_error.is_some() || timeout_due || (stop_due && is_stopped(&app, &run.id)))
            {
                killed = true;
                kill_child(&child);
            }
            let result = child
                .lock()
                .ok()
                .and_then(|mut child| child.try_wait().ok())
                .flatten();
            if let Some(status) = result {
                break status;
            }
            thread::sleep(SUPERVISOR_POLL_INTERVAL);
        };
        let readers: Vec<_> = [stdout_reader, stderr_reader].into_iter().flatten().collect();
        let deadline = Instant::now() + READER_JOIN_TIMEOUT;
        if !readers.iter().all(|reader| wait_until_finished(reader, deadline)) {
            crate::diag::write(&format!("RUN readers stuck {}, killing process group", run.id));
            kill_child(&child);
        }
        // 正常退出也清一遍进程组：CLI 退出后它起的 MCP 服务器、后台进程不该留下来。
        if let Some(pid) = pid {
            kill_process_group(pid);
        }
        let grace = Instant::now() + READER_KILL_GRACE;
        for reader in readers {
            finish_reader(reader, grace);
        }
        if let Ok(mut slot) = stdin_slot.lock() {
            *slot = None;
        }
        if let Ok(mut active) = app.state::<AppState>().active_runs.lock() {
            active.remove(&run.id);
        }
        let text = output.lock().map(|value| value.clone()).unwrap_or_default();
        let stderr_text = stderr_output
            .lock()
            .map(|value| value.clone())
            .unwrap_or_default();
        let stopped = current_status(&app, &run.id).as_deref() == Some("stopped");
        if timed_out {
            let minutes = timeout.map(|limit| limit.as_secs() / 60).unwrap_or(0);
            finish(
                &app,
                &run,
                "failed",
                &text,
                Some(format!("任务超过 {minutes} 分钟，已自动停止")),
            );
        } else if stopped {
            finish(&app, &run, "stopped", &text, None);
        } else if let Some(error) = stdin_error {
            finish(&app, &run, "failed", &text, Some(error));
        } else if status.success() {
            finish(&app, &run, "completed", &text, None);
        } else {
            let detail = stderr_text.trim();
            let error = if detail.is_empty() {
                format!("运行时退出码：{}", status.code().unwrap_or(-1))
            } else {
                format!(
                    "运行时退出码：{}\n{}",
                    status.code().unwrap_or(-1),
                    truncate_error(detail, 4000)
                )
            };
            finish(&app, &run, "failed", &text, Some(error));
        }
    });
}

type StdinWriter = thread::JoinHandle<Result<Option<ChildStdin>, String>>;

/// 写线程结束后，需要保持打开的 stdin（Claude / CodeBuddy）交给槽位；返回发送失败的原因。
fn settle_stdin_writer(writer: StdinWriter, slot: &Mutex<Option<ChildStdin>>) -> Option<String> {
    match writer.join() {
        Ok(Ok(stdin)) => {
            if let (Some(stdin), Ok(mut slot)) = (stdin, slot.lock()) {
                *slot = Some(stdin);
            }
            None
        }
        Ok(Err(error)) => Some(error),
        Err(_) => Some("发送任务到运行时的线程异常退出".to_string()),
    }
}

#[cfg(test)]
fn runtime_stdin_payload(runtime_id: &str, prompt: &str) -> Vec<u8> {
    runtime::stdin_prompt(runtime_id, prompt).unwrap_or_else(|| prompt.as_bytes().to_vec())
}

/// Claude / CodeBuddy 一轮结束时的 `{"type":"result",...}`。关掉 stdin 后进程才会退出。
fn is_stream_turn_result(parser_id: &str, value: &Value) -> bool {
    matches!(parser_id, "claude" | "codebuddy")
        && value.get("type").and_then(Value::as_str) == Some("result")
}

fn close_active_stdin(app: &AppHandle, run_id: &str) {
    let state = app.state::<AppState>();
    let Ok(active) = state.active_runs.lock() else {
        return;
    };
    let Some(stdin_slot) = active.get(run_id).map(|run| run.stdin.clone()) else {
        return;
    };
    drop(active);
    let Ok(mut stdin) = stdin_slot.lock() else {
        return;
    };
    *stdin = None;
}

fn truncate_error(value: &str, max_chars: usize) -> String {
    let chars = value.chars().collect::<Vec<_>>();
    if chars.len() <= max_chars {
        return value.to_string();
    }
    format!(
        "…{}",
        chars[chars.len() - max_chars..].iter().collect::<String>()
    )
}

/// `0` 或未设置 = 不限时（长任务可能跑很久）。正数按分钟封顶 1440。
fn run_timeout(app: &AppHandle) -> Option<Duration> {
    let state = app.state::<AppState>();
    let minutes = state
        .db
        .lock()
        .ok()
        .and_then(|db| {
            db.query_row(
                "SELECT value FROM app_settings WHERE key = 'runTimeoutMinutes'",
                [],
                |row| row.get::<_, String>(0),
            )
            .ok()
        })
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    if minutes == 0 {
        return None;
    }
    Some(Duration::from_secs(minutes.clamp(1, 1440) * 60))
}

#[allow(clippy::too_many_arguments)]
fn read_stream<R: Read + Send + 'static>(
    app: AppHandle,
    run_id: String,
    session_id: String,
    agent_id: String,
    runtime_id: String,
    parser_id: String,
    stream: R,
    output: Arc<Mutex<String>>,
    event_seq: Arc<AtomicU32>,
    message_id: Option<String>,
    usage: Arc<Mutex<UsageAccumulator>>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut previous: Option<TimelineItem> = None;
        let mut tool_files: HashMap<String, String> = HashMap::new();
        let mut consecutive_tool_errors = 0usize;
        let mut tool_error_loop = false;
        let mut last_conversation_id: Option<String> = None;
        let mut usage_dirty = false;
        let mut last_usage_persist = Instant::now();
        // 正文/思考是逐 token 到达的：先攒着，约 STREAM_FLUSH_INTERVAL 才落库并发一次事件，
        // 否则每个 token 都整段 upsert + 整段 IPC。
        let pending = Arc::new(Mutex::new(PendingStream::new()));
        let stream_done = Arc::new(AtomicBool::new(false));
        // 读线程 panic 时也要让 flusher 退出。
        let done_guard = StreamDone(Arc::clone(&stream_done));
        let flusher = {
            let (app, run_id, session_id, agent_id) =
                (app.clone(), run_id.clone(), session_id.clone(), agent_id.clone());
            let (pending, stream_done) = (Arc::clone(&pending), Arc::clone(&stream_done));
            thread::spawn(move || {
                while !stream_done.load(Ordering::Relaxed) {
                    thread::sleep(STREAM_FLUSH_INTERVAL);
                    if let Ok(mut pending) = pending.lock() {
                        if pending.due() {
                            pending.flush(&app, &run_id, &session_id, &agent_id);
                        }
                    }
                }
            })
        };
        let mut last_stop_check = Instant::now();
        'lines: for line in crate::process::BoundedLines(BufReader::new(stream)) {
            // 每行都查一次数据库会和写事件抢全局锁；停止信号半秒内到达即可。
            if last_stop_check.elapsed() >= STOP_CHECK_INTERVAL {
                last_stop_check = Instant::now();
                if is_stopped(&app, &run_id) {
                    break;
                }
            }
            let parsed = serde_json::from_str::<Value>(&line).ok();
            if let Some(value) = &parsed {
                if let Some(id) = conversation::id_from_value(value)
                    .filter(|id| last_conversation_id.as_ref() != Some(id))
                {
                    conversation::persist_id(&app, &session_id, &agent_id, &runtime_id, &id);
                    last_conversation_id = Some(id);
                }
                if crate::usage::mentions_usage(&line) {
                    if let Ok(mut acc) = usage.lock() {
                        usage_dirty |= acc.ingest_value(value);
                        if usage_dirty && last_usage_persist.elapsed() >= USAGE_PERSIST_INTERVAL {
                            persist_run_usage(&app, &run_id, &acc.finalize());
                            usage_dirty = false;
                            last_usage_persist = Instant::now();
                        }
                    }
                }
                // Claude/CodeBuddy 在 stream-json 下会等 stdin；收到 result 后关掉，否则一直「输入中」。
                if is_stream_turn_result(&parser_id, value) {
                    close_active_stdin(&app, &run_id);
                }
            }
            for event in normalize_runtime_value(&parser_id, &line, parsed) {
                if is_garbage_tool_use(&event) {
                    continue;
                }
                if event.event_type == "tool_result" {
                    if is_tool_use_error(&event) {
                        consecutive_tool_errors += 1;
                    } else {
                        consecutive_tool_errors = 0;
                    }
                    if consecutive_tool_errors >= MAX_CONSECUTIVE_TOOL_ERRORS {
                        tool_error_loop = true;
                        break 'lines;
                    }
                }
                if event.event_type == "tool_use" {
                    maybe_capture_file_snapshot(
                        &app,
                        &run_id,
                        &session_id,
                        &agent_id,
                        message_id.as_deref(),
                        &event,
                        &mut tool_files,
                    );
                }
                if let Some(chunk) = event.content.as_deref().filter(|_| event.event_type == "text") {
                    if let Ok(mut full) = output.lock() {
                        if event.replaces_final_text {
                            full.clear();
                        }
                        full.push_str(chunk);
                    }
                }
                let merge = previous
                    .as_ref()
                    .is_some_and(|item| should_merge_timeline(item, &event));
                if matches!(event.event_type, "text" | "thinking") {
                    let Ok(mut pending) = pending.lock() else {
                        continue;
                    };
                    // 同一段正文/思考原地追加，不再每个 token 复制整段内容。
                    if merge && pending.item.is_some() {
                        pending.append(event);
                    } else {
                        pending.flush(&app, &run_id, &session_id, &agent_id);
                        let seq = event_seq.fetch_add(1, Ordering::SeqCst) + 1;
                        pending.start(timeline_item(seq, event));
                        previous = pending.item.as_ref().map(timeline_shell);
                    }
                    if pending.due() {
                        pending.flush(&app, &run_id, &session_id, &agent_id);
                    }
                    continue;
                }
                let seq = match previous.as_ref().filter(|_| merge) {
                    Some(old) => old.seq,
                    None => event_seq.fetch_add(1, Ordering::SeqCst) + 1,
                };
                let mut item = timeline_item(seq, event);
                if let Some(old) = previous.as_ref().filter(|_| merge) {
                    item = merge_timeline_item(old, item);
                }
                if let Ok(mut pending) = pending.lock() {
                    pending.flush(&app, &run_id, &session_id, &agent_id);
                    pending.clear();
                }
                persist_timeline_item(&app, &run_id, &session_id, &item);
                // 只有 tool_use 的参数会继续拼接，需要完整保留；其余只留合并判断用的骨架。
                previous = Some(if item.event_type == "tool_use" {
                    item.clone()
                } else {
                    timeline_shell(&item)
                });
                let affected_file = (item.event_type == "tool_result")
                    .then(|| {
                        item.call_id
                            .as_ref()
                            .and_then(|call_id| tool_files.get(call_id).cloned())
                    })
                    .flatten();
                let mut outgoing = run_event(&run_id, &session_id, &agent_id, "running");
                outgoing.replaces_final_text = item.replaces_final_text;
                outgoing.error =
                    (item.event_type == "error").then(|| item.content.clone().unwrap_or_default());
                outgoing.timeline_item = Some(item);
                outgoing.affected_file = affected_file;
                let _ = app.emit("run-event", outgoing);
            }
        }
        drop(done_guard);
        let _ = flusher.join();
        if let Ok(mut pending) = pending.lock() {
            pending.flush(&app, &run_id, &session_id, &agent_id);
        }
        if let Ok(acc) = usage.lock() {
            persist_run_usage(&app, &run_id, &acc.finalize());
        }
        if tool_error_loop {
            abort_tool_error_loop(&app, &run_id, &session_id, &agent_id, &event_seq);
        }
    })
}

/// stderr 不逐 token 合并，不需要 flusher；逐行归入批次，按时间或行数合并成一条 error 事件。
fn read_error_stream<R: Read + Send + 'static>(
    app: AppHandle,
    run_id: String,
    session_id: String,
    agent_id: String,
    stream: R,
    output: Arc<Mutex<String>>,
    event_seq: Arc<AtomicU32>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut batch = String::new();
        let mut batch_lines = 0usize;
        let mut last_emit: Option<Instant> = None;
        let mut last_stop_check = Instant::now();
        for line in crate::process::BoundedLines(BufReader::new(stream)) {
            if last_stop_check.elapsed() >= STOP_CHECK_INTERVAL {
                last_stop_check = Instant::now();
                if is_stopped(&app, &run_id) {
                    break;
                }
            }
            if line.trim().is_empty() {
                continue;
            }
            let line = truncate_and_redact(&line, STDERR_LINE_CHARS);
            push_tail(&output, &line, STDERR_TAIL_BYTES);
            if !batch.is_empty() {
                batch.push('\n');
            }
            batch.push_str(&line);
            batch_lines += 1;
            // 首行立即发出；之后的刷屏行攒到间隔或行数上限再合并发送。
            if batch_lines >= STDERR_BATCH_MAX_LINES
                || last_emit.is_none_or(|at| at.elapsed() >= STDERR_BATCH_INTERVAL)
            {
                emit_error_batch(&app, &run_id, &session_id, &agent_id, &event_seq, &mut batch);
                batch_lines = 0;
                last_emit = Some(Instant::now());
            }
        }
        emit_error_batch(&app, &run_id, &session_id, &agent_id, &event_seq, &mut batch);
    })
}

fn emit_error_batch(
    app: &AppHandle,
    run_id: &str,
    session_id: &str,
    agent_id: &str,
    event_seq: &AtomicU32,
    batch: &mut String,
) {
    if batch.is_empty() {
        return;
    }
    let seq = event_seq.fetch_add(1, Ordering::SeqCst) + 1;
    let item = timeline_item(seq, normalized("error", Some(std::mem::take(batch))));
    persist_timeline_item(app, run_id, session_id, &item);
    let mut outgoing = run_event(run_id, session_id, agent_id, "running");
    outgoing.error = item.content.clone();
    outgoing.timeline_item = Some(item);
    let _ = app.emit("run-event", outgoing);
}

/// 只保留末尾 `max_bytes` 字节（按字符边界），供失败时拼进错误详情。
fn push_tail(output: &Mutex<String>, line: &str, max_bytes: usize) {
    let Ok(mut text) = output.lock() else {
        return;
    };
    if !text.is_empty() {
        text.push('\n');
    }
    text.push_str(line);
    if text.len() <= max_bytes {
        return;
    }
    let cut = (text.len() - max_bytes..text.len())
        .find(|index| text.is_char_boundary(*index))
        .unwrap_or(text.len());
    text.drain(..cut);
}

const STREAM_FLUSH_INTERVAL: Duration = Duration::from_millis(80);

struct StreamDone(Arc<AtomicBool>);

impl Drop for StreamDone {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// 只保留合并判断要用的字段，正文/参数/输出不复制。
fn timeline_shell(item: &TimelineItem) -> TimelineItem {
    TimelineItem {
        content: None,
        input: None,
        output: None,
        ..item.clone()
    }
}

/// 逐 token 到达的正文/思考：正在累积的 item（原地追加）+ 自上次发送以来新增的正文片段。
struct PendingStream {
    item: Option<TimelineItem>,
    chunk: String,
    dirty: bool,
    last_flush: Instant,
}

impl PendingStream {
    fn new() -> Self {
        Self {
            item: None,
            chunk: String::new(),
            dirty: false,
            last_flush: Instant::now(),
        }
    }

    fn start(&mut self, item: TimelineItem) {
        if item.event_type == "text" {
            self.chunk.push_str(item.content.as_deref().unwrap_or_default());
        }
        self.item = Some(item);
        self.dirty = true;
    }

    fn append(&mut self, event: NormalizedEvent) {
        let Some(item) = self.item.as_mut() else {
            return;
        };
        if let Some(content) = event.content.as_deref() {
            item.content.get_or_insert_with(String::new).push_str(content);
            if item.event_type == "text" {
                self.chunk.push_str(content);
            }
        }
        item.replaces_final_text |= event.replaces_final_text;
        item.tool = event.tool.or(item.tool.take());
        item.call_id = event.call_id.or(item.call_id.take());
        item.input = event.input.or(item.input.take());
        item.output = event.output.or(item.output.take());
        item.parent_call_id = event.parent_call_id.or(item.parent_call_id.take());
        item.status = event.status.or(item.status.take());
        self.dirty = true;
    }

    fn due(&self) -> bool {
        self.dirty && self.last_flush.elapsed() >= STREAM_FLUSH_INTERVAL
    }

    fn clear(&mut self) {
        self.item = None;
        self.chunk.clear();
        self.dirty = false;
    }

    fn flush(&mut self, app: &AppHandle, run_id: &str, session_id: &str, agent_id: &str) {
        self.last_flush = Instant::now();
        let Some(item) = self.item.as_ref().filter(|_| self.dirty) else {
            return;
        };
        self.dirty = false;
        persist_timeline_item(app, run_id, session_id, item);
        let mut outgoing = run_event(run_id, session_id, agent_id, "running");
        outgoing.replaces_final_text = item.replaces_final_text;
        // 前端遇到 replacesFinalText 会用 chunk 整段替换正文，只发增量会把前面的内容截掉。
        outgoing.chunk = (!self.chunk.is_empty()).then(|| std::mem::take(&mut self.chunk));
        if item.replaces_final_text && outgoing.chunk.is_some() {
            outgoing.chunk = item.content.clone();
        }
        outgoing.timeline_item = Some(item.clone());
        let _ = app.emit("run-event", outgoing);
    }
}

fn persist_run_usage(app: &AppHandle, run_id: &str, usage: &TokenUsage) {
    if usage.total_tokens() == 0 && usage.cost_usd <= 0.0 {
        return;
    }
    let state = app.state::<AppState>();
    let Ok(db) = state.lock_db() else {
        return;
    };
    let _ = db.execute(
        "UPDATE agent_runs
         SET input_tokens = ?1, output_tokens = ?2, cache_read_tokens = ?3,
             cache_write_tokens = ?4, cost_usd = ?5, usage_model = ?6
         WHERE id = ?7",
        params![
            usage.input_tokens,
            usage.output_tokens,
            usage.cache_read_tokens,
            usage.cache_write_tokens,
            usage.cost_usd,
            usage.model,
            run_id,
        ],
    );
}

fn finish(
    app: &AppHandle,
    run: &QueuedRun,
    status: &'static str,
    output: &str,
    error: Option<String>,
) {
    let _span = crate::diag::span("finish_run");
    crate::diag::write(&format!("RUN end {} {status}", run.id));
    let state = app.state::<AppState>();
    let completed_at = Utc::now();
    let mut elapsed_ms = None;
    let mut member_runs = Vec::new();
    let mut parent_resume: Option<(String, String, String)> = None;
    if let Ok(db) = state.lock_db() {
        elapsed_ms = db
            .query_row(
                "SELECT started_at FROM agent_runs WHERE id=?1",
                [&run.id],
                |row| row.get::<_, Option<String>>(0),
            )
            .ok()
            .flatten()
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(&value).ok())
            .map(|started| {
                completed_at
                    .signed_duration_since(started.with_timezone(&Utc))
                    .num_milliseconds()
                    .max(0) as u64
            });
        let updated = db
            .execute(
                "UPDATE agent_runs SET status = ?1, completed_at = ?2
                 WHERE id = ?3 AND status IN ('queued', 'running')",
                params![status, completed_at.to_rfc3339(), run.id],
            )
            .unwrap_or(0);
        if updated == 0 {
            // 状态已被标成 failed/stopped，但没有 completed_at，界面收不到结束事件。
            let still_open: bool = db
                .query_row(
                    "SELECT completed_at IS NULL FROM agent_runs
                     WHERE id=?1 AND status IN ('failed', 'stopped')",
                    [&run.id],
                    |row| row.get(0),
                )
                .unwrap_or(false);
            if !still_open {
                return;
            }
            let _ = db.execute(
                "UPDATE agent_runs SET completed_at=?1 WHERE id=?2 AND completed_at IS NULL",
                params![completed_at.to_rfc3339(), run.id],
            );
        }
        if let Some(detail) = error.as_deref() {
            let seq: u32 = db
                .query_row(
                    "SELECT COALESCE(MAX(seq), 0) + 1 FROM run_events WHERE run_id=?1",
                    [&run.id],
                    |row| row.get(0),
                )
                .unwrap_or(1);
            let item = TimelineItem {
                seq,
                event_type: "error",
                call_id: None,
                tool: None,
                content: Some(truncate_and_redact(detail, 32_000)),
                input: None,
                output: None,
                created_at: completed_at.to_rfc3339(),
                replaces_final_text: false,
                parent_call_id: None,
                status: None,
            };
            let _ = db.execute(
                "INSERT INTO run_events
                 (id, run_id, session_id, seq, event_type, content, input_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, 'error', ?5, 'null', ?6)
                 ON CONFLICT(run_id, seq) DO UPDATE SET content=excluded.content",
                params![
                    format!("{}:{seq}", run.id),
                    run.id,
                    run.session_id,
                    seq,
                    item.content,
                    item.created_at
                ],
            );
            save_terminal_message(
                &db,
                run,
                status,
                output,
                error.as_deref(),
                elapsed_ms,
                &completed_at.to_rfc3339(),
            );
        } else {
            save_terminal_message(
                &db,
                run,
                status,
                output,
                None,
                elapsed_ms,
                &completed_at.to_rfc3339(),
            );
        }
        if status == "completed" {
            if let Ok(created) =
                dispatch_squad_members(&db, &run.id, output, &completed_at.to_rfc3339())
            {
                member_runs = created;
            }
        }
        if let Some(automation_run_id) = run.session_id.strip_prefix("automation-run:") {
            let active: u32 = db
                .query_row(
                    "SELECT COUNT(*) FROM agent_runs
                     WHERE session_id=?1 AND status IN ('queued', 'running')",
                    [&run.session_id],
                    |row| row.get(0),
                )
                .unwrap_or(1);
            if active == 0 {
                let failed: u32 = db
                    .query_row(
                        "SELECT COUNT(*) FROM agent_runs
                         WHERE session_id=?1 AND status IN ('failed', 'stopped')",
                        [&run.session_id],
                        |row| row.get(0),
                    )
                    .unwrap_or(1);
                let automation_status = if failed == 0 { "completed" } else { "failed" };
                let reason_code = (failed > 0).then_some("runtime_failed");
                let _ = db.execute(
                    "UPDATE automation_runs SET status=?1, completed_at=?2, reason_code=?3,
                     failure_reason=?4 WHERE id=?5",
                    params![
                        automation_status,
                        Utc::now().to_rfc3339(),
                        reason_code,
                        error.as_deref(),
                        automation_run_id
                    ],
                );
                let _ = db.execute(
                    "UPDATE webhook_deliveries SET status=?1, reason_code=?2
                     WHERE automation_run_id=?3",
                    params![
                        if failed == 0 { "dispatched" } else { "failed" },
                        reason_code,
                        automation_run_id
                    ],
                );
                let _ =
                    crate::ops::automation::publish_result(&db, &run.session_id, automation_status);
            }
        }
        let _ = crate::ops::task::sync_from_run(
            &db,
            &run.session_id,
            status,
            &completed_at.to_rfc3339(),
        );
        if status == "completed" || status == "failed" {
            if let Ok(Some(resume)) = crate::ops::task::maybe_resume_parent(
                &db,
                &run.session_id,
                &completed_at.to_rfc3339(),
            ) {
                parent_resume = Some(resume);
            }
        }
    }
    if let Some(turn_id) = &run.turn_id {
        let _ = session::complete_turn(app, turn_id);
    }
    emit_deleted_snapshots(app, run);
    if let Ok(db) = app.state::<AppState>().lock_db() {
        let _ = crate::snapshot_db::cleanup_old_snapshots(&db);
    }
    emit(
        app,
        run,
        status,
        (status == "completed" && !output.trim().is_empty()).then(|| output.trim().to_string()),
        error,
        elapsed_ms,
    );
    for (run_id, agent_id) in &member_runs {
        notify_run(app, run_id, &run.session_id, agent_id, "queued");
    }
    if let Some((run_id, session_id, agent_id)) = &parent_resume {
        notify_run(app, run_id, session_id, agent_id, "queued");
    }
    crate::channel::outbound::on_run_finished(app, &run.session_id);
    dispatch(app.clone());
}

fn save_terminal_message(
    db: &rusqlite::Connection,
    run: &QueuedRun,
    status: &str,
    output: &str,
    failure_reason: Option<&str>,
    elapsed_ms: Option<u64>,
    created_at: &str,
) {
    let final_text = output.trim();
    let message_id = format!("run:{}", run.id);
    let prior = if status == "stopped" && final_text.is_empty() {
        db.query_row(
            "SELECT content FROM messages WHERE id = ?1",
            [&message_id],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty() && text != "运行已停止。")
    } else {
        None
    };
    let message_kind = if status == "completed" && final_text.is_empty() {
        "no_response"
    } else {
        "message"
    };
    // 停止时保留已经流式出来的正文，不要用「运行已停止。」盖掉。
    let content = match status {
        "completed" if final_text.is_empty() => "运行已完成，但运行时没有返回文本。",
        "completed" => final_text,
        "stopped" if !final_text.is_empty() => final_text,
        "stopped" => prior.as_deref().unwrap_or("运行已停止。"),
        _ if !final_text.is_empty() => final_text,
        _ => "这次运行没有成功完成。",
    };
    let quick_actions_json = "[]".to_string();
    let parent_agent_id: Option<String> = db
        .query_row(
            "SELECT parent.agent_id FROM agent_runs child
             JOIN agent_runs parent ON parent.id = child.parent_run_id
             WHERE child.id = ?1",
            [&run.id],
            |row| row.get(0),
        )
        .ok();
    let (audience_type, audience_id) = if parent_agent_id.is_some() {
        ("agent", parent_agent_id)
    } else {
        ("user", None)
    };
    let _ = db.execute(
        "INSERT INTO messages
         (id, session_id, turn_id, role, content, run_id, agent_id, audience_type, audience_id,
          failure_reason, elapsed_ms, message_kind, quick_actions_json, created_at)
         VALUES (?1, ?2, ?3, 'assistant', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
         ON CONFLICT(id) DO UPDATE SET
           content=excluded.content, failure_reason=excluded.failure_reason,
           elapsed_ms=excluded.elapsed_ms, message_kind=excluded.message_kind,
           quick_actions_json=excluded.quick_actions_json,
           agent_id=excluded.agent_id, audience_type=excluded.audience_type,
           audience_id=excluded.audience_id",
        params![
            format!("run:{}", run.id),
            run.session_id,
            run.turn_id,
            content,
            run.id,
            run.agent_id,
            audience_type,
            audience_id,
            failure_reason.map(|value| truncate_and_redact(value, 32_000)),
            elapsed_ms.map(|value| value as i64),
            message_kind,
            quick_actions_json,
            created_at
        ],
    );
}

fn dispatch_squad_members(
    db: &rusqlite::Connection,
    leader_run_id: &str,
    leader_output: &str,
    now: &str,
) -> Result<Vec<(String, String)>, String> {
    let (session_id, turn_id, leader_id, parent_run_id, original): (
        String,
        Option<String>,
        String,
        Option<String>,
        String,
    ) = db
        .query_row(
            "SELECT session_id, turn_id, agent_id, parent_run_id, assignment FROM agent_runs WHERE id = ?1",
            [leader_run_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .map_err(|error| error.to_string())?;
    if parent_run_id.is_some() {
        return Ok(Vec::new());
    }
    let (target_type, target_id): (String, String) = db
        .query_row(
            "SELECT target_type, target_id FROM chat_sessions WHERE id = ?1",
            [&session_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| error.to_string())?;
    if target_type != "squad" {
        return Ok(Vec::new());
    }
    let already: u32 = db
        .query_row(
            "SELECT COUNT(*) FROM agent_runs WHERE parent_run_id = ?1",
            [leader_run_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if already > 0 {
        return Ok(Vec::new());
    }
    let mut members = db
        .prepare("SELECT agent_id FROM squad_members WHERE squad_id = ?1 AND agent_id != ?2 ORDER BY agent_id")
        .map_err(|error| error.to_string())?;
    let member_ids = members
        .query_map(params![target_id, leader_id], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    if member_ids.is_empty() {
        return Ok(Vec::new());
    }
    let user_request = original
        .split_once("用户需求：\n")
        .map(|(_, rest)| rest.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or(original.trim());
    let plan = leader_output.trim();
    let spoken = if plan.is_empty() {
        "请按用户这句话处理你负责的部分，完成后向我汇报。".to_string()
    } else {
        "请按刚才的拆分处理你负责的部分，完成后向我汇报。".to_string()
    };
    let prompt = format!(
        "你是小队队员，向队长汇报。请根据队长说明完成你负责的部分。\n\n队长说明：\n{}\n\n用户需求：\n{user_request}",
        if plan.is_empty() { user_request } else { plan }
    );
    let mut created = Vec::new();
    for member_id in &member_ids {
        db.execute(
            "INSERT INTO messages
             (id, session_id, turn_id, role, content, agent_id, audience_type, audience_id, message_kind, created_at)
             VALUES (?1, ?2, ?3, 'assistant', ?4, ?5, 'agent', ?6, 'assignment', ?7)",
            params![
                uuid::Uuid::new_v4().to_string(),
                session_id,
                turn_id,
                spoken,
                leader_id,
                member_id,
                now
            ],
        )
        .map_err(|error| error.to_string())?;
        let runtime_id: String = db
            .query_row(
                "SELECT runtime_id FROM agents WHERE id = ?1",
                [member_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        let id = uuid::Uuid::new_v4().to_string();
        db.execute(
            "INSERT INTO agent_runs
             (id, session_id, turn_id, agent_id, runtime_id, parent_run_id, assignment, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'queued', ?8)",
            params![
                id,
                session_id,
                turn_id,
                member_id,
                runtime_id,
                leader_run_id,
                prompt,
                now
            ],
        )
        .map_err(|error| error.to_string())?;
        created.push((id, member_id.clone()));
    }
    Ok(created)
}

fn fail_without_process(app: &AppHandle, run: &QueuedRun, error: &str) {
    finish(app, run, "failed", "", Some(error.to_string()));
}

fn emit(
    app: &AppHandle,
    run: &QueuedRun,
    status: &'static str,
    chunk: Option<String>,
    error: Option<String>,
    elapsed_ms: Option<u64>,
) {
    let mut outgoing = run_event(&run.id, &run.session_id, &run.agent_id, status);
    outgoing.chunk = chunk;
    outgoing.error = error;
    outgoing.elapsed_ms = elapsed_ms;
    let _ = app.emit("run-event", outgoing);
}

fn run_event(run_id: &str, session_id: &str, agent_id: &str, status: &'static str) -> RunEvent {
    RunEvent {
        run_id: run_id.to_string(),
        session_id: session_id.to_string(),
        agent_id: agent_id.to_string(),
        status,
        chunk: None,
        error: None,
        timeline_item: None,
        replaces_final_text: false,
        elapsed_ms: None,
        event_kind: None,
        message_id: None,
        file_path: None,
        snapshot_content: None,
        affected_file: None,
        workspace_id: None,
    }
}

fn user_message_id_for_run(app: &AppHandle, run_id: &str, turn_id: Option<&str>) -> Option<String> {
    let state = app.state::<AppState>();
    let db = state.lock_db().ok()?;
    if let Some(turn_id) = turn_id {
        if let Ok(message_id) = db.query_row(
            "SELECT user_message_id FROM agent_turns WHERE id=?1",
            [turn_id],
            |row| row.get(0),
        ) {
            return Some(message_id);
        }
    }
    db.query_row(
        "SELECT id FROM messages
         WHERE session_id=(SELECT session_id FROM agent_runs WHERE id=?1)
           AND role='user'
           AND created_at <= (SELECT created_at FROM agent_runs WHERE id=?1)
         ORDER BY created_at DESC, rowid DESC LIMIT 1",
        [run_id],
        |row| row.get(0),
    )
    .ok()
}

fn maybe_capture_file_snapshot(
    app: &AppHandle,
    run_id: &str,
    session_id: &str,
    agent_id: &str,
    message_id: Option<&str>,
    event: &NormalizedEvent,
    tool_files: &mut HashMap<String, String>,
) {
    let Some(tool) = event.tool.as_deref() else {
        return;
    };
    if !snapshot::is_file_mutation_tool(tool, event.input.as_ref()) {
        return;
    }
    let Some(raw_path) = snapshot::extract_file_path(event.input.as_ref()) else {
        return;
    };
    if let Some(call_id) = event.call_id.as_deref() {
        tool_files.insert(call_id.to_string(), raw_path.clone());
    }
    let Some(message_id) = message_id else {
        return;
    };
    let captured = (|| {
        let state = app.state::<AppState>();
        // 读文件、sha256、zstd 都在全局 DB 锁外做，改大文件时不卡住其它写库操作。
        let (workspace, rel) = {
            let db = state.lock_db()?;
            let workspaces = session::session_workspaces(&db, session_id)?;
            let (workspace, rel) = snapshot::resolve_workspace_file(&workspaces, &raw_path)
                .ok_or_else(|| "没有可快照的工作区".to_string())?;
            if let Some(call_id) = event.call_id.as_deref() {
                tool_files.insert(call_id.to_string(), rel.clone());
            }
            let rel = snapshot::snapshot_rel_path(&rel)?;
            if rel.is_empty()
                || crate::snapshot_db::snapshot_exists(&db, message_id, &workspace.id, &rel)?
            {
                return Ok(None);
            }
            (workspace.clone(), rel)
        };
        let Some(prepared) = snapshot::prepare_snapshot(&workspace, &rel)? else {
            return Ok(None);
        };
        let db = state.lock_db()?;
        snapshot::save_prepared_snapshot(&db, message_id, session_id, &workspace.id, prepared)
    })();
    if let Ok(Some(snap)) = captured {
        let mut outgoing = run_event(run_id, session_id, agent_id, "running");
        outgoing.event_kind = Some("file_snapshot");
        outgoing.message_id = Some(message_id.to_string());
        outgoing.file_path = Some(snap.file_path);
        outgoing.snapshot_content = Some(snap.content);
        outgoing.workspace_id = Some(snap.workspace_id);
        let _ = app.emit("run-event", outgoing);
    }
}

fn emit_deleted_snapshots(app: &AppHandle, run: &QueuedRun) {
    let Some(message_id) = run.user_message_id.as_deref() else {
        return;
    };
    // 一次拿锁查出 (workspace_id, file_path, work_path)，不要在循环里逐行抢全局数据库锁。
    let rows = {
        let state = app.state::<AppState>();
        let Ok(db) = state.lock_db() else {
            return;
        };
        let mut statement = match db.prepare(
            "SELECT s.workspace_id, s.file_path, COALESCE(w.work_path, '')
             FROM turn_file_snapshots s
             LEFT JOIN workspaces w ON w.id = s.workspace_id
             WHERE s.message_id=?1 AND s.file_path != ''",
        ) {
            Ok(statement) => statement,
            Err(_) => return,
        };
        statement
            .query_map([message_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .ok()
            .and_then(|rows| rows.collect::<rusqlite::Result<Vec<_>>>().ok())
            .unwrap_or_default()
    };
    for (workspace_id, file_path, work_path) in rows {
        if work_path.is_empty() {
            continue;
        }
        if std::path::Path::new(&work_path).join(&file_path).exists() {
            continue;
        }
        let mut outgoing = run_event(&run.id, &run.session_id, &run.agent_id, "running");
        outgoing.event_kind = Some("file_deleted");
        outgoing.message_id = Some(message_id.to_string());
        outgoing.file_path = Some(file_path);
        outgoing.workspace_id = Some(workspace_id);
        let _ = app.emit("run-event", outgoing);
    }
}

fn current_status(app: &AppHandle, run_id: &str) -> Option<String> {
    let state = app.state::<AppState>();
    let db = state.lock_db().ok()?;
    db.query_row(
        "SELECT status FROM agent_runs WHERE id = ?1",
        [run_id],
        |row| row.get(0),
    )
    .ok()
}

fn workspace_context(
    app: &AppHandle,
    session_id: &str,
) -> Result<(String, (String, Vec<String>, bool)), String> {
    let state = app.state::<AppState>();
    let db = state.lock_db()?;
    let project_ids = session::linked_project_ids(&db, session_id)?;
    if project_ids.is_empty() {
        drop(db);
        let cwd = scratch_workdir(app, session_id)?;
        return Ok((
            cwd.clone(),
            (
                format!(
                    "这次运行没有关联项目。文件都写在这个工作目录里，不要修改目录以外的路径：\n{cwd}"
                ),
                vec![cwd],
                false,
            ),
        ));
    }
    let paths = session::session_workspaces(&db, session_id)?
        .into_iter()
        .map(|workspace| workspace.work_path)
        .collect::<Vec<_>>();
    let cwd = paths
        .first()
        .cloned()
        .ok_or_else(|| "没有可执行任务的项目目录，请先为需求添加 Git 项目或文件夹".to_string())?;
    Ok((
        cwd,
        (
            format!(
                "只能修改以下需求工作区目录。用户拖入或点名的工作区外文件可以阅读，不要修改那些路径：\n{}",
                paths.join("\n")
            ),
            paths,
            true,
        ),
    ))
}

/// 没关联项目时，给这次会话一块应用数据目录里的工作目录。同一会话重复运行沿用同一目录。
fn scratch_workdir(app: &AppHandle, session_id: &str) -> Result<String, String> {
    let session_id = session_id.trim();
    if session_id.is_empty() || session_id.contains(['/', '\\', '.']) || session_id.contains('\0') {
        return Err("会话标识无效，无法准备工作目录".to_string());
    }
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("task-workspaces")
        .join(session_id);
    std::fs::create_dir_all(&dir).map_err(|error| format!("无法准备工作目录：{error}"))?;
    Ok(dir.to_string_lossy().to_string())
}

fn apply_options(
    mut args: Vec<String>,
    runtime_id: &str,
    model: &str,
    workspace_paths: &[String],
    mcp_config_path: Option<&str>,
) -> Vec<String> {
    if runtime_id == "cursor" {
        if let Some(path) = workspace_paths.first() {
            args.push("--workspace".into());
            args.push(path.clone());
        }
    }
    if model != "auto"
        && matches!(
            runtime_id,
            "cursor"
                | "claude"
                | "codebuddy"
                | "codex"
                | "copilot"
                | "opencode"
                | "codearts"
                | "deveco"
                | "gemini"
                | "qwen"
                | "pi"
                | "omp"
        )
    {
        args.push("--model".into());
        args.push(model.into());
    }
    if matches!(runtime_id, "cursor" | "claude" | "codex") {
        for path in workspace_paths.iter().skip(1) {
            args.push("--add-dir".into());
            args.push(path.clone());
        }
    }
    if runtime_id == "claude" {
        if let Some(path) = mcp_config_path {
            args.push("--strict-mcp-config".into());
            args.push("--mcp-config".into());
            args.push(path.into());
        }
    }
    args
}

/// 消息里以 `/name` 开头（或单独成词）点名的 Skill 名称。
fn invoked_skill_names(message: &str) -> Vec<String> {
    message
        .split_whitespace()
        .filter_map(|word| word.strip_prefix('/'))
        .map(|name| {
            name.trim_end_matches([',', '，', '。', '.', ':', '：'])
                .to_string()
        })
        .filter(|name| !name.is_empty() && !name.contains('/'))
        .collect()
}

fn resource_context(
    app: &AppHandle,
    agent_id: &str,
    runtime_id: &str,
    message: &str,
) -> Result<(String, Option<tempfile::NamedTempFile>), String> {
    let state = app.state::<AppState>();
    let db = state.lock_db()?;
    let mut statement = db
        .prepare(
            "SELECT resource_id, kind FROM agent_resources WHERE agent_id = ?1 ORDER BY resource_id",
        )
        .map_err(|error| error.to_string())?;
    let assigned = statement
        .query_map([agent_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    drop(statement);
    drop(db);

    let scanned = resources::scan();
    let mut skill_sections = Vec::new();
    let mut skill_bytes = 0_u64;
    let mut mcp_ids = Vec::new();
    let mut skill_ids = Vec::new();
    for (resource_id, kind) in assigned {
        if kind == "mcp" {
            mcp_ids.push(resource_id);
        } else {
            skill_ids.push(resource_id);
        }
    }
    // 用户在消息里用 /name 点名的已启用 Skill 也一起注入（同名时优先 Multara 自己的）。
    for name in invoked_skill_names(message) {
        let mut matches: Vec<_> = scanned
            .iter()
            .filter(|resource| resource.kind == "skill" && resource.name == name)
            .collect();
        matches.sort_by_key(|resource| resource.owner != "multara");
        if let Some(resource) = matches.first() {
            if !skill_ids.contains(&resource.id) {
                skill_ids.push(resource.id.clone());
            }
        }
    }
    for resource_id in skill_ids {
        let Some(resource) = scanned.iter().find(|resource| resource.id == resource_id) else {
            continue;
        };
        let skill_path = std::path::Path::new(&resource.source_path).join("SKILL.md");
        let size = std::fs::metadata(&skill_path)
            .map_err(|error| error.to_string())?
            .len();
        if size > 128 * 1024 || skill_bytes + size > 256 * 1024 {
            return Err(format!("Skill 内容过大，无法安全注入：{}", resource.name));
        }
        let content = std::fs::read_to_string(skill_path).map_err(|error| error.to_string())?;
        skill_bytes += size;
        skill_sections.push(format!("### {}\n{}", resource.name, content));
    }
    let mut context = if skill_sections.is_empty() {
        "本次未授权任何 Multara Skill。".to_string()
    } else {
        format!(
            "本次只授权以下 Skills；不要调用其他 Skill：\n{}",
            skill_sections.join("\n\n")
        )
    };
    let mcp_file = if mcp_ids.is_empty() && runtime_id != "claude" {
        context.push_str("\n本次未由 Multara 注入任何 MCP。");
        None
    } else {
        let config = resources::selected_mcp_config(&mcp_ids)?;
        let names = mcp_ids
            .iter()
            .filter_map(|id| id.splitn(3, ':').nth(2))
            .collect::<Vec<_>>();
        if names.is_empty() {
            context.push_str("\n本次未授权任何 MCP。");
        } else {
            context.push_str(&format!("\n本次只授权以下 MCP：{}。", names.join("、")));
        }
        let mut file = tempfile::NamedTempFile::new().map_err(|error| error.to_string())?;
        file.write_all(
            serde_json::to_string_pretty(&config)
                .map_err(|error| error.to_string())?
                .as_bytes(),
        )
        .map_err(|error| error.to_string())?;
        Some(file)
    };
    Ok((context, mcp_file))
}

fn replace_assignment(mut args: Vec<String>, assignment: &str, prompt: &str) -> Vec<String> {
    if let Some(arg) = args.iter_mut().find(|arg| *arg == assignment) {
        *arg = prompt.to_string();
    }
    args
}

fn execution_for(
    app: &AppHandle,
    run: &QueuedRun,
    assignment: &str,
) -> Result<(std::path::PathBuf, Vec<String>, u8), String> {
    if !run.runtime_id.starts_with("custom:") {
        return runtime::execution(&run.runtime_id, assignment);
    }
    let launch = custom_runtime_launch(app, &run.runtime_id)?;
    let detected = runtime::detect_custom(
        run.runtime_id.clone(),
        launch.name.clone(),
        launch.command.clone(),
        launch.args.clone(),
        launch.limit,
        launch.provider.clone(),
        launch.protocol.clone(),
        launch.api_url.clone(),
        launch.models.clone(),
        launch.kind.clone(),
        String::new(),
    );
    let path = detected
        .path
        .map(std::path::PathBuf::from)
        .ok_or_else(|| format!("未找到自定义运行时命令：{}", detected.command))?;
    let args = runtime::compose_args(
        &launch.provider,
        &launch.args,
        assignment,
        false,
        "",
        "",
        &launch.wire_api,
    );
    Ok((path, args, launch.limit.clamp(1, 6)))
}

fn provider_for(app: &AppHandle, runtime_id: &str) -> String {
    if !runtime_id.starts_with("custom:") {
        return runtime_id.to_string();
    }
    custom_runtime_launch(app, runtime_id)
        .map(|launch| launch.provider)
        .unwrap_or_else(|_| runtime_id.to_string())
}

struct PreparedGateway {
    _proxy: crate::gateway_proxy::GatewayProxy,
    args: Vec<String>,
    env: std::collections::HashMap<String, String>,
    cli_model: String,
}

fn prepare_gateway(launch: &CustomLaunch, selected_model: &str) -> Result<PreparedGateway, String> {
    use crate::gateway_proxy::{ApiFormat, GatewayProxy, GatewayProxyConfig};

    let model = if selected_model.trim().is_empty() || selected_model == "auto" {
        launch.models.first().map(String::as_str).unwrap_or("")
    } else {
        selected_model.trim()
    };
    if model.is_empty() {
        return Err("自定义 API 中转站需要配置默认模型".into());
    }
    let client_format = ApiFormat::parse(
        runtime::native_api_format(&launch.provider),
        ApiFormat::Chat,
    );
    let upstream_format = ApiFormat::parse(&launch.api_format, client_format);
    let client_model = runtime::gateway_client_model(&launch.provider);
    let proxy = GatewayProxy::start(GatewayProxyConfig {
        client_format,
        upstream_format,
        api_url: launch.api_url.clone(),
        api_key: launch.api_key.clone(),
        key_style: launch.key_style.clone(),
        model: model.to_string(),
        client_model: client_model.to_string(),
        client_provider: launch.provider.clone(),
        options: launch.gateway_options.clone(),
    })?;
    let base_url = runtime::gateway_local_base_url(&launch.provider, proxy.origin());
    let env_model = if launch.provider == "opencode" {
        "default"
    } else {
        client_model
    };
    let env = runtime::gateway_env(
        &launch.provider,
        &base_url,
        proxy.token(),
        env_model,
        "bearer",
    );
    let mut args = runtime::gateway_args(
        &launch.provider,
        &base_url,
        "",
        runtime::native_api_format(&launch.provider),
    );
    args.extend(runtime::gateway_settings_args(
        &launch.provider,
        &base_url,
        proxy.token(),
    ));
    let cli_model = if launch.provider == "opencode" {
        format!("multara/{env_model}")
    } else {
        client_model.to_string()
    };
    Ok(PreparedGateway {
        _proxy: proxy,
        args,
        env,
        cli_model,
    })
}

struct CustomLaunch {
    name: String,
    command: String,
    args: Vec<String>,
    limit: u8,
    provider: String,
    protocol: String,
    api_url: String,
    api_key: String,
    models: Vec<String>,
    env: std::collections::HashMap<String, String>,
    kind: String,
    wire_api: String,
    api_format: String,
    key_style: String,
    gateway_options: crate::models::GatewayOptions,
}

fn custom_runtime_launch(app: &AppHandle, runtime_id: &str) -> Result<CustomLaunch, String> {
    if !runtime_id.starts_with("custom:") {
        return Err("不是自定义运行时".into());
    }
    let state = app.state::<AppState>();
    let db = state.lock_db()?;
    db.query_row(
        "SELECT c.name, c.command, c.args_json, COALESCE(s.max_concurrent, 1),
                c.provider, c.protocol, c.api_url, c.api_key, c.models_json, c.env_json,
                COALESCE(c.kind, 'profile'), COALESCE(c.wire_api, 'responses'),
                COALESCE(c.api_format, ''), COALESCE(c.key_style, 'bearer'),
                COALESCE(c.gateway_options_json, '{}')
         FROM custom_runtimes c
         LEFT JOIN runtime_settings s ON s.runtime_id = c.id
         WHERE c.id = ?1",
        [runtime_id],
        |row| {
            let args: Vec<String> =
                serde_json::from_str(&row.get::<_, String>(2)?).unwrap_or_default();
            let models: Vec<String> =
                serde_json::from_str(&row.get::<_, String>(8)?).unwrap_or_default();
            let mut env: std::collections::HashMap<String, String> =
                serde_json::from_str(&row.get::<_, String>(9)?).unwrap_or_default();
            let protocol: String = row.get(5)?;
            let api_url: String = row.get(6)?;
            let api_key: String = row.get(7)?;
            let provider: String = row.get(4)?;
            let kind: String = row.get(10)?;
            let wire_api: String = row.get(11)?;
            let api_format: String = row.get(12)?;
            let key_style: String = row.get(13)?;
            let gateway_options =
                serde_json::from_str(&row.get::<_, String>(14)?).unwrap_or_default();
            for key in runtime::managed_env_keys() {
                env.remove(*key);
            }
            Ok(CustomLaunch {
                name: row.get(0)?,
                command: row.get(1)?,
                args,
                limit: row.get(3)?,
                provider,
                protocol,
                api_url,
                api_key,
                models,
                env,
                kind,
                wire_api,
                api_format,
                key_style,
                gateway_options,
            })
        },
    )
    .map_err(|_| "自定义运行时不存在".to_string())
}

fn normalize_runtime_line(runtime_id: &str, line: &str, is_error: bool) -> Vec<NormalizedEvent> {
    if is_error {
        return (!line.trim().is_empty())
            .then(|| normalized("error", Some(truncate_and_redact(line, 32_000))))
            .into_iter()
            .collect();
    }
    normalize_runtime_value(runtime_id, line, serde_json::from_str(line).ok())
}

/// `parsed` 是调用方已经解析好的 `line`，避免同一行重复 `from_str`。
fn normalize_runtime_value(runtime_id: &str, line: &str, parsed: Option<Value>) -> Vec<NormalizedEvent> {
    // Codex 在非 TTY 下（Multara 把 stdin 接到 /dev/null）会往 stdout 打这行进度，
    // 不是模型回复；当成 text 会冒充成「报错后再出一堆身份说明」。
    if runtime_id == "codex" && is_codex_cli_noise(line) {
        return Vec::new();
    }
    let Some(mut value) = parsed else {
        return (!line.trim().is_empty())
            .then(|| normalized("text", Some(truncate_and_redact(line, 64_000))))
            .into_iter()
            .collect();
    };
    sanitize_value(&mut value);
    match runtime_id {
        "claude" | "codebuddy" => normalize_claude(&value),
        "cursor" => normalize_cursor(&value),
        "codex" => normalize_codex(&value),
        "qwen" | "gemini" => normalize_qwen_gemini(&value),
        "copilot" => normalize_copilot(&value),
        "opencode" | "codearts" | "deveco" => normalize_opencode(&value),
        "pi" | "omp" => normalize_pi(&value),
        "wire" => normalize_wire(&value),
        _ => normalize_generic(&value),
    }
}

fn normalize_claude(value: &Value) -> Vec<NormalizedEvent> {
    let parent = value
        .get("parent_tool_use_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(str::to_string);
    let mut events = normalize_claude_events(value);
    if parent.is_some() {
        for event in &mut events {
            event.parent_call_id = parent.clone();
            // 子代理的文字是它给父代理的中间产物，不能覆盖最终回复。
            if event.event_type == "text" {
                event.replaces_final_text = false;
                event.event_type = "thinking";
            }
        }
    }
    events
}

fn normalize_claude_events(value: &Value) -> Vec<NormalizedEvent> {
    let event_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if event_type == "assistant" {
        return value
            .pointer("/message/content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|block| match block.get("type").and_then(Value::as_str) {
                Some("text") => {
                    Some(normalized("text", text_at(block, &["/text"])).with_final_replacement())
                }
                Some("thinking") => Some(normalized(
                    "thinking",
                    text_at(block, &["/thinking", "/text"]),
                )),
                Some("tool_use") => Some(tool_use_event(block)),
                _ => None,
            })
            .collect();
    }
    if event_type == "content_block_start" {
        let block = value.get("content_block").unwrap_or(value);
        return match block.get("type").and_then(Value::as_str) {
            Some("text") => text_at(block, &["/text"])
                .map(|text| normalized("text", Some(text)))
                .into_iter()
                .collect(),
            Some("thinking") => vec![normalized(
                "thinking",
                text_at(block, &["/thinking", "/text"]),
            )],
            Some("tool_use") => vec![tool_use_event(block)],
            _ => Vec::new(),
        };
    }
    if event_type == "content_block_delta" {
        return match value.pointer("/delta/type").and_then(Value::as_str) {
            Some("text_delta") => vec![normalized("text", text_at(value, &["/delta/text"]))],
            Some("thinking_delta") => {
                vec![normalized("thinking", text_at(value, &["/delta/thinking"]))]
            }
            Some("input_json_delta") => vec![NormalizedEvent {
                event_type: "tool_use",
                call_id: value
                    .pointer("/content_block/id")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                tool: value
                    .pointer("/content_block/name")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                content: text_at(value, &["/delta/partial_json"]),
                input: None,
                output: None,
                replaces_final_text: false,
                parent_call_id: None,
                status: None,
            }],
            _ => Vec::new(),
        };
    }
    if event_type == "user" {
        return value
            .pointer("/message/content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|block| block.get("type").and_then(Value::as_str) == Some("tool_result"))
            .map(tool_result_event)
            .collect();
    }
    Vec::new()
}

fn normalize_cursor(value: &Value) -> Vec<NormalizedEvent> {
    let event_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match event_type {
        "assistant" | "assistant_message" | "message" => text_at(
            value,
            &[
                "/message/content/0/text",
                "/message/content",
                "/content",
                "/text",
            ],
        )
        .map(|text| normalized("text", Some(text)).with_final_replacement())
        .into_iter()
        .collect(),
        "text" | "text_delta" => text_at(value, &["/text", "/delta/text", "/content"])
            .map(|text| normalized("text", Some(text)))
            .into_iter()
            .collect(),
        "thinking" | "reasoning" => {
            vec![normalized(
                "thinking",
                text_at(value, &["/text", "/content", "/delta/text"]),
            )]
        }
        "tool" | "tool_call" | "tool_calls" | "tool_use" => normalize_cursor_tool(value),
        "tool_result" | "terminal_result" | "command_result" => vec![tool_result_event(value)],
        _ => normalize_generic(value),
    }
}

fn normalize_cursor_tool(value: &Value) -> Vec<NormalizedEvent> {
    if let Some(event) = cursor_nested_tool_event(value) {
        return vec![event];
    }
    let subtype = value.get("subtype").and_then(Value::as_str).unwrap_or("");
    if subtype == "completed" {
        vec![tool_result_event(value)]
    } else if subtype == "progress" {
        Vec::new()
    } else {
        vec![tool_use_event(value)]
    }
}

fn cursor_nested_tool_event(value: &Value) -> Option<NormalizedEvent> {
    let parts = parse_cursor_tool(value)?;
    let subtype = value.get("subtype").and_then(Value::as_str).unwrap_or("");
    if subtype == "progress" {
        return None;
    }
    if subtype == "completed" {
        Some(NormalizedEvent {
            event_type: "tool_result",
            call_id: parts.call_id,
            tool: Some(parts.name),
            content: None,
            input: parts.args,
            output: parts
                .result
                .map(|result| truncate_and_redact(&json_preview(&result), 32_000)),
            replaces_final_text: false,
            parent_call_id: None,
            status: None,
        })
    } else {
        Some(NormalizedEvent {
            event_type: "tool_use",
            call_id: parts.call_id,
            tool: Some(parts.name),
            content: None,
            input: parts.args,
            output: None,
            replaces_final_text: false,
            parent_call_id: None,
            status: None,
        })
    }
}

struct CursorToolParts {
    name: String,
    call_id: Option<String>,
    args: Option<Value>,
    result: Option<Value>,
}

fn parse_cursor_tool(value: &Value) -> Option<CursorToolParts> {
    let envelope = value.get("tool_call")?.as_object()?;
    let call_id = cursor_call_id(value, envelope);
    if let Some(function) = envelope.get("function") {
        if let Some(name) = function.get("name").and_then(Value::as_str) {
            if !name.trim().is_empty() {
                return Some(CursorToolParts {
                    name: cursor_title_tool(name),
                    call_id,
                    args: parse_tool_args(function.get("arguments")),
                    result: envelope.get("result").cloned(),
                });
            }
        }
    }
    let key = cursor_tool_payload_key(envelope)?;
    let payload = envelope.get(&key)?;
    Some(CursorToolParts {
        name: cursor_title_tool(key.trim_end_matches("ToolCall")),
        call_id,
        args: payload.get("args").cloned(),
        result: payload.get("result").cloned(),
    })
}

fn cursor_tool_payload_key(envelope: &serde_json::Map<String, Value>) -> Option<String> {
    let mut keys: Vec<&String> = envelope
        .keys()
        .filter(|key| key.len() > "ToolCall".len() && key.ends_with("ToolCall"))
        .collect();
    keys.sort();
    keys.into_iter().next().cloned()
}

fn cursor_call_id(value: &Value, envelope: &serde_json::Map<String, Value>) -> Option<String> {
    text_at(value, &["/call_id", "/id", "/tool_call_id"])
        .or_else(|| {
            envelope
                .get("toolCallId")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .map(cursor_first_line)
        .filter(|id| !id.is_empty())
}

fn cursor_first_line(id: String) -> String {
    id.split('\n').next().unwrap_or(&id).trim().to_string()
}

fn cursor_title_tool(name: &str) -> String {
    let trimmed = name.trim();
    let mut chars = trimmed.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn parse_tool_args(value: Option<&Value>) -> Option<Value> {
    let value = value?;
    if let Some(text) = value.as_str() {
        return serde_json::from_str(text)
            .ok()
            .or_else(|| Some(Value::String(text.to_string())));
    }
    Some(value.clone())
}

fn json_preview(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn is_codex_cli_noise(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.eq_ignore_ascii_case("Reading additional input from stdin...")
        || trimmed.starts_with("Reading additional input from stdin")
}

fn normalize_codex(value: &Value) -> Vec<NormalizedEvent> {
    let event_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !matches!(
        event_type,
        "item.started" | "item.updated" | "item.completed"
    ) {
        return normalize_generic(value);
    }
    let item = value.get("item").unwrap_or(value);
    match item.get("type").and_then(Value::as_str).unwrap_or_default() {
        "agent_message" => text_at(item, &["/text", "/content"])
            .map(|text| {
                let event = normalized("text", Some(text));
                if event_type == "item.completed" {
                    event.with_final_replacement()
                } else {
                    event
                }
            })
            .into_iter()
            .collect(),
        "reasoning" => vec![normalized(
            "thinking",
            text_at(item, &["/text", "/content"]),
        )],
        "command_execution" | "mcp_tool_call" | "tool_call" => {
            if event_type == "item.completed" {
                vec![tool_result_event(item)]
            } else {
                vec![tool_use_event(item)]
            }
        }
        _ => Vec::new(),
    }
}

fn normalize_qwen_gemini(value: &Value) -> Vec<NormalizedEvent> {
    let event_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match event_type {
        "tool_call" | "tool_use" | "functionCall" => vec![tool_use_event(value)],
        "tool_result" | "functionResponse" => vec![tool_result_event(value)],
        "thinking" | "reasoning" => vec![normalized(
            "thinking",
            text_at(value, &["/text", "/content"]),
        )],
        _ => normalize_generic(value),
    }
}

fn normalize_copilot(value: &Value) -> Vec<NormalizedEvent> {
    let event_type = value.get("type").and_then(Value::as_str).unwrap_or("");
    let data = value.get("data").unwrap_or(value);
    match event_type {
        "assistant.message" => {
            let mut events = Vec::new();
            if let Some(thinking) =
                text_at(data, &["/reasoningText"]).filter(|text| !text.is_empty())
            {
                events.push(normalized("thinking", Some(thinking)));
            }
            if let Some(text) = text_at(data, &["/content"]).filter(|text| !text.is_empty()) {
                events.push(normalized("text", Some(text)).with_final_replacement());
            }
            if let Some(tools) = data.get("toolRequests").and_then(Value::as_array) {
                for tool in tools {
                    events.push(NormalizedEvent {
                        event_type: "tool_use",
                        call_id: tool
                            .get("toolCallId")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        tool: tool.get("name").and_then(Value::as_str).map(str::to_string),
                        content: None,
                        input: tool.get("arguments").cloned(),
                        output: None,
                        replaces_final_text: false,
                        parent_call_id: None,
                        status: None,
                    });
                }
            }
            events
        }
        "assistant.message_delta" => text_at(data, &["/deltaContent"])
            .filter(|text| !text.is_empty())
            .map(|text| normalized("text", Some(text)))
            .into_iter()
            .collect(),
        "assistant.reasoning" | "assistant.reasoning_delta" => {
            text_at(data, &["/content", "/delta"])
                .filter(|text| !text.is_empty())
                .map(|text| normalized("thinking", Some(text)))
                .into_iter()
                .collect()
        }
        "tool.execution_complete" => vec![NormalizedEvent {
            event_type: "tool_result",
            call_id: data
                .get("toolCallId")
                .and_then(Value::as_str)
                .map(str::to_string),
            tool: None,
            content: text_at(data, &["/result/content", "/error/message"]),
            input: None,
            output: None,
            replaces_final_text: false,
            parent_call_id: None,
            status: None,
        }],
        _ => Vec::new(),
    }
}

fn normalize_opencode(value: &Value) -> Vec<NormalizedEvent> {
    match value.get("type").and_then(Value::as_str).unwrap_or("") {
        "text" => text_at(value, &["/part/text"])
            .filter(|text| !text.is_empty())
            .map(|text| normalized("text", Some(text)))
            .into_iter()
            .collect(),
        "tool_use" => {
            let call_id = value
                .pointer("/part/callID")
                .and_then(Value::as_str)
                .map(str::to_string);
            let mut events = vec![NormalizedEvent {
                event_type: "tool_use",
                call_id: call_id.clone(),
                tool: value
                    .pointer("/part/tool")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                content: None,
                input: value.pointer("/part/state/input").cloned(),
                output: None,
                replaces_final_text: false,
                parent_call_id: None,
                status: None,
            }];
            if let Some(output) =
                text_at(value, &["/part/state/output"]).filter(|text| !text.is_empty())
            {
                events.push(NormalizedEvent {
                    event_type: "tool_result",
                    call_id,
                    tool: None,
                    content: Some(output),
                    input: None,
                    output: None,
                    replaces_final_text: false,
                    parent_call_id: None,
                    status: None,
                });
            }
            events
        }
        "error" => text_at(value, &["/error/data/message", "/error/name"])
            .map(|text| normalized("error", Some(text)))
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}

fn normalize_pi(value: &Value) -> Vec<NormalizedEvent> {
    match value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
    {
        "message_update" => text_at(
            value,
            &[
                "/assistantMessageEvent/delta",
                "/assistantMessageEvent/content",
            ],
        )
        .map(|text| normalized("text", Some(text)))
        .into_iter()
        .collect(),
        "tool_execution_start" => vec![NormalizedEvent {
            event_type: "tool_use",
            call_id: text_at(value, &["/toolCallId"]),
            tool: text_at(value, &["/toolName"]),
            content: None,
            input: value.get("args").cloned(),
            output: None,
            replaces_final_text: false,
            parent_call_id: None,
            status: None,
        }],
        "tool_execution_end" => text_at(value, &["/result"])
            .map(|output| NormalizedEvent {
                event_type: "tool_result",
                call_id: text_at(value, &["/toolCallId"]),
                tool: text_at(value, &["/toolName"]),
                content: Some(output),
                input: None,
                output: None,
                replaces_final_text: false,
                parent_call_id: None,
                status: None,
            })
            .into_iter()
            .collect(),
        "error" => text_at(value, &["/message", "/errorMessage"])
            .map(|text| normalized("error", Some(text)))
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}

fn normalize_wire(value: &Value) -> Vec<NormalizedEvent> {
    match value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
    {
        "text" => text_at(value, &["/text"])
            .map(|text| normalized("text", Some(text)))
            .into_iter()
            .collect(),
        "thinking" => text_at(value, &["/thinking"])
            .map(|text| normalized("thinking", Some(text)))
            .into_iter()
            .collect(),
        "tool_use" => vec![NormalizedEvent {
            event_type: "tool_use",
            call_id: text_at(value, &["/id"]),
            tool: text_at(value, &["/name"]),
            content: None,
            input: value.get("input").cloned(),
            output: None,
            replaces_final_text: false,
            parent_call_id: None,
            status: None,
        }],
        "tool_result" => vec![NormalizedEvent {
            event_type: "tool_result",
            call_id: text_at(value, &["/call_id"]),
            tool: None,
            content: text_at(value, &["/output"]),
            input: None,
            output: None,
            replaces_final_text: false,
            parent_call_id: None,
            status: None,
        }],
        "error" => text_at(value, &["/error"])
            .map(|text| normalized("error", Some(text)))
            .into_iter()
            .collect(),
        "plan" => vec![NormalizedEvent {
            event_type: "plan",
            call_id: None,
            tool: Some("plan".into()),
            content: None,
            input: value.get("input").cloned(),
            output: None,
            replaces_final_text: false,
            parent_call_id: None,
            status: None,
        }],
        "permission" => vec![NormalizedEvent {
            event_type: "permission",
            call_id: text_at(value, &["/id"]),
            tool: text_at(value, &["/tool"]),
            content: None,
            input: Some(json!({
                "options": value.get("options").cloned().unwrap_or(Value::Null),
                "toolCall": value.get("input").cloned().unwrap_or(Value::Null)
            })),
            output: None,
            replaces_final_text: false,
            parent_call_id: None,
            status: Some("pending".into()),
        }],
        _ => Vec::new(),
    }
}

fn normalize_generic(value: &Value) -> Vec<NormalizedEvent> {
    let event_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if matches!(
        event_type,
        "result" | "system" | "user" | "turn.started" | "turn.completed"
    ) {
        return Vec::new();
    }
    text_at(
        value,
        &[
            "/message/content/0/text",
            "/message/content",
            "/content",
            "/text",
            "/event/delta/text",
            "/delta/text",
            "/item/text",
            "/candidates/0/content/parts/0/text",
        ],
    )
    .map(|text| normalized("text", Some(text)))
    .into_iter()
    .collect()
}

fn normalized(event_type: &'static str, content: Option<String>) -> NormalizedEvent {
    NormalizedEvent {
        event_type,
        call_id: None,
        tool: None,
        content,
        input: None,
        output: None,
        replaces_final_text: false,
        parent_call_id: None,
        status: None,
    }
}

impl NormalizedEvent {
    fn with_final_replacement(mut self) -> Self {
        self.replaces_final_text = true;
        self
    }
}

fn tool_use_event(value: &Value) -> NormalizedEvent {
    NormalizedEvent {
        event_type: "tool_use",
        call_id: text_at(value, &["/id", "/call_id", "/tool_call_id", "/item/id"]),
        tool: text_at(
            value,
            &["/name", "/tool", "/tool_name", "/function/name", "/command"],
        ),
        content: None,
        input: value
            .get("input")
            .or_else(|| value.get("arguments"))
            .or_else(|| value.pointer("/function/arguments"))
            .cloned(),
        output: None,
        replaces_final_text: false,
        parent_call_id: None,
        status: None,
    }
}

fn tool_result_event(value: &Value) -> NormalizedEvent {
    let raw_output = text_at(
        value,
        &[
            "/output",
            "/content",
            "/result",
            "/aggregated_output",
            "/text",
        ],
    )
    .or_else(|| {
        let blocks = value.get("content")?.as_array()?;
        let text = blocks
            .iter()
            .filter_map(|block| match block.get("type").and_then(Value::as_str) {
                Some("text") => block
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                Some("image") => Some("[图片]".to_string()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        (!text.is_empty()).then_some(text)
    });
    let failed = value.get("is_error").and_then(Value::as_bool) == Some(true)
        || matches!(
            value.get("status").and_then(Value::as_str),
            Some("failed" | "error")
        );
    NormalizedEvent {
        event_type: "tool_result",
        call_id: text_at(
            value,
            &[
                "/tool_use_id",
                "/call_id",
                "/tool_call_id",
                "/id",
                "/item/id",
            ],
        ),
        tool: text_at(value, &["/name", "/tool", "/tool_name", "/command"]),
        content: None,
        input: None,
        output: raw_output.map(|text| truncate_and_redact(&text, 32_000)),
        replaces_final_text: false,
        parent_call_id: None,
        status: Some(if failed { "error" } else { "ok" }.to_string()),
    }
}

fn text_at(value: &Value, pointers: &[&str]) -> Option<String> {
    pointers.iter().find_map(|pointer| {
        value
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(|text| truncate_and_redact(text, 64_000))
    })
}

const REDACTED: &str = "[已隐藏敏感信息]";

/// 先截断再脱敏：只对保留下来的那段跑正则。脱敏只替换已知的密钥形态，不整行抹掉正文。
fn truncate_and_redact(value: &str, max_chars: usize) -> String {
    match value.char_indices().nth(max_chars) {
        Some((end, _)) => format!("{}\n…（内容已截断）", redact_secrets(&value[..end])),
        None => redact_secrets(value),
    }
}

fn redact_secrets(value: &str) -> String {
    static PATTERNS: OnceLock<Vec<(Regex, String)>> = OnceLock::new();
    let patterns = PATTERNS.get_or_init(|| {
        [
            (r"\bsk-[A-Za-z0-9_\-]{16,}", REDACTED.to_string()),
            (r"\bgh[pousr]_[A-Za-z0-9]{20,}", REDACTED.to_string()),
            (r"\bgithub_pat_[A-Za-z0-9_]{20,}", REDACTED.to_string()),
            (r"(?i)\b(bearer)\s+[A-Za-z0-9._~+/=\-]{8,}", format!("$1 {REDACTED}")),
            (
                r#"(?i)("(?:api_?key|apikey|password|passwd|secret|client_secret|token|access_token|refresh_token|authorization)"\s*:\s*)"[^"]*""#,
                format!("$1\"{REDACTED}\""),
            ),
            (
                r#"(?i)\b(password|passwd|token|api_?key|apikey|secret|client_secret|access_token)=[^\s&"']+"#,
                format!("$1={REDACTED}"),
            ),
            (
                r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?(?:-----END [A-Z ]*PRIVATE KEY-----|\z)",
                REDACTED.to_string(),
            ),
        ]
        .into_iter()
        .filter_map(|(pattern, replacement)| Some((Regex::new(pattern).ok()?, replacement)))
        .collect()
    });
    let mut text = value.to_string();
    for (pattern, replacement) in patterns {
        if pattern.is_match(&text) {
            text = pattern.replace_all(&text, replacement.as_str()).into_owned();
        }
    }
    text
}

/// 归一化（小写、去 `_`/`-`）后整键匹配：`max_tokens`、`token_count` 这类计数字段不算敏感。
fn is_sensitive_key(key: &str) -> bool {
    let normalized = key
        .chars()
        .filter(|c| !matches!(c, '_' | '-'))
        .collect::<String>()
        .to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "token"
            | "accesstoken"
            | "refreshtoken"
            | "idtoken"
            | "authtoken"
            | "sessiontoken"
            | "bearertoken"
            | "apikey"
            | "xapikey"
            | "secret"
            | "clientsecret"
            | "secretkey"
            | "secretaccesskey"
            | "awssecretaccesskey"
            | "password"
            | "passwd"
            | "authorization"
            | "privatekey"
    )
}

fn sanitize_value(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if is_sensitive_key(key) {
                    *child = Value::String(REDACTED.to_string());
                } else {
                    sanitize_value(child);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(sanitize_value),
        _ => {}
    }
}

fn timeline_item(seq: u32, event: NormalizedEvent) -> TimelineItem {
    TimelineItem {
        seq,
        event_type: event.event_type,
        call_id: event.call_id,
        tool: event.tool,
        content: event.content,
        input: event.input,
        output: event.output,
        created_at: Utc::now().to_rfc3339(),
        replaces_final_text: event.replaces_final_text,
        parent_call_id: event.parent_call_id,
        status: event.status,
    }
}

fn should_merge_timeline(previous: &TimelineItem, event: &NormalizedEvent) -> bool {
    if event.replaces_final_text && event.event_type == "text" {
        return false;
    }
    if matches!(event.event_type, "text" | "thinking") && previous.event_type == event.event_type {
        return true;
    }
    if previous.event_type != "tool_use" || event.event_type != "tool_use" {
        return false;
    }
    match (&previous.call_id, &event.call_id) {
        (Some(left), Some(right)) => left == right,
        (Some(_), None) => event.tool.is_none() && event.input.is_none(),
        _ => false,
    }
}

fn merge_timeline_item(previous: &TimelineItem, mut item: TimelineItem) -> TimelineItem {
    item.replaces_final_text = previous.replaces_final_text || item.replaces_final_text;
    if item.tool.is_none() {
        item.tool = previous.tool.clone();
    }
    if item.call_id.is_none() {
        item.call_id = previous.call_id.clone();
    }
    let mut combined = previous.content.clone().unwrap_or_default();
    combined.push_str(item.content.as_deref().unwrap_or_default());
    // 流式工具参数是半截 JSON，没闭合的不必尝试解析。
    let parsed = combined
        .trim_end()
        .ends_with(['}', ']'])
        .then(|| serde_json::from_str::<Value>(&combined).ok())
        .flatten();
    item.input = parsed.or(item.input).or_else(|| previous.input.clone());
    if !combined.is_empty() {
        item.content = Some(combined);
    }
    if item.output.is_none() {
        item.output = previous.output.clone();
    }
    if item.parent_call_id.is_none() {
        item.parent_call_id = previous.parent_call_id.clone();
    }
    if item.status.is_none() {
        item.status = previous.status.clone();
    }
    item
}

fn persist_timeline_item(app: &AppHandle, run_id: &str, session_id: &str, item: &TimelineItem) {
    let input_json = item
        .input
        .as_ref()
        .and_then(|value| serde_json::to_string(value).ok());
    let state = app.state::<AppState>();
    let Ok(db) = state.lock_db() else {
        crate::diag::write(&format!("RUN_EVENT persist {run_id}:{} skipped: db lock failed", item.seq));
        return;
    };
    let result = db.execute(
        "INSERT INTO run_events
             (id, run_id, session_id, seq, event_type, call_id, tool, content, input_json, output, created_at,
              parent_call_id, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(run_id, seq) DO UPDATE SET
               call_id=excluded.call_id, tool=excluded.tool, content=excluded.content,
               input_json=excluded.input_json, output=excluded.output,
               parent_call_id=excluded.parent_call_id, status=excluded.status",
        params![
            format!("{run_id}:{}", item.seq),
            run_id,
            session_id,
            item.seq,
            item.event_type,
            item.call_id,
            item.tool,
            item.content,
            input_json,
            item.output,
            item.created_at,
            item.parent_call_id,
            item.status
        ],
    );
    if let Err(error) = result {
        crate::diag::write(&format!("RUN_EVENT persist {run_id}:{} failed: {error}", item.seq));
    }
}

#[cfg(test)]
mod tests {
    use super::{
        merge_timeline_item, normalize_runtime_line, push_tail, replace_assignment,
        runtime_stdin_payload, sanitize_value, should_merge_timeline, timeline_item,
        tool_use_event, truncate_and_redact, NormalizedEvent, PendingStream,
    };

    fn text_event(text: &str, replaces: bool) -> NormalizedEvent {
        NormalizedEvent {
            event_type: "text",
            call_id: None,
            tool: None,
            content: Some(text.into()),
            input: None,
            output: None,
            replaces_final_text: replaces,
            parent_call_id: None,
            status: None,
        }
    }

    #[test]
    fn replaces_only_assignment_argument() {
        assert_eq!(
            replace_assignment(vec!["-p".into(), "task".into()], "task", "full prompt"),
            vec!["-p", "full prompt"]
        );
        assert_eq!(
            replace_assignment(
                vec!["task".into(), "--note".into(), "task".into()],
                "task",
                "full prompt"
            ),
            vec!["full prompt", "--note", "task"]
        );
    }

    #[test]
    fn pending_stream_appends_in_place() {
        let mut pending = PendingStream::new();
        pending.start(timeline_item(1, text_event("你好", true)));
        pending.append(text_event("，世界", false));
        let item = pending.item.as_ref().unwrap();
        assert_eq!(item.content.as_deref(), Some("你好，世界"));
        assert!(item.replaces_final_text);
        assert_eq!(pending.chunk, "你好，世界");
    }

    #[test]
    fn normalizes_claude_fixture() {
        let events = normalize_runtime_line(
            "claude",
            r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"check"},{"type":"tool_use","id":"call-1","name":"Read","input":{"path":"a.rs"}},{"type":"text","text":"done"}]}}"#,
            false,
        );
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].event_type, "thinking");
        assert_eq!(events[1].call_id.as_deref(), Some("call-1"));
        assert_eq!(events[2].content.as_deref(), Some("done"));
        assert!(events[2].replaces_final_text);
    }

    #[test]
    fn normalizes_claude_tool_block_start() {
        let events = normalize_runtime_line(
            "claude",
            r#"{"type":"content_block_start","content_block":{"type":"tool_use","id":"call-9","name":"Read","input":{}}}"#,
            false,
        );
        assert_eq!(events[0].event_type, "tool_use");
        assert_eq!(events[0].tool.as_deref(), Some("Read"));
        assert_eq!(events[0].call_id.as_deref(), Some("call-9"));
    }

    #[test]
    fn merges_streaming_tool_json_into_one_call() {
        let start = timeline_item(
            1,
            tool_use_event(&serde_json::json!({
                "id": "call-9",
                "name": "Read",
                "input": {}
            })),
        );
        let delta = timeline_item(
            2,
            NormalizedEvent {
                event_type: "tool_use",
                call_id: None,
                tool: None,
                content: Some(r#"{"path":"ChatPanel.vue","offset":20,"limit":11}"#.into()),
                input: None,
                output: None,
                replaces_final_text: false,
                parent_call_id: None,
                status: None,
            },
        );
        assert!(should_merge_timeline(
            &start,
            &NormalizedEvent {
                event_type: "tool_use",
                call_id: None,
                tool: None,
                content: Some(r#"{"path":"ChatPanel.vue"}"#.into()),
                input: None,
                output: None,
                replaces_final_text: false,
                parent_call_id: None,
                status: None,
            }
        ));
        let merged = merge_timeline_item(&start, delta);
        assert_eq!(merged.tool.as_deref(), Some("Read"));
        assert_eq!(merged.input.as_ref().unwrap()["path"], "ChatPanel.vue");
        assert_eq!(merged.input.as_ref().unwrap()["offset"], 20);
    }

    #[test]
    fn encodes_claude_prompt_as_stream_json_input() {
        let payload = runtime_stdin_payload("claude", "hello \"world\"");
        let value: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        assert_eq!(value["type"], "user");
        assert_eq!(value["message"]["content"][0]["text"], "hello \"world\"");
        assert!(payload.ends_with(b"\n"));
    }

    #[test]
    fn normalizes_cursor_codex_and_qwen_fixtures() {
        let cursor = normalize_runtime_line(
            "cursor",
            r#"{"type":"tool_call","id":"c1","name":"Shell","input":{"command":"pwd"}}"#,
            false,
        );
        assert_eq!(cursor[0].event_type, "tool_use");
        let nested = normalize_runtime_line(
            "cursor",
            r#"{"type":"tool_call","subtype":"started","call_id":"call-1\nfc_9","tool_call":{"readToolCall":{"args":{"path":"ChatPanel.vue"}}}}"#,
            false,
        );
        assert_eq!(nested[0].event_type, "tool_use");
        assert_eq!(nested[0].tool.as_deref(), Some("Read"));
        assert_eq!(nested[0].call_id.as_deref(), Some("call-1"));
        assert_eq!(nested[0].input.as_ref().unwrap()["path"], "ChatPanel.vue");
        let edited = normalize_runtime_line(
            "cursor",
            r#"{"type":"tool_call","subtype":"completed","call_id":"call-2","tool_call":{"editToolCall":{"args":{"path":"gz-commit.config.js"},"result":{"success":{"diffString":"- messagePrefix: '[git gz] '\n+ messagePrefix: '[git-gz] '"}}}}}"#,
            false,
        );
        assert_eq!(edited[0].event_type, "tool_result");
        assert_eq!(edited[0].tool.as_deref(), Some("Edit"));
        assert!(edited[0]
            .output
            .as_deref()
            .unwrap_or_default()
            .contains("diffString"));
        let codex = normalize_runtime_line(
            "codex",
            r#"{"type":"item.completed","item":{"type":"agent_message","text":"ok"}}"#,
            false,
        );
        assert_eq!(codex[0].content.as_deref(), Some("ok"));
        assert!(codex[0].replaces_final_text);
        assert!(
            normalize_runtime_line("codex", "Reading additional input from stdin...", false,)
                .is_empty()
        );
        let qwen = normalize_runtime_line(
            "qwen",
            r#"{"type":"tool_result","call_id":"q1","output":"saved"}"#,
            false,
        );
        assert_eq!(qwen[0].event_type, "tool_result");
        assert_eq!(qwen[0].call_id.as_deref(), Some("q1"));
    }

    #[test]
    fn raw_json_does_not_become_user_text() {
        assert!(normalize_runtime_line(
            "claude",
            r#"{"type":"user","message":{"content":[{"type":"text","text":"private prompt"}]}}"#,
            false,
        )
        .is_empty());
        assert!(
            normalize_runtime_line("cursor", r#"{"type":"result","result":"done"}"#, false,)
                .is_empty()
        );
    }

    #[test]
    fn normalizes_copilot_and_opencode_events() {
        let copilot = normalize_runtime_line(
            "copilot",
            r#"{"type":"assistant.message","data":{"content":"done","toolRequests":[{"toolCallId":"t1","name":"shell","arguments":{"command":"ls"}}]}}"#,
            false,
        );
        assert_eq!(copilot[0].event_type, "text");
        assert_eq!(copilot[0].content.as_deref(), Some("done"));
        assert_eq!(copilot[1].event_type, "tool_use");
        assert_eq!(copilot[1].tool.as_deref(), Some("shell"));
        let opencode = normalize_runtime_line(
            "opencode",
            r#"{"type":"text","sessionID":"ses-1","part":{"text":"hello"}}"#,
            false,
        );
        assert_eq!(opencode[0].content.as_deref(), Some("hello"));
        assert_eq!(
            crate::conversation::id_from_line(
                r#"{"type":"text","sessionID":"ses-1","part":{"text":"hello"}}"#
            )
            .as_deref(),
            Some("ses-1")
        );
    }

    #[test]
    fn redacts_and_truncates_provider_data() {
        assert_eq!(
            truncate_and_redact("Authorization: Bearer abcdefgh12345678\nsafe", 100),
            "Authorization: Bearer [已隐藏敏感信息]\nsafe"
        );
        let mut value = serde_json::json!({"input": {"api_key": "secret", "path": "ok"}});
        sanitize_value(&mut value);
        assert_eq!(value["input"]["api_key"], "[已隐藏敏感信息]");
        assert_eq!(value["input"]["path"], "ok");
        assert_eq!(truncate_and_redact("abcdef", 3), "abc\n…（内容已截断）");
        assert_eq!(truncate_and_redact("末尾换行\n", 100), "末尾换行\n");
    }

    #[test]
    fn redacts_only_secret_shapes() {
        let prose = "如何设置 token= 参数？password 字段要加密";
        assert_eq!(truncate_and_redact(prose, 1000), prose);
        assert_eq!(
            truncate_and_redact("key sk-abcdefghijklmnopqrstuv end", 1000),
            "key [已隐藏敏感信息] end"
        );
        assert_eq!(
            truncate_and_redact("url?token=abc123&x=1", 1000),
            "url?token=[已隐藏敏感信息]&x=1"
        );
        assert_eq!(
            truncate_and_redact(r#"{"api_key": "abc", "path": "a"}"#, 1000),
            r#"{"api_key": "[已隐藏敏感信息]", "path": "a"}"#
        );
        assert_eq!(
            truncate_and_redact(
                "a\n-----BEGIN RSA PRIVATE KEY-----\nMIIE\n-----END RSA PRIVATE KEY-----\nb",
                1000
            ),
            "a\n[已隐藏敏感信息]\nb"
        );
        assert_eq!(
            truncate_and_redact("ghp_abcdefghijklmnopqrstuvwxyz", 1000),
            "[已隐藏敏感信息]"
        );
    }

    #[test]
    fn sanitize_keeps_token_counters() {
        let mut value = serde_json::json!({
            "max_tokens": 1024,
            "token_count": 3,
            "access_token": "abc",
            "X-Api-Key": "abc",
        });
        sanitize_value(&mut value);
        assert_eq!(value["max_tokens"], 1024);
        assert_eq!(value["token_count"], 3);
        assert_eq!(value["access_token"], "[已隐藏敏感信息]");
        assert_eq!(value["X-Api-Key"], "[已隐藏敏感信息]");
    }

    #[test]
    fn push_tail_keeps_last_bytes_on_char_boundary() {
        let output = std::sync::Mutex::new(String::new());
        push_tail(&output, "第一行", 8);
        push_tail(&output, "第二行", 8);
        let text = output.lock().unwrap().clone();
        assert!(text.len() <= 8);
        assert!(text.ends_with("二行"));
    }

    #[test]
    fn complete_assistant_messages_do_not_merge() {
        let previous = timeline_item(1, text_event("先查看扫描实现", true));
        assert!(!should_merge_timeline(
            &previous,
            &text_event("开始实现", true)
        ));
        assert!(should_merge_timeline(
            &previous,
            &text_event("，再补 PWA", false)
        ));
    }
}
