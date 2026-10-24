//! 飞书开放平台 HTTP API 的最小封装（tenant token、发消息、表情、下载资源）。

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::http_client;

use super::super::Installation;

static TOKENS: OnceLock<Mutex<HashMap<String, (String, Instant)>>> = OnceLock::new();

fn tokens() -> &'static Mutex<HashMap<String, (String, Instant)>> {
    TOKENS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub struct FeishuApi {
    pub base: String,
    pub app_id: String,
    app_secret: String,
    cache_key: String,
}

impl FeishuApi {
    pub fn new(installation: &Installation, secret: &str) -> FeishuApi {
        FeishuApi {
            base: installation.config.api_base(),
            app_id: installation.config.app_id.clone(),
            app_secret: secret.to_string(),
            cache_key: format!("{}:{}", installation.id, installation.config.app_id),
        }
    }

    pub fn tenant_token(&self) -> Result<String, String> {
        if let Ok(cache) = tokens().lock() {
            if let Some((token, expires)) = cache.get(&self.cache_key) {
                if *expires > Instant::now() {
                    return Ok(token.clone());
                }
            }
        }
        let body = json!({
            "app_id": self.app_id,
            "app_secret": self.app_secret,
        });
        let value = post_json(
            &format!("{}/open-apis/auth/v3/tenant_access_token/internal", self.base),
            None,
            &body,
            30,
        )?;
        let token = value
            .get("tenant_access_token")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if token.is_empty() {
            return Err("飞书没有返回 tenant_access_token".to_string());
        }
        let expire = value
            .get("expire")
            .and_then(Value::as_u64)
            .unwrap_or(7200);
        let ttl = Duration::from_secs(expire.saturating_sub(300).max(60));
        if let Ok(mut cache) = tokens().lock() {
            cache.insert(self.cache_key.clone(), (token.clone(), Instant::now() + ttl));
        }
        Ok(token)
    }

    pub fn bot_info(&self) -> Result<(String, String), String> {
        let value = self.get("/open-apis/bot/v3/info")?;
        let bot = value.get("bot").cloned().unwrap_or(Value::Null);
        let open_id = bot
            .get("open_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let name = bot
            .get("app_name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        Ok((open_id, name))
    }

    pub fn send_text(&self, chat_id: &str, text: &str) -> Result<String, String> {
        self.send_message_raw(chat_id, "text", &json!({ "text": text }).to_string())
    }

    pub fn send_card(&self, chat_id: &str, card: &Value) -> Result<String, String> {
        self.send_message_raw(chat_id, "interactive", &card.to_string())
    }

    pub fn send_message_raw(
        &self,
        chat_id: &str,
        msg_type: &str,
        content: &str,
    ) -> Result<String, String> {
        let token = self.tenant_token()?;
        let value = post_json(
            &format!("{}/open-apis/im/v1/messages?receive_id_type=chat_id", self.base),
            Some(&token),
            &json!({
                "receive_id": chat_id,
                "msg_type": msg_type,
                "content": content,
            }),
            30,
        )?;
        Ok(message_id_of(&value))
    }

    pub fn reply(
        &self,
        message_id: &str,
        msg_type: &str,
        content: &str,
        in_thread: bool,
    ) -> Result<String, String> {
        let token = self.tenant_token()?;
        let value = post_json(
            &format!(
                "{}/open-apis/im/v1/messages/{message_id}/reply",
                self.base
            ),
            Some(&token),
            &json!({
                "msg_type": msg_type,
                "content": content,
                "reply_in_thread": in_thread,
            }),
            30,
        )?;
        Ok(message_id_of(&value))
    }

    pub fn add_typing(&self, message_id: &str) -> Result<Option<String>, String> {
        let token = self.tenant_token()?;
        let value = post_json(
            &format!(
                "{}/open-apis/im/v1/messages/{message_id}/reactions",
                self.base
            ),
            Some(&token),
            &json!({ "reaction_type": { "emoji_type": "Typing" } }),
            20,
        )?;
        Ok(value
            .get("data")
            .and_then(|data| data.get("reaction_id"))
            .and_then(Value::as_str)
            .map(str::to_string))
    }

    pub fn remove_typing(&self, message_id: &str, reaction_id: &str) {
        let Ok(token) = self.tenant_token() else {
            return;
        };
        let _ = http_client::request(
            "DELETE",
            &format!(
                "{}/open-apis/im/v1/messages/{message_id}/reactions/{reaction_id}",
                self.base
            ),
            &bearer_headers(&token),
            None,
            20,
        );
    }

    pub fn update_card(&self, message_id: &str, card: &Value) -> Result<(), String> {
        let token = self.tenant_token()?;
        let _ = http_client::request(
            "PATCH",
            &format!("{}/open-apis/im/v1/messages/{message_id}", self.base),
            &bearer_headers(&token),
            Some(&json!({ "content": card.to_string() }).to_string()),
            20,
        )?;
        Ok(())
    }

    pub fn download_resource(
        &self,
        message_id: &str,
        file_key: &str,
        kind: &str,
        dest: &std::path::Path,
    ) -> Result<(), String> {
        let token = self.tenant_token()?;
        let resource_type = if kind == "image" { "image" } else { "file" };
        let url = format!(
            "{}/open-apis/im/v1/messages/{message_id}/resources/{file_key}?type={resource_type}",
            self.base
        );
        let mut response = http_client::request_stream(
            "GET",
            &url,
            &bearer_headers(&token),
            None,
            120,
        )?;
        if response.status != 200 {
            return Err(format!("下载飞书资源失败：HTTP {}", response.status));
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let mut file = fs::File::create(dest).map_err(|error| error.to_string())?;
        let mut buffer = [0u8; 32 * 1024];
        let mut total = 0usize;
        loop {
            let read = response.read(&mut buffer).map_err(|error| error.to_string())?;
            if read == 0 {
                break;
            }
            total += read;
            if total > 25 * 1024 * 1024 {
                let _ = file.flush();
                let _ = fs::remove_file(dest);
                return Err("飞书附件超过 25 MB".to_string());
            }
            file.write_all(&buffer[..read])
                .map_err(|error| error.to_string())?;
        }
        file.flush().map_err(|error| error.to_string())?;
        Ok(())
    }

    fn get(&self, path: &str) -> Result<Value, String> {
        let token = self.tenant_token()?;
        let response = http_client::request(
            "GET",
            &format!("{}{}", self.base, path),
            &bearer_headers(&token),
            None,
            30,
        )?;
        parse_response(&response.body)
    }

    /// 拿长连接地址。返回 (wss url, ping 间隔秒数)。
    pub fn ws_endpoint(&self) -> Result<(String, u64), String> {
        let value = post_json(
            &format!("{}/callback/ws/endpoint", self.base),
            None,
            &json!({ "AppID": self.app_id, "AppSecret": self.app_secret }),
            30,
        )?;
        let data = value.get("data").cloned().unwrap_or(Value::Null);
        let url = data
            .get("URL")
            .or_else(|| data.get("url"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if url.is_empty() {
            return Err("飞书没有返回长连接地址".to_string());
        }
        let ping = data
            .get("ClientConfig")
            .and_then(|config| config.get("PingInterval"))
            .and_then(Value::as_u64)
            .unwrap_or(120);
        Ok((url, ping))
    }
}

fn bearer_headers(token: &str) -> Vec<(String, String)> {
    vec![
        ("Authorization".to_string(), format!("Bearer {token}")),
        ("Content-Type".to_string(), "application/json".to_string()),
    ]
}

fn post_json(
    url: &str,
    token: Option<&str>,
    body: &Value,
    timeout: u32,
) -> Result<Value, String> {
    let headers = match token {
        Some(token) => bearer_headers(token),
        None => vec![("Content-Type".to_string(), "application/json".to_string())],
    };
    let response = http_client::request("POST", url, &headers, Some(&body.to_string()), timeout)?;
    let value = parse_response(&response.body)?;
    if value.get("code").and_then(Value::as_i64).unwrap_or(0) != 0 {
        let code = value.get("code").and_then(Value::as_i64).unwrap_or(-1);
        let message = value
            .get("msg")
            .and_then(Value::as_str)
            .unwrap_or("飞书接口返回错误");
        return Err(format!("飞书接口错误 {code}：{message}"));
    }
    Ok(value)
}

fn parse_response(body: &str) -> Result<Value, String> {
    serde_json::from_str(body).map_err(|error| {
        let clipped: String = body.chars().take(200).collect();
        format!("无法解析飞书响应（{error}）：{clipped}")
    })
}

fn message_id_of(value: &Value) -> String {
    value
        .get("data")
        .and_then(|data| data.get("message_id"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}
