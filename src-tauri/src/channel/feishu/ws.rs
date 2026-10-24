//! 一个只覆盖飞书长连接所需子集的同步 WebSocket 客户端。
//!
//! 不引入 tokio，也不依赖额外的 WebSocket crate：握手、帧编解码和 TLS
//! 都在这里完成。读循环同时负责应用层心跳，因此同一连接只有一个写者。

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Once;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rustls::pki_types::ServerName;
use rustls::{ClientConnection, StreamOwned};
use rustls_platform_verifier::ConfigVerifierExt;

const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;

pub enum WsError {
    /// 这一段没有读到完整帧，调用方可以继续循环（也用来触发心跳）。
    Timeout,
    Closed(String),
}

pub struct WsConn {
    stream: StreamOwned<ClientConnection, TcpStream>,
    buffer: Vec<u8>,
    fragments: Vec<u8>,
    pending: Option<u8>,
}

impl WsConn {
    pub fn connect(url: &str, timeout: Duration) -> Result<WsConn, String> {
        let parsed = url::Url::parse(url).map_err(|error| format!("长连接地址无效：{error}"))?;
        if parsed.scheme() != "wss" {
            return Err("长连接只支持 wss".to_string());
        }
        let host = parsed
            .host_str()
            .ok_or_else(|| "长连接地址缺少主机".to_string())?
            .to_string();
        let port = parsed.port_or_known_default().unwrap_or(443);
        let mut path = parsed.path().to_string();
        if path.is_empty() {
            path.push('/');
        }
        if let Some(query) = parsed.query() {
            path.push('?');
            path.push_str(query);
        }

        let mut addrs = (host.as_str(), port)
            .to_socket_addrs()
            .map_err(|error| format!("无法解析长连接主机：{error}"))?;
        let address = addrs
            .next()
            .ok_or_else(|| "长连接主机没有可用地址".to_string())?;
        let tcp = TcpStream::connect_timeout(&address, timeout)
            .map_err(|error| format!("连接长连接网关失败：{error}"))?;
        let _ = tcp.set_nodelay(true);
        tcp.set_read_timeout(Some(Duration::from_secs(15)))
            .map_err(|error| error.to_string())?;
        tcp.set_write_timeout(Some(Duration::from_secs(10)))
            .map_err(|error| error.to_string())?;

        let config = rustls_config()?;
        let server_name = ServerName::try_from(host.clone())
            .map_err(|_| "无效的 TLS 主机名".to_string())?;
        let connection =
            ClientConnection::new(config, server_name).map_err(|error| error.to_string())?;
        let mut stream = StreamOwned {
            conn: connection,
            sock: tcp,
        };
        // 先完成 TLS 握手，再发 Upgrade，错误信息更清晰。
        while stream.conn.is_handshaking() {
            stream
                .conn
                .complete_io(&mut stream.sock)
                .map_err(|error| format!("TLS 握手失败：{error}"))?;
        }
        handshake_stream(&mut stream, &host, port, &path)?;
        let _ = stream.sock.set_read_timeout(Some(Duration::from_secs(2)));
        Ok(WsConn {
            stream,
            buffer: Vec::new(),
            fragments: Vec::new(),
            pending: None,
        })
    }

    pub fn send_binary(&mut self, payload: &[u8]) -> Result<(), String> {
        self.write_frame(0x2, payload)
    }

    pub fn send_pong(&mut self, payload: &[u8]) -> Result<(), String> {
        self.write_frame(0xA, payload)
    }

    pub fn send_close(&mut self) {
        let _ = self.write_frame(0x8, &[0x03, 0xE8]);
    }

    /// 读下一条完整的二进制消息。控制帧在内部处理（ping 直接回 pong）。
    pub fn read_binary(&mut self) -> Result<Vec<u8>, WsError> {
        loop {
            if let Some(frame) = self.take_frame()? {
                match frame.opcode {
                    0x0 => {
                        self.fragments.extend_from_slice(&frame.payload);
                        if frame.fin {
                            let data = std::mem::take(&mut self.fragments);
                            return Ok(data);
                        }
                    }
                    0x1 | 0x2 => {
                        if frame.fin {
                            let _ = self.pending.take();
                            return Ok(frame.payload);
                        }
                        self.fragments = frame.payload;
                        self.pending = Some(frame.opcode);
                    }
                    0x8 => {
                        return Err(WsError::Closed("对端关闭了长连接".to_string()));
                    }
                    0x9 => {
                        let payload = frame.payload;
                        let _ = self.send_pong(&payload);
                    }
                    0xA => {}
                    _ => {}
                }
            }
        }
    }

    fn take_frame(&mut self) -> Result<Option<WsFrame>, WsError> {
        loop {
            if let Some(frame) = parse_frame(&self.buffer)? {
                self.buffer.drain(..frame.consumed);
                return Ok(Some(frame.frame));
            }
            let mut chunk = [0u8; 16 * 1024];
            match self.stream.read(&mut chunk) {
                Ok(0) => {
                    return Err(WsError::Closed("长连接被对端关闭".to_string()));
                }
                Ok(read) => self.buffer.extend_from_slice(&chunk[..read]),
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    return Err(WsError::Timeout);
                }
                Err(error) => return Err(WsError::Closed(error.to_string())),
            }
            if self.buffer.len() > MAX_FRAME_BYTES {
                return Err(WsError::Closed("长连接帧过大".to_string()));
            }
        }
    }

    fn write_frame(&mut self, opcode: u8, payload: &[u8]) -> Result<(), String> {
        let mut frame = Vec::with_capacity(payload.len() + 14);
        frame.push(0x80 | opcode);
        let mask = mask_key();
        if payload.len() < 126 {
            frame.push(0x80 | payload.len() as u8);
        } else if payload.len() <= u16::MAX as usize {
            frame.push(0x80 | 126);
            frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        } else {
            frame.push(0x80 | 127);
            frame.extend_from_slice(&(payload.len() as u64).to_be_bytes());
        }
        frame.extend_from_slice(&mask);
        frame.extend(
            payload
                .iter()
                .enumerate()
                .map(|(index, byte)| byte ^ mask[index % 4]),
        );
        self.stream
            .write_all(&frame)
            .map_err(|error| format!("写入长连接失败：{error}"))?;
        self.stream
            .flush()
            .map_err(|error| format!("刷新长连接失败：{error}"))
    }
}

struct WsFrame {
    fin: bool,
    opcode: u8,
    payload: Vec<u8>,
}

struct ParsedFrame {
    consumed: usize,
    frame: WsFrame,
}

fn parse_frame(buffer: &[u8]) -> Result<Option<ParsedFrame>, WsError> {
    if buffer.len() < 2 {
        return Ok(None);
    }
    let fin = buffer[0] & 0x80 != 0;
    let opcode = buffer[0] & 0x0f;
    let masked = buffer[1] & 0x80 != 0;
    let length_code = (buffer[1] & 0x7f) as usize;
    let mut offset = 2;
    let length = match length_code {
        126 => {
            if buffer.len() < offset + 2 {
                return Ok(None);
            }
            let value = u16::from_be_bytes([buffer[offset], buffer[offset + 1]]) as usize;
            offset += 2;
            value
        }
        127 => {
            if buffer.len() < offset + 8 {
                return Ok(None);
            }
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&buffer[offset..offset + 8]);
            offset += 8;
            u64::from_be_bytes(bytes) as usize
        }
        value => value,
    };
    if length > MAX_FRAME_BYTES {
        return Err(WsError::Closed("长连接帧过大".to_string()));
    }
    let mask = if masked {
        if buffer.len() < offset + 4 {
            return Ok(None);
        }
        let mask = [
            buffer[offset],
            buffer[offset + 1],
            buffer[offset + 2],
            buffer[offset + 3],
        ];
        offset += 4;
        Some(mask)
    } else {
        None
    };
    if buffer.len() < offset + length {
        return Ok(None);
    }
    let mut payload = buffer[offset..offset + length].to_vec();
    if let Some(mask) = mask {
        for (index, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask[index % 4];
        }
    }
    Ok(Some(ParsedFrame {
        consumed: offset + length,
        frame: WsFrame {
            fin,
            opcode,
            payload,
        },
    }))
}

fn handshake_stream<S: Read + Write>(
    stream: &mut S,
    host: &str,
    port: u16,
    path: &str,
) -> Result<(), String> {
    let key = base64_encode(&random_bytes(16));
    let host_header = if port == 443 || port == 80 {
        host.to_string()
    } else {
        format!("{host}:{port}")
    };
    let request = format!(
        "GET {path} HTTP/1.1\r\n\
         Host: {host_header}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: {key}\r\n\
         Sec-WebSocket-Version: 13\r\n\
         User-Agent: Multara\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("发送 WebSocket 握手失败：{error}"))?;
    stream.flush().map_err(|error| error.to_string())?;

    let mut response = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let read = stream
            .read(&mut chunk)
            .map_err(|error| format!("读取 WebSocket 握手响应失败：{error}"))?;
        if read == 0 {
            return Err("WebSocket 握手被对端关闭".to_string());
        }
        response.extend_from_slice(&chunk[..read]);
        if response.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
        if response.len() > 16 * 1024 {
            return Err("WebSocket 握手响应过大".to_string());
        }
    }
    let text = String::from_utf8_lossy(&response);
    if !text.starts_with("HTTP/1.1 101") && !text.starts_with("HTTP/1.0 101") {
        return Err(format!(
            "长连接网关拒绝了握手：{}",
            text.lines().next().unwrap_or("").trim()
        ));
    }
    Ok(())
}

fn rustls_config() -> Result<std::sync::Arc<rustls::ClientConfig>, String> {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
    let config = rustls::ClientConfig::with_platform_verifier()
        .map_err(|error| format!("初始化 TLS 失败：{error}"))?;
    Ok(std::sync::Arc::new(config))
}

fn mask_key() -> [u8; 4] {
    let bytes = random_bytes(4);
    [bytes[0], bytes[1], bytes[2], bytes[3]]
}

fn random_bytes(len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(len);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos() as u64)
        .unwrap_or(0);
    while out.len() < len {
        let uuid = uuid::Uuid::new_v4();
        for byte in uuid.as_bytes() {
            if out.len() >= len {
                break;
            }
            out.push(byte ^ (now.rotate_left(out.len() as u32) as u8));
        }
    }
    out
}

fn base64_encode(value: &[u8]) -> String {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    STANDARD.encode(value)
}

/// WebSocket 握手用的 SHA-1（RFC 3174）。只用于 `Sec-WebSocket-Accept`。
#[allow(dead_code)]
fn sha1(value: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let mut data = value.to_vec();
    let bit_len = (value.len() as u64) * 8;
    data.push(0x80);
    while data.len() % 64 != 56 {
        data.push(0);
    }
    data.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in data.chunks(64) {
        let mut w = [0u32; 80];
        for index in 0..16 {
            w[index] = u32::from_be_bytes([
                chunk[index * 4],
                chunk[index * 4 + 1],
                chunk[index * 4 + 2],
                chunk[index * 4 + 3],
            ]);
        }
        for index in 16..80 {
            w[index] = (w[index - 3] ^ w[index - 8] ^ w[index - 14] ^ w[index - 16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        for index in 0..80 {
            let (f, k) = match index {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(w[index]);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }
    let mut out = [0u8; 20];
    for (index, value) in h.iter().enumerate() {
        out[index * 4..index * 4 + 4].copy_from_slice(&value.to_be_bytes());
    }
    out
}

#[allow(dead_code)]
pub fn accept_key(key: &str) -> String {
    base64_encode(&sha1(format!("{key}{WS_GUID}").as_bytes()))
}
