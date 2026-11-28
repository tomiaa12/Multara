use std::{
    io::{ErrorKind, Read, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, TryRecvError},
    },
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 单帧上限：文件写入的请求体最大的那一类也在这之内。
pub const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;
pub const READ_POLL: Duration = Duration::from_millis(50);
pub const WRITE_TIMEOUT: Duration = Duration::from_secs(15);
pub const PING_INTERVAL: Duration = Duration::from_secs(15);
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(45);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteError {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub perm: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<String>,
}

impl RemoteError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            perm: None,
            approval_id: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "camelCase")]
pub enum Frame {
    Req {
        id: u64,
        cmd: String,
        #[serde(default)]
        args: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        approval: Option<String>,
    },
    Res {
        id: u64,
        ok: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        data: Option<Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<RemoteError>,
    },
    Evt {
        event: String,
        #[serde(default)]
        payload: Value,
    },
    Ping,
    Pong,
}

impl Frame {
    pub fn ok(id: u64, data: Value) -> Self {
        Frame::Res {
            id,
            ok: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn fail(id: u64, error: RemoteError) -> Self {
        Frame::Res {
            id,
            ok: false,
            data: None,
            error: Some(error),
        }
    }
}

pub fn write_frame<W: Write>(stream: &mut W, frame: &Frame) -> std::io::Result<()> {
    let body = serde_json::to_vec(frame).map_err(std::io::Error::other)?;
    if body.len() > MAX_FRAME_BYTES {
        return Err(std::io::Error::other("frame too large"));
    }
    stream.write_all(&(body.len() as u32).to_be_bytes())?;
    stream.write_all(&body)?;
    stream.flush()
}

/// 增量解析「4 字节大端长度 + JSON」帧；读超时返回后已收到的半帧留在缓冲里。
#[derive(Default)]
pub struct FrameReader {
    buffer: Vec<u8>,
}

impl FrameReader {
    pub fn push(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    pub fn next(&mut self) -> Result<Option<Frame>, String> {
        if self.buffer.len() < 4 {
            return Ok(None);
        }
        let length = u32::from_be_bytes([self.buffer[0], self.buffer[1], self.buffer[2], self.buffer[3]]) as usize;
        if length > MAX_FRAME_BYTES {
            return Err("frame too large".into());
        }
        if self.buffer.len() < 4 + length {
            return Ok(None);
        }
        let body: Vec<u8> = self.buffer.drain(..4 + length).skip(4).collect();
        serde_json::from_slice(&body)
            .map(Some)
            .map_err(|error| error.to_string())
    }
}

pub enum LoopEnd {
    Closed,
    Timeout,
    Stopped,
    Error(String),
}

/// 一条连接的读写循环：同一线程轮询读（短超时）并发送待发帧。
/// `ping` 为 true 的一端（控制端）定时发心跳；两端都在空闲超时后断开。
pub fn run_loop<S: Read + Write>(
    stream: &mut S,
    outbound: &Receiver<Frame>,
    stop: &AtomicBool,
    ping: bool,
    mut on_frame: impl FnMut(Frame) -> Option<Frame>,
) -> LoopEnd {
    let mut reader = FrameReader::default();
    let mut chunk = vec![0u8; 64 * 1024];
    let mut last_received = Instant::now();
    let mut last_ping = Instant::now();
    loop {
        if stop.load(Ordering::SeqCst) {
            return LoopEnd::Stopped;
        }
        match stream.read(&mut chunk) {
            Ok(0) => return LoopEnd::Closed,
            Ok(count) => {
                last_received = Instant::now();
                reader.push(&chunk[..count]);
                loop {
                    match reader.next() {
                        Ok(Some(Frame::Ping)) => {
                            if let Err(error) = write_frame(stream, &Frame::Pong) {
                                return LoopEnd::Error(error.to_string());
                            }
                        }
                        Ok(Some(Frame::Pong)) => {}
                        Ok(Some(frame)) => {
                            if let Some(reply) = on_frame(frame) {
                                if let Err(error) = write_frame(stream, &reply) {
                                    return LoopEnd::Error(error.to_string());
                                }
                            }
                        }
                        Ok(None) => break,
                        Err(error) => return LoopEnd::Error(error),
                    }
                }
            }
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted) => {}
            Err(error) => return LoopEnd::Error(error.to_string()),
        }
        loop {
            match outbound.try_recv() {
                Ok(frame) => {
                    if let Err(error) = write_frame(stream, &frame) {
                        return LoopEnd::Error(error.to_string());
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return LoopEnd::Stopped,
            }
        }
        if ping && last_ping.elapsed() >= PING_INTERVAL {
            last_ping = Instant::now();
            if let Err(error) = write_frame(stream, &Frame::Ping) {
                return LoopEnd::Error(error.to_string());
            }
        }
        if last_received.elapsed() >= IDLE_TIMEOUT {
            return LoopEnd::Timeout;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_frames_split_across_chunks() {
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &Frame::Ping).unwrap();
        let first_frame_end = bytes.len();
        write_frame(
            &mut bytes,
            &Frame::Evt {
                event: "x".into(),
                payload: serde_json::json!({"a": 1}),
            },
        )
        .unwrap();
        let mut reader = FrameReader::default();
        let (head, tail) = bytes.split_at(first_frame_end + 6);
        reader.push(head);
        assert!(matches!(reader.next().unwrap(), Some(Frame::Ping)));
        assert!(reader.next().unwrap().is_none());
        reader.push(tail);
        assert!(matches!(reader.next().unwrap(), Some(Frame::Evt { .. })));
        assert!(reader.next().unwrap().is_none());
    }
}
