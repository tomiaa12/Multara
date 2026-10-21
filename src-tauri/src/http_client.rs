//! 通过系统 curl 发 HTTP 请求，供 MCP 探测和资源商店使用。允许 http（本机 MCP 常见）和 https。

use std::fs;

const MAX_BODY_BYTES: usize = 8_000_000;

pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .rev()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

pub fn get(url: &str, accept: &str) -> Result<Response, String> {
    request("GET", url, &[("Accept".into(), accept.into())], None, 30)
}

pub fn request(
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: Option<&str>,
    timeout_secs: u32,
) -> Result<Response, String> {
    let parsed = url::Url::parse(url.trim()).map_err(|_| format!("地址无效：{url}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("只支持 http 和 https 地址".to_string());
    }
    let header_file = tempfile::NamedTempFile::new().map_err(|error| error.to_string())?;
    let body_file = tempfile::NamedTempFile::new().map_err(|error| error.to_string())?;
    let mut command = crate::process::command("curl");
    command.args(["--silent", "--show-error", "--location", "--max-redirs", "3"]);
    command.args(["--proto", "=http,https", "--max-time", &timeout_secs.to_string()]);
    command.arg("--dump-header").arg(header_file.path());
    command.arg("--output").arg(body_file.path());
    command.args(["-X", method]);
    command.args(["-H", "User-Agent: Multara"]);
    for (key, value) in headers {
        command.args(["-H", &format!("{key}: {value}")]);
    }
    let payload = body.map(|_| tempfile::NamedTempFile::new()).transpose().map_err(|error| error.to_string())?;
    if let (Some(text), Some(file)) = (body, payload.as_ref()) {
        fs::write(file.path(), text).map_err(|error| error.to_string())?;
        command.arg("--data-binary").arg(format!("@{}", file.path().display()));
    }
    command.arg(parsed.as_str());
    let output = command.output().map_err(|error| format!("无法调用 curl：{error}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if detail.is_empty() { format!("请求失败：{url}") } else { detail });
    }
    let raw_headers = fs::read(header_file.path()).map_err(|error| error.to_string())?;
    let raw_body = fs::read(body_file.path()).map_err(|error| error.to_string())?;
    if raw_body.len() > MAX_BODY_BYTES {
        return Err("响应过大".to_string());
    }
    let header_text = String::from_utf8_lossy(&raw_headers).to_string();
    // 跟随重定向时会有多段响应头，只取最后一段。
    let last = header_text
        .split("\r\n\r\n")
        .filter(|block| block.trim_start().starts_with("HTTP/"))
        .last()
        .unwrap_or("");
    let status = last
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    let headers = last
        .lines()
        .skip(1)
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            Some((key.trim().to_string(), value.trim().to_string()))
        })
        .collect();
    Ok(Response {
        status,
        headers,
        body: String::from_utf8_lossy(&raw_body).to_string(),
    })
}
