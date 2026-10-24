//! 运行结束后把结果发回渠道。

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

fn deliver(app: &AppHandle, session_id: &str) {
    let (deliveries, content) = {
        let state = app.state::<AppState>();
        let Ok(db) = state.lock_db() else {
            return;
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
            return;
        }
        let deliveries = match store::pending_deliveries(&db, session_id) {
            Ok(deliveries) => deliveries,
            Err(_) => return,
        };
        if deliveries.is_empty() {
            return;
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

    for delivery in deliveries {
        let result = send_one(app, &delivery, content.as_deref());
        let state = app.state::<AppState>();
        if let Ok(db) = state.lock_db() {
            match &result {
                Ok(()) => {
                    let _ = store::mark_delivery(&db, &delivery.run_id, "sent", None);
                }
                Err(error) => {
                    let _ = store::mark_delivery(&db, &delivery.run_id, "failed", Some(error));
                }
            }
        };
    }
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
        return Err("运行没有产生可回发的文本".to_string());
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
                .map_err(|second| format!("{first_error}；降级发送也失败：{second}"))?,
        };
        first = false;
        let state = app.state::<AppState>();
        if let Ok(db) = state.lock_db() {
            store::record_outbound(&db, &installation.id, &sent, Some(&delivery.run_id), "reply");
        };
    }
    if !questions.is_empty() {
        let payload = card::questions_card(&delivery.run_id, &questions).to_string();
        let sent = match send_target(&api, delivery, "interactive", &payload, first) {
            Ok(sent) => sent,
            Err(_) => api
                .send_message_raw(&delivery.chat_id, "interactive", &payload)
                .map_err(|error| format!("题目卡片发送失败：{error}"))?,
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
        return Err("运行没有产生可回发的文本".to_string());
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
        if let Some(sender) = delivery.sender_id.as_deref().filter(|value| !value.is_empty()) {
            // 仅第一段加 @；后续分段保持正文干净。
            text = format!("<at user_id=\"{sender}\"></at> {text}");
        }
    }
    text
}

fn split_message(text: &str, limit: usize) -> Vec<String> {
    if text.chars().count() <= limit {
        return vec![text.to_string()];
    }
    let mut chunks = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        if current.chars().count() + line.chars().count() + 1 > limit && !current.is_empty() {
            chunks.push(std::mem::take(&mut current));
        }
        current.push_str(line);
        current.push('\n');
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
                chunk.push_str("\n\n（内容较长，已拆分发送；完整内容见 Multara。）");
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
        .unwrap_or("工具调用")
        .to_string();
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
                store::record_outbound(
                    &db,
                    &installation.id,
                    &sent,
                    Some(&run_id),
                    "permission",
                );
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
        let mut statement = match db.prepare(
            "SELECT DISTINCT session_id FROM channel_run_deliveries WHERE state='pending'",
        ) {
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
        let text = (0..40).map(|index| format!("line {index}")).collect::<Vec<_>>().join("\n");
        let chunks = split_message(&text, 60);
        assert!(chunks.len() > 1);
        assert!(chunks.last().unwrap().contains("内容较长"));
    }
}
