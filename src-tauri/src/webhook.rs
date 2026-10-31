use std::{
    io::Read,
    sync::atomic::{AtomicUsize, Ordering},
};

use tauri::{AppHandle, Manager};
use tiny_http::{Header, Method, Response, Server, StatusCode};

use crate::{commands::process_automation_webhook, database::AppState, models::WebhookInput};

const MAX_BODY_BYTES: usize = 256 * 1024;
/// 每个请求一个线程处理，同时在处理的请求超过这个数就直接回 503。
/// tiny_http 不暴露底层 socket，没法给读请求体设超时；发完头不发体的客户端只会占住一个名额。
const MAX_CONCURRENT_REQUESTS: usize = 8;
static ACTIVE_REQUESTS: AtomicUsize = AtomicUsize::new(0);

struct ActiveSlot;

impl Drop for ActiveSlot {
    fn drop(&mut self) {
        ACTIVE_REQUESTS.fetch_sub(1, Ordering::AcqRel);
    }
}

pub fn start(app: AppHandle) {
    let port = {
        let state = app.state::<AppState>();
        let Ok(db) = state.lock_db() else {
            return;
        };
        db.query_row(
            "SELECT value FROM app_settings WHERE key='webhookPort'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(43127)
    };
    std::thread::spawn(move || {
        let Ok(server) = Server::http(format!("127.0.0.1:{port}")) else {
            eprintln!("Multara Webhook 服务无法监听 127.0.0.1:{port}");
            return;
        };
        for request in server.incoming_requests() {
            if ACTIVE_REQUESTS.fetch_add(1, Ordering::AcqRel) >= MAX_CONCURRENT_REQUESTS {
                ACTIVE_REQUESTS.fetch_sub(1, Ordering::AcqRel);
                respond(request, 503, &error_body("busy"));
                continue;
            }
            let slot = ActiveSlot;
            let app = app.clone();
            let _ = std::thread::Builder::new()
                .name("multara-webhook-request".into())
                .spawn(move || {
                    let _slot = slot;
                    handle_request(request, &app);
                });
        }
    });
}

fn error_body(error: &str) -> String {
    serde_json::json!({ "error": error }).to_string()
}

fn handle_request(mut request: tiny_http::Request, app: &AppHandle) {
    if request.method() != &Method::Post {
        respond(request, 405, r#"{"error":"method_not_allowed"}"#);
        return;
    }
    let token = request
        .url()
        .strip_prefix("/webhooks/")
        .filter(|value| !value.is_empty() && !value.contains('/'))
        .map(str::to_string);
    let Some(token) = token else {
        respond(request, 404, r#"{"error":"not_found"}"#);
        return;
    };
    let event = request_header(&request, "X-Multara-Event");
    let signature = request_header(&request, "X-Multara-Signature");
    let dedupe_key = request_header(&request, "X-Multara-Delivery");
    if request
        .body_length()
        .is_some_and(|length| length > MAX_BODY_BYTES)
    {
        respond(request, 413, r#"{"error":"payload_too_large"}"#);
        return;
    }
    let mut body = Vec::new();
    if request
        .as_reader()
        .take((MAX_BODY_BYTES + 1) as u64)
        .read_to_end(&mut body)
        .is_err()
    {
        respond(request, 400, r#"{"error":"invalid_body"}"#);
        return;
    }
    if body.len() > MAX_BODY_BYTES {
        respond(request, 413, r#"{"error":"payload_too_large"}"#);
        return;
    }
    let Some(dedupe_key) = dedupe_key else {
        respond(request, 400, r#"{"error":"missing_delivery_id"}"#);
        return;
    };
    let payload = match String::from_utf8(body) {
        Ok(payload) => payload,
        Err(_) => {
            respond(request, 400, r#"{"error":"body_must_be_utf8"}"#);
            return;
        }
    };
    let input = WebhookInput {
        token,
        event,
        dedupe_key,
        payload,
        signature,
    };
    match process_automation_webhook(input, app) {
        Ok(status) => respond(
            request,
            202,
            &serde_json::json!({ "status": status }).to_string(),
        ),
        Err(error) => respond(
            request,
            if is_auth_error(&error) {
                401
            } else {
                400
            },
            &error_body(&error),
        ),
    }
}

/// 错误文案已按界面语言翻译，只能和同样翻译过的原文逐条比对
fn is_auth_error(error: &str) -> bool {
    [
        tr!("Webhook 缺少签名"),
        tr!("Webhook 签名格式无效"),
        tr!("Webhook 签名校验失败"),
        tr!("Webhook token 无效或触发器已停用"),
    ]
    .iter()
    .any(|text| text == error)
}

fn request_header(request: &tiny_http::Request, name: &'static str) -> Option<String> {
    request
        .headers()
        .iter()
        .find(|header| header.field.equiv(name))
        .map(|header| header.value.as_str().to_string())
}

fn respond(request: tiny_http::Request, status: u16, body: &str) {
    let response = Response::from_string(body)
        .with_status_code(StatusCode(status))
        .with_header(
            Header::from_bytes("Content-Type", "application/json; charset=utf-8")
                .expect("静态响应头有效"),
        );
    let _ = request.respond(response);
}
