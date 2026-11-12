//! MCP Apps（L3，方案 §7）：外部 MCP 工具声明的 `ui://` HTML 界面。
//!
//! - CLI 不会把 `_meta.ui` 交给 Multara，所以 UI 地址一律从 Multara 自己探测缓存的工具定义里找（R1、R2）。
//! - 页面的 `tools/call` / `resources/read` 走 Multara 自己的常驻连接（R3），不经过智能体。
//!   注意：stdio server 意味着另起一个进程，和 CLI 里那个互不相识（§7.4 第 2 条）。
//! - 托管：每个 server 一个端口（一个源），端口记在 app_settings 里，下次优先绑原端口，
//!   页面的 localStorage 大多数时候能跨重启保留（R21）。CSP 照 VS Code `_injectPreamble`（R19）。

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use base64::Engine as _;
use rusqlite::params;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager, State};

use crate::{
    database::AppState,
    mcp_probe::{McpSession, McpToolUi},
    mcp_tools_cache::McpToolsCache,
    ui_host::{self, HostRequest, HostResponse},
};

const IDLE: Duration = Duration::from_secs(5 * 60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_HTML_BYTES: usize = 5 * 1024 * 1024;
const CALLS_PER_MINUTE: usize = 60;

struct Pooled {
    session: McpSession,
    last_used: Instant,
}

struct HostedServer {
    site: ui_host::HostedSite,
    /// 资源 id → (HTML, CSP)
    pages: Arc<Mutex<HashMap<String, (String, String)>>>,
}

#[derive(Default)]
pub struct McpAppHost {
    /// server key（`owner:name`）→ 常驻连接
    pool: Mutex<HashMap<String, Arc<Mutex<Pooled>>>>,
    hosted: Mutex<HashMap<String, HostedServer>>,
    /// server key → 最近一分钟的 tools/call 时间
    calls: Mutex<HashMap<String, Vec<Instant>>>,
}

pub fn mcp_apps_enabled(db: &rusqlite::Connection) -> bool {
    crate::commands::genui_enabled(db)
        && crate::commands::setting_value(db, "mcpAppsEnabled", "true") == "true"
}

/// `mcp__<server>__<tool>`（claude 原生；codex 的在归一化时改成了同样格式，R27）。
/// server 或 tool 自己可能带 `__`，所以返回所有可能的拆法，由调用方按缓存匹配。
pub fn split_tool_name(name: &str) -> Vec<(String, String)> {
    let Some(rest) = name.strip_prefix("mcp__") else {
        return Vec::new();
    };
    rest.match_indices("__")
        .map(|(index, _)| (rest[..index].to_string(), rest[index + 2..].to_string()))
        .filter(|(server, tool)| !server.is_empty() && !tool.is_empty())
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedTool {
    pub owner: String,
    pub server: String,
    pub tool: String,
    pub ui: McpToolUi,
}

impl ResolvedTool {
    pub fn server_key(&self) -> String {
        format!("{}:{}", self.owner, self.server)
    }
}

/// 在缓存的工具定义里找这个工具的 UI。同名 server 有多份配置时取第一个声明了这个工具 UI 的。
pub fn resolve_tool(entries: &[crate::mcp_tools_cache::McpToolsCacheEntry], tool_name: &str) -> Option<ResolvedTool> {
    for (server, tool) in split_tool_name(tool_name) {
        for entry in entries.iter().filter(|entry| entry.name == server) {
            if let Some(ui) = entry
                .probe
                .tools
                .iter()
                .find(|item| item.name == tool)
                .and_then(|item| item.ui.clone())
                .filter(|ui| !ui.resource_uri.is_empty())
            {
                return Some(ResolvedTool {
                    owner: entry.owner.clone(),
                    server: server.clone(),
                    tool: tool.clone(),
                    ui,
                });
            }
        }
    }
    None
}

/// 域名列表只做最小处理：丢掉能拼出额外指令的项（`;` `,` 空白、换行、引号）
fn domains(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|item| {
            !item.is_empty()
                && !item.chars().any(|ch| ch == ';' || ch == ',' || ch == '\'' || ch == '"' || ch.is_whitespace())
        })
        .map(str::to_string)
        .collect()
}

/// 照 VS Code `chatMcpAppModel.ts::_injectPreamble` 的整条策略，再加 frame-ancestors。
pub fn app_csp(meta_csp: Option<&Value>) -> String {
    let resource = domains(meta_csp.and_then(|csp| csp.get("resourceDomains"))).join(" ");
    let connect = domains(meta_csp.and_then(|csp| csp.get("connectDomains"))).join(" ");
    let frame = domains(meta_csp.and_then(|csp| csp.get("frameDomains")));
    let base = domains(meta_csp.and_then(|csp| csp.get("baseUriDomains")));
    let join = |head: &str, extra: &str| {
        if extra.is_empty() {
            head.to_string()
        } else {
            format!("{head} {extra}")
        }
    };
    [
        "default-src 'none'".to_string(),
        join("script-src 'self' 'unsafe-inline' 'unsafe-eval'", &resource),
        join("style-src 'self' 'unsafe-inline' 'unsafe-eval'", &resource),
        join("connect-src 'self'", &connect),
        join("img-src 'self' data:", &resource),
        join("font-src 'self'", &resource),
        join("media-src 'self' data:", &resource),
        format!("frame-src {}", if frame.is_empty() { "'none'".to_string() } else { frame.join(" ") }),
        "object-src 'none'".to_string(),
        format!("base-uri {}", if base.is_empty() { "'self'".to_string() } else { base.join(" ") }),
        format!("frame-ancestors {}", ui_host::frame_ancestors()),
    ]
    .join("; ")
}

/// `resources/read` 的结果 → HTML + 资源上的 `_meta.ui`（照 `readResourceContentToHtml`）
pub fn resource_html(result: &Value) -> Result<(String, Value), String> {
    let content = result
        .get("contents")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .ok_or_else(|| tr!("界面资源是空的"))?;
    let mime = content.get("mimeType").and_then(Value::as_str).unwrap_or("text/html");
    let mime_base = mime.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    if mime_base != "text/html" {
        return Err(tr!("界面资源不是 HTML：{mime}", mime = mime));
    }
    let html = if let Some(text) = content.get("text").and_then(Value::as_str) {
        text.to_string()
    } else if let Some(blob) = content.get("blob").and_then(Value::as_str) {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(blob.trim())
            .map_err(|error| error.to_string())?;
        String::from_utf8(bytes).map_err(|error| error.to_string())?
    } else {
        return Err(tr!("界面资源是空的"));
    };
    if html.len() > MAX_HTML_BYTES {
        return Err(tr!("界面资源超过 5 MB"));
    }
    let meta = content.pointer("/_meta/ui").cloned().unwrap_or(Value::Null);
    Ok((html, meta))
}

fn page_id(uri: &str) -> String {
    uri.bytes().map(|byte| format!("{byte:02x}")).collect()
}

impl McpAppHost {
    fn session(&self, app: &AppHandle, owner: &str, server: &str) -> Result<Arc<Mutex<Pooled>>, String> {
        let key = format!("{owner}:{server}");
        if let Some(existing) = self.pool.lock().map_err(|_| "lock")?.get(&key).cloned() {
            return Ok(existing);
        }
        let entry_json = crate::resources::mcp_entry_json(owner, server)?;
        let wrapped: Value = serde_json::from_str(&entry_json).map_err(|error| error.to_string())?;
        let entry = wrapped.get(server).cloned().unwrap_or(Value::Null);
        let _ = app;
        let session = McpSession::open(server, &entry)?;
        let pooled = Arc::new(Mutex::new(Pooled {
            session,
            last_used: Instant::now(),
        }));
        self.pool
            .lock()
            .map_err(|_| "lock")?
            .insert(key, pooled.clone());
        Ok(pooled)
    }

    fn request(&self, app: &AppHandle, owner: &str, server: &str, method: &str, params: Value) -> Result<Value, String> {
        let pooled = self.session(app, owner, server)?;
        let mut guard = pooled.lock().map_err(|_| "lock")?;
        guard.last_used = Instant::now();
        let result = guard.session.request(method, params, REQUEST_TIMEOUT);
        if result.is_err() {
            // 连接坏了：丢掉，下次重连
            drop(guard);
            if let Ok(mut pool) = self.pool.lock() {
                pool.remove(&format!("{owner}:{server}"));
            }
        }
        result
    }

    /// 读 UI 资源并托管，返回页面地址
    fn host_page(&self, app: &AppHandle, resolved: &ResolvedTool) -> Result<(String, Value), String> {
        let key = resolved.server_key();
        let result = self.request(
            app,
            &resolved.owner,
            &resolved.server,
            "resources/read",
            json!({ "uri": resolved.ui.resource_uri }),
        )?;
        let (html, meta) = resource_html(&result)?;
        let csp = app_csp(meta.get("csp"));
        let id = page_id(&resolved.ui.resource_uri);
        let mut hosted = self.hosted.lock().map_err(|_| "lock")?;
        if hosted.get(&key).is_some_and(|server| server.site.is_stopped()) {
            hosted.remove(&key);
        }
        if !hosted.contains_key(&key) {
            let pages: Arc<Mutex<HashMap<String, (String, String)>>> = Arc::default();
            let handler_pages = pages.clone();
            let handler: ui_host::Handler = Arc::new(move |request: HostRequest| {
                if request.method != "GET" && request.method != "HEAD" {
                    return HostResponse::Status(405);
                }
                let Some(id) = request.path.strip_prefix("/r/") else {
                    return HostResponse::Status(404);
                };
                match handler_pages.lock().ok().and_then(|pages| pages.get(id).cloned()) {
                    Some((html, csp)) => HostResponse::HtmlWithCsp { html, csp },
                    None => HostResponse::Status(404),
                }
            });
            let preferred = preferred_port(app, &key);
            let site = ui_host::start(app_csp(None), preferred, handler)?;
            remember_port(app, &key, site.port);
            hosted.insert(key.clone(), HostedServer { site, pages });
        }
        let server = hosted.get(&key).ok_or_else(|| "hosted".to_string())?;
        server
            .pages
            .lock()
            .map_err(|_| "lock")?
            .insert(id.clone(), (html, csp));
        Ok((format!("{}r/{id}", server.site.url()), meta))
    }

    fn allow_call(&self, key: &str) -> bool {
        let Ok(mut calls) = self.calls.lock() else {
            return false;
        };
        let window = calls.entry(key.to_string()).or_default();
        let cutoff = Instant::now() - Duration::from_secs(60);
        window.retain(|at| *at > cutoff);
        if window.len() >= CALLS_PER_MINUTE {
            return false;
        }
        window.push(Instant::now());
        true
    }

    /// 页面能调的工具：同一 server、`visibility` 含 `app`（缺省视为含，R18）
    fn app_tool_allowed(&self, app: &AppHandle, owner: &str, server: &str, tool: &str) -> Result<bool, String> {
        let pooled = self.session(app, owner, server)?;
        let guard = pooled.lock().map_err(|_| "lock")?;
        Ok(guard
            .session
            .tools
            .iter()
            .find(|item| item.name == tool)
            .is_some_and(|item| item.ui.as_ref().map(McpToolUi::app_visible).unwrap_or(true)))
    }

    pub fn close_all(&self) {
        if let Ok(mut pool) = self.pool.lock() {
            pool.clear();
        }
        if let Ok(mut hosted) = self.hosted.lock() {
            for server in hosted.values() {
                server.site.stop();
            }
            hosted.clear();
        }
    }
}

fn preferred_port(app: &AppHandle, key: &str) -> Option<u16> {
    let state = app.state::<AppState>();
    let db = state.lock_db().ok()?;
    crate::commands::setting_value(&db, &format!("mcpAppPort.{key}"), "")
        .parse()
        .ok()
}

fn remember_port(app: &AppHandle, key: &str, port: u16) {
    let state = app.state::<AppState>();
    let Ok(db) = state.lock_db() else { return };
    let _ = db.execute(
        "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![format!("mcpAppPort.{key}"), port.to_string()],
    );
}

/// 空闲 5 分钟的连接关掉；设置里关了 MCP Apps 就全部关掉
pub fn start_reaper(app: AppHandle) {
    let _ = thread::Builder::new().name("mcp-app-reaper".into()).spawn(move || loop {
        thread::sleep(Duration::from_secs(60));
        let host = app.state::<McpAppHost>();
        let enabled = app
            .state::<AppState>()
            .lock_db()
            .map(|db| mcp_apps_enabled(&db))
            .unwrap_or(false);
        if !enabled {
            host.close_all();
            continue;
        }
        let Ok(mut pool) = host.pool.lock() else { continue };
        pool.retain(|_, pooled| {
            pooled
                .try_lock()
                .map(|guard| guard.last_used.elapsed() < IDLE)
                .unwrap_or(true)
        });
        drop(pool);
    });
}

fn cache_entries(app: &AppHandle) -> Vec<crate::mcp_tools_cache::McpToolsCacheEntry> {
    app.try_state::<Arc<McpToolsCache>>()
        .map(|cache| cache.list())
        .unwrap_or_default()
}

// ---------- Tauri 命令 ----------

/// 工具调用有没有界面；有就托管，返回页面地址
#[tauri::command]
pub async fn mcp_app_resolve(tool: String, app: AppHandle) -> Result<Option<Value>, String> {
    let _span = crate::diag::span("mcp_app_resolve");
    tauri::async_runtime::spawn_blocking(move || {
        {
            let state = app.state::<AppState>();
            let db = state.lock_db()?;
            if !mcp_apps_enabled(&db) {
                return Ok(None);
            }
        }
        let Some(resolved) = resolve_tool(&cache_entries(&app), &tool) else {
            return Ok(None);
        };
        let host = app.state::<McpAppHost>();
        let (url, meta) = host.host_page(&app, &resolved)?;
        let origin = url
            .split('/')
            .take(3)
            .collect::<Vec<_>>()
            .join("/");
        Ok(Some(json!({
            "serverKey": resolved.server_key(),
            "server": resolved.server,
            "tool": resolved.tool,
            "resourceUri": resolved.ui.resource_uri,
            "url": url,
            "origin": origin,
            "csp": meta.get("csp").cloned().unwrap_or(Value::Null),
            "permissions": {
                "clipboardWrite": meta.pointer("/permissions/clipboardWrite").is_some(),
            },
            "prefersBorder": meta.get("prefersBorder").and_then(Value::as_bool).unwrap_or(true),
        })))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// 页面发来的 JSON-RPC 请求里需要 MCP server 处理的部分（`tools/call`、`resources/read`、日志）
#[tauri::command]
pub async fn mcp_app_call(
    server_key: String,
    method: String,
    params: Value,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let _span = crate::diag::span("mcp_app_call");
    if !mcp_apps_enabled(&*state.lock_db()?) {
        return Err(tr!("MCP Apps 未启用"));
    }
    let (owner, server) = server_key
        .split_once(':')
        .map(|(owner, server)| (owner.to_string(), server.to_string()))
        .ok_or_else(|| tr!("无效的 server"))?;
    tauri::async_runtime::spawn_blocking(move || {
        let host = app.state::<McpAppHost>();
        match method.as_str() {
            "tools/call" => {
                let tool = params.get("name").and_then(Value::as_str).unwrap_or("");
                if !host.app_tool_allowed(&app, &owner, &server, tool)? {
                    return Err(tr!("找不到工具：{tool}", tool = tool));
                }
                if !host.allow_call(&server_key) {
                    return Err(tr!("界面调用太频繁，请稍后再试"));
                }
                host.request(&app, &owner, &server, "tools/call", params)
            }
            "resources/read" => host.request(&app, &owner, &server, "resources/read", params),
            "notifications/message" => {
                crate::diag::write(&format!("MCP_APP {server_key} log: {params}"));
                Ok(Value::Null)
            }
            _ => Err(tr!("不支持的方法：{method}", method = method)),
        }
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp_probe::{McpCapability, McpProbe};
    use crate::mcp_tools_cache::McpToolsCacheEntry;

    fn entry(owner: &str, name: &str, tools: Vec<McpCapability>) -> McpToolsCacheEntry {
        McpToolsCacheEntry {
            id: format!("mcp:{owner}:{name}"),
            owner: owner.into(),
            name: name.into(),
            probe: McpProbe {
                tools,
                ..Default::default()
            },
            updated_at_ms: 0,
        }
    }

    fn tool(name: &str, ui: Option<&str>) -> McpCapability {
        McpCapability {
            name: name.into(),
            ui: ui.map(|uri| McpToolUi {
                resource_uri: uri.into(),
                visibility: vec!["model".into(), "app".into()],
            }),
            ..Default::default()
        }
    }

    #[test]
    fn splits_mcp_tool_names() {
        assert_eq!(split_tool_name("mcp__charts__draw"), vec![("charts".into(), "draw".into())]);
        assert_eq!(
            split_tool_name("mcp__my__srv__go"),
            vec![("my".into(), "srv__go".into()), ("my__srv".into(), "go".into())]
        );
        assert!(split_tool_name("Bash").is_empty());
        assert!(split_tool_name("mcp____x").is_empty());
    }

    #[test]
    fn resolves_only_tools_with_ui() {
        let entries = vec![
            entry("claude", "charts", vec![tool("draw", Some("ui://draw")), tool("plain", None)]),
            entry("cursor", "my__srv", vec![tool("go", Some("ui://go"))]),
        ];
        let found = resolve_tool(&entries, "mcp__charts__draw").unwrap();
        assert_eq!(found.server_key(), "claude:charts");
        assert_eq!(found.ui.resource_uri, "ui://draw");
        assert!(resolve_tool(&entries, "mcp__charts__plain").is_none());
        assert!(resolve_tool(&entries, "mcp__other__draw").is_none());
        assert_eq!(resolve_tool(&entries, "mcp__my__srv__go").unwrap().server, "my__srv");
    }

    #[test]
    fn csp_matches_vscode_and_drops_injection() {
        let csp = app_csp(Some(&json!({
            "resourceDomains": ["https://cdn.example", "https://evil; script-src *"],
            "connectDomains": ["https://api.example", "a,b"],
        })));
        assert!(csp.starts_with("default-src 'none'; script-src 'self' 'unsafe-inline' 'unsafe-eval' https://cdn.example;"));
        assert!(csp.contains("connect-src 'self' https://api.example;"));
        assert!(csp.contains("img-src 'self' data: https://cdn.example;"));
        assert!(csp.contains("media-src 'self' data: https://cdn.example;"));
        assert!(csp.contains("frame-src 'none';"));
        assert!(csp.contains("object-src 'none';"));
        assert!(csp.contains("base-uri 'self';"));
        assert!(csp.contains("frame-ancestors tauri://localhost"));
        assert!(!csp.contains("evil"));
        assert!(!csp.contains("a,b"));
        let bare = app_csp(None);
        assert!(bare.contains("script-src 'self' 'unsafe-inline' 'unsafe-eval';"));
    }

    #[test]
    fn reads_html_resources() {
        let (html, meta) = resource_html(&json!({
            "contents": [{ "uri": "ui://x", "mimeType": "text/html;profile=mcp-app", "text": "<p>x</p>",
                           "_meta": { "ui": { "prefersBorder": false } } }]
        }))
        .unwrap();
        assert_eq!(html, "<p>x</p>");
        assert_eq!(meta["prefersBorder"], false);
        let blob = base64::engine::general_purpose::STANDARD.encode("<b>y</b>");
        let (html, _) = resource_html(&json!({ "contents": [{ "mimeType": "text/html", "blob": blob }] })).unwrap();
        assert_eq!(html, "<b>y</b>");
        assert!(resource_html(&json!({ "contents": [{ "mimeType": "image/png", "blob": "" }] })).is_err());
        assert!(resource_html(&json!({ "contents": [] })).is_err());
    }

    #[test]
    fn rate_limits_tool_calls() {
        let host = McpAppHost::default();
        for _ in 0..CALLS_PER_MINUTE {
            assert!(host.allow_call("a:b"));
        }
        assert!(!host.allow_call("a:b"));
        assert!(host.allow_call("other:b"));
    }
}
