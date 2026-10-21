//! MCP 远程服务的 OAuth：发现元数据、动态注册、打开系统浏览器、本地回调换 token。
//! 流程对齐 Multica `remotemcp` 和 VS Code 的 MCP 授权，浏览器走系统默认（macOS 的 `open`）。

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const DISCOVERY_TIMEOUT_SECS: &str = "20";
const CALLBACK_WAIT: Duration = Duration::from_secs(180);
const MAX_RESPONSE_BYTES: usize = 1_000_000;
const CLIENT_NAME: &str = "Multara";
const PROTOCOL_VERSION: &str = "2025-03-26";

pub fn has_token(name: &str) -> bool {
    token_path(name).is_some_and(|path| path.is_file())
}

pub fn sign_out(name: &str) -> Result<(), String> {
    let Some(path) = token_path(name) else {
        return Ok(());
    };
    if path.is_file() {
        fs::remove_file(path).map_err(|error| format!("删除授权失败：{error}"))?;
    }
    Ok(())
}

/// 读取已保存的 access token；快过期且有 refresh token 时先刷新再返回。
pub fn access_token(name: &str) -> Option<String> {
    let path = token_path(name)?;
    let stored: Value = serde_json::from_str(&fs::read_to_string(&path).ok()?).ok()?;
    let token = stored.get("access_token").and_then(Value::as_str)?.to_string();
    let obtained = stored.get("obtained_at").and_then(Value::as_u64).unwrap_or(0);
    let Some(expires_in) = stored.get("expires_in").and_then(Value::as_u64) else {
        return Some(token);
    };
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|value| value.as_secs()).unwrap_or(0);
    if obtained + expires_in > now + 60 {
        return Some(token);
    }
    refresh(&path, &stored).ok().or(Some(token))
}

fn refresh(path: &PathBuf, stored: &Value) -> Result<String, String> {
    let field = |key: &str| stored.get(key).and_then(Value::as_str).unwrap_or("").to_string();
    let refresh_token = field("refresh_token");
    if refresh_token.is_empty() {
        return Err("没有 refresh token".to_string());
    }
    let mut pairs = vec![
        ("grant_type".to_string(), "refresh_token".to_string()),
        ("refresh_token".to_string(), refresh_token.clone()),
        ("client_id".to_string(), field("client_id")),
    ];
    if !field("resource").is_empty() {
        pairs.push(("resource".to_string(), field("resource")));
    }
    let response = http_exchange(
        "POST",
        &field("token_endpoint"),
        Some(form_encode(&pairs)),
        Some("application/x-www-form-urlencoded"),
    )?;
    if !(200..300).contains(&response.status) {
        return Err(format!("刷新 token 失败：HTTP {}", response.status));
    }
    let token: TokenResponse = serde_json::from_str(&response.body).map_err(|error| error.to_string())?;
    let mut next = stored.clone();
    if let Some(object) = next.as_object_mut() {
        object.insert("access_token".into(), json!(token.access_token));
        object.insert("refresh_token".into(), json!(token.refresh_token.unwrap_or(refresh_token)));
        object.insert("expires_in".into(), json!(token.expires_in));
        object.insert(
            "obtained_at".into(),
            json!(SystemTime::now().duration_since(UNIX_EPOCH).map(|value| value.as_secs()).unwrap_or(0)),
        );
    }
    fs::write(path, serde_json::to_string(&next).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    Ok(token.access_token)
}

pub fn authorize(name: &str, endpoint: &str) -> Result<(), String> {
    let endpoint = https_url(endpoint)?;
    let metadata = discover(&endpoint)?;
    let server = tiny_http::Server::http("127.0.0.1:0")
        .map_err(|error| format!("无法监听本地授权回调：{error}"))?;
    let port = callback_port(&server)?;
    let redirect_uri = format!("http://127.0.0.1:{port}/callback");
    let registration = register(&metadata, &redirect_uri)?;
    let verifier = pkce_verifier();
    let state = pkce_verifier();
    let scope = metadata.scopes.join(" ");
    let url = authorization_url(&metadata, &registration.client_id, &redirect_uri, &state, &verifier, &scope)?;
    crate::commands::open_external_url(url)?;
    let code = wait_for_code(&server, &state)?;
    let token = exchange_code(&metadata, &registration, &redirect_uri, &code, &verifier)?;
    write_token(name, &metadata, &registration, &token)
}

fn discover(endpoint: &str) -> Result<OAuthMetadata, String> {
    let probe = http_exchange(
        "POST",
        endpoint,
        Some(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": CLIENT_NAME, "version": "1" }
            }
        }).to_string()),
        Some("application/json"),
    );
    let metadata_url = probe
        .as_ref()
        .ok()
        .and_then(|response| resource_metadata_url(&response.headers))
        .unwrap_or_else(|| protected_resource_metadata_url(endpoint));
    let resource = http_json(&metadata_url)?;
    let advertised = resource.get("resource").and_then(Value::as_str).unwrap_or("");
    if !advertised.is_empty() && !same_resource(advertised, endpoint) {
        return Err("受保护资源元数据和 MCP 地址不一致".to_string());
    }
    let issuer = resource
        .get("authorization_servers")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(Value::as_str)
        .ok_or_else(|| "受保护资源没有声明授权服务器".to_string())?;
    let server = authorization_server(issuer)?;
    let methods = string_list(server.get("code_challenge_methods_supported"));
    if !methods.is_empty() && !methods.iter().any(|item| item == "S256") {
        return Err("授权服务器不支持 PKCE S256".to_string());
    }
    let authorization_endpoint = required_https(&server, "authorization_endpoint")?;
    let token_endpoint = required_https(&server, "token_endpoint")?;
    let registration_endpoint = server
        .get("registration_endpoint")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(https_url)
        .transpose()?;
    Ok(OAuthMetadata {
        resource: endpoint.to_string(),
        authorization_endpoint,
        token_endpoint,
        registration_endpoint,
        scopes: string_list(resource.get("scopes_supported")),
    })
}

fn authorization_server(issuer: &str) -> Result<Value, String> {
    let issuer = https_url(issuer)?;
    let mut last = "找不到授权服务器元数据".to_string();
    for url in authorization_metadata_urls(&issuer) {
        match http_json(&url) {
            Ok(value) => return Ok(value),
            Err(error) => last = error,
        }
    }
    Err(last)
}

fn register(metadata: &OAuthMetadata, redirect_uri: &str) -> Result<ClientRegistration, String> {
    let endpoint = metadata
        .registration_endpoint
        .as_deref()
        .ok_or_else(|| "授权服务器需要预先注册的客户端，当前还不能自动授权".to_string())?;
    let response = http_json_body(
        endpoint,
        json!({
            "client_name": CLIENT_NAME,
            "redirect_uris": [redirect_uri],
            "grant_types": ["authorization_code", "refresh_token"],
            "response_types": ["code"],
            "token_endpoint_auth_method": "none"
        })
        .to_string(),
    )?;
    let client_id = response
        .get("client_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "动态注册没有返回 client id".to_string())?
        .to_string();
    Ok(ClientRegistration { client_id })
}

fn authorization_url(
    metadata: &OAuthMetadata,
    client_id: &str,
    redirect_uri: &str,
    state: &str,
    verifier: &str,
    scope: &str,
) -> Result<String, String> {
    let mut url = reqwest_free_parse(&metadata.authorization_endpoint)?;
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let mut query = url.query_pairs().into_owned().collect::<Vec<_>>();
    query.extend([
        ("response_type".to_string(), "code".to_string()),
        ("client_id".to_string(), client_id.to_string()),
        ("redirect_uri".to_string(), redirect_uri.to_string()),
        ("state".to_string(), state.to_string()),
        ("code_challenge".to_string(), challenge),
        ("code_challenge_method".to_string(), "S256".to_string()),
        ("resource".to_string(), metadata.resource.clone()),
    ]);
    if !scope.trim().is_empty() {
        query.push(("scope".to_string(), scope.trim().to_string()));
    }
    url.set_query(Some(&form_encode(&query)));
    Ok(url.to_string())
}

fn exchange_code(
    metadata: &OAuthMetadata,
    registration: &ClientRegistration,
    redirect_uri: &str,
    code: &str,
    verifier: &str,
) -> Result<TokenResponse, String> {
    let body = form_encode(&[
        ("grant_type".to_string(), "authorization_code".to_string()),
        ("code".to_string(), code.to_string()),
        ("redirect_uri".to_string(), redirect_uri.to_string()),
        ("client_id".to_string(), registration.client_id.clone()),
        ("code_verifier".to_string(), verifier.to_string()),
        ("resource".to_string(), metadata.resource.clone()),
    ]);
    let response = http_exchange(
        "POST",
        &metadata.token_endpoint,
        Some(body),
        Some("application/x-www-form-urlencoded"),
    )?;
    if !(200..300).contains(&response.status) {
        return Err(format!("换取 token 失败：HTTP {}", response.status));
    }
    serde_json::from_str(&response.body).map_err(|error| format!("token 响应无法解析：{error}"))
}

fn wait_for_code(server: &tiny_http::Server, expected_state: &str) -> Result<String, String> {
    let request = server
        .recv_timeout(CALLBACK_WAIT)
        .map_err(|_| "等待授权超时，请重试".to_string())?
        .ok_or_else(|| "等待授权超时，请重试".to_string())?;
    let url = request.url().to_string();
    let code = query_value(&url, "code");
    let state = query_value(&url, "state");
    let error = query_value(&url, "error");
    let page = if error.is_some() || state.as_deref() != Some(expected_state) || code.is_none() {
        "授权没有完成，可以关闭这个页面。"
    } else {
        "授权完成，可以关闭这个页面。"
    };
    let header = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/plain; charset=utf-8"[..])
        .map_err(|_| "回调响应失败".to_string())?;
    let _ = request.respond(tiny_http::Response::from_string(page).with_header(header));
    if let Some(error) = error {
        return Err(format!("授权被拒绝：{error}"));
    }
    if state.as_deref() != Some(expected_state) {
        return Err("授权回调的 state 不一致".to_string());
    }
    code.ok_or_else(|| "授权回调没有 code".to_string())
}

fn write_token(
    name: &str,
    metadata: &OAuthMetadata,
    registration: &ClientRegistration,
    token: &TokenResponse,
) -> Result<(), String> {
    let path = token_path(name).ok_or_else(|| "无法定位授权文件".to_string())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let payload = json!({
        "access_token": token.access_token,
        "refresh_token": token.refresh_token,
        "expires_in": token.expires_in,
        "token_type": token.token_type,
        "client_id": registration.client_id,
        "token_endpoint": metadata.token_endpoint,
        "resource": metadata.resource,
        "obtained_at": SystemTime::now().duration_since(UNIX_EPOCH).map(|value| value.as_secs()).unwrap_or(0),
    });
    let mut file = fs::File::create(&path).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = file.set_permissions(fs::Permissions::from_mode(0o600));
    }
    file.write_all(serde_json::to_string(&payload).map_err(|error| error.to_string())?.as_bytes())
        .map_err(|error| error.to_string())
}

fn token_path(name: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    let digest = Sha256::digest(name.as_bytes());
    let file = digest.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
    Some(PathBuf::from(home).join(".multara/mcp-oauth").join(format!("{file}.json")))
}

struct OAuthMetadata {
    resource: String,
    authorization_endpoint: String,
    token_endpoint: String,
    registration_endpoint: Option<String>,
    scopes: Vec<String>,
}

struct ClientRegistration {
    client_id: String,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    token_type: Option<String>,
}

struct HttpResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

fn http_json(url: &str) -> Result<Value, String> {
    let response = http_exchange("GET", url, None, None)?;
    json_body(&response)
}

fn http_json_body(url: &str, body: String) -> Result<Value, String> {
    let response = http_exchange("POST", url, Some(body), Some("application/json"))?;
    json_body(&response)
}

fn json_body(response: &HttpResponse) -> Result<Value, String> {
    if !(200..300).contains(&response.status) {
        return Err(format!("HTTP {}", response.status));
    }
    serde_json::from_str(&response.body).map_err(|error| format!("JSON 无法解析：{error}"))
}

fn http_exchange(
    method: &str,
    url: &str,
    body: Option<String>,
    content_type: Option<&str>,
) -> Result<HttpResponse, String> {
    https_url(url)?;
    let header_file = tempfile::NamedTempFile::new().map_err(|error| error.to_string())?;
    let body_file = tempfile::NamedTempFile::new().map_err(|error| error.to_string())?;
    let mut command = crate::process::command("curl");
    command.args([
        "--silent",
        "--show-error",
        "--max-time",
        DISCOVERY_TIMEOUT_SECS,
        "--proto",
        "=https",
        "--max-redirs",
        "0",
        "--dump-header",
    ]);
    command.arg(header_file.path());
    command.arg("--output");
    command.arg(body_file.path());
    command.args(["-X", method, "-H", "Accept: application/json"]);
    if let Some(content_type) = content_type {
        command.args(["-H", &format!("Content-Type: {content_type}")]);
    }
    let payload = body.as_ref().map(|_| tempfile::NamedTempFile::new()).transpose().map_err(|error| error.to_string())?;
    if let (Some(text), Some(file)) = (body.as_ref(), payload.as_ref()) {
        fs::write(file.path(), text).map_err(|error| error.to_string())?;
        command.arg("--data-binary");
        command.arg(format!("@{}", file.path().display()));
    }
    command.arg(url);
    let output = command.output().map_err(|error| format!("无法调用 curl：{error}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if detail.is_empty() { "请求授权服务器失败".to_string() } else { detail });
    }
    let headers = fs::read(header_file.path()).map_err(|error| error.to_string())?;
    let body = fs::read(body_file.path()).map_err(|error| error.to_string())?;
    if body.len() > MAX_RESPONSE_BYTES {
        return Err("授权响应过大".to_string());
    }
    Ok(HttpResponse {
        status: status_code(&headers).unwrap_or(0),
        headers: header_pairs(&headers),
        body: String::from_utf8_lossy(&body).to_string(),
    })
}

fn status_code(headers: &[u8]) -> Option<u16> {
    let text = String::from_utf8_lossy(headers);
    text.lines().rev().find_map(|line| {
        let rest = line.strip_prefix("HTTP/")?;
        rest.split_whitespace().nth(1)?.parse().ok()
    })
}

fn header_pairs(headers: &[u8]) -> Vec<(String, String)> {
    String::from_utf8_lossy(headers)
        .lines()
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            Some((name.trim().to_string(), value.trim().to_string()))
        })
        .collect()
}

fn resource_metadata_url(headers: &[(String, String)]) -> Option<String> {
    let pattern = regex::Regex::new(r#"(?i)(?:^|[, ]+)resource_metadata="([^"]+)""#).ok()?;
    headers.iter().find_map(|(name, value)| {
        if !name.eq_ignore_ascii_case("www-authenticate") {
            return None;
        }
        pattern.captures(value).and_then(|caps| caps.get(1).map(|item| item.as_str().to_string()))
    })
}

fn protected_resource_metadata_url(endpoint: &str) -> String {
    let Ok(url) = url::Url::parse(endpoint) else {
        return String::new();
    };
    let path = url.path().trim_end_matches('/');
    format!("{}://{}/.well-known/oauth-protected-resource{path}", url.scheme(), host_port(&url))
}

fn authorization_metadata_urls(issuer: &str) -> Vec<String> {
    let Ok(url) = url::Url::parse(issuer) else {
        return Vec::new();
    };
    let path = url.path().trim_end_matches('/');
    ["oauth-authorization-server", "openid-configuration"]
        .into_iter()
        .map(|name| format!("{}://{}/.well-known/{name}{path}", url.scheme(), host_port(&url)))
        .collect()
}

fn host_port(url: &url::Url) -> String {
    match url.port() {
        Some(port) => format!("{}:{port}", url.host_str().unwrap_or("")),
        None => url.host_str().unwrap_or("").to_string(),
    }
}

fn required_https(value: &Value, field: &str) -> Result<String, String> {
    let raw = value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("授权元数据缺少 {field}"))?;
    https_url(raw)
}

fn https_url(raw: &str) -> Result<String, String> {
    let url = reqwest_free_parse(raw.trim())?;
    if url.scheme() != "https" || url.host_str().unwrap_or("").is_empty() || url.username() != "" || url.password().is_some() || url.fragment().is_some() {
        return Err("授权地址必须是不带账号和片段的 HTTPS".to_string());
    }
    Ok(url.to_string())
}

fn same_resource(left: &str, right: &str) -> bool {
    left.trim_end_matches('/') == right.trim_end_matches('/')
}

fn string_list(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default()
}

fn pkce_verifier() -> String {
    URL_SAFE_NO_PAD.encode(uuid::Uuid::new_v4().as_bytes().repeat(2))
}

fn query_value(url: &str, key: &str) -> Option<String> {
    let query = url.split_once('?')?.1;
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name == key).then(|| form_decode(value))
    })
}

fn form_encode(pairs: &[(String, String)]) -> String {
    pairs
        .iter()
        .map(|(key, value)| format!("{}={}", form_item(key), form_item(value)))
        .collect::<Vec<_>>()
        .join("&")
}

fn form_item(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn form_decode(value: &str) -> String {
    let mut bytes = Vec::new();
    let raw = value.as_bytes();
    let mut index = 0;
    while index < raw.len() {
        if raw[index] == b'%' && index + 2 < raw.len() {
            if let Ok(byte) = u8::from_str_radix(&value[index + 1..index + 3], 16) {
                bytes.push(byte);
                index += 3;
                continue;
            }
        }
        bytes.push(if raw[index] == b'+' { b' ' } else { raw[index] });
        index += 1;
    }
    String::from_utf8_lossy(&bytes).to_string()
}

fn callback_port(server: &tiny_http::Server) -> Result<u16, String> {
    server
        .server_addr()
        .to_ip()
        .map(|addr| addr.port())
        .ok_or_else(|| "本地回调地址无效".to_string())
}

fn reqwest_free_parse(raw: &str) -> Result<url::Url, String> {
    url::Url::parse(raw).map_err(|_| "授权地址无效".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_resource_keeps_the_mcp_path() {
        let url = protected_resource_metadata_url("https://project.feishu.cn/mcp_server/v1");
        assert_eq!(url, "https://project.feishu.cn/.well-known/oauth-protected-resource/mcp_server/v1");
    }

    #[test]
    fn challenge_header_points_at_metadata() {
        let headers = vec![(
            "WWW-Authenticate".to_string(),
            r#"Bearer realm="mcp", resource_metadata="https://project.feishu.cn/.well-known/oauth-protected-resource""#.to_string(),
        )];
        assert_eq!(
            resource_metadata_url(&headers).as_deref(),
            Some("https://project.feishu.cn/.well-known/oauth-protected-resource")
        );
    }
}
