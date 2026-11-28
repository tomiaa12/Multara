//! 收件箱内置浏览器：主窗口 child Webview（对齐 VS Code BrowserView），
//! 顶层浏览上下文可写 Cookie，避免 iframe 第三方 Cookie 被拦。

#[cfg(target_os = "macos")]
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::Serialize;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Mutex, OnceLock,
    },
    thread,
    time::Duration,
};
use tauri::{
    webview::{NewWindowResponse, PageLoadEvent, WebviewBuilder},
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl,
};
use url::Url;
use uuid::Uuid;

const LABEL_PREFIX: &str = "inbox-browser-";
pub const NAVIGATED_EVENT: &str = "inbox-browser:navigated";

/// 子页面里缓冲 console，供「控制台日志添加到聊天」，并记下页面报错。
const CONSOLE_INIT_SCRIPT: &str = r#"(function(){
  if (window.__multaraConsoleReady) return;
  window.__multaraConsoleReady = true;
  window.__multaraConsoleLogs = [];
  var cap = 500;
  function push(level, message) {
    try {
      window.__multaraConsoleLogs.push({ level: level, ts: Date.now(), message: String(message) });
      if (window.__multaraConsoleLogs.length > cap) {
        window.__multaraConsoleLogs.splice(0, window.__multaraConsoleLogs.length - cap);
      }
    } catch (e) {}
  }
  function textOf(value) {
    if (value instanceof Error) return value.stack || value.message || String(value);
    if (typeof value === 'string') return value;
    try { return JSON.stringify(value); }
    catch (e) { return String(value); }
  }
  function wrap(level, fn) {
    return function() {
      try {
        var parts = [];
        for (var i = 0; i < arguments.length; i++) parts.push(textOf(arguments[i]));
        push(level, parts.join(' '));
      } catch (e) {}
      return fn.apply(console, arguments);
    };
  }
  console.log = wrap('log', console.log.bind(console));
  console.info = wrap('info', console.info.bind(console));
  console.warn = wrap('warn', console.warn.bind(console));
  console.error = wrap('error', console.error.bind(console));
  console.debug = wrap('debug', console.debug.bind(console));
  window.addEventListener('error', function(event) {
    push('error', (event.message || 'error') + ' @ ' + (event.filename || '') + ':' + (event.lineno || 0));
  });
  window.addEventListener('unhandledrejection', function(event) {
    push('error', 'unhandledrejection ' + textOf(event.reason));
  });
})();"#;

const PAGE_PROBE_SCRIPT: &str = r#"(function(){
  try {
    var nav = performance.getEntriesByType('navigation')[0] || null;
    var body = document.body;
    var text = '';
    try { text = body ? (body.innerText || '').replace(/\s+/g, ' ').trim().slice(0, 160) : ''; } catch (e) {}
    var problems = (window.__multaraConsoleLogs || []).filter(function(item) {
      return item && (item.level === 'error' || item.level === 'warn');
    }).slice(-8).map(function(item) {
      return { level: item.level, message: String(item.message || '').slice(0, 400) };
    });
    return JSON.stringify({
      href: String(location.href || ''),
      title: String(document.title || ''),
      readyState: String(document.readyState || ''),
      children: body ? body.childElementCount : 0,
      text: text,
      status: nav && typeof nav.responseStatus === 'number' ? nav.responseStatus : null,
      problems: problems
    });
  } catch (error) {
    return JSON.stringify({
      href: String(location.href || ''),
      title: '',
      readyState: '',
      children: 0,
      text: '',
      status: null,
      problems: [{ level: 'error', message: String(error && (error.stack || error.message) || error) }]
    });
  }
})()"#;

#[derive(Default)]
struct Slot {
    source: String,
    visible: bool,
    known: bool,
    load_gen: u64,
    inflight: u32,
}

static SLOTS: OnceLock<Mutex<HashMap<String, Slot>>> = OnceLock::new();
static LOAD_SEQ: AtomicU64 = AtomicU64::new(0);

fn slots() -> std::sync::MutexGuard<'static, HashMap<String, Slot>> {
    SLOTS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|err| err.into_inner())
}

fn webview_label(id: &str) -> Result<String, String> {
    let id = id.trim();
    if id.is_empty()
        || id.len() > 80
        || !id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ':'))
    {
        return Err(tr!("浏览器标识无效"));
    }
    let safe: String = id.chars().map(|ch| if ch == ':' { '-' } else { ch }).collect();
    Ok(format!("{LABEL_PREFIX}{safe}"))
}

fn browser_log(label: &str, message: &str) {
    let source = slots()
        .get(label)
        .map(|slot| slot.source.clone())
        .unwrap_or_default();
    let line = if source.is_empty() {
        format!("BROWSER {message}")
    } else {
        format!("BROWSER [{source}] {message}")
    };
    crate::diag::echo(&line);
}

fn remember_source(label: &str, source: Option<String>) {
    let Some(source) = source
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return;
    };
    slots().entry(label.to_string()).or_default().source = source;
}

fn note_visible(label: &str, visible: bool, x: f64, y: f64, width: f64, height: f64) {
    let changed = {
        let mut map = slots();
        let slot = map.entry(label.to_string()).or_default();
        let changed = !slot.known || slot.visible != visible;
        slot.known = true;
        slot.visible = visible;
        changed
    };
    if !changed {
        return;
    }
    if !visible {
        browser_log(label, "隐藏");
        return;
    }
    if width < 2.0 || height < 2.0 || x < -100.0 || y < -100.0 {
        browser_log(
            label,
            &format!("显示异常 {width:.0}x{height:.0} @ {x:.0},{y:.0}"),
        );
    } else {
        browser_log(label, &format!("显示 {width:.0}x{height:.0} @ {x:.0},{y:.0}"));
    }
}

fn arm_load_watch(label: &str, url: String) {
    let gen = LOAD_SEQ.fetch_add(1, Ordering::Relaxed) + 1;
    let label = label.to_string();
    {
        let mut map = slots();
        let slot = map.entry(label.clone()).or_default();
        slot.load_gen = gen;
        slot.inflight = slot.inflight.saturating_add(1);
    }
    thread::Builder::new()
        .name("multara-browser-load".into())
        .spawn(move || {
            thread::sleep(Duration::from_secs(8));
            let pending = slots()
                .get(&label)
                .is_some_and(|slot| slot.load_gen == gen && slot.inflight > 0);
            if pending {
                browser_log(&label, &format!("加载失败 8s 内没有完成 {url}"));
            }
        })
        .ok();
}

fn note_load_finished(label: &str) {
    if let Some(slot) = slots().get_mut(label) {
        slot.inflight = slot.inflight.saturating_sub(1);
    }
}

fn schedule_probe(webview: tauri::Webview, label: String) {
    thread::Builder::new()
        .name("multara-browser-probe".into())
        .spawn(move || {
            thread::sleep(Duration::from_millis(500));
            match eval_string(&webview, PAGE_PROBE_SCRIPT) {
                Ok(raw) => {
                    for line in format_page_probe(&raw) {
                        browser_log(&label, &line);
                    }
                }
                Err(error) => browser_log(&label, &format!("页面探测失败 {error}")),
            }
        })
        .ok();
}

fn clip(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let clipped: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{clipped}…")
    } else {
        clipped
    }
}

fn format_page_probe(raw: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return vec![format!("页面探测结果无法解析 {}", clip(raw, 200))];
    };
    let href = value.get("href").and_then(|item| item.as_str()).unwrap_or("");
    let title = value.get("title").and_then(|item| item.as_str()).unwrap_or("");
    let ready = value.get("readyState").and_then(|item| item.as_str()).unwrap_or("");
    let children = value.get("children").and_then(|item| item.as_u64()).unwrap_or(0);
    let text = value.get("text").and_then(|item| item.as_str()).unwrap_or("");
    let status = value.get("status").and_then(|item| item.as_i64());
    let status_text = status.map(|code| code.to_string()).unwrap_or_else(|| "未知".into());
    let mut lines = vec![format!(
        "页面 title={} ready={} 状态={} 节点={} href={}",
        clip(title, 80),
        ready,
        status_text,
        children,
        href
    )];
    if let Some(code) = status {
        if code >= 400 {
            lines.push(format!("页面出错 HTTP {code} {href}"));
        }
    }
    if ready == "complete" && children == 0 && text.is_empty() {
        lines.push(format!("页面空白 {href}"));
    }
    if let Some(problems) = value.get("problems").and_then(|item| item.as_array()) {
        for item in problems.iter().take(8) {
            let level = item.get("level").and_then(|value| value.as_str()).unwrap_or("error");
            let message = item.get("message").and_then(|value| value.as_str()).unwrap_or("");
            let kind = if level == "warn" { "页面警告" } else { "页面出错" };
            lines.push(format!("{kind} {}", clip(message, 400)));
        }
    }
    lines
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct NavigatedPayload {
    id: String,
    url: String,
}

fn validate_http_url(raw: &str) -> Result<Url, String> {
    let url = raw.trim();
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(tr!("只能打开 http(s) 链接"));
    }
    if url
        .chars()
        .any(|ch| ch.is_whitespace() || ch.is_control() || "\"<>^|`".contains(ch))
    {
        return Err(tr!("链接包含不允许的字符"));
    }
    Url::parse(url).map_err(|error| tr!("无效链接：{error}", error = error))
}

fn data_directory(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|error| error.to_string())?
        .join("inbox-browser");
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    Ok(dir)
}

fn main_window(app: &AppHandle) -> Result<tauri::Window, String> {
    // 有子 webview 后 get_webview_window("main") 可能失败，必须用 get_window。
    app.get_window("main")
        .ok_or_else(|| tr!("主窗口不存在"))
}

fn apply_bounds(webview: &tauri::Webview, x: f64, y: f64, width: f64, height: f64) -> Result<(), String> {
    webview
        .set_position(LogicalPosition::new(x, y))
        .map_err(|error| error.to_string())?;
    webview
        .set_size(LogicalSize::new(width.max(1.0), height.max(1.0)))
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn hide_webview(webview: &tauri::Webview) -> Result<(), String> {
    let _ = webview.set_position(LogicalPosition::new(-10_000.0, -10_000.0));
    let _ = webview.set_size(LogicalSize::new(1.0, 1.0));
    webview.hide().map_err(|error| error.to_string())
}

fn emit_navigated(app: &AppHandle, id: &str, url: &str) {
    let _ = app.emit(
        NAVIGATED_EVENT,
        NavigatedPayload {
            id: id.to_string(),
            url: url.to_string(),
        },
    );
}

fn build_child(
    app: &AppHandle,
    id: String,
    label: String,
    url: Url,
) -> Result<WebviewBuilder<tauri::Wry>, String> {
    let data_dir = data_directory(app)?;
    let app_for_load = app.clone();
    let app_for_new = app.clone();
    let label_for_load = label.clone();
    let label_for_nav = label.clone();
    let label_for_window = label.clone();
    let id_for_load = id.clone();
    let id_for_nav = id;
    let builder = WebviewBuilder::new(label.as_str(), WebviewUrl::External(url))
        .data_directory(data_dir)
        .initialization_script(CONSOLE_INIT_SCRIPT)
        .on_page_load(move |webview, payload| {
            let href = payload.url().as_str().to_string();
            match payload.event() {
                PageLoadEvent::Started => {
                    browser_log(&label_for_load, &format!("加载开始 {href}"));
                }
                PageLoadEvent::Finished => {
                    note_load_finished(&label_for_load);
                    browser_log(&label_for_load, &format!("加载完成 {href}"));
                    emit_navigated(&app_for_load, &id_for_load, &href);
                    schedule_probe(webview, label_for_load.clone());
                }
            }
        })
        .on_new_window(move |url, _features| {
            let href = url.as_str().to_string();
            browser_log(&label_for_window, &format!("新窗口改由系统浏览器打开 {href}"));
            let _ = crate::commands::open_external_url(href);
            NewWindowResponse::Deny
        })
        .on_navigation(move |url| {
            let href = url.as_str().to_string();
            browser_log(&label_for_nav, &format!("导航 {href}"));
            arm_load_watch(&label_for_nav, href.clone());
            emit_navigated(&app_for_new, &id_for_nav, &href);
            true
        });

    // 不用独立的 WKWebsiteDataStore。macOS 上子 WebView 一旦走
    // dataStoreForIdentifier，主窗口新输入的中文会丢字体回退，变成方框。
    // VS Code 的内嵌浏览器是 Chromium 的 WebContentsView，地址栏留在主页面，没有这条路径。
    // 默认 store 仍然是顶层页面，Cookie 可以写。
    Ok(builder)
}

/// 创建或更新收件箱子 webview 的位置、可见性与 URL。
/// `url` 为空则只改 bounds / 可见性。
#[tauri::command]
pub async fn inbox_browser_set(
    app: AppHandle,
    browser_id: String,
    url: Option<String>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    visible: bool,
    log_label: Option<String>,
) -> Result<(), String> {
    let label = webview_label(&browser_id)?;
    remember_source(&label, log_label);
    let parsed_url = match url.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        Some(raw) => Some(validate_http_url(raw).map_err(|error| {
            browser_log(&label, &format!("地址无效 {raw}: {error}"));
            error
        })?),
        None => None,
    };

    if let Some(existing) = app.get_webview(&label) {
        if let Some(next) = parsed_url {
            navigate_logged(&label, &existing, next)?;
        }
        if visible {
            apply_bounds(&existing, x, y, width, height).map_err(|error| {
                browser_log(&label, &format!("调整位置失败 {error}"));
                error
            })?;
            existing.show().map_err(|error| {
                let error = error.to_string();
                browser_log(&label, &format!("显示失败 {error}"));
                error
            })?;
        } else {
            hide_webview(&existing).map_err(|error| {
                browser_log(&label, &format!("隐藏失败 {error}"));
                error
            })?;
        }
        note_visible(&label, visible, x, y, width, height);
        return Ok(());
    }

    if !visible {
        return Ok(());
    }

    let next = parsed_url.ok_or_else(|| {
        browser_log(&label, "创建失败 没有地址");
        tr!("请先输入地址")
    })?;
    browser_log(&label, &format!("创建 {}", next.as_str()));
    let window = main_window(&app).map_err(|error| {
        browser_log(&label, &format!("创建失败 {error}"));
        error
    })?;
    let builder = build_child(&app, browser_id, label.clone(), next.clone())?;
    let webview = window
        .add_child(
            builder,
            LogicalPosition::new(x, y),
            LogicalSize::new(width.max(1.0), height.max(1.0)),
        )
        .map_err(|error| {
            let error = error.to_string();
            browser_log(&label, &format!("创建失败 {error}"));
            error
        })?;
    webview.show().map_err(|error| {
        let error = error.to_string();
        browser_log(&label, &format!("显示失败 {error}"));
        error
    })?;
    // wry 在挂进窗口之前就开始加载，macOS 上会停在白屏。挂上之后再导航一次才会画出来。
    navigate_logged(&label, &webview, next)?;
    note_visible(&label, true, x, y, width, height);
    Ok(())
}

fn navigate_logged(label: &str, webview: &tauri::Webview, url: Url) -> Result<(), String> {
    let href = url.to_string();
    webview.navigate(url).map_err(|error| {
        let error = error.to_string();
        browser_log(label, &format!("导航失败 {href}: {error}"));
        error
    })
}

#[tauri::command]
pub async fn inbox_browser_back(app: AppHandle, browser_id: String) -> Result<(), String> {
    let (webview, label) = require_webview(&app, &browser_id)?;
    webview.eval("history.back()").map_err(|error| {
        let error = error.to_string();
        browser_log(&label, &format!("后退失败 {error}"));
        error
    })
}

#[tauri::command]
pub async fn inbox_browser_forward(app: AppHandle, browser_id: String) -> Result<(), String> {
    let (webview, label) = require_webview(&app, &browser_id)?;
    webview.eval("history.forward()").map_err(|error| {
        let error = error.to_string();
        browser_log(&label, &format!("前进失败 {error}"));
        error
    })
}

#[tauri::command]
pub async fn inbox_browser_reload(app: AppHandle, browser_id: String) -> Result<(), String> {
    let (webview, label) = require_webview(&app, &browser_id)?;
    webview.reload().map_err(|error| {
        let error = error.to_string();
        browser_log(&label, &format!("刷新失败 {error}"));
        error
    })
}

#[tauri::command]
pub async fn inbox_browser_destroy(app: AppHandle, browser_id: String) -> Result<(), String> {
    let label = webview_label(&browser_id)?;
    close_label(&app, &label);
    Ok(())
}

fn require_webview(app: &AppHandle, browser_id: &str) -> Result<(tauri::Webview, String), String> {
    let label = webview_label(browser_id)?;
    let webview = app
        .get_webview(&label)
        .ok_or_else(|| tr!("浏览器未打开"))?;
    Ok((webview, label))
}

fn eval_string(webview: &tauri::Webview, script: &str) -> Result<String, String> {
    let (tx, rx) = mpsc::channel();
    webview
        .eval_with_callback(script.to_string(), move |result| {
            let _ = tx.send(result);
        })
        .map_err(|error| error.to_string())?;
    let raw = rx
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| tr!("页面脚本执行超时"))?;
    // 回调值已经是 JSON。脚本返回字符串时再剥一层，否则菜单选择和元素结果对不上。
    match serde_json::from_str::<serde_json::Value>(&raw) {
        Ok(serde_json::Value::String(text)) => Ok(text),
        _ => Ok(raw),
    }
}

/// 在子页面执行 JS，返回 JSON 字符串结果（供选元素 / 读控制台）。
#[tauri::command]
pub async fn inbox_browser_eval(
    app: AppHandle,
    browser_id: String,
    script: String,
) -> Result<String, String> {
    if script.trim().is_empty() {
        return Err(tr!("脚本为空"));
    }
    let (webview, _) = require_webview(&app, &browser_id)?;
    eval_string(&webview, &script)
}

/// 读取缓冲的 console 日志，拼成纯文本。
#[tauri::command]
pub async fn inbox_browser_console_logs(app: AppHandle, browser_id: String) -> Result<String, String> {
    let (webview, _) = require_webview(&app, &browser_id)?;
    let raw = eval_string(
        &webview,
        "JSON.stringify(window.__multaraConsoleLogs || [])",
    )?;
    let logs: Vec<serde_json::Value> =
        serde_json::from_str(&raw).unwrap_or_default();
    if logs.is_empty() {
        return Ok(String::new());
    }
    let mut lines = Vec::with_capacity(logs.len());
    for item in logs {
        let level = item
            .get("level")
            .and_then(|v| v.as_str())
            .unwrap_or("log");
        let message = item
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        lines.push(format!("[{level}] {message}"));
    }
    Ok(lines.join("\n"))
}

/// 截取子 webview 可见区域（可选相对裁剪，单位逻辑像素），返回 PNG base64。
#[tauri::command]
pub async fn inbox_browser_screenshot(
    app: AppHandle,
    browser_id: String,
    x: Option<f64>,
    y: Option<f64>,
    width: Option<f64>,
    height: Option<f64>,
) -> Result<String, String> {
    let (webview, _) = require_webview(&app, &browser_id)?;
    let window = main_window(&app)?;
    let origin = window
        .inner_position()
        .map_err(|error| error.to_string())?;
    let pos = webview.position().map_err(|error| error.to_string())?;
    let size = webview.size().map_err(|error| error.to_string())?;
    let scale = window.scale_factor().unwrap_or(1.0).max(0.1);

    let crop_x = x.unwrap_or(0.0).max(0.0);
    let crop_y = y.unwrap_or(0.0).max(0.0);
    let crop_w = width.unwrap_or(size.width as f64 / scale).max(1.0);
    let crop_h = height.unwrap_or(size.height as f64 / scale).max(1.0);

    // screencapture -R 用逻辑点；webview.position/size 是物理像素。
    let screen_x = (origin.x as f64 + pos.x as f64) / scale + crop_x;
    let screen_y = (origin.y as f64 + pos.y as f64) / scale + crop_y;
    let path = std::env::temp_dir().join(format!("multara-browser-{}.png", Uuid::new_v4()));
    let path_str = path
        .to_str()
        .ok_or_else(|| tr!("临时路径无效"))?
        .to_string();

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (screen_x, screen_y, crop_w, crop_h, &path_str, &path);
        return Err(tr!("当前平台暂不支持浏览器截图"));
    }

    #[cfg(target_os = "macos")]
    {
        let region = format!(
            "{:.0},{:.0},{:.0},{:.0}",
            screen_x, screen_y, crop_w, crop_h
        );
        let status = std::process::Command::new("screencapture")
            .args(["-x", "-t", "png", "-R", &region, &path_str])
            .status()
            .map_err(|error| tr!("截图失败：{error}", error = error))?;
        if !status.success() {
            return Err(tr!("截图失败"));
        }
        let bytes =
            std::fs::read(&path).map_err(|error| tr!("读取截图失败：{error}", error = error))?;
        let _ = std::fs::remove_file(&path);
        if bytes.is_empty() {
            return Err(tr!("截图为空"));
        }
        Ok(BASE64.encode(bytes))
    }
}

fn close_label(app: &AppHandle, label: &str) {
    if app.get_webview(label).is_none() {
        return;
    }
    browser_log(label, "销毁");
    slots().remove(label);
    if let Some(webview) = app.get_webview(label) {
        let _ = webview.hide();
        let _ = webview.close();
    }
}

pub fn destroy(app: &AppHandle) {
    let labels: Vec<String> = app
        .webviews()
        .keys()
        .filter(|label| label.starts_with(LABEL_PREFIX))
        .cloned()
        .collect();
    for label in labels {
        close_label(app, &label);
    }
}

#[cfg(test)]
mod tests {
    use super::{format_page_probe, validate_http_url, webview_label};

    #[test]
    fn rejects_non_http_and_unsafe_chars() {
        assert!(validate_http_url("ftp://example.com").is_err());
        assert!(validate_http_url("https://example.com/a b").is_err());
        assert!(validate_http_url("https://example.com/`").is_err());
    }

    #[test]
    fn accepts_http_urls() {
        assert_eq!(
            validate_http_url(" https://example.com/path ").unwrap().as_str(),
            "https://example.com/path"
        );
    }

    #[test]
    fn page_probe_reports_http_error_blank_page_and_console_error() {
        let lines = format_page_probe(
            r#"{"href":"https://example.com/missing","title":"Missing","readyState":"complete","children":0,"text":"","status":404,"problems":[{"level":"error","message":"boom"},{"level":"warn","message":"careful"}]}"#,
        );
        assert_eq!(
            lines,
            vec![
                "页面 title=Missing ready=complete 状态=404 节点=0 href=https://example.com/missing",
                "页面出错 HTTP 404 https://example.com/missing",
                "页面空白 https://example.com/missing",
                "页面出错 boom",
                "页面警告 careful",
            ]
        );
    }

    #[test]
    fn page_probe_keeps_a_loaded_page_quiet() {
        let lines = format_page_probe(
            r#"{"href":"https://example.com/","title":"Example","readyState":"complete","children":2,"text":"hello","status":200,"problems":[]}"#,
        );
        assert_eq!(
            lines,
            vec!["页面 title=Example ready=complete 状态=200 节点=2 href=https://example.com/"]
        );
    }

    #[test]
    fn webview_labels_are_per_tab_and_reject_unsafe_ids() {
        assert_eq!(webview_label("browser:1").unwrap(), "inbox-browser-browser-1");
        assert_eq!(
            webview_label("6f0c2b7e-1").unwrap(),
            "inbox-browser-6f0c2b7e-1"
        );
        assert!(webview_label("").is_err());
        assert!(webview_label("a/b").is_err());
        assert!(webview_label("a b").is_err());
    }
}
