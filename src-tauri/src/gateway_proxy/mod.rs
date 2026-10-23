mod transform;

use std::{
    io::Read,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

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
}

pub struct GatewayProxy {
    origin: String,
    token: String,
    stopped: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl GatewayProxy {
    pub fn start(config: GatewayProxyConfig) -> Result<Self, String> {
        let server =
            Server::http("127.0.0.1:0").map_err(|error| format!("无法监听本地代理：{error}"))?;
        let address = server
            .server_addr()
            .to_ip()
            .ok_or_else(|| "本地代理地址无效".to_string())?;
        let token = format!("multara-{}", uuid::Uuid::new_v4());
        let thread_token = token.clone();
        let stopped = Arc::new(AtomicBool::new(false));
        let thread_stopped = stopped.clone();
        let handle = thread::Builder::new()
            .name("multara-gateway-proxy".into())
            .spawn(move || {
                while !thread_stopped.load(Ordering::Relaxed) {
                    match server.recv_timeout(Duration::from_millis(100)) {
                        Ok(Some(request)) => handle_request(request, &config, &thread_token),
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

fn handle_request(mut request: Request, config: &GatewayProxyConfig, token: &str) {
    if !authorized(&request, token) {
        respond_json(
            request,
            401,
            &json!({ "error": { "message": "本地代理鉴权失败" } }),
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
        respond_error(request, 405, "只支持 POST 请求");
        return;
    }
    let incoming_stream = path.contains("streamGenerateContent");
    let mut raw = Vec::new();
    if request
        .as_reader()
        .take(32 * 1024 * 1024)
        .read_to_end(&mut raw)
        .is_err()
    {
        respond_error(request, 400, "读取请求失败");
        return;
    }
    let input: Value = match serde_json::from_slice(&raw) {
        Ok(input) => input,
        Err(error) => {
            respond_error(request, 400, &format!("请求 JSON 无效：{error}"));
            return;
        }
    };
    let (mapped, body_stream) = match transform::rewrite_request(
        input,
        config.client_format,
        config.upstream_format,
        &config.model,
    ) {
        Ok(result) => result,
        Err(error) => {
            respond_error(request, 400, &error);
            return;
        }
    };
    let mut headers = forwarded_headers(&request);
    apply_upstream_auth(&mut headers, config);
    let upstream = match crate::http_client::request(
        "POST",
        &upstream_url(config),
        &headers,
        Some(&mapped.to_string()),
        600,
    ) {
        Ok(response) => response,
        Err(error) => {
            respond_error(request, 502, &format!("上游请求失败：{error}"));
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
    let output: Value = match serde_json::from_str(&upstream.body) {
        Ok(output) => output,
        Err(error) => {
            respond_error(request, 502, &format!("上游返回了无法解析的响应：{error}"));
            return;
        }
    };
    let (mapped_response, canonical) = match transform::rewrite_response(
        output,
        config.upstream_format,
        config.client_format,
        &config.model,
    ) {
        Ok(result) => result,
        Err(error) => {
            respond_error(request, 502, &error);
            return;
        }
    };
    if incoming_stream || body_stream {
        respond_text(
            request,
            200,
            "text/event-stream",
            transform::synthetic_sse(config.client_format, &canonical).into_bytes(),
        );
        return;
    }
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
    headers.push(("Authorization".into(), format!("Bearer {}", config.api_key)));
}

fn upstream_url(config: &GatewayProxyConfig) -> String {
    let base = config.api_url.trim_end_matches('/');
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
}
