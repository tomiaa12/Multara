mod protocol;
mod streaming;
mod transform;

use std::{
    collections::HashMap,
    io::Read,
    panic::{self, AssertUnwindSafe},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex, MutexGuard, PoisonError,
    },
    thread,
    time::Duration,
};

use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

/// 每个请求一个线程，长 SSE 流不再挡住并行子智能体、标题生成等其它请求。
const MAX_CONCURRENT_REQUESTS: usize = 16;
const ACCEPT_POLL: Duration = Duration::from_millis(100);
const MAX_REQUEST_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApiFormat {
    Anthropic,
    Responses,
    Chat,
    Gemini,
}

impl ApiFormat {
    pub fn parse(value: &str, fallback: Self) -> Self {
        match value.trim() {
            "anthropic" => Self::Anthropic,
            "responses" => Self::Responses,
            "chat" => Self::Chat,
            "gemini" => Self::Gemini,
            _ => fallback,
        }
    }
}

#[derive(Clone)]
pub struct GatewayProxyConfig {
    pub client_format: ApiFormat,
    pub upstream_format: ApiFormat,
    pub api_url: String,
    pub api_key: String,
    pub key_style: String,
    pub model: String,
    pub client_model: String,
    pub client_provider: String,
    pub options: crate::models::GatewayOptions,
}

pub struct GatewayProxy {
    origin: String,
    token: String,
    stopped: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

#[derive(Default)]
struct GatewayState {
    gemini_tools: HashMap<String, (String, Option<String>)>,
}

impl GatewayState {
    fn prepare_gemini_request(&self, body: &mut Value) {
        for part in body
            .get_mut("contents")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
            .filter_map(|content| content.get_mut("parts").and_then(Value::as_array_mut))
            .flatten()
        {
            if let Some(call) = part.get_mut("functionCall") {
                let id = call.get("id").and_then(Value::as_str).unwrap_or_default();
                if let Some((_, Some(signature))) = self.gemini_tools.get(id) {
                    call["thoughtSignature"] = Value::String(signature.clone());
                }
            }
            if let Some(result) = part.get_mut("functionResponse") {
                let id = result.get("id").and_then(Value::as_str).unwrap_or_default();
                if let Some((name, _)) = self.gemini_tools.get(id) {
                    result["name"] = Value::String(name.clone());
                }
            }
        }
    }

    fn capture_gemini_response(&mut self, body: &mut Value) {
        for part in body
            .pointer_mut("/candidates/0/content/parts")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            let Some(call) = part.get_mut("functionCall") else {
                continue;
            };
            let name = call
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("tool")
                .to_string();
            let id = call
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| format!("gemini_{}", uuid::Uuid::new_v4().simple()));
            let signature = call
                .get("thoughtSignature")
                .and_then(Value::as_str)
                .map(str::to_string);
            call["id"] = Value::String(id.clone());
            self.gemini_tools.insert(id, (name, signature));
        }
    }
}

impl GatewayProxy {
    pub fn start(config: GatewayProxyConfig) -> Result<Self, String> {
        let server =
            Server::http("127.0.0.1:0")
                .map_err(|error| tr!("无法监听本地代理：{error}", error = error))?;
        let address = server
            .server_addr()
            .to_ip()
            .ok_or_else(|| tr!("本地代理地址无效"))?;
        let token = format!("multara-{}", uuid::Uuid::new_v4());
        let thread_token: Arc<str> = token.clone().into();
        let stopped = Arc::new(AtomicBool::new(false));
        let thread_stopped = stopped.clone();
        let config = Arc::new(config);
        let handle = thread::Builder::new()
            .name("multara-gateway-proxy".into())
            .spawn(move || {
                let state = Arc::new(Mutex::new(GatewayState::default()));
                let active = Arc::new(AtomicUsize::new(0));
                while !thread_stopped.load(Ordering::Relaxed) {
                    // 满了就先不取新请求，tiny_http 会把它们排在队列里
                    if active.load(Ordering::Acquire) >= MAX_CONCURRENT_REQUESTS {
                        thread::sleep(ACCEPT_POLL);
                        continue;
                    }
                    match server.recv_timeout(ACCEPT_POLL) {
                        Ok(Some(request)) => {
                            active.fetch_add(1, Ordering::AcqRel);
                            let slot = ActiveSlot(active.clone());
                            let (config, token, state) =
                                (config.clone(), thread_token.clone(), state.clone());
                            let _ = thread::Builder::new()
                                .name("multara-gateway-request".into())
                                .spawn(move || {
                                    let _slot = slot;
                                    // 请求已在 panic 展开时被丢弃，tiny_http 会回 500；这里只保证线程和名额不泄漏
                                    let _ = panic::catch_unwind(AssertUnwindSafe(|| {
                                        handle_request(request, &config, &token, &state)
                                    }));
                                });
                        }
                        Ok(None) => {}
                        Err(_) => break,
                    }
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(Self {
            origin: format!("http://{address}"),
            token,
            stopped,
            thread: Some(handle),
        })
    }

    pub fn origin(&self) -> &str {
        &self.origin
    }

    pub fn token(&self) -> &str {
        &self.token
    }
}

impl Drop for GatewayProxy {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

struct ActiveSlot(Arc<AtomicUsize>);

impl Drop for ActiveSlot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

/// 转换代码 panic 时回 502，而不是让请求在展开时被丢弃。
fn guarded<T>(work: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    panic::catch_unwind(AssertUnwindSafe(work))
        .unwrap_or_else(|_| Err(tr!("本地代理内部错误")))
}

fn lock_state(state: &Mutex<GatewayState>) -> MutexGuard<'_, GatewayState> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

fn handle_request(
    mut request: Request,
    config: &GatewayProxyConfig,
    token: &str,
    state: &Mutex<GatewayState>,
) {
    if !authorized(&request, token) {
        respond_json(
            request,
            401,
            &json!({ "error": { "message": tr!("本地代理鉴权失败") } }),
        );
        return;
    }
    let path = request.url().split('?').next().unwrap_or_default();
    if request.method() == &Method::Get && matches!(path, "/v1/models" | "/models") {
        respond_json(
            request,
            200,
            &json!({
                "object": "list",
                "data": [{
                    "id": config.client_model,
                    "object": "model",
                    "owned_by": "multara"
                }]
            }),
        );
        return;
    }
    if request.method() != &Method::Post {
        respond_error(request, 405, &tr!("只支持 POST 请求"));
        return;
    }
    let incoming_stream = path.contains("streamGenerateContent");
    let mut raw = Vec::new();
    if request
        .as_reader()
        .take(MAX_REQUEST_BYTES)
        .read_to_end(&mut raw)
        .is_err()
    {
        respond_error(request, 400, &tr!("读取请求失败"));
        return;
    }
    let input: Value = match serde_json::from_slice(&raw) {
        Ok(input) => input,
        Err(error) => {
            respond_error(request, 400, &tr!("请求 JSON 无效：{error}", error = error));
            return;
        }
    };
    let rewritten = panic::catch_unwind(AssertUnwindSafe(|| {
        transform::rewrite_request(
            input,
            config.client_format,
            config.upstream_format,
            &config.model,
        )
    }));
    let (mut mapped, body_stream) = match rewritten {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => {
            respond_error(request, 400, &error);
            return;
        }
        Err(_) => {
            respond_error(request, 502, &tr!("本地代理内部错误"));
            return;
        }
    };
    apply_request_options(&mut mapped, config);
    if config.upstream_format == ApiFormat::Gemini {
        lock_state(state).prepare_gemini_request(&mut mapped);
    }
    let mut headers = forwarded_headers(&request);
    apply_custom_headers(&mut headers, config, token);
    apply_upstream_auth(&mut headers, config);
    if incoming_stream || body_stream {
        mapped["stream"] = Value::Bool(true);
        headers.retain(|(key, _)| !key.eq_ignore_ascii_case("accept"));
        headers.push(("Accept".into(), "text/event-stream".into()));
        let mut upstream = match crate::http_client::request_stream(
            "POST",
            &upstream_stream_url(config),
            &headers,
            Some(&mapped.to_string()),
            600,
        ) {
            Ok(response) => response,
            Err(error) => {
                respond_error(request, 502, &tr!("上游请求失败：{error}", error = error));
                return;
            }
        };
        let status = upstream.status;
        if !(200..300).contains(&status) {
            respond_reader(request, status, "application/json", upstream);
            return;
        }
        let streaming_content = upstream.headers.iter().any(|(key, value)| {
            key.eq_ignore_ascii_case("content-type")
                && (value.contains("text/event-stream")
                    || value.contains("application/x-ndjson")
                    || value.contains("application/json-seq"))
        });
        if !streaming_content {
            let mut raw = String::new();
            if upstream
                .by_ref()
                .take(MAX_REQUEST_BYTES)
                .read_to_string(&mut raw)
                .is_err()
            {
                respond_error(request, 502, &tr!("读取上游响应失败"));
                return;
            }
            let mut output: Value = match serde_json::from_str(&raw) {
                Ok(output) => output,
                Err(error) => {
                    respond_error(
                        request,
                        502,
                        &tr!("上游返回了无法解析的响应：{error}", error = error),
                    );
                    return;
                }
            };
            if config.upstream_format == ApiFormat::Gemini {
                lock_state(state).capture_gemini_response(&mut output);
            }
            let canonical = match guarded(|| {
                transform::canonical_response(output, config.upstream_format, &config.model)
            }) {
                Ok(result) => result,
                Err(error) => {
                    respond_error(request, 502, &error);
                    return;
                }
            };
            respond_text(
                request,
                200,
                "text/event-stream",
                transform::synthetic_sse(config.client_format, &canonical).into_bytes(),
            );
            return;
        }
        if config.client_format == config.upstream_format {
            respond_reader(request, status, "text/event-stream", upstream);
        } else {
            respond_reader(
                request,
                status,
                "text/event-stream",
                streaming::BridgeReader::new(
                    upstream,
                    config.upstream_format,
                    config.client_format,
                    config.model.clone(),
                ),
            );
        }
        return;
    }
    let upstream = match crate::http_client::request(
        "POST",
        &upstream_url(config),
        &headers,
        Some(&mapped.to_string()),
        600,
    ) {
        Ok(response) => response,
        Err(error) => {
            respond_error(request, 502, &tr!("上游请求失败：{error}", error = error));
            return;
        }
    };
    if !(200..300).contains(&upstream.status) {
        let content_type = upstream
            .header("content-type")
            .unwrap_or("application/json")
            .to_string();
        respond_text(
            request,
            upstream.status,
            &content_type,
            upstream.body.into_bytes(),
        );
        return;
    }
    let mut output: Value = match serde_json::from_str(&upstream.body) {
        Ok(output) => output,
        Err(error) => {
            respond_error(request, 502, &tr!("上游返回了无法解析的响应：{error}", error = error));
            return;
        }
    };
    if config.upstream_format == ApiFormat::Gemini {
        lock_state(state).capture_gemini_response(&mut output);
    }
    let mapped_response = match guarded(|| {
        transform::rewrite_response(
            output,
            config.upstream_format,
            config.client_format,
            &config.model,
        )
    }) {
        Ok(result) => result,
        Err(error) => {
            respond_error(request, 502, &error);
            return;
        }
    };
    respond_json(request, 200, &mapped_response);
}

fn authorized(request: &Request, token: &str) -> bool {
    let bearer = request_header(request, "Authorization")
        .and_then(|value| value.strip_prefix("Bearer ").map(str::to_string));
    let direct =
        request_header(request, "x-api-key").or_else(|| request_header(request, "x-goog-api-key"));
    let query = request.url().split_once('?').map(|(_, query)| query);
    let query_key = query.and_then(|query| {
        url::form_urlencoded::parse(query.as_bytes())
            .find(|(key, _)| key == "key")
            .map(|(_, value)| value.into_owned())
    });
    bearer.as_deref() == Some(token)
        || direct.as_deref() == Some(token)
        || query_key.as_deref() == Some(token)
}

fn forwarded_headers(request: &Request) -> Vec<(String, String)> {
    let mut headers = vec![
        ("Content-Type".into(), "application/json".into()),
        ("Accept".into(), "application/json".into()),
    ];
    for header in request.headers() {
        let name = header.field.as_str().as_str();
        if matches!(
            name.to_ascii_lowercase().as_str(),
            "authorization"
                | "x-api-key"
                | "x-goog-api-key"
                | "host"
                | "content-length"
                | "content-type"
                | "accept"
        ) {
            continue;
        }
        headers.push((name.to_string(), header.value.as_str().to_string()));
    }
    headers
}

fn apply_upstream_auth(headers: &mut Vec<(String, String)>, config: &GatewayProxyConfig) {
    if config.upstream_format == ApiFormat::Anthropic && config.key_style == "x-api-key" {
        headers.push(("x-api-key".into(), config.api_key.clone()));
        headers.push(("anthropic-version".into(), "2023-06-01".into()));
        return;
    }
    if config.upstream_format == ApiFormat::Gemini && config.key_style == "x-api-key" {
        headers.push(("x-goog-api-key".into(), config.api_key.clone()));
        return;
    }
    if config.key_style == "x-api-key" {
        headers.push(("x-api-key".into(), config.api_key.clone()));
        return;
    }
    headers.push(("Authorization".into(), format!("Bearer {}", config.api_key)));
}

fn apply_custom_headers(
    headers: &mut Vec<(String, String)>,
    config: &GatewayProxyConfig,
    session_id: &str,
) {
    for (key, value) in &config.options.custom_headers {
        if !matches!(
            key.to_ascii_lowercase().as_str(),
            "authorization"
                | "x-api-key"
                | "x-goog-api-key"
                | "host"
                | "content-length"
                | "content-type"
                | "accept"
        ) {
            headers.push((key.clone(), value.clone()));
        }
    }
    if !config.options.custom_user_agent.is_empty() {
        headers.retain(|(key, _)| !key.eq_ignore_ascii_case("user-agent"));
        headers.push((
            "User-Agent".into(),
            config.options.custom_user_agent.clone(),
        ));
    }
    if config.client_provider == "opencode" || compatibility_mode(config) == "opencode" {
        headers.retain(|(key, _)| !key.eq_ignore_ascii_case("x-opencode-session"));
        headers.push(("x-opencode-session".into(), session_id.to_string()));
        if config.options.custom_user_agent.is_empty() {
            headers.push(("User-Agent".into(), "opencode/multara".into()));
        }
    }
    if config.upstream_format == ApiFormat::Anthropic {
        headers.retain(|(key, _)| !key.eq_ignore_ascii_case("anthropic-beta"));
        headers.push(("anthropic-beta".into(), "claude-code-20250219".into()));
    }
}

fn apply_request_options(body: &mut Value, config: &GatewayProxyConfig) {
    if let Some(object) = body.as_object_mut() {
        if config.options.reasoning_effort != "auto" && !config.options.reasoning_effort.is_empty()
        {
            let effort = Value::String(config.options.reasoning_effort.clone());
            match config.upstream_format {
                ApiFormat::Responses => {
                    object.insert("reasoning".into(), json!({ "effort": effort }));
                }
                ApiFormat::Chat => {
                    object.insert("reasoning_effort".into(), effort);
                }
                ApiFormat::Anthropic => {
                    object.insert("output_config".into(), json!({ "effort": effort }));
                    object.insert("thinking".into(), json!({ "type": "adaptive" }));
                }
                ApiFormat::Gemini => {}
            }
        }
        if config.options.prompt_cache && config.upstream_format == ApiFormat::Responses {
            object.insert(
                "prompt_cache_key".into(),
                Value::String(format!(
                    "{}:{}",
                    config.client_provider, config.client_model
                )),
            );
        }
        match compatibility_mode(config) {
            "codex_oauth" => {
                object.insert("store".into(), Value::Bool(false));
                object.insert("include".into(), json!(["reasoning.encrypted_content"]));
                object.remove("temperature");
            }
            "xai" => {
                object.remove("prompt_cache_key");
                object.remove("safety_identifier");
            }
            "moonshot" => {
                object.remove("parallel_tool_calls");
            }
            _ => {}
        }
    }
}

fn compatibility_mode(config: &GatewayProxyConfig) -> &str {
    let configured = config.options.compatibility_mode.trim();
    if !configured.is_empty() && configured != "auto" {
        return configured;
    }
    let url = config.api_url.to_ascii_lowercase();
    if url.contains("chatgpt.com/backend-api/codex") {
        "codex_oauth"
    } else if url.contains("api.x.ai") {
        "xai"
    } else if url.contains("moonshot") || url.contains("kimi") {
        "moonshot"
    } else if url.contains("openrouter") {
        "openrouter"
    } else if url.contains("generativelanguage.googleapis.com") {
        "gemini"
    } else if config.client_provider == "opencode" {
        "opencode"
    } else {
        "generic"
    }
}

fn upstream_url(config: &GatewayProxyConfig) -> String {
    let base = config.api_url.trim_end_matches('/');
    if config.options.full_url {
        return base.to_string();
    }
    let endpoint = match config.upstream_format {
        ApiFormat::Anthropic => "/v1/messages",
        ApiFormat::Responses => "/v1/responses",
        ApiFormat::Chat => "/v1/chat/completions",
        ApiFormat::Gemini => return gemini_url(base, &config.model),
    };
    if base.ends_with(endpoint) {
        return base.to_string();
    }
    if base.ends_with("/v1") && endpoint.starts_with("/v1/") {
        return format!("{base}/{}", endpoint.trim_start_matches("/v1/"));
    }
    format!("{base}{endpoint}")
}

fn upstream_stream_url(config: &GatewayProxyConfig) -> String {
    let url = upstream_url(config);
    if config.upstream_format != ApiFormat::Gemini || config.options.full_url {
        return url;
    }
    url.replace(":generateContent", ":streamGenerateContent?alt=sse")
}

fn gemini_url(base: &str, model: &str) -> String {
    if base.contains(":generateContent") {
        return base.to_string();
    }
    let root = base.trim_end_matches("/v1beta").trim_end_matches("/v1");
    format!("{root}/v1beta/models/{model}:generateContent")
}

fn request_header(request: &Request, name: &'static str) -> Option<String> {
    request
        .headers()
        .iter()
        .find(|header| header.field.equiv(name))
        .map(|header| header.value.as_str().to_string())
}

fn respond_json(request: Request, status: u16, body: &Value) {
    respond_text(
        request,
        status,
        "application/json; charset=utf-8",
        body.to_string().into_bytes(),
    );
}

fn respond_error(request: Request, status: u16, message: &str) {
    respond_json(request, status, &json!({ "error": { "message": message } }));
}

fn respond_text(request: Request, status: u16, content_type: &str, body: Vec<u8>) {
    let mut response = Response::from_data(body).with_status_code(StatusCode(status));
    if let Ok(header) = Header::from_bytes("Content-Type", content_type) {
        response = response.with_header(header);
    }
    if let Ok(header) = Header::from_bytes("Cache-Control", "no-cache") {
        response = response.with_header(header);
    }
    let _ = request.respond(response);
}

fn respond_reader(
    request: Request,
    status: u16,
    content_type: &str,
    reader: impl Read + Send + 'static,
) {
    let mut headers = Vec::new();
    if let Ok(header) = Header::from_bytes("Content-Type", content_type) {
        headers.push(header);
    }
    if let Ok(header) = Header::from_bytes("Cache-Control", "no-cache") {
        headers.push(header);
    }
    let response = Response::new(StatusCode(status), headers, reader, None, None);
    let _ = request.respond(response);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(format: ApiFormat, url: &str) -> GatewayProxyConfig {
        GatewayProxyConfig {
            client_format: ApiFormat::Anthropic,
            upstream_format: format,
            api_url: url.into(),
            api_key: "key".into(),
            key_style: "bearer".into(),
            model: "model".into(),
            client_model: "sonnet".into(),
            client_provider: "claude".into(),
            options: crate::models::GatewayOptions::default(),
        }
    }

    #[test]
    fn builds_upstream_urls_without_duplicate_v1() {
        assert_eq!(
            upstream_url(&config(ApiFormat::Responses, "https://example.com/v1")),
            "https://example.com/v1/responses"
        );
        assert_eq!(
            upstream_url(&config(ApiFormat::Anthropic, "https://example.com")),
            "https://example.com/v1/messages"
        );
    }

    #[test]
    fn proxies_claude_messages_to_responses_with_the_real_model() {
        let upstream = Server::http("127.0.0.1:0").unwrap();
        let upstream_address = upstream.server_addr().to_ip().unwrap();
        let upstream_thread = thread::spawn(move || {
            let mut request = upstream.recv().unwrap();
            assert_eq!(request.url(), "/v1/responses");
            let mut body = String::new();
            request.as_reader().read_to_string(&mut body).unwrap();
            let body: Value = serde_json::from_str(&body).unwrap();
            assert_eq!(body["model"], "gpt-6.1-sol");
            respond_json(
                request,
                200,
                &json!({
                    "id": "resp-1",
                    "model": "gpt-6.1-sol",
                    "output": [{
                        "type": "message",
                        "content": [{ "type": "output_text", "text": "ok" }]
                    }],
                    "usage": { "input_tokens": 2, "output_tokens": 1 }
                }),
            );
        });
        let proxy = GatewayProxy::start(GatewayProxyConfig {
            client_format: ApiFormat::Anthropic,
            upstream_format: ApiFormat::Responses,
            api_url: format!("http://{upstream_address}"),
            api_key: "upstream-key".into(),
            key_style: "bearer".into(),
            model: "gpt-6.1-sol".into(),
            client_model: "sonnet".into(),
            client_provider: "claude".into(),
            options: crate::models::GatewayOptions::default(),
        })
        .unwrap();
        let response = crate::http_client::request(
            "POST",
            &format!("{}/v1/messages", proxy.origin()),
            &[
                ("Authorization".into(), format!("Bearer {}", proxy.token())),
                ("Content-Type".into(), "application/json".into()),
            ],
            Some(
                r#"{"model":"sonnet","max_tokens":16,"messages":[{"role":"user","content":"hi"}]}"#,
            ),
            10,
        )
        .unwrap();
        assert_eq!(response.status, 200);
        let body: Value = serde_json::from_str(&response.body).unwrap();
        assert_eq!(body["content"][0]["text"], "ok");
        upstream_thread.join().unwrap();
    }

    #[test]
    fn streams_chat_upstream_as_anthropic_sse() {
        let upstream = Server::http("127.0.0.1:0").unwrap();
        let upstream_address = upstream.server_addr().to_ip().unwrap();
        let upstream_thread = thread::spawn(move || {
            let mut request = upstream.recv().unwrap();
            let mut body = String::new();
            request.as_reader().read_to_string(&mut body).unwrap();
            let body: Value = serde_json::from_str(&body).unwrap();
            assert_eq!(body["stream"], true);
            let response = Response::from_string(concat!(
                "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":null}]}\n\n",
                "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
                "data: [DONE]\n\n"
            ))
            .with_header(Header::from_bytes("Content-Type", "text/event-stream").unwrap());
            request.respond(response).unwrap();
        });
        let proxy = GatewayProxy::start(GatewayProxyConfig {
            client_format: ApiFormat::Anthropic,
            upstream_format: ApiFormat::Chat,
            api_url: format!("http://{upstream_address}"),
            api_key: "upstream-key".into(),
            key_style: "bearer".into(),
            model: "gpt-5".into(),
            client_model: "sonnet".into(),
            client_provider: "claude".into(),
            options: crate::models::GatewayOptions::default(),
        })
        .unwrap();
        let response = crate::http_client::request(
            "POST",
            &format!("{}/v1/messages", proxy.origin()),
            &[
                ("Authorization".into(), format!("Bearer {}", proxy.token())),
                ("Content-Type".into(), "application/json".into()),
            ],
            Some(r#"{"model":"sonnet","stream":true,"messages":[{"role":"user","content":"hi"}]}"#),
            10,
        )
        .unwrap();
        assert_eq!(response.status, 200);
        assert!(response.body.contains("text_delta"));
        assert!(response.body.contains("message_stop"));
        upstream_thread.join().unwrap();
    }
}
