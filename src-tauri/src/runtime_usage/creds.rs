//! 从本机 CLI 登录文件 / 环境变量 / 钥匙串读 token。

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde_json::Value;

use crate::runtime_accounts::keychain::{self, Keychain, SystemKeychain};

pub fn home_dir() -> Result<PathBuf, String> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| tr!("找不到用户主目录"))
}

pub fn read_json(path: &Path) -> Option<Value> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(text.trim()).ok()
}

pub fn env_nonempty(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn jwt_claims(token: &str) -> Option<Value> {
    let mut parts = token.split('.');
    let (_, payload, _) = (parts.next()?, parts.next()?, parts.next()?);
    let bytes = URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub struct CodexAuth {
    pub access_token: String,
    pub account_id: Option<String>,
    pub email: Option<String>,
    pub plan: Option<String>,
}

pub fn codex_auth(home: &Path) -> Option<CodexAuth> {
    let codex_home = env::var_os("CODEX_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".codex"));
    let root = read_json(&codex_home.join("auth.json"))?;
    let tokens = root.get("tokens")?;
    let access_token = tokens
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let mut account_id = tokens
        .get("account_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let mut email = None;
    let mut plan = None;
    if let Some(id_token) = tokens.get("id_token").and_then(Value::as_str) {
        if let Some(claims) = jwt_claims(id_token) {
            email = claims
                .get("email")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            if let Some(auth) = claims.get("https://api.openai.com/auth") {
                if account_id.is_none() {
                    account_id = auth
                        .get("chatgpt_account_id")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string);
                }
                plan = auth
                    .get("chatgpt_plan_type")
                    .or_else(|| auth.get("plan_type"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string);
            }
        }
    }
    Some(CodexAuth {
        access_token,
        account_id,
        email,
        plan,
    })
}

pub struct ClaudeAuth {
    pub access_token: String,
    pub email: Option<String>,
}

pub fn claude_auth(home: &Path) -> Option<ClaudeAuth> {
    if env_nonempty("ANTHROPIC_API_KEY").is_some() {
        // API key 走计量计费，没有订阅用量窗口
        return None;
    }
    if let Some(token) = env_nonempty("ANTHROPIC_AUTH_TOKEN") {
        return Some(ClaudeAuth {
            access_token: token,
            email: claude_email(home),
        });
    }
    let path = home.join(".claude").join(".credentials.json");
    if let Some(root) = read_json(&path) {
        if let Some(token) = root
            .pointer("/claudeAiOauth/accessToken")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Some(ClaudeAuth {
                access_token: token.to_string(),
                email: claude_email(home),
            });
        }
    }
    if keychain::supported() {
        let user = env::var("USER")
            .or_else(|_| env::var("USERNAME"))
            .unwrap_or_default();
        if !user.is_empty() {
            if let Ok(Some(raw)) = SystemKeychain.read("Claude Code-credentials", &user) {
                if let Ok(root) = serde_json::from_str::<Value>(raw.trim()) {
                    if let Some(token) = root
                        .pointer("/claudeAiOauth/accessToken")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                    {
                        return Some(ClaudeAuth {
                            access_token: token.to_string(),
                            email: claude_email(home),
                        });
                    }
                }
            }
        }
    }
    None
}

fn claude_email(home: &Path) -> Option<String> {
    let account = read_json(&home.join(".claude.json"))?
        .get("oauthAccount")?
        .clone();
    account
        .get("emailAddress")
        .or_else(|| account.get("email"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub struct GrokAuth {
    pub token: String,
    pub email: Option<String>,
}

const GROK_AUTH_KEYS: &[&str] = &[
    "https://auth.x.ai::b1a00492-073a-47ea-816f-4c329264a828",
    "https://accounts.x.ai/sign-in",
];

pub fn grok_auth(home: &Path) -> Option<GrokAuth> {
    if env_nonempty("XAI_API_KEY").is_some() {
        return None;
    }
    let root = read_json(&home.join(".grok").join("auth.json"))?;
    for key in GROK_AUTH_KEYS {
        let Some(entry) = root.get(*key) else {
            continue;
        };
        if entry.get("auth_mode").and_then(Value::as_str) == Some("api_key") {
            continue;
        }
        let token = entry
            .get("key")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())?;
        let email = entry
            .get("email")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        return Some(GrokAuth {
            token: token.to_string(),
            email,
        });
    }
    None
}

pub struct CursorAuth {
    pub token: String,
    pub email: Option<String>,
}

pub fn cursor_auth(home: &Path) -> Option<CursorAuth> {
    let email = read_json(&home.join(".cursor").join("cli-config.json")).and_then(|root| {
        root.pointer("/authInfo/email")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    });
    if let Some(token) = env_nonempty("CURSOR_AUTH_TOKEN") {
        return Some(CursorAuth { token, email });
    }
    if let Some(api_key) = env_nonempty("CURSOR_API_KEY") {
        let token = exchange_cursor_api_key(&api_key).ok()?;
        return Some(CursorAuth { token, email });
    }
    if keychain::supported() {
        if let Ok(Some(token)) = SystemKeychain.read("cursor-access-token", "cursor-user") {
            let token = token.trim().to_string();
            if !token.is_empty() {
                return Some(CursorAuth { token, email });
            }
        }
    }
    None
}

fn exchange_cursor_api_key(api_key: &str) -> Result<String, String> {
    let response = crate::http_client::request(
        "POST",
        "https://api2.cursor.sh/auth/exchange_user_api_key",
        &[
            ("Authorization".into(), format!("Bearer {api_key}")),
            ("Content-Type".into(), "application/json".into()),
        ],
        Some("{}"),
        super::PROBE_TIMEOUT_SECS,
    )?;
    if !(200..300).contains(&response.status) {
        return Err(tr!(
            "Cursor API Key 换取 token 失败（HTTP {status}）",
            status = response.status
        ));
    }
    let root: Value = serde_json::from_str(&response.body).map_err(|error| error.to_string())?;
    root.get("accessToken")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| tr!("Cursor 未返回 accessToken"))
}

pub fn opencode_go_key(home: &Path) -> Option<String> {
    if let Some(key) = env_nonempty("OPENCODE_API_KEY") {
        return Some(key);
    }
    let data_home = env::var_os("XDG_DATA_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local").join("share"));
    let root = read_json(&data_home.join("opencode").join("auth.json"))?;
    let entry = root.get("opencode-go")?;
    if entry.get("type").and_then(Value::as_str) != Some("api") {
        return None;
    }
    entry
        .get("key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Google OAuth（Gemini CLI / Antigravity）共用：用 refresh_token 换 access_token。
pub struct GoogleOAuth {
    pub access_token: String,
    pub project_id: Option<String>,
    pub email: Option<String>,
}

pub fn gemini_oauth(home: &Path) -> Option<GoogleOAuth> {
    if env_nonempty("GEMINI_API_KEY").is_some() || env_nonempty("GOOGLE_API_KEY").is_some() {
        return None;
    }
    let root = read_json(&home.join(".gemini").join("oauth_creds.json"))?;
    let email = gemini_account_email(home);
    let project_id = env_nonempty("GOOGLE_CLOUD_PROJECT")
        .or_else(|| env_nonempty("GOOGLE_CLOUD_QUOTA_PROJECT"))
        .or_else(|| {
            root.get("project_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        });
    google_access_token(&root).map(|access_token| GoogleOAuth {
        access_token,
        project_id,
        email,
    })
}

pub fn antigravity_oauth(home: &Path) -> Option<GoogleOAuth> {
    let email = gemini_account_email(home);
    let mut candidates: Vec<Value> = Vec::new();
    if keychain::supported() {
        if let Ok(Some(raw)) = SystemKeychain.read("gemini", "antigravity") {
            if let Ok(value) = serde_json::from_str::<Value>(raw.trim()) {
                candidates.push(value);
            }
        }
    }
    for path in [
        home.join(".gemini")
            .join("antigravity-acp")
            .join("acp_token.json"),
        home.join(".gemini")
            .join("antigravity")
            .join("antigravity-acp")
            .join("acp_token.json"),
        home.join(".antigravity").join("acp_token.json"),
    ] {
        if let Some(value) = read_json(&path) {
            candidates.push(value);
        }
    }
    for root in candidates {
        let project_id = root
            .get("project_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| env_nonempty("GOOGLE_CLOUD_PROJECT"));
        if let Some(access_token) = google_access_token(&root) {
            return Some(GoogleOAuth {
                access_token,
                project_id,
                email,
            });
        }
    }
    None
}

fn gemini_account_email(home: &Path) -> Option<String> {
    let accounts = read_json(&home.join(".gemini").join("google_accounts.json"))?;
    accounts
        .get("active")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.contains('@'))
        .map(str::to_string)
        .or_else(|| {
            accounts
                .get("accounts")
                .and_then(Value::as_object)
                .and_then(|map| map.keys().next().cloned())
                .filter(|email| email.contains('@'))
        })
}

fn google_access_token(root: &Value) -> Option<String> {
    let access = root
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let expiry_ms = root.get("expiry_date").and_then(Value::as_f64).or_else(|| {
        root.get("expires_at")
            .and_then(Value::as_f64)
            .map(|secs| secs * 1000.0)
    });
    let fresh = match (access.as_ref(), expiry_ms) {
        (Some(token), Some(expiry)) => {
            let now = chrono::Utc::now().timestamp_millis() as f64;
            if expiry > now + 60_000.0 {
                Some(token.clone())
            } else {
                None
            }
        }
        (Some(token), None) => Some(token.clone()),
        _ => None,
    };
    if let Some(token) = fresh {
        return Some(token);
    }
    refresh_google_token(root).ok()
}

fn refresh_google_token(root: &Value) -> Result<String, String> {
    let refresh = root
        .get("refresh_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| tr!("Google 登录缺少 refresh_token"))?;
    let client_id = root
        .get("client_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| tr!("Google 登录缺少 client_id"))?;
    let client_secret = root
        .get("client_secret")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("");
    let body = form_urlencoded(&[
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("refresh_token", refresh),
        ("grant_type", "refresh_token"),
    ]);
    let response = crate::http_client::request(
        "POST",
        "https://oauth2.googleapis.com/token",
        &[(
            "Content-Type".into(),
            "application/x-www-form-urlencoded".into(),
        )],
        Some(&body),
        super::PROBE_TIMEOUT_SECS,
    )?;
    if !(200..300).contains(&response.status) {
        return Err(tr!(
            "刷新 Google token 失败（HTTP {status}）",
            status = response.status
        ));
    }
    let root: Value = serde_json::from_str(&response.body).map_err(|error| error.to_string())?;
    root.get("access_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| tr!("Google 未返回 access_token"))
}

fn form_urlencoded(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(key, value)| format!("{}={}", urlencoding_encode(key), urlencoding_encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

fn urlencoding_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
