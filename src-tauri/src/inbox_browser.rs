//! 收件箱内置浏览器：主窗口 child Webview（对齐 VS Code BrowserView），
//! 顶层浏览上下文可写 Cookie，避免 iframe 第三方 Cookie 被拦。

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::mpsc,
    time::Duration,
};
use tauri::{
    webview::{NewWindowResponse, PageLoadEvent, WebviewBuilder},
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl,
};
use url::Url;
use uuid::Uuid;

pub const WEBVIEW_LABEL: &str = "inbox-browser";
pub const NAVIGATED_EVENT: &str = "inbox-browser:navigated";

/// 子页面里缓冲 console，供「控制台日志添加到聊天」。
const CONSOLE_INIT_SCRIPT: &str = r#"(function(){
  if (window.__multaraConsoleReady) return;
  window.__multaraConsoleReady = true;
  window.__multaraConsoleLogs = [];
  var cap = 500;
  function wrap(level, fn) {
    return function() {
      try {
        var parts = [];
        for (var i = 0; i < arguments.length; i++) {
          var a = arguments[i];
          try { parts.push(typeof a === 'string' ? a : JSON.stringify(a)); }
          catch (e) { parts.push(String(a)); }
        }
        window.__multaraConsoleLogs.push({ level: level, ts: Date.now(), message: parts.join(' ') });
        if (window.__multaraConsoleLogs.length > cap) {
          window.__multaraConsoleLogs.splice(0, window.__multaraConsoleLogs.length - cap);
        }
      } catch (e) {}
      return fn.apply(console, arguments);
    };
  }
  console.log = wrap('log', console.log.bind(console));
  console.info = wrap('info', console.info.bind(console));
  console.warn = wrap('warn', console.warn.bind(console));
  console.error = wrap('error', console.error.bind(console));
  console.debug = wrap('debug', console.debug.bind(console));
})();"#;

/// macOS WKWebsiteDataStore 隔离 id（固定，与 data_directory 配套）。
#[cfg(target_os = "macos")]
const DATA_STORE_ID: [u8; 16] = [
    0x6d, 0x75, 0x6c, 0x74, 0x61, 0x72, 0x61, 0x2d, // "multara-"
    0x69, 0x6e, 0x62, 0x6f, 0x78, 0x2d, 0x62, 0x72, // "inbox-br"
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct NavigatedPayload {
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

fn emit_navigated(app: &AppHandle, url: &str) {
    let _ = app.emit(
        NAVIGATED_EVENT,
        NavigatedPayload {
            url: url.to_string(),
        },
    );
}

fn build_child(app: &AppHandle, url: Url) -> Result<WebviewBuilder<tauri::Wry>, String> {
    let data_dir = data_directory(app)?;
    let app_for_load = app.clone();
    let app_for_new = app.clone();
    let mut builder = WebviewBuilder::new(WEBVIEW_LABEL, WebviewUrl::External(url))
        .data_directory(data_dir)
        .initialization_script(CONSOLE_INIT_SCRIPT)
        .on_page_load(move |_webview, payload| {
            if payload.event() != PageLoadEvent::Finished {
                return;
            }
            emit_navigated(&app_for_load, payload.url().as_str());
        })
        .on_new_window(move |url, _features| {
            let href = url.as_str().to_string();
            let _ = crate::commands::open_external_url(href);
            NewWindowResponse::Deny
        })
        .on_navigation(move |url| {
            emit_navigated(&app_for_new, url.as_str());
            true
        });

    #[cfg(target_os = "macos")]
    {
        builder = builder.data_store_identifier(DATA_STORE_ID);
    }

    Ok(builder)
}

/// 创建或更新收件箱子 webview 的位置、可见性与 URL。
/// `url` 为空则只改 bounds / 可见性。
#[tauri::command]
pub async fn inbox_browser_set(
    app: AppHandle,
    url: Option<String>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    visible: bool,
) -> Result<(), String> {
    let parsed_url = match url.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(raw) => Some(validate_http_url(raw)?),
        None => None,
    };

    if let Some(existing) = app.get_webview(WEBVIEW_LABEL) {
        if let Some(next) = parsed_url {
            existing.navigate(next).map_err(|error| error.to_string())?;
        }
        if visible {
            apply_bounds(&existing, x, y, width, height)?;
            existing.show().map_err(|error| error.to_string())?;
        } else {
            hide_webview(&existing)?;
        }
        return Ok(());
    }

    if !visible {
        return Ok(());
    }

    let next = parsed_url.ok_or_else(|| tr!("请先输入地址"))?;
    let window = main_window(&app)?;
    let builder = build_child(&app, next)?;
    let webview = window
        .add_child(
            builder,
            LogicalPosition::new(x, y),
            LogicalSize::new(width.max(1.0), height.max(1.0)),
        )
        .map_err(|error| error.to_string())?;
    webview.show().map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn inbox_browser_back(app: AppHandle) -> Result<(), String> {
    let webview = app
        .get_webview(WEBVIEW_LABEL)
        .ok_or_else(|| tr!("浏览器未打开"))?;
    webview
        .eval("history.back()")
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn inbox_browser_forward(app: AppHandle) -> Result<(), String> {
    let webview = app
        .get_webview(WEBVIEW_LABEL)
        .ok_or_else(|| tr!("浏览器未打开"))?;
    webview
        .eval("history.forward()")
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn inbox_browser_reload(app: AppHandle) -> Result<(), String> {
    let webview = app
        .get_webview(WEBVIEW_LABEL)
        .ok_or_else(|| tr!("浏览器未打开"))?;
    webview.reload().map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn inbox_browser_destroy(app: AppHandle) -> Result<(), String> {
    destroy(&app);
    Ok(())
}

fn require_webview(app: &AppHandle) -> Result<tauri::Webview, String> {
    app.get_webview(WEBVIEW_LABEL)
        .ok_or_else(|| tr!("浏览器未打开"))
}

fn eval_string(webview: &tauri::Webview, script: &str) -> Result<String, String> {
    let (tx, rx) = mpsc::channel();
    webview
        .eval_with_callback(script.to_string(), move |result| {
            let _ = tx.send(result);
        })
        .map_err(|error| error.to_string())?;
    rx.recv_timeout(Duration::from_secs(5))
        .map_err(|_| tr!("页面脚本执行超时"))
}

/// 在子页面执行 JS，返回 JSON 字符串结果（供选元素 / 读控制台）。
#[tauri::command]
pub async fn inbox_browser_eval(app: AppHandle, script: String) -> Result<String, String> {
    if script.trim().is_empty() {
        return Err(tr!("脚本为空"));
    }
    let webview = require_webview(&app)?;
    eval_string(&webview, &script)
}

/// 读取缓冲的 console 日志，拼成纯文本。
#[tauri::command]
pub async fn inbox_browser_console_logs(app: AppHandle) -> Result<String, String> {
    let webview = require_webview(&app)?;
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
    x: Option<f64>,
    y: Option<f64>,
    width: Option<f64>,
    height: Option<f64>,
) -> Result<String, String> {
    let webview = require_webview(&app)?;
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

pub fn destroy(app: &AppHandle) {
    if let Some(webview) = app.get_webview(WEBVIEW_LABEL) {
        let _ = webview.hide();
        let _ = webview.close();
    }
}

#[cfg(test)]
mod tests {
    use super::validate_http_url;

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
}
