//! 从凭据里认出「这是谁」：解 JWT、读 JSON 里的邮箱 / 用户 id。
//!
//! `key` 用来判断两份凭据是不是同一个账号，优先用稳定 id；什么都认不出时用内容摘要。
//! `label` 只用于显示。

use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine as _,
};
use serde_json::Value;
use sha2::{Digest, Sha256};

const GO_KEYRING_PREFIX: &str = "go-keyring-base64:";
/// 只在顶层附近找名字，避免把配置里 MCP、模型的 name 当成账号名
const NAME_MAX_DEPTH: usize = 2;
const WALK_MAX_DEPTH: usize = 8;
const DIGEST_CHARS: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub key: String,
    pub label: Option<String>,
}

#[derive(Default)]
struct Found {
    id: Option<String>,
    email: Option<String>,
    name: Option<String>,
    hint: Option<String>,
}

/// 所有项都不存在时返回 `None`（未登录）。
pub fn identify(values: &[Option<Vec<u8>>]) -> Option<Identity> {
    if values.iter().all(Option::is_none) {
        return None;
    }
    let mut found = Found::default();
    for bytes in values.iter().flatten() {
        if let Some(value) = parse(bytes) {
            walk(&value, &mut found, 0);
        } else {
            scan_lines(&String::from_utf8_lossy(bytes), &mut found);
        }
    }
    let label = found
        .email
        .clone()
        .or(found.name)
        .or(found.hint);
    let key = found
        .id
        .map(|id| format!("id:{id}"))
        .or_else(|| found.email.map(|email| format!("email:{}", email.to_lowercase())))
        .unwrap_or_else(|| format!("sha:{}", digest(values)));
    Some(Identity { key, label })
}

fn parse(bytes: &[u8]) -> Option<Value> {
    let text = std::str::from_utf8(bytes).ok()?.trim();
    if let Some(encoded) = text.strip_prefix(GO_KEYRING_PREFIX) {
        let decoded = STANDARD.decode(encoded).ok()?;
        return serde_json::from_slice(&decoded).ok();
    }
    serde_json::from_str(text).ok()
}

fn jwt_claims(token: &str) -> Option<Value> {
    let mut parts = token.split('.');
    let (_, payload, _) = (parts.next()?, parts.next()?, parts.next()?);
    let bytes = URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn walk(value: &Value, found: &mut Found, depth: usize) {
    if depth > WALK_MAX_DEPTH {
        return;
    }
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                visit_field(&key.to_ascii_lowercase(), child, found, depth);
                walk(child, found, depth + 1);
            }
        }
        Value::Array(items) => items.iter().for_each(|item| walk(item, found, depth + 1)),
        _ => {}
    }
}

fn visit_field(key: &str, value: &Value, found: &mut Found, depth: usize) {
    let text = match value {
        Value::String(text) => text.trim().to_string(),
        Value::Number(number) => number.to_string(),
        _ => return,
    };
    if text.is_empty() {
        return;
    }
    match key {
        "id_token" | "idtoken" | "access_token" | "accesstoken" => {
            if let Some(claims) = jwt_claims(&text) {
                walk(&claims, found, 0);
            }
        }
        "email" | "emailaddress" | "email_address" | "user_email" if text.contains('@') => {
            found.email.get_or_insert(text);
        }
        "sub" | "accountuuid" | "account_uuid" | "userid" | "user_id" | "account_id"
        | "accountid" => {
            found.id.get_or_insert(text);
        }
        "active" if text.contains('@') => {
            found.email.get_or_insert(text);
        }
        "login" | "username" | "displayname" | "nickname" | "name" if depth < NAME_MAX_DEPTH => {
            found.name.get_or_insert(text);
        }
        "defaultprovider" | "defaultmodel" | "model" => {
            found.hint.get_or_insert(text);
        }
        _ => {}
    }
}

/// YAML / .env 这类文本：只取 `defaultModel: xxx` 这样的一行作为显示名。
fn scan_lines(text: &str, found: &mut Found) {
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':').or_else(|| line.split_once('=')) else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim().trim_matches(['"', '\'']);
        if value.is_empty() || line.starts_with([' ', '\t']) {
            continue;
        }
        if matches!(key.as_str(), "defaultmodel" | "model" | "default_model") {
            found.hint.get_or_insert(value.to_string());
        }
    }
}

fn digest(values: &[Option<Vec<u8>>]) -> String {
    let mut hasher = Sha256::new();
    for value in values {
        match value {
            Some(bytes) => {
                hasher.update([1]);
                hasher.update((bytes.len() as u64).to_le_bytes());
                hasher.update(bytes);
            }
            None => hasher.update([0]),
        }
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
        .chars()
        .take(DIGEST_CHARS)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jwt(claims: &str) -> String {
        format!("e30.{}.sig", URL_SAFE_NO_PAD.encode(claims))
    }

    #[test]
    fn reads_identity_from_id_token() {
        let auth = format!(
            r#"{{"tokens":{{"id_token":"{}","account_id":"ws-1"}}}}"#,
            jwt(r#"{"sub":"user-1","email":"me@example.com"}"#)
        );
        let identity = identify(&[Some(auth.into_bytes())]).unwrap();
        assert_eq!(identity.key, "id:user-1");
        assert_eq!(identity.label.as_deref(), Some("me@example.com"));
    }

    #[test]
    fn decodes_go_keyring_values() {
        let inner = format!(r#"{{"id_token":"{}"}}"#, jwt(r#"{"sub":"g-1","email":"g@x.com"}"#));
        let raw = format!("{GO_KEYRING_PREFIX}{}", STANDARD.encode(inner));
        let identity = identify(&[Some(raw.into_bytes())]).unwrap();
        assert_eq!(identity.key, "id:g-1");
    }

    #[test]
    fn cursor_auth_info_and_claude_account() {
        let cursor = identify(&[
            Some(b"opaque-token".to_vec()),
            Some(br#"{"email":"c@x.com","userId":42}"#.to_vec()),
        ])
        .unwrap();
        assert_eq!(cursor.key, "id:42");
        assert_eq!(cursor.label.as_deref(), Some("c@x.com"));

        let claude =
            identify(&[None, Some(br#"{"emailAddress":"d@x.com","accountUuid":"u-9"}"#.to_vec())])
                .unwrap();
        assert_eq!(claude.key, "id:u-9");
        assert_eq!(claude.label.as_deref(), Some("d@x.com"));
    }

    #[test]
    fn unknown_content_falls_back_to_digest_with_model_hint() {
        let a = identify(&[Some(b"defaultModel: minimax/M3\napiKey: one\n".to_vec())]).unwrap();
        let b = identify(&[Some(b"defaultModel: minimax/M3\napiKey: two\n".to_vec())]).unwrap();
        assert!(a.key.starts_with("sha:"));
        assert_ne!(a.key, b.key);
        assert_eq!(a.label.as_deref(), Some("minimax/M3"));
    }

    #[test]
    fn nothing_present_is_signed_out() {
        assert_eq!(identify(&[None, None]), None);
    }
}
