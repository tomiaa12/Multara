//! 本机托管基础：Canvas 实例（L2）和 MCP App（L3）共用。
//!
//! - 每个站点一个监听端口，绑 `127.0.0.1:0`。端口不同即源不同，站点之间
//!   `localStorage`、DOM 互相不可见（方案 R4）。
//! - URL 是 `http://127.0.0.1:<port>/<token>/`；路径第一段不是 token 一律 404。
//! - 校验 `Host`（防 DNS rebinding）；非 GET/HEAD 请求校验 `Origin` 等于自身源且带 JSON content-type。
//! - 每个请求一个线程，响应后关闭连接。
//!
//! 没用 tiny_http：它的分块输出要攒满 8 KB 且有下一次写入才发出去，SSE 事件会卡在缓冲区里。
//! 这里只需要很小的 HTTP/1.1 子集（无 keep-alive、无分块请求体），自己实现更可控。

use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

/// 单个静态文件上限
pub const MAX_FILE_BYTES: u64 = 20 * 1024 * 1024;
/// 请求体上限
pub const MAX_BODY_BYTES: usize = 1024 * 1024;
const MAX_HEAD_BYTES: usize = 16 * 1024;

pub struct HostRequest {
    pub method: String,
    /// token 之后的路径（未解码），不含查询串；总以 `/` 开头
    pub path: String,
    pub query: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl HostRequest {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(&name.to_ascii_lowercase()).map(String::as_str)
    }
}

pub enum HostResponse {
    Json(u16, serde_json::Value),
    File(PathBuf),
    Html(String),
    /// 用自己的 CSP 代替站点默认的（MCP App：CSP 由资源声明）
    HtmlWithCsp { html: String, csp: String },
    Status(u16),
    /// 流式响应（SSE）：每读到一段立即写出并 flush，读到 EOF 结束
    Stream {
        content_type: &'static str,
        body: Box<dyn Read + Send>,
    },
}

pub type Handler = Arc<dyn Fn(HostRequest) -> HostResponse + Send + Sync>;

pub struct HostedSite {
    pub port: u16,
    pub token: String,
    stopped: Arc<AtomicBool>,
}

impl HostedSite {
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/{}/", self.port, self.token)
    }

    pub fn origin(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub fn stop(&self) {
        if !self.stopped.swap(true, Ordering::SeqCst) {
            // 叫醒阻塞在 accept 上的线程
            let _ = TcpStream::connect_timeout(
                &([127, 0, 0, 1], self.port).into(),
                Duration::from_millis(200),
            );
        }
    }

    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }
}

impl Drop for HostedSite {
    fn drop(&mut self) {
        self.stop();
    }
}

/// 128 位以上的随机 token（两个 v4 UUID 拼接，十六进制）。
pub fn random_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

/// 允许嵌入站点的宿主源：release 为 Tauri 自身；dev 构建追加 Vite 开发服务器（方案 R6）。
pub fn frame_ancestors() -> &'static str {
    if cfg!(debug_assertions) {
        "tauri://localhost http://tauri.localhost https://tauri.localhost http://localhost:1420"
    } else {
        "tauri://localhost http://tauri.localhost https://tauri.localhost"
    }
}

/// 起一个站点。`preferred_port` 被占用时回落到随机端口。
pub fn start(csp: String, preferred_port: Option<u16>, handler: Handler) -> Result<HostedSite, String> {
    let listener = preferred_port
        .and_then(|port| TcpListener::bind(("127.0.0.1", port)).ok())
        .map(Ok)
        .unwrap_or_else(|| TcpListener::bind("127.0.0.1:0"))
        .map_err(|error| error.to_string())?;
    let port = listener.local_addr().map_err(|error| error.to_string())?.port();
    let token = random_token();
    let stopped = Arc::new(AtomicBool::new(false));
    let site = HostedSite {
        port,
        token: token.clone(),
        stopped: stopped.clone(),
    };
    let csp = Arc::new(csp);
    let token = Arc::new(token);
    thread::Builder::new()
        .name(format!("ui-host-{port}"))
        .spawn(move || {
            for stream in listener.incoming() {
                if stopped.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(stream) = stream else { continue };
                let handler = handler.clone();
                let token = token.clone();
                let csp = csp.clone();
                let stopped = stopped.clone();
                let _ = thread::Builder::new()
                    .name(format!("ui-host-{port}-req"))
                    .spawn(move || serve(stream, port, &token, &csp, &handler, &stopped));
            }
        })
        .map_err(|error| error.to_string())?;
    Ok(site)
}

struct Parsed {
    method: String,
    target: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

/// 读一个请求；格式不对返回 HTTP 状态码
fn read_request(stream: &TcpStream) -> Result<Parsed, u16> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let mut reader = BufReader::new(stream.try_clone().map_err(|_| 500u16)?);
    let mut line = String::new();
    let mut head_bytes = 0;
    reader.read_line(&mut line).map_err(|_| 400u16)?;
    head_bytes += line.len();
    let mut parts = line.trim_end().split(' ');
    let method = parts.next().unwrap_or("").to_ascii_uppercase();
    let target = parts.next().unwrap_or("").to_string();
    if method.is_empty() || !target.starts_with('/') {
        return Err(400);
    }
    let mut headers = HashMap::new();
    loop {
        let mut header = String::new();
        let read = reader.read_line(&mut header).map_err(|_| 400u16)?;
        head_bytes += read;
        if head_bytes > MAX_HEAD_BYTES {
            return Err(431);
        }
        let header = header.trim_end_matches(['\r', '\n']);
        if read == 0 || header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }
    let length = headers
        .get("content-length")
        .map(|value| value.parse::<usize>().map_err(|_| 400u16))
        .transpose()?
        .unwrap_or(0);
    if length > MAX_BODY_BYTES {
        return Err(413);
    }
    if headers.get("transfer-encoding").is_some() {
        return Err(411);
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).map_err(|_| 400u16)?;
    Ok(Parsed {
        method,
        target,
        headers,
        body,
    })
}

fn serve(
    stream: TcpStream,
    port: u16,
    token: &str,
    csp: &str,
    handler: &Handler,
    stopped: &AtomicBool,
) {
    if stopped.load(Ordering::SeqCst) {
        return;
    }
    let parsed = match read_request(&stream) {
        Ok(parsed) => parsed,
        Err(code) => return write_status(stream, code, csp),
    };
    let host_ok = parsed
        .headers
        .get("host")
        .is_some_and(|host| host == &format!("127.0.0.1:{port}"));
    if !host_ok {
        return write_status(stream, 421, csp);
    }
    let (raw_path, query) = match parsed.target.split_once('?') {
        Some((path, query)) => (path.to_string(), query.to_string()),
        None => (parsed.target.clone(), String::new()),
    };
    let Some(path) = strip_token(&raw_path, token) else {
        return write_status(stream, 404, csp);
    };
    let is_read = matches!(parsed.method.as_str(), "GET" | "HEAD");
    if !is_read {
        let origin_ok = parsed
            .headers
            .get("origin")
            .is_some_and(|origin| origin == &format!("http://127.0.0.1:{port}"));
        let json_ok = parsed
            .headers
            .get("content-type")
            .is_some_and(|value| value.to_ascii_lowercase().starts_with("application/json"));
        if !origin_ok || !json_ok {
            return write_status(stream, 403, csp);
        }
    }
    let head = parsed.method == "HEAD";
    let response = handler(HostRequest {
        method: parsed.method,
        path,
        query,
        headers: parsed.headers,
        body: parsed.body,
    });
    respond(stream, response, csp, head);
}

/// `/<token>` 或 `/<token>/...` → 之后的部分（至少是 `/`）
pub fn strip_token(path: &str, token: &str) -> Option<String> {
    let rest = path.strip_prefix('/')?.strip_prefix(token)?;
    if rest.is_empty() {
        return Some("/".to_string());
    }
    rest.starts_with('/').then(|| rest.to_string())
}

fn reason(code: u16) -> &'static str {
    match code {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        411 => "Length Required",
        412 => "Precondition Failed",
        413 => "Payload Too Large",
        421 => "Misdirected Request",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        503 => "Service Unavailable",
        _ => "Error",
    }
}

fn head_bytes(code: u16, csp: &str, content_type: Option<&str>, length: Option<usize>) -> Vec<u8> {
    let mut head = format!("HTTP/1.1 {code} {}\r\n", reason(code));
    for (name, value) in [
        ("Content-Security-Policy", csp),
        ("X-Content-Type-Options", "nosniff"),
        ("Cache-Control", "no-store"),
        ("Referrer-Policy", "no-referrer"),
        ("Cross-Origin-Resource-Policy", "same-origin"),
        ("Connection", "close"),
    ] {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    if let Some(kind) = content_type {
        head.push_str(&format!("Content-Type: {kind}\r\n"));
    }
    if let Some(length) = length {
        head.push_str(&format!("Content-Length: {length}\r\n"));
    }
    head.push_str("\r\n");
    head.into_bytes()
}

fn write_status(mut stream: TcpStream, code: u16, csp: &str) {
    let _ = stream.write_all(&head_bytes(code, csp, None, Some(0)));
    let _ = stream.flush();
    let _ = stream.shutdown(Shutdown::Both);
}

fn write_body(mut stream: TcpStream, code: u16, csp: &str, kind: &str, body: &[u8], head: bool) {
    let _ = stream.write_all(&head_bytes(code, csp, Some(kind), Some(body.len())));
    if !head {
        let _ = stream.write_all(body);
    }
    let _ = stream.flush();
    let _ = stream.shutdown(Shutdown::Both);
}

fn respond(mut stream: TcpStream, response: HostResponse, csp: &str, head: bool) {
    match response {
        HostResponse::Status(code) => write_status(stream, code, csp),
        HostResponse::Json(code, value) => write_body(
            stream,
            code,
            csp,
            "application/json; charset=utf-8",
            value.to_string().as_bytes(),
            head,
        ),
        HostResponse::Html(html) => write_body(stream, 200, csp, "text/html; charset=utf-8", html.as_bytes(), head),
        HostResponse::HtmlWithCsp { html, csp: own } => {
            write_body(stream, 200, &own, "text/html; charset=utf-8", html.as_bytes(), head)
        }
        HostResponse::File(path) => {
            let size = fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
            if size > MAX_FILE_BYTES {
                return write_status(stream, 413, csp);
            }
            match fs::read(&path) {
                Ok(bytes) => write_body(stream, 200, csp, mime_for(&path), &bytes, head),
                Err(_) => write_status(stream, 404, csp),
            }
        }
        HostResponse::Stream { content_type, mut body } => {
            let _ = stream.set_read_timeout(None);
            if stream.write_all(&head_bytes(200, csp, Some(content_type), None)).is_err() {
                return;
            }
            let _ = stream.flush();
            let mut buffer = [0u8; 8192];
            loop {
                match body.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => {
                        if stream.write_all(&buffer[..read]).is_err() || stream.flush().is_err() {
                            break;
                        }
                    }
                }
            }
            let _ = stream.shutdown(Shutdown::Both);
        }
    }
}

/// 把请求路径解析成 `root` 下的文件：URL 解码 → 拒绝 `..`、NUL、反斜杠、绝对路径段 →
/// `root.join` → `canonicalize` → 仍须在 `canonicalize(root)` 之下。目录回落到 `index.html`。
/// 符号链接只要最终落在 root 内就允许。
pub fn resolve_path(root: &Path, request_path: &str, index: &str) -> Option<PathBuf> {
    let decoded = percent_decode(request_path)?;
    let mut relative = PathBuf::new();
    for segment in decoded.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." || segment.contains('\0') || segment.contains('\\') || segment.contains(':') {
            return None;
        }
        relative.push(segment);
    }
    if relative
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return None;
    }
    let root = root.canonicalize().ok()?;
    let mut target = root.join(&relative);
    if target.is_dir() {
        target = target.join(index);
    }
    let target = target.canonicalize().ok()?;
    (target.starts_with(&root) && target.is_file()).then_some(target)
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = value.get(index + 1..index + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

pub fn mime_for(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "txt" | "md" => "text/plain; charset=utf-8",
        "csv" => "text/csv; charset=utf-8",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "wasm" => "application/wasm",
        "mp3" => "audio/mpeg",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        _ => "application/octet-stream",
    }
}

/// 日志里不出现 token
pub fn redact_url(url: &str, token: &str) -> String {
    url.replace(token, "<token>")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpStream;

    fn site_with_root() -> (tempfile::TempDir, HostedSite) {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("index.html"), "<h1>hi</h1>").unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub").join("a.js"), "1").unwrap();
        let root = dir.path().to_path_buf();
        let handler: Handler = Arc::new(move |request: HostRequest| {
            if request.path == "/__echo" {
                return HostResponse::Json(200, serde_json::json!({ "method": request.method }));
            }
            match resolve_path(&root, &request.path, "index.html") {
                Some(path) => HostResponse::File(path),
                None => HostResponse::Status(404),
            }
        });
        let site = start("default-src 'self'".into(), None, handler).unwrap();
        (dir, site)
    }

    fn raw(site: &HostedSite, request: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", site.port)).unwrap();
        stream.write_all(request.as_bytes()).unwrap();
        let mut out = String::new();
        let _ = stream.read_to_string(&mut out);
        out
    }

    fn get(site: &HostedSite, path: &str, host: &str) -> String {
        raw(site, &format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"))
    }

    #[test]
    fn serves_files_under_the_token_only() {
        let (_dir, site) = site_with_root();
        let host = format!("127.0.0.1:{}", site.port);
        let ok = get(&site, &format!("/{}/", site.token), &host);
        assert!(ok.starts_with("HTTP/1.1 200"), "{ok}");
        assert!(ok.contains("<h1>hi</h1>"));
        assert!(ok.contains("Content-Security-Policy: default-src 'self'"));
        assert!(ok.contains("Cache-Control: no-store"));
        assert!(get(&site, &format!("/{}/sub/a.js", site.token), &host).starts_with("HTTP/1.1 200"));
        assert!(get(&site, "/wrong/", &host).starts_with("HTTP/1.1 404"));
        assert!(get(&site, "/", &host).starts_with("HTTP/1.1 404"));
    }

    #[test]
    fn rejects_wrong_host_header() {
        let (_dir, site) = site_with_root();
        let response = get(&site, &format!("/{}/", site.token), "evil.example:80");
        assert!(response.starts_with("HTTP/1.1 421"), "{response}");
    }

    #[test]
    fn post_requires_same_origin_json() {
        let (_dir, site) = site_with_root();
        let host = format!("127.0.0.1:{}", site.port);
        let path = format!("/{}/__echo", site.token);
        let foreign = raw(&site, &format!(
            "POST {path} HTTP/1.1\r\nHost: {host}\r\nOrigin: http://evil.example\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
        ));
        assert!(foreign.starts_with("HTTP/1.1 403"), "{foreign}");
        let form = raw(&site, &format!(
            "POST {path} HTTP/1.1\r\nHost: {host}\r\nOrigin: http://{host}\r\nContent-Type: text/plain\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
        ));
        assert!(form.starts_with("HTTP/1.1 403"), "{form}");
        let ok = raw(&site, &format!(
            "POST {path} HTTP/1.1\r\nHost: {host}\r\nOrigin: http://{host}\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
        ));
        assert!(ok.starts_with("HTTP/1.1 200"), "{ok}");
    }

    #[test]
    fn path_resolution_blocks_traversal() {
        let outer = tempfile::tempdir().unwrap();
        let root = outer.path().join("site");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("index.html"), "x").unwrap();
        fs::write(outer.path().join("secret.txt"), "s").unwrap();
        assert!(resolve_path(&root, "/", "index.html").is_some());
        assert!(resolve_path(&root, "/index.html", "index.html").is_some());
        assert!(resolve_path(&root, "/../secret.txt", "index.html").is_none());
        assert!(resolve_path(&root, "/%2e%2e/secret.txt", "index.html").is_none());
        assert!(resolve_path(&root, "/..%2fsecret.txt", "index.html").is_none());
        assert!(resolve_path(&root, "/a%5c..%5csecret.txt", "index.html").is_none());
        assert!(resolve_path(&root, "/x%00", "index.html").is_none());
        assert!(resolve_path(&root, "/C:/secret.txt", "index.html").is_none());
        assert!(resolve_path(&root, "/missing.html", "index.html").is_none());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(outer.path().join("secret.txt"), root.join("link.txt")).unwrap();
            assert!(resolve_path(&root, "/link.txt", "index.html").is_none());
            std::os::unix::fs::symlink(root.join("index.html"), root.join("inside.html")).unwrap();
            assert!(resolve_path(&root, "/inside.html", "index.html").is_some());
        }
    }

    #[test]
    fn stopped_site_refuses_connections() {
        let (_dir, site) = site_with_root();
        let port = site.port;
        site.stop();
        drop(site);
        std::thread::sleep(std::time::Duration::from_millis(100));
        let response = TcpStream::connect(("127.0.0.1", port))
            .ok()
            .map(|mut stream| {
                let _ = stream.write_all(b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n");
                let mut out = String::new();
                let _ = stream.read_to_string(&mut out);
                out
            })
            .unwrap_or_default();
        assert!(!response.starts_with("HTTP/1.1 200"));
    }

    #[test]
    fn helpers() {
        assert_eq!(strip_token("/abc", "abc").as_deref(), Some("/"));
        assert_eq!(strip_token("/abc/x", "abc").as_deref(), Some("/x"));
        assert_eq!(strip_token("/abcd/x", "abc"), None);
        assert_eq!(mime_for(Path::new("a.HTML")), "text/html; charset=utf-8");
        assert_eq!(mime_for(Path::new("a.bin")), "application/octet-stream");
        assert_eq!(redact_url("http://h/abc/x", "abc"), "http://h/<token>/x");
        assert!(random_token().len() >= 32);
        assert!(frame_ancestors().contains("tauri://localhost"));
    }
}
