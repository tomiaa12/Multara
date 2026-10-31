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
    let (deliveries, content) = {
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
        let content: Option<String> = db
            .query_row(
                "SELECT content FROM messages
                 WHERE session_id=?1 AND role='assistant'
                 ORDER BY created_at DESC, rowid DESC LIMIT 1",
                [session_id],
                |row| row.get(0),
            )
            .ok();
        (deliveries, content)
    };

    // 有记录没能标记成功就不再进下一轮，否则会对同一条记录反复发送
    let mut all_marked = true;
    for delivery in deliveries {
        let result = send_one(app, &delivery, content.as_deref());
        let state = app.state::<AppState>();
        let marked = state.lock_db().is_ok_and(|db| match &result {
            Ok(()) => store::mark_delivery(&db, &delivery.run_id, "sent", None).is_ok(),
            Err(error) => {
                store::mark_delivery(&db, &delivery.run_id, "failed", Some(error)).is_ok()
            }
        });
        all_marked &= marked;
    }
    all_marked
}

fn send_one(
    app: &AppHandle,
    delivery: &store::Delivery,
    content: Option<&str>,
) -> Result<(), String> {
    let (installation, api) = api_for(app, &delivery.installation_id)?;
    if let Some(reaction) = delivery.typing_reaction_id.as_deref() {
        if let Some(message_id) = delivery.reply_to_message_id.as_deref() {
            api.remove_typing(message_id, reaction);
        }
    }
    let Some(content) = content.filter(|value| !value.trim().is_empty()) else {
        return Err(tr!("运行没有产生可回发的文本"));
    };
    let (body, questions) = split_questions(content);
    let text = format_outbound(&installation, delivery, &body);
    let chunks = split_message(&text, 18_000);
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
            let Ok((installation, api)) = api_for(&app, &delivery.installation_id) else {
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
    use super::{split_message, split_questions};

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
