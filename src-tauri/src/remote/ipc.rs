use std::{
    sync::{mpsc, OnceLock},
    time::Duration,
};

use serde_json::Value;
use tauri::{
    ipc::{CallbackFn, InvokeBody, InvokeError, InvokeResponse, InvokeResponseBody},
    webview::InvokeRequest,
    AppHandle, Manager, Url,
};

use super::frame::RemoteError;

/// 远程请求里最慢的是 git push / pull 和合并，给足时间。
const DISPATCH_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const MAIN_WEBVIEW: &str = "main";

static WEBVIEW_URL: OnceLock<Url> = OnceLock::new();

/// 经由与本机界面相同的命令处理器执行一个已通过网关校验的 Tauri 命令，
/// 参数与返回值和前端 `invoke` 完全一致。
pub fn dispatch(app: &AppHandle, cmd: &str, args: Value) -> Result<Value, RemoteError> {
    let webview = app
        .get_webview(MAIN_WEBVIEW)
        .or_else(|| app.webviews().into_values().next())
        .ok_or_else(|| RemoteError::new("unavailable", tr!("主机界面未运行")))?;
    let url = match WEBVIEW_URL.get() {
        Some(url) => url.clone(),
        None => {
            let url = webview
                .url()
                .map_err(|error| RemoteError::new("unavailable", error.to_string()))?;
            WEBVIEW_URL.get_or_init(|| url).clone()
        }
    };
    let (reply, receive) = mpsc::sync_channel(1);
    webview.on_message(
        InvokeRequest {
            cmd: cmd.to_string(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url,
            body: InvokeBody::Json(args),
            headers: Default::default(),
            invoke_key: app.invoke_key().to_string(),
        },
        Box::new(move |_, _, response, _, _| {
            let _ = reply.send(response);
        }),
    );
    match receive.recv_timeout(DISPATCH_TIMEOUT) {
        Ok(InvokeResponse::Ok(InvokeResponseBody::Json(json))) => {
            Ok(serde_json::from_str(&json).unwrap_or(Value::Null))
        }
        Ok(InvokeResponse::Ok(InvokeResponseBody::Raw(_))) => Err(RemoteError::new(
            "unsupported",
            tr!("这个操作不支持远程调用"),
        )),
        Ok(InvokeResponse::Err(InvokeError(error))) => Err(RemoteError::new(
            "failed",
            error
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| error.to_string()),
        )),
        Err(_) => Err(RemoteError::new("timeout", tr!("主机处理超时"))),
    }
}
