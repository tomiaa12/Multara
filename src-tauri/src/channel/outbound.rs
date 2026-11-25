//! 运行结束后把结果发回渠道。

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};
use std::thread;
use std::time::Duration;

use serde_json::json;
use tauri::{AppHandle, Manager};

use crate::database::AppState;

use super::feishu::api_for;
use super::feishu::card;
use super::{store, ChatType, Installation};

/// 在 `executor::finish` 里调用；网络操作放到独立线程，不阻塞运行收尾。
pub fn on_run_finished(app: &AppHandle, session_id: &str) {
    let has_pending = {
        let state = app.state::<AppState>();
        let Ok(db) = state.lock_db() else {
            return;
        };
        db.query_row(
            "SELECT EXISTS(SELECT 1 FROM channel_run_deliveries
                           WHERE session_id=?1 AND state='pending')",
            [session_id],
            |row| row.get::<_, bool>(0),
        )
        .unwrap_or(false)
    };
    if !has_pending {
        return;
    }
    let app = app.clone();
    let session_id = session_id.to_string();
    thread::spawn(move || deliver(&app, &session_id));
}

/// 同一会话同时只有一个投递在跑（`on_run_finished`、`recover_pending` 和并发运行都会触发），
/// 否则会对同一条待投递记录重复回复。值为 true 表示认领期间又有人要求投递，持有者要再跑一轮。
static DELIVERING: OnceLock<Mutex<HashMap<String, bool>>> = OnceLock::new();

/// 投递线程 panic 时释放认领，否则这个会话再也不会投递。
struct DeliveryClaim {
    session_id: String,
    held: bool,
}

impl Drop for DeliveryClaim {
    fn drop(&mut self) {
        if self.held {
            delivering().remove(&self.session_id);
        }
    }
}

fn delivering() -> MutexGuard<'static, HashMap<String, bool>> {
    DELIVERING
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

fn deliver(app: &AppHandle, session_id: &str) {
    {
        let mut claims = delivering();
        if let Some(rerun) = claims.get_mut(session_id) {
            *rerun = true;
            return;
        }
        claims.insert(session_id.to_string(), false);
    }
    let mut claim = DeliveryClaim {
        session_id: session_id.to_string(),
        held: true,
    };
    loop {
        while deliver_pending(app, session_id) {}
        let mut claims = delivering();
        if claims.get(session_id).copied().unwrap_or(false) {
            claims.insert(session_id.to_string(), false);
            continue;
        }
        claims.remove(session_id);
        claim.held = false;
        break;
    }
}

/// 投递当前所有待发记录；没有可投递的（或会话还在运行）时返回 false。
fn deliver_pending(app: &AppHandle, session_id: &str) -> bool {
    let groups = {
        let state = app.state::<AppState>();
        let Ok(db) = state.lock_db() else {
            return false;
        };
        let active: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM agent_runs
                 WHERE session_id=?1 AND status IN ('queued', 'running')",
                [session_id],
                |row| row.get(0),
            )
            .unwrap_or(0);
        if active > 0 {
            return false;
        }
        let deliveries = match store::pending_deliveries(&db, session_id) {
            Ok(deliveries) => deliveries,
            Err(_) => return false,
        };
        if deliveries.is_empty() {
            return false;
        }
        group_deliveries(deliveries)
            .into_iter()
            .map(|group| {
                let run_ids: Vec<String> = group.iter().map(|item| item.run_id.clone()).collect();
                let content = collect_turn_replies(&db, session_id, &run_ids);
                (group, content)
            })
            .collect::<Vec<_>>()
    };

    // 有记录没能标记成功就不再进下一轮，否则会对同一条记录反复发送
    let mut all_marked = true;
    for (group, content) in groups {
        let Some(primary) = group.first() else {
            continue;
        };
        for delivery in &group {
            clear_typing(app, delivery);
        }
        let result = send_one(app, primary, content.as_deref());
        let state = app.state::<AppState>();
        let marked = state.lock_db().is_ok_and(|db| {
            let mut ok = true;
            for delivery in &group {
                let marked = match &result {
                    Ok(()) => store::mark_delivery(&db, &delivery.run_id, "sent", None).is_ok(),
                    Err(error) => {
                        store::mark_delivery(&db, &delivery.run_id, "failed", Some(error)).is_ok()
                    }
                };
                ok &= marked;
            }
            ok
        });
        all_marked &= marked;
    }
    all_marked
}

/// 同一入站触发的多条 pending 合成一次回发，避免每人各发一遍同一段组合正文。
fn group_deliveries(deliveries: Vec<store::Delivery>) -> Vec<Vec<store::Delivery>> {
    let mut groups: HashMap<String, Vec<store::Delivery>> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for delivery in deliveries {
        let key = format!(
            "{}|{}|{}|{}",
            delivery.installation_id,
            delivery.chat_id,
            delivery.reply_to_message_id.as_deref().unwrap_or(""),
            delivery.thread_id.as_deref().unwrap_or("")
        );
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push(delivery);
    }
    order
        .into_iter()
        .filter_map(|key| groups.remove(&key))
        .collect()
}

/// 把本轮（同一 turn / 同一运行树）里各智能体的助手回复按时间拼成一段再回飞书。
fn collect_turn_replies(
    db: &rusqlite::Connection,
    session_id: &str,
    run_ids: &[String],
) -> Option<String> {
    if run_ids.is_empty() {
        return None;
    }
    let tree_ids = run_tree_ids(db, run_ids);
    let turn_ids = turn_ids_for_runs(db, &tree_ids);
    let parts = if turn_ids.is_empty() {
        assistant_parts_for_runs(db, session_id, &tree_ids)
    } else {
        let mut parts = assistant_parts_for_turns(db, session_id, &turn_ids);
        if parts.is_empty() {
            parts = assistant_parts_for_runs(db, session_id, &tree_ids);
        }
        parts
    };
    join_reply_parts(&parts)
}

fn run_tree_ids(db: &rusqlite::Connection, run_ids: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut queue = run_ids.to_vec();
    while let Some(id) = queue.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        out.push(id.clone());
        let Ok(mut statement) = db.prepare("SELECT id FROM agent_runs WHERE parent_run_id=?1") else {
            continue;
        };
        let Ok(rows) = statement.query_map([&id], |row| row.get::<_, String>(0)) else {
            continue;
        };
        for child in rows.filter_map(Result::ok) {
            queue.push(child);
        }
    }
    out
}

fn turn_ids_for_runs(db: &rusqlite::Connection, run_ids: &[String]) -> Vec<String> {
    let mut turns = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for run_id in run_ids {
        let Ok(turn_id) = db.query_row(
            "SELECT turn_id FROM agent_runs
             WHERE id=?1 AND turn_id IS NOT NULL AND trim(turn_id) != ''",
            [run_id],
            |row| row.get::<_, String>(0),
        ) else {
            continue;
        };
        if seen.insert(turn_id.clone()) {
            turns.push(turn_id);
        }
    }
    turns
}

fn assistant_parts_for_turns(
    db: &rusqlite::Connection,
    session_id: &str,
    turn_ids: &[String],
) -> Vec<(String, String)> {
    if turn_ids.is_empty() {
        return Vec::new();
    }
    let placeholders = turn_ids
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT m.content, COALESCE(a.name, '')
         FROM messages m
         LEFT JOIN agents a ON a.id = m.agent_id
         WHERE m.session_id=?1
           AND m.role='assistant'
           AND COALESCE(m.message_kind, 'message') NOT IN ('channel_outbound', 'assignment', 'no_response')
           AND m.turn_id IN ({placeholders})
           AND trim(m.content) != ''
         ORDER BY m.created_at ASC, m.rowid ASC"
    );
    let Ok(mut statement) = db.prepare(&sql) else {
        return Vec::new();
    };
    let mut params: Vec<rusqlite::types::Value> = vec![session_id.to_string().into()];
    params.extend(turn_ids.iter().map(|id| id.clone().into()));
    let Ok(rows) = statement.query_map(rusqlite::params_from_iter(params), |row| {
        Ok((row.get::<_, String>(1)?, row.get::<_, String>(0)?))
    }) else {
        return Vec::new();
    };
    rows.filter_map(Result::ok)
        .filter(|(_, content)| !content.trim().is_empty())
        .collect()
}

fn assistant_parts_for_runs(
    db: &rusqlite::Connection,
    session_id: &str,
    run_ids: &[String],
) -> Vec<(String, String)> {
    if run_ids.is_empty() {
        return Vec::new();
    }
    let placeholders = run_ids
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT m.content, COALESCE(a.name, '')
         FROM messages m
         LEFT JOIN agents a ON a.id = m.agent_id
         WHERE m.session_id=?1
           AND m.role='assistant'
           AND COALESCE(m.message_kind, 'message') NOT IN ('channel_outbound', 'assignment', 'no_response')
           AND m.run_id IN ({placeholders})
           AND trim(m.content) != ''
         ORDER BY m.created_at ASC, m.rowid ASC"
    );
    let Ok(mut statement) = db.prepare(&sql) else {
        return Vec::new();
    };
    let mut params: Vec<rusqlite::types::Value> = vec![session_id.to_string().into()];
    params.extend(run_ids.iter().map(|id| id.clone().into()));
    let Ok(rows) = statement.query_map(rusqlite::params_from_iter(params), |row| {
        Ok((row.get::<_, String>(1)?, row.get::<_, String>(0)?))
    }) else {
        return Vec::new();
    };
    rows.filter_map(Result::ok)
        .filter(|(_, content)| !content.trim().is_empty())
        .collect()
}

fn join_reply_parts(parts: &[(String, String)]) -> Option<String> {
    if parts.is_empty() {
        return None;
    }
    let named = parts
        .iter()
        .map(|(name, _)| name.as_str())
        .filter(|name| !name.is_empty())
        .collect::<std::collections::HashSet<_>>();
    let multi_agent = named.len() > 1;
    let text = parts
        .iter()
        .map(|(name, content)| {
            if multi_agent && !name.is_empty() {
                format!("**{name}**\n{content}")
            } else {
                content.clone()
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let trimmed = text.trim().to_string();
    (!trimmed.is_empty()).then_some(trimmed)
}

fn send_plain(
    app: &AppHandle,
    delivery: &store::Delivery,
    content: Option<&str>,
) -> Result<(), String> {
    let installation = {
        let state = app.state::<AppState>();
        let db = state.lock_db()?;
        store::get_installation(&db, &delivery.installation_id)?
            .ok_or_else(|| tr!("渠道连接不存在"))?
    };
    let Some(content) = content.filter(|value| !value.trim().is_empty()) else {
        return Err(tr!("运行没有产生可回发的文本"));
    };
    let (body, questions) = split_questions(content);
    let body = expand_genui(&body);
    let text = body.trim();
    if text.is_empty() && questions.is_empty() {
        return Err(tr!("运行没有产生可回发的文本"));
    }
    if !text.is_empty() {
        let sent = super::link::send_delivery(app, &installation, delivery, text)?;
        record_reply(app, &installation.id, &sent, &delivery.run_id, "reply");
    }
    if !questions.is_empty() {
        let sent = super::link::send_delivery(app, &installation, delivery, &questions_as_text(&questions))?;
        record_reply(app, &installation.id, &sent, &delivery.run_id, "question");
    }
    if let Some(plan) = plan_text(app, &delivery.run_id) {
        let sent = super::link::send_delivery(app, &installation, delivery, &plan)?;
        record_reply(app, &installation.id, &sent, &delivery.run_id, "plan");
    }
    Ok(())
}

fn record_reply(app: &AppHandle, installation_id: &str, message_id: &str, run_id: &str, kind: &str) {
    let state = app.state::<AppState>();
    if let Ok(db) = state.lock_db() {
        store::record_outbound(&db, installation_id, message_id, Some(run_id), kind);
    };
}

fn questions_as_text(questions: &[serde_json::Value]) -> String {
    let mut lines = vec![tr!("请直接回复下面的问题：")];
    for (index, question) in questions.iter().enumerate() {
        let prompt = question
            .get("question")
            .or_else(|| question.get("prompt"))
            .or_else(|| question.get("header"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        if !prompt.is_empty() {
            lines.push(format!("{}. {prompt}", index + 1));
        }
        if let Some(options) = question.get("options").and_then(serde_json::Value::as_array) {
            for option in options {
                let label = option
                    .as_str()
                    .or_else(|| option.get("label").and_then(serde_json::Value::as_str))
                    .or_else(|| option.get("name").and_then(serde_json::Value::as_str))
                    .unwrap_or("");
                if !label.is_empty() {
                    lines.push(format!("- {label}"));
                }
            }
        }
    }
    lines.join("\n")
}

fn plan_text(app: &AppHandle, run_id: &str) -> Option<String> {
    let state = app.state::<AppState>();
    let plan = state.lock_db().ok().and_then(|db| {
        db.query_row(
            "SELECT input_json FROM run_events
             WHERE run_id=?1 AND event_type='plan' ORDER BY seq DESC LIMIT 1",
            [run_id],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
    })?;
    let title = plan.get("title").and_then(serde_json::Value::as_str).unwrap_or("");
    let summary = plan.get("summary").and_then(serde_json::Value::as_str).unwrap_or("");
    let text = format!("{title}\n{summary}").trim().to_string();
    (!text.is_empty()).then_some(tr!("计划：{text}", text = text))
}

fn clear_typing(app: &AppHandle, delivery: &store::Delivery) {
    let Some(reaction) = delivery.typing_reaction_id.as_deref() else {
        return;
    };
    let Some(message_id) = delivery.reply_to_message_id.as_deref() else {
        return;
    };
    let Ok((_, api)) = api_for(app, &delivery.installation_id) else {
        return;
    };
    api.remove_typing(message_id, reaction);
}

fn send_one(
    app: &AppHandle,
    delivery: &store::Delivery,
    content: Option<&str>,
) -> Result<(), String> {
    let channel_type = {
        let state = app.state::<AppState>();
        let db = state.lock_db()?;
        store::get_installation(&db, &delivery.installation_id)?
            .map(|item| item.channel_type)
            .ok_or_else(|| tr!("渠道连接不存在"))?
    };
    if !super::is_feishu(&channel_type) {
        return send_plain(app, delivery, content);
    }
    let (installation, api) = api_for(app, &delivery.installation_id)?;
    // typing 表情在 deliver_pending 分组时已清过；这里再清一次无伤
    if let Some(reaction) = delivery.typing_reaction_id.as_deref() {
        if let Some(message_id) = delivery.reply_to_message_id.as_deref() {
            api.remove_typing(message_id, reaction);
        }
    }
    let Some(content) = content.filter(|value| !value.trim().is_empty()) else {
        return Err(tr!("运行没有产生可回发的文本"));
    };
    let (body, questions) = split_questions(content);
    // IM 渲不了 GenUI：把界面块转成可读文字（与前端 to-text 同思路），截图回传另做。
    let body = expand_genui(&body);
    let text = format_outbound(&installation, delivery, &body);
    let chunks = split_message(&text, OUTBOUND_CHUNK_CHARS);
    let mut first = true;
    for chunk in chunks {
        if chunk.trim().is_empty() {
            continue;
        }
        let (msg_type, payload) = if super::router::looks_like_markdown(&chunk) {
            ("interactive", card::markdown_card(&chunk).to_string())
        } else {
            ("text", json!({ "text": chunk }).to_string())
        };
        let sent = match send_target(&api, delivery, msg_type, &payload, first) {
            Ok(sent) => sent,
            Err(first_error) => api
                .send_message_raw(&delivery.chat_id, msg_type, &payload)
                .map_err(|second| {
                    tr!(
                        "{first_error}；降级发送也失败：{second}",
                        first_error = first_error,
                        second = second
                    )
                })?,
        };
        first = false;
        let state = app.state::<AppState>();
        if let Ok(db) = state.lock_db() {
            store::record_outbound(
                &db,
                &installation.id,
                &sent,
                Some(&delivery.run_id),
                "reply",
            );
        };
    }
    let plan = {
        let state = app.state::<AppState>();
        state.lock_db().ok().and_then(|db| {
            db.query_row(
                "SELECT input_json FROM run_events
                 WHERE run_id=?1 AND event_type='plan' ORDER BY seq DESC LIMIT 1",
                [&delivery.run_id],
                |row| row.get::<_, String>(0),
            )
            .ok()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        })
    };
    if let Some(plan) = plan {
        let plan_id = plan
            .get("planId")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let revision = plan
            .get("revision")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or(1);
        let locale = app
            .state::<AppState>()
            .lock_db()
            .map(|db| crate::app_locale::from_db(&db))
            .unwrap_or("zh-CN");
        let payload = card::plan_card(
            &delivery.run_id,
            plan_id,
            revision,
            plan.get("title")
                .and_then(serde_json::Value::as_str)
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| crate::plan_prompts::untitled(locale)),
            plan.get("summary")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default(),
            locale,
        )
        .to_string();
        let sent = send_target(&api, delivery, "interactive", &payload, first)
            .or_else(|_| api.send_message_raw(&delivery.chat_id, "interactive", &payload))
            .map_err(|error| tr!("计划卡片发送失败：{error}", error = error))?;
        let state = app.state::<AppState>();
        if let Ok(db) = state.lock_db() {
            store::record_outbound(&db, &installation.id, &sent, Some(&delivery.run_id), "plan");
        };
    }
    if !questions.is_empty() {
        let payload = card::questions_card(&delivery.run_id, &questions).to_string();
        let sent = match send_target(&api, delivery, "interactive", &payload, first) {
            Ok(sent) => sent,
            Err(_) => api
                .send_message_raw(&delivery.chat_id, "interactive", &payload)
                .map_err(|error| tr!("题目卡片发送失败：{error}", error = error))?,
        };
        let state = app.state::<AppState>();
        if let Ok(db) = state.lock_db() {
            store::record_outbound(
                &db,
                &installation.id,
                &sent,
                Some(&delivery.run_id),
                "question",
            );
        };
    } else if body.trim().is_empty() {
        return Err(tr!("运行没有产生可回发的文本"));
    }
    Ok(())
}

/// IM 渠道渲染不了 ```multara-ui：把界面块转成纯文字，前后正文保留。
/// 只认行首的围栏；嵌在其它代码块里的（例如演示协议）不动。
fn expand_genui(content: &str) -> String {
    if !content.contains("multara-ui") {
        return content.to_string();
    }
    let placeholder = tr!("（此处有一个界面块，请在 Multara 中查看）");
    let mut out: Vec<String> = Vec::new();
    // (围栏字符, 长度, 是否界面块)
    let mut open: Option<(char, usize, bool)> = None;
    let mut ui_body: Vec<&str> = Vec::new();
    for line in content.split('\n') {
        let trimmed = line.trim_start_matches([' ', '\t']);
        let fence = trimmed
            .chars()
            .next()
            .filter(|ch| *ch == '`' || *ch == '~')
            .map(|ch| (ch, trimmed.chars().take_while(|c| *c == ch).count()))
            .filter(|(_, len)| *len >= 3);
        match (open, fence) {
            (None, Some((ch, len))) => {
                let info = trimmed[len * ch.len_utf8()..].trim();
                let is_ui = info.split_whitespace().next() == Some("multara-ui");
                open = Some((ch, len, is_ui));
                if is_ui {
                    ui_body.clear();
                } else {
                    out.push(line.to_string());
                }
            }
            (Some((ch, len, is_ui)), Some((close_ch, close_len)))
                if close_ch == ch
                    && close_len >= len
                    && trimmed[close_len * ch.len_utf8()..].trim().is_empty() =>
            {
                open = None;
                if is_ui {
                    out.push(genui_fence_to_text(&ui_body.join("\n"), &placeholder));
                    ui_body.clear();
                } else {
                    out.push(line.to_string());
                }
            }
            (Some((_, _, true)), _) => ui_body.push(line),
            _ => out.push(line.to_string()),
        }
    }
    if open.is_some_and(|(_, _, is_ui)| is_ui) {
        out.push(genui_fence_to_text(&ui_body.join("\n"), &placeholder));
    }
    out.join("\n")
}

fn genui_fence_to_text(raw: &str, placeholder: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw.trim()) else {
        return placeholder.to_string();
    };
    let list = if let Some(blocks) = value.get("blocks").and_then(serde_json::Value::as_array) {
        blocks.as_slice()
    } else if let Some(items) = value.as_array() {
        items.as_slice()
    } else {
        std::slice::from_ref(&value)
    };
    let mut parts = Vec::new();
    for item in list {
        let Some(block_type) = item.get("type").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let title = item
            .get("title")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty());
        let props = item.get("props").unwrap_or(item);
        if let Some(text) = genui_spec_to_text(block_type, title, props) {
            parts.push(text);
        }
    }
    if parts.is_empty() {
        placeholder.to_string()
    } else {
        parts.join("\n\n")
    }
}

fn genui_spec_to_text(
    block_type: &str,
    title: Option<&str>,
    props: &serde_json::Value,
) -> Option<String> {
    let body = match block_type {
        "card" => Some(card_to_text(props)),
        "stats" => Some(stats_to_text(props)),
        "table" => Some(table_to_text(props)),
        "chart" => Some(chart_to_text(props)),
        "form" => Some(form_to_text(props)),
        "question" => Some(question_to_text(props)),
        _ => None,
    }?;
    let mut lines = Vec::new();
    if let Some(title) = title {
        lines.push(format!("**{title}**"));
    }
    if !body.trim().is_empty() {
        lines.push(body);
    }
    if block_type == "form" {
        lines.push(tr!("（请在 Multara 中填写并提交）"));
    }
    let text = lines.join("\n").trim().to_string();
    (!text.is_empty()).then_some(text)
}

fn json_cell(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(text) => text.split_whitespace().collect::<Vec<_>>().join(" "),
        other => other
            .to_string()
            .trim_matches('"')
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn card_to_text(props: &serde_json::Value) -> String {
    let mut lines = Vec::new();
    if let Some(fields) = props.get("fields").and_then(serde_json::Value::as_array) {
        for field in fields {
            let label = field
                .get("label")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let value = field
                .get("value")
                .map(json_cell)
                .unwrap_or_default();
            if !label.is_empty() {
                lines.push(format!("{label}: {value}"));
            }
        }
    }
    if let Some(body) = props
        .get("body")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
    {
        lines.push(body.to_string());
    }
    if let Some(actions) = props.get("actions").and_then(serde_json::Value::as_array) {
        let labels: Vec<&str> = actions
            .iter()
            .filter_map(|action| action.get("label").and_then(serde_json::Value::as_str))
            .collect();
        if !labels.is_empty() {
            lines.push(format!("[{}]", labels.join(" / ")));
        }
    }
    lines.join("\n")
}

fn stats_to_text(props: &serde_json::Value) -> String {
    props
        .get("items")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let label = item.get("label").and_then(serde_json::Value::as_str)?;
            let value = item.get("value").map(json_cell).unwrap_or_default();
            let unit = item
                .get("unit")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let delta = item
                .get("delta")
                .and_then(serde_json::Value::as_f64)
                .map(|delta| {
                    if delta > 0.0 {
                        format!(" (+{delta})")
                    } else {
                        format!(" ({delta})")
                    }
                })
                .unwrap_or_default();
            Some(format!("{label}: {value}{unit}{delta}"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn table_to_text(props: &serde_json::Value) -> String {
    const MAX_ROWS: usize = 20;
    let columns = props
        .get("columns")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    if columns.is_empty() {
        return String::new();
    }
    let head = columns
        .iter()
        .map(|column| {
            column
                .get("label")
                .or_else(|| column.get("key"))
                .map(json_cell)
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" | ");
    let rows = props
        .get("rows")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut lines = vec![head];
    for row in rows.iter().take(MAX_ROWS) {
        let line = columns
            .iter()
            .map(|column| {
                let key = column
                    .get("key")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                row.get(key).map(json_cell).unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join(" | ");
        lines.push(line);
    }
    if rows.len() > MAX_ROWS {
        lines.push(format!("… ({})", rows.len()));
    }
    lines.join("\n")
}

fn chart_to_text(props: &serde_json::Value) -> String {
    let unit = props
        .get("unit")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let x_key = props
        .get("xKey")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let data = props
        .get("data")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    props
        .get("series")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|series| {
            let key = series.get("key").and_then(serde_json::Value::as_str)?;
            let label = series
                .get("label")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(key);
            let values: Vec<f64> = data
                .iter()
                .map(|row| {
                    row.get(key)
                        .and_then(serde_json::Value::as_f64)
                        .or_else(|| {
                            row.get(key)
                                .and_then(serde_json::Value::as_str)
                                .and_then(|text| text.parse().ok())
                        })
                        .unwrap_or(0.0)
                })
                .collect();
            let last = data.last();
            let last_label = last
                .and_then(|row| row.get(x_key))
                .map(json_cell)
                .unwrap_or_default();
            let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
            let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let last_value = values.last().copied().unwrap_or(0.0);
            Some(format!(
                "{label}: n={n}, min={min}{unit}, max={max}{unit}, {last_label}={last_value}{unit}",
                n = values.len()
            ))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn form_to_text(props: &serde_json::Value) -> String {
    let mut lines = Vec::new();
    if let Some(fields) = props.get("fields").and_then(serde_json::Value::as_array) {
        for field in fields {
            let label = field
                .get("label")
                .or_else(|| field.get("name"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            if label.is_empty() {
                continue;
            }
            let required = field
                .get("required")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let mut line = if required {
                format!("{label}*")
            } else {
                label.to_string()
            };
            let default = field.get("default").filter(|value| !value.is_null());
            if let Some(default) = default {
                let text = json_cell(default);
                if !text.is_empty() && text != "[]" && text != "{}" {
                    line.push_str(": ");
                    line.push_str(&text);
                }
            }
            if let Some(options) = field.get("options").and_then(serde_json::Value::as_array) {
                let labels: Vec<String> = options
                    .iter()
                    .filter_map(|option| {
                        option
                            .get("label")
                            .or_else(|| option.get("value"))
                            .map(json_cell)
                            .filter(|text| !text.is_empty())
                    })
                    .collect();
                if !labels.is_empty() {
                    line.push_str(&format!(" ({})", labels.join(" / ")));
                }
            }
            lines.push(line);
        }
    }
    if let Some(submit) = props
        .get("submitLabel")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
    {
        lines.push(format!("[{submit}]"));
    }
    lines.join("\n")
}

fn question_to_text(props: &serde_json::Value) -> String {
    let questions = if let Some(items) = props.get("questions").and_then(serde_json::Value::as_array)
    {
        items.as_slice()
    } else if let Some(items) = props.as_array() {
        items.as_slice()
    } else {
        std::slice::from_ref(props)
    };
    questions
        .iter()
        .filter_map(|question| {
            let text = question
                .get("question")
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.is_empty())?;
            let mut lines = vec![text.to_string()];
            if let Some(options) = question
                .get("options")
                .and_then(serde_json::Value::as_array)
            {
                for option in options {
                    if let Some(label) = option
                        .get("label")
                        .and_then(serde_json::Value::as_str)
                        .filter(|value| !value.is_empty())
                    {
                        lines.push(format!("- {label}"));
                    }
                }
            }
            Some(lines.join("\n"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 从回复里取出 ```multara-questions 代码块，剩余正文照常发送。
fn split_questions(content: &str) -> (String, Vec<serde_json::Value>) {
    let Some(start) = content.find("```multara-questions") else {
        return (content.to_string(), Vec::new());
    };
    let after = &content[start..];
    let Some(body_start) = after.find('\n') else {
        return (content.to_string(), Vec::new());
    };
    let json_start = start + body_start + 1;
    let Some(end_offset) = content[json_start..].find("```") else {
        return (content.to_string(), Vec::new());
    };
    let raw = &content[json_start..json_start + end_offset];
    let mut parsed: serde_json::Value = match serde_json::from_str(raw.trim()) {
        Ok(value) => value,
        Err(_) => return (content.to_string(), Vec::new()),
    };
    let questions = if parsed.is_array() {
        std::mem::take(&mut parsed)
    } else {
        parsed
            .get("questions")
            .cloned()
            .unwrap_or(serde_json::Value::Array(Vec::new()))
    };
    let questions = questions.as_array().cloned().unwrap_or_default();
    if questions.is_empty() {
        return (content.to_string(), Vec::new());
    }
    let mut body = String::new();
    body.push_str(content[..start].trim_end());
    body.push('\n');
    body.push_str(content[json_start + end_offset + 3..].trim_start());
    (body.trim().to_string(), questions)
}

fn send_target(
    api: &super::feishu::api::FeishuApi,
    delivery: &store::Delivery,
    msg_type: &str,
    payload: &str,
    first: bool,
) -> Result<String, String> {
    let Some(trigger) = delivery.reply_to_message_id.as_deref() else {
        return api.send_message_raw(&delivery.chat_id, msg_type, payload);
    };
    if delivery.chat_type == ChatType::P2p && delivery.thread_id.is_none() {
        return api.send_message_raw(&delivery.chat_id, msg_type, payload);
    }
    let in_thread = delivery.thread_id.is_some();
    if !in_thread && !first {
        return api.send_message_raw(&delivery.chat_id, msg_type, payload);
    }
    api.reply(trigger, msg_type, payload, in_thread)
}

pub fn format_outbound(
    installation: &Installation,
    delivery: &store::Delivery,
    content: &str,
) -> String {
    let _ = installation;
    let mut text = content.trim().to_string();
    if delivery.chat_type == ChatType::Group {
        if let Some(sender) = delivery
            .sender_id
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            // 仅第一段加 @；后续分段保持正文干净。
            text = format!("<at user_id=\"{sender}\"></at> {text}");
        }
    }
    text
}

/// 按行拆分，单行超过上限时再按字符硬切；长度累计计数，整体 O(n)。
/// 单条飞书消息的分段长度。主动发送超过这段就拒绝，不拆成多条。
pub(crate) const OUTBOUND_CHUNK_CHARS: usize = 18_000;

fn split_message(text: &str, limit: usize) -> Vec<String> {
    if text.chars().count() <= limit {
        return vec![text.to_string()];
    }
    let piece_limit = limit.saturating_sub(1).max(1);
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut current_len = 0;
    for line in text.lines() {
        let mut rest = line;
        loop {
            let cut = rest
                .char_indices()
                .nth(piece_limit)
                .map_or(rest.len(), |(index, _)| index);
            let (piece, tail) = rest.split_at(cut);
            let piece_len = piece.chars().count();
            if current_len + piece_len + 1 > limit && !current.is_empty() {
                chunks.push(std::mem::take(&mut current));
                current_len = 0;
            }
            current.push_str(piece);
            current.push('\n');
            current_len += piece_len + 1;
            rest = tail;
            if rest.is_empty() {
                break;
            }
        }
    }
    if !current.trim().is_empty() {
        chunks.push(current);
    }
    if chunks.is_empty() {
        chunks.push(text.chars().take(limit).collect());
    }
    let total = chunks.len();
    if total > 1 {
        for (index, chunk) in chunks.iter_mut().enumerate() {
            if index + 1 == total {
                chunk.push_str(&tr!("\n\n（内容较长，已拆分发送；完整内容见 Multara。）"));
            }
        }
    }
    chunks
}

/// 运行中需要用户确认工具调用时，把权限卡片发到聊天里。
pub fn on_permission_request(
    app: &AppHandle,
    session_id: &str,
    run_id: &str,
    request_id: &str,
    params: Option<&serde_json::Value>,
) {
    let deliveries = {
        let state = app.state::<AppState>();
        let Ok(db) = state.lock_db() else {
            return;
        };
        match store::pending_deliveries(&db, session_id) {
            Ok(deliveries) => deliveries,
            Err(_) => return,
        }
    };
    if deliveries.is_empty() {
        return;
    }
    let params = params.cloned().unwrap_or(serde_json::Value::Null);
    let tool = params
        .get("toolCall")
        .and_then(|call| {
            call.get("title")
                .or_else(|| call.get("kind"))
                .or_else(|| call.get("name"))
        })
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| tr!("工具调用"));
    let options: Vec<(String, String)> = params
        .get("options")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|option| {
                    let id = option
                        .get("optionId")
                        .or_else(|| option.get("option_id"))
                        .and_then(serde_json::Value::as_str)?;
                    let label = option
                        .get("name")
                        .or_else(|| option.get("label"))
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or(id);
                    Some((id.to_string(), label.to_string()))
                })
                .take(4)
                .collect()
        })
        .unwrap_or_default();
    let card = card::permission_card(run_id, request_id, &tool, &options);
    let app = app.clone();
    let run_id = run_id.to_string();
    thread::spawn(move || {
        for delivery in deliveries {
            let installation = {
                let state = app.state::<AppState>();
                let Ok(db) = state.lock_db() else {
                    continue;
                };
                match store::get_installation(&db, &delivery.installation_id) {
                    Ok(Some(installation)) => installation,
                    _ => continue,
                }
            };
            if !super::is_feishu(&installation.channel_type) {
                let notice = tr!("需要在 Multara 里确认：{tool}", tool = tool.clone());
                if let Ok(sent) = super::link::send_delivery(&app, &installation, &delivery, &notice) {
                    record_reply(&app, &installation.id, &sent, &run_id, "permission");
                }
                continue;
            }
            let Ok((_, api)) = api_for(&app, &installation.id) else {
                continue;
            };
            let payload = card.to_string();
            let sent = match send_target(&api, &delivery, "interactive", &payload, true) {
                Ok(sent) => sent,
                Err(_) => match api.send_message_raw(&delivery.chat_id, "interactive", &payload) {
                    Ok(sent) => sent,
                    Err(_) => continue,
                },
            };
            let state = app.state::<AppState>();
            if let Ok(db) = state.lock_db() {
                store::record_outbound(&db, &installation.id, &sent, Some(&run_id), "permission");
            };
        }
    });
}

/// 启动时补发上次退出前没发出去的结果。
pub fn recover_pending(app: &AppHandle) {
    let sessions: Vec<String> = {
        let state = app.state::<AppState>();
        let Ok(db) = state.lock_db() else {
            return;
        };
        let mut statement = match db
            .prepare("SELECT DISTINCT session_id FROM channel_run_deliveries WHERE state='pending'")
        {
            Ok(statement) => statement,
            Err(_) => return,
        };
        statement
            .query_map([], |row| row.get(0))
            .ok()
            .map(|rows| rows.filter_map(Result::ok).collect())
            .unwrap_or_default()
    };
    for session_id in sessions {
        thread::sleep(Duration::from_millis(200));
        let app = app.clone();
        thread::spawn(move || deliver(&app, &session_id));
    }
}

#[cfg(test)]
mod tests {
    use super::{expand_genui, join_reply_parts, split_message, split_questions};

    #[test]
    fn joins_multi_agent_replies_with_names() {
        let parts = vec![
            ("Miku".to_string(), "先建仓。".to_string()),
            ("需求建仓助手".to_string(), "| 仓库 |".to_string()),
            ("Miku".to_string(), "本轮无产出。".to_string()),
        ];
        let text = join_reply_parts(&parts).unwrap();
        assert!(text.contains("**Miku**\n先建仓。"));
        assert!(text.contains("**需求建仓助手**\n| 仓库 |"));
        assert!(text.contains("**Miku**\n本轮无产出。"));
    }

    #[test]
    fn joins_single_agent_without_name_prefix() {
        let parts = vec![("Miku".to_string(), "只有一句。".to_string())];
        assert_eq!(join_reply_parts(&parts).as_deref(), Some("只有一句。"));
    }

    #[test]
    fn expands_table_genui_and_keeps_text() {
        let text = "前面。\n\n```multara-ui\n{\"type\":\"table\",\"props\":{\"columns\":[{\"key\":\"repo\",\"label\":\"仓库\"}],\"rows\":[{\"repo\":\"pages\"}]}}\n```\n\n后面。";
        let out = expand_genui(text);
        assert!(out.contains("前面。"));
        assert!(out.contains("仓库"));
        assert!(out.contains("pages"));
        assert!(out.contains("后面。"));
        assert!(!out.contains("multara-ui"));
    }

    #[test]
    fn expands_form_genui_with_multara_hint() {
        let text = "```multara-ui\n{\"type\":\"form\",\"title\":\"建仓确认\",\"props\":{\"submitLabel\":\"下一步\",\"fields\":[{\"name\":\"repos\",\"label\":\"仓库\",\"kind\":\"multiselect\",\"options\":[{\"value\":\"pages\",\"label\":\"pages\"}]}]}}\n```";
        let out = expand_genui(text);
        assert!(out.contains("**建仓确认**"));
        assert!(out.contains("仓库"));
        assert!(out.contains("pages"));
        assert!(out.contains("[下一步]"));
        assert!(out.contains("请在 Multara 中填写并提交"));
    }

    #[test]
    fn keeps_genui_demo_inside_other_fence() {
        let text = "````markdown\n```multara-ui\n{}\n```\n````";
        assert_eq!(expand_genui(text), text);
    }

    #[test]
    fn unclosed_invalid_genui_falls_back_to_placeholder() {
        let out = expand_genui("看这里：\n```multara-ui\n{\"type\"");
        assert!(out.contains("看这里："));
        assert!(out.contains("请在 Multara 中查看"));
    }

    #[test]
    fn extracts_questions_and_keeps_body() {
        let text = "先说结论。\n\n```multara-questions\n{\"questions\":[{\"header\":\"下一步\",\"question\":\"先做哪个？\",\"options\":[{\"label\":\"改代码\"}]}]}\n```\n";
        let (body, questions) = split_questions(text);
        assert_eq!(body, "先说结论。");
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0]["question"], "先做哪个？");
    }

    #[test]
    fn leaves_plain_replies_untouched() {
        let (body, questions) = split_questions("普通回复");
        assert_eq!(body, "普通回复");
        assert!(questions.is_empty());
    }

    #[test]
    fn splits_long_messages_on_line_boundaries() {
        let text = (0..40)
            .map(|index| format!("line {index}"))
            .collect::<Vec<_>>()
            .join("\n");
        let chunks = split_message(&text, 60);
        assert!(chunks.len() > 1);
        assert!(chunks.last().unwrap().contains("内容较长"));
    }

    #[test]
    fn splits_single_overlong_line() {
        let text = "字".repeat(250);
        let chunks = split_message(&text, 100);
        assert_eq!(chunks.len(), 3);
        assert!(chunks[..2].iter().all(|chunk| chunk.chars().count() <= 100));
        let joined: String = chunks
            .iter()
            .map(|chunk| chunk.split('\n').next().unwrap())
            .collect();
        assert_eq!(joined, text);
    }
}
