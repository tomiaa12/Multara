//! 通过系统 curl 发 HTTP 请求，供 MCP 探测和资源商店使用。允许 http（本机 MCP 常见）和 https。

use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    process::{Child, ChildStdout, Stdio},
};

const MAX_BODY_BYTES: usize = 8_000_000;
const MAX_BODY_BYTES_ARG: &str = "8000000";
const MAX_REDIRECTS: usize = 3;
const MAX_HEADER_LINE_BYTES: u64 = 16 * 1024;
/// 跨域重定向时只保留这些不含凭据的请求头（curl 自己只会剥离 `Authorization`，不管 `x-api-key` 之类）。
const CROSS_ORIGIN_SAFE_HEADERS: &[&str] = &["accept", "user-agent", "content-type", "mcp-protocol-version"];

pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

pub struct StreamResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    reader: BufReader<ChildStdout>,
    child: Child,
    _payload: Option<tempfile::NamedTempFile>,
    _headers: tempfile::NamedTempFile,
}

impl Read for StreamResponse {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.reader.read(buffer)
    }
}

impl Drop for StreamResponse {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
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

fn parse_http_url(url: &str) -> Result<url::Url, String> {
    let parsed = url::Url::parse(url.trim()).map_err(|_| format!("地址无效：{url}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("只支持 http 和 https 地址".to_string());
    }
    Ok(parsed)
}

/// 请求头写进临时文件（Unix 上创建即 0600）用 `-H @file` 传给 curl，密钥不出现在 `ps` 可见的命令行里。
/// 调用方要让返回的文件活到 curl 读完为止。
pub(crate) fn curl_header_file(headers: &[(String, String)]) -> Result<tempfile::NamedTempFile, String> {
    let mut file = tempfile::NamedTempFile::new().map_err(|error| error.to_string())?;
    let mut text = String::new();
    if !headers
        .iter()
        .any(|(key, _)| key.eq_ignore_ascii_case("user-agent"))
    {
        text.push_str("User-Agent: Multara\n");
    }
    for (key, value) in headers {
        text.push_str(&format!("{}: {}\n", key.trim(), value.replace(['\r', '\n'], " ")));
    }
    file.write_all(text.as_bytes()).map_err(|error| error.to_string())?;
    file.flush().map_err(|error| error.to_string())?;
    Ok(file)
}

/// 手动跟随重定向：跨域时去掉可能带凭据的自定义头，301/302 的 POST 和 303 按 curl 的习惯改成 GET。
pub fn request(
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: Option<&str>,
    timeout_secs: u32,
) -> Result<Response, String> {
    let mut current = parse_http_url(url)?;
    let mut method = method.to_string();
    let mut headers = headers.to_vec();
    let mut body = body;
    for _ in 0..=MAX_REDIRECTS {
        let response = request_once(&method, &current, &headers, body, timeout_secs)?;
        let Some(next) = redirect_target(&response, &current) else {
            return Ok(response);
        };
        if next.origin() != current.origin() {
            headers.retain(|(key, _)| CROSS_ORIGIN_SAFE_HEADERS.iter().any(|safe| key.eq_ignore_ascii_case(safe)));
        }
        if response.status == 303 || (matches!(response.status, 301 | 302) && method == "POST") {
            method = "GET".to_string();
            body = None;
        }
        current = next;
    }
    Err(format!("重定向次数过多：{url}"))
}

fn redirect_target(response: &Response, current: &url::Url) -> Option<url::Url> {
    if !matches!(response.status, 301 | 302 | 303 | 307 | 308) {
        return None;
    }
    current
        .join(response.header("location")?)
        .ok()
        .filter(|next| matches!(next.scheme(), "http" | "https"))
}

fn request_once(
    method: &str,
    parsed: &url::Url,
    headers: &[(String, String)],
    body: Option<&str>,
    timeout_secs: u32,
) -> Result<Response, String> {
    let url = parsed.as_str();
    let header_file = tempfile::NamedTempFile::new().map_err(|error| error.to_string())?;
    let body_file = tempfile::NamedTempFile::new().map_err(|error| error.to_string())?;
    let request_headers = curl_header_file(headers)?;
    let mut command = crate::process::command("curl");
    command.args([
        "--silent",
        "--show-error",
        "--max-filesize",
        MAX_BODY_BYTES_ARG,
    ]);
    command.args([
        "--proto",
        "=http,https",
        "--max-time",
        &timeout_secs.to_string(),
    ]);
    command.arg("--dump-header").arg(header_file.path());
    command.arg("--output").arg(body_file.path());
    command.args(["-X", method]);
    command.arg("-H").arg(format!("@{}", request_headers.path().display()));
    let payload = body
        .map(|_| tempfile::NamedTempFile::new())
        .transpose()
        .map_err(|error| error.to_string())?;
    if let (Some(text), Some(file)) = (body, payload.as_ref()) {
        fs::write(file.path(), text).map_err(|error| error.to_string())?;
        command
            .arg("--data-binary")
            .arg(format!("@{}", file.path().display()));
    }
    command.arg(parsed.as_str());
    let output = command
        .output()
        .map_err(|error| format!("无法调用 curl：{error}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if detail.is_empty() {
            format!("请求失败：{url}")
        } else {
            detail
        });
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

pub fn request_stream(
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: Option<&str>,
    timeout_secs: u32,
) -> Result<StreamResponse, String> {
    let parsed = parse_http_url(url)?;
    let payload = body
        .map(|_| tempfile::NamedTempFile::new())
        .transpose()
        .map_err(|error| error.to_string())?;
    if let (Some(text), Some(file)) = (body, payload.as_ref()) {
        fs::write(file.path(), text).map_err(|error| error.to_string())?;
    }
    let request_headers = curl_header_file(headers)?;
    let mut command = crate::process::command("curl");
    command.args([
        "--silent",
        "--show-error",
        "--no-buffer",
        "--include",
        "--max-time",
        &timeout_secs.to_string(),
        "--proto",
        "=http,https",
        "-X",
        method,
    ]);
    command.arg("-H").arg(format!("@{}", request_headers.path().display()));
    if let Some(file) = payload.as_ref() {
        command
            .arg("--data-binary")
            .arg(format!("@{}", file.path().display()));
    }
    command.arg(parsed.as_str());
    // stderr 没人读，接管道会在 curl 写满 64KB 后把它卡住
    command.stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|error| format!("无法调用 curl：{error}"))?;
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err("无法读取上游响应".to_string());
    };
    let mut reader = BufReader::new(stdout);
    match read_stream_head(&mut reader) {
        Ok((status, response_headers)) => Ok(StreamResponse {
            status,
            headers: response_headers,
            reader,
            child,
            _payload: payload,
            _headers: request_headers,
        }),
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(error)
        }
    }
}

fn read_stream_head(reader: &mut BufReader<ChildStdout>) -> Result<(u16, Vec<(String, String)>), String> {
    let mut status_line = String::new();
    reader
        .by_ref()
        .take(MAX_HEADER_LINE_BYTES)
        .read_line(&mut status_line)
        .map_err(|error| error.to_string())?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| format!("上游响应头无效：{}", status_line.trim()))?;
    let mut response_headers = Vec::new();
    loop {
        let mut line = String::new();
        reader
            .by_ref()
            .take(MAX_HEADER_LINE_BYTES)
            .read_line(&mut line)
            .map_err(|error| error.to_string())?;
        if line == "\r\n" || line == "\n" || line.is_empty() {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            response_headers.push((key.trim().to_string(), value.trim().to_string()));
        }
    }
    Ok((status, response_headers))
}
