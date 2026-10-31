//! 飞书事件解码：`im.message.receive_v1` 和卡片回调。

use serde_json::Value;

use super::super::{CardAction, ChatType, InboundEvent, InboundMessage, MediaRef};

pub fn decode(
    payload: &[u8],
    installation_id: &str,
    bot_open_id: Option<&str>,
    bot_name: Option<&str>,
) -> Result<Option<InboundEvent>, String> {
    let value: Value =
        serde_json::from_slice(payload).map_err(|error| format!("事件不是 JSON：{error}"))?;
    let event_type = value
        .get("header")
        .and_then(|header| header.get("event_type"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    match event_type {
        "im.message.receive_v1" => {
            decode_message(&value, installation_id, bot_open_id, bot_name).map(Some)
        }
        "card.action.trigger" => decode_card_action(&value, installation_id),
        // 心跳和其它未订阅事件：ACK 掉但不上报。
        _ => Ok(None),
    }
}

fn decode_message(
    value: &Value,
    installation_id: &str,
    bot_open_id: Option<&str>,
    bot_name: Option<&str>,
) -> Result<InboundEvent, String> {
    let event = value.get("event").cloned().unwrap_or(Value::Null);
    let message = event.get("message").cloned().unwrap_or(Value::Null);
    let sender = event.get("sender").cloned().unwrap_or(Value::Null);

    let message_id = message
        .get("message_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if message_id.is_empty() {
        return Err("消息缺少 message_id".to_string());
    }
    let sender_id = sender
        .get("sender_id")
        .and_then(|id| id.get("open_id"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if sender_id.is_empty() {
        return Err("消息缺少发送者 open_id".to_string());
    }
    let sender_type = sender
        .get("sender_type")
        .and_then(Value::as_str)
        .unwrap_or("user");
    if sender_type != "user" {
        // 机器人自己发的消息不再进入路由，避免回环。
        return Err("忽略机器人消息".to_string());
    }

    let chat_id = message
        .get("chat_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let chat_type = ChatType::parse(
        message
            .get("chat_type")
            .and_then(Value::as_str)
            .unwrap_or("group"),
    );
    let thread_id = message
        .get("thread_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let root_id = message
        .get("root_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let parent_id = message
        .get("parent_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let mentions: Vec<Value> = message
        .get("mentions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let bot_open_id = bot_open_id.filter(|value| !value.is_empty());
    let addressed_to_bot = chat_type == ChatType::P2p
        || mentions.iter().any(|mention| {
            let open_id = mention
                .get("id")
                .and_then(|id| id.get("open_id"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            let name = mention
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            bot_open_id.is_some_and(|bot| bot == open_id)
                || bot_name.is_some_and(|bot| !bot.is_empty() && bot == name)
        });

    let message_type = message
        .get("message_type")
        .and_then(Value::as_str)
        .unwrap_or("text");
    let content = message
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or("{}");
    let parsed = serde_json::from_str::<Value>(content).unwrap_or(Value::Null);

    let mut media = Vec::new();
    let raw_text = match message_type {
        "text" => parsed
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        "post" => post_to_text(&parsed, &message_id, &mut media),
        "image" => {
            let key = parsed
                .get("image_key")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if !key.is_empty() {
                media.push(MediaRef {
                    message_id: message_id.clone(),
                    file_key: key,
                    kind: "image".to_string(),
                    name: "image.png".to_string(),
                });
            }
            String::new()
        }
        "file" | "audio" | "media" => {
            let key = parsed
                .get("file_key")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let name = parsed
                .get("file_name")
                .and_then(Value::as_str)
                .unwrap_or(if message_type == "audio" {
                    "audio.opus"
                } else {
                    "file"
                })
                .to_string();
            if key.is_empty() {
                String::new()
            } else {
                media.push(MediaRef {
                    message_id: message_id.clone(),
                    file_key: key,
                    kind: "file".to_string(),
                    name,
                });
                String::new()
            }
        }
        "merge_forward" => tr!("（收到一条合并转发消息，暂不支持展开）"),
        "sticker" => String::new(),
        _ => String::new(),
    };

    let text = replace_mentions(&raw_text, &mentions, bot_open_id, bot_name);
    let text = text.trim().to_string();
    if text.is_empty() && media.is_empty() {
        return Err("消息没有可处理的内容".to_string());
    }

    let created_at = message
        .get("create_time")
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<i64>().ok())
        .and_then(|millis| chrono::DateTime::from_timestamp_millis(millis))
        .map(|value| value.to_rfc3339())
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    let event_id = value
        .get("header")
        .and_then(|header| header.get("event_id"))
        .and_then(Value::as_str)
        .unwrap_or(&message_id)
        .to_string();

    Ok(InboundEvent::Message(InboundMessage {
        installation_id: installation_id.to_string(),
        event_id,
        message_id,
        chat_id,
        chat_type,
        thread_id,
        root_id,
        parent_id,
        sender_id,
        sender_name: None,
        text,
        addressed_to_bot,
        media,
        created_at,
    }))
}

fn post_to_text(value: &Value, message_id: &str, media: &mut Vec<MediaRef>) -> String {
    let mut out = String::new();
    if let Some(title) = value.get("title").and_then(Value::as_str) {
        out.push_str(title);
        out.push('\n');
    }
    if let Some(paragraphs) = value.get("content").and_then(Value::as_array) {
        for paragraph in paragraphs {
            let Some(elements) = paragraph.as_array() else {
                continue;
            };
            let mut line = String::new();
            for element in elements {
                match element.get("tag").and_then(Value::as_str) {
                    Some("text") => {
                        if let Some(text) = element.get("text").and_then(Value::as_str) {
                            line.push_str(text);
                        }
                    }
                    Some("a") => {
                        if let Some(text) = element.get("text").and_then(Value::as_str) {
                            line.push_str(text);
                            if let Some(href) = element.get("href").and_then(Value::as_str) {
                                line.push_str(&format!(" ({href})"));
                            }
                        }
                    }
                    Some("at") => {
                        if let Some(name) = element.get("user_name").and_then(Value::as_str) {
                            line.push('@');
                            line.push_str(name);
                        }
                    }
                    Some("img") => {
                        if let Some(key) = element.get("image_key").and_then(Value::as_str) {
                            media.push(MediaRef {
                                message_id: message_id.to_string(),
                                file_key: key.to_string(),
                                kind: "image".to_string(),
                                name: "image.png".to_string(),
                            });
                        }
                    }
                    _ => {}
                }
            }
            if !line.trim().is_empty() {
                out.push_str(line.trim());
                out.push('\n');
            }
        }
    }
    out
}

fn replace_mentions(
    text: &str,
    mentions: &[Value],
    bot_open_id: Option<&str>,
    bot_name: Option<&str>,
) -> String {
    let mut out = text.to_string();
    for mention in mentions {
        let key = mention
            .get("key")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if key.is_empty() {
            continue;
        }
        let name = mention
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let open_id = mention
            .get("id")
            .and_then(|id| id.get("open_id"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let is_bot = bot_open_id.is_some_and(|bot| !bot.is_empty() && bot == open_id)
            || bot_name.is_some_and(|bot| !bot.is_empty() && bot == name);
        let replacement = if is_bot {
            String::new()
        } else {
            format!("@{name}")
        };
        out = out.replace(key, &replacement);
    }
    out.trim().to_string()
}

fn decode_card_action(
    value: &Value,
    installation_id: &str,
) -> Result<Option<InboundEvent>, String> {
    let event = value.get("event").cloned().unwrap_or(Value::Null);
    let operator = event
        .get("operator")
        .and_then(|operator| operator.get("open_id"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if operator.is_empty() {
        return Err("卡片回调缺少操作人".to_string());
    }
    let context = event.get("context").cloned().unwrap_or(Value::Null);
    let chat_id = context
        .get("open_chat_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let message_id = context
        .get("open_message_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let action = event.get("action").cloned().unwrap_or(Value::Null);
    let action_value = action.get("value").cloned().unwrap_or(Value::Null);
    let event_id = value
        .get("header")
        .and_then(|header| header.get("event_id"))
        .and_then(Value::as_str)
        .unwrap_or(&message_id)
        .to_string();
    Ok(Some(InboundEvent::CardAction(CardAction {
        installation_id: installation_id.to_string(),
        event_id,
        chat_id,
        message_id,
        operator_id: operator,
        value: action_value,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(event_type: &str, event: serde_json::Value) -> Vec<u8> {
        serde_json::json!({
            "schema": "2.0",
            "header": { "event_id": "ev_1", "event_type": event_type },
            "event": event,
        })
        .to_string()
        .into_bytes()
    }

    #[test]
    fn decodes_text_and_strips_bot_mention() {
        let payload = envelope(
            "im.message.receive_v1",
            serde_json::json!({
                "sender": { "sender_type": "user", "sender_id": { "open_id": "ou_user" } },
                "message": {
                    "message_id": "om_1",
                    "chat_id": "oc_1",
                    "chat_type": "group",
                    "message_type": "text",
                    "content": "{\"text\":\"@_user_1 看一下这个\"}",
                    "mentions": [{
                        "key": "@_user_1",
                        "name": "Miku",
                        "id": { "open_id": "ou_bot" }
                    }]
                }
            }),
        );
        match decode(&payload, "inst", Some("ou_bot"), Some("Miku")).unwrap() {
            Some(InboundEvent::Message(message)) => {
                assert_eq!(message.text, "看一下这个");
                assert!(message.addressed_to_bot);
                assert_eq!(message.chat_id, "oc_1");
                assert_eq!(message.sender_id, "ou_user");
            }
            _ => panic!("应当解码成消息"),
        }
    }

    #[test]
    fn ignores_messages_from_other_bots() {
        let payload = envelope(
            "im.message.receive_v1",
            serde_json::json!({
                "sender": { "sender_type": "app", "sender_id": { "open_id": "ou_other_bot" } },
                "message": {
                    "message_id": "om_2",
                    "chat_id": "oc_1",
                    "chat_type": "p2p",
                    "message_type": "text",
                    "content": "{\"text\":\"hi\"}"
                }
            }),
        );
        assert!(decode(&payload, "inst", Some("ou_bot"), None).is_err());
    }

    #[test]
    fn decodes_card_action() {
        let payload = envelope(
            "card.action.trigger",
            serde_json::json!({
                "operator": { "open_id": "ou_user" },
                "context": { "open_chat_id": "oc_1", "open_message_id": "om_1" },
                "action": { "value": { "kind": "permission", "requestId": "r1", "optionId": "allow" } }
            }),
        );
        match decode(&payload, "inst", None, None).unwrap() {
            Some(InboundEvent::CardAction(action)) => {
                assert_eq!(action.operator_id, "ou_user");
                assert_eq!(action.value["kind"], "permission");
            }
            _ => panic!("应当解码成卡片回调"),
        }
    }
}
