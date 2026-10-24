//! 飞书消息卡片。

use serde_json::{json, Value};

pub fn markdown_card(text: &str) -> Value {
    json!({
        "schema": "2.0",
        "config": { "update_multi": true },
        "body": {
            "elements": [
                { "tag": "markdown", "content": text }
            ]
        }
    })
}

pub fn text_card(text: &str) -> Value {
    json!({
        "schema": "2.0",
        "config": { "update_multi": true },
        "body": {
            "elements": [
                { "tag": "div", "text": { "tag": "plain_text", "content": text } }
            ]
        }
    })
}

pub fn permission_card(
    run_id: &str,
    request_id: &str,
    tool: &str,
    options: &[(String, String)],
) -> Value {
    let mut elements = vec![json!({
        "tag": "markdown",
        "content": format!("**需要确认**\n工具：`{tool}`")
    })];
    let mut actions = Vec::new();
    for (option_id, label) in options {
        actions.push(json!({
            "tag": "button",
            "text": { "tag": "plain_text", "content": label },
            "type": if option_id.contains("allow") || option_id.contains("accept") { "primary" } else { "default" },
            "behaviors": [{
                "type": "callback",
                "value": {
                    "kind": "permission",
                    "runId": run_id,
                    "requestId": request_id,
                    "optionId": option_id,
                }
            }]
        }));
    }
    if !actions.is_empty() {
        elements.push(json!({ "tag": "action", "actions": actions }));
    }
    json!({
        "schema": "2.0",
        "config": { "update_multi": true },
        "body": { "elements": elements }
    })
}

pub fn questions_card(run_id: &str, questions: &[Value]) -> Value {
    let mut elements = Vec::new();
    for (index, question) in questions.iter().enumerate() {
        let header = question
            .get("header")
            .and_then(Value::as_str)
            .unwrap_or("确认");
        let text = question
            .get("question")
            .and_then(Value::as_str)
            .unwrap_or_default();
        elements.push(json!({
            "tag": "markdown",
            "content": format!("**{header}**\n{text}")
        }));
        if let Some(options) = question.get("options").and_then(Value::as_array) {
            let mut actions = Vec::new();
            for option in options {
                let label = option
                    .get("label")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                actions.push(json!({
                    "tag": "button",
                    "text": { "tag": "plain_text", "content": label },
                    "type": "default",
                    "behaviors": [{
                        "type": "callback",
                        "value": {
                            "kind": "question",
                            "runId": run_id,
                            "questionIndex": index,
                            "label": label,
                            "description": option.get("description").cloned().unwrap_or(Value::Null),
                        }
                    }]
                }));
            }
            if !actions.is_empty() {
                elements.push(json!({ "tag": "action", "actions": actions }));
            }
        }
    }
    json!({
        "schema": "2.0",
        "config": { "update_multi": true },
        "body": { "elements": elements }
    })
}
