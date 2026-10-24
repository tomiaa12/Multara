//! 飞书长连接的 pbbp2 protobuf 帧。字段与官方 SDK 的 `pbbp2.Frame` 一致。

#[derive(Clone, Debug, Default)]
pub struct Header {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, Default)]
pub struct Frame {
    pub seq_id: u64,
    pub log_id: u64,
    pub service: i32,
    pub method: i32,
    pub headers: Vec<Header>,
    pub payload_encoding: String,
    pub payload_type: String,
    pub payload: Option<Vec<u8>>,
    pub log_id_new: String,
}

pub const METHOD_CONTROL: i32 = 0;
pub const METHOD_DATA: i32 = 1;

pub const HEADER_TYPE: &str = "type";
pub const HEADER_SUM: &str = "sum";
pub const HEADER_SEQ: &str = "seq";
pub const HEADER_MESSAGE_ID: &str = "message_id";

impl Frame {
    pub fn header_value(&self, key: &str) -> &str {
        self.headers
            .iter()
            .find(|header| header.key == key)
            .map(|header| header.value.as_str())
            .unwrap_or("")
    }

    pub fn ping(service: i32) -> Frame {
        Frame {
            method: METHOD_CONTROL,
            service,
            headers: vec![Header {
                key: HEADER_TYPE.to_string(),
                value: "ping".to_string(),
            }],
            ..Default::default()
        }
    }

    pub fn pong(service: i32) -> Frame {
        Frame {
            method: METHOD_CONTROL,
            service,
            headers: vec![Header {
                key: HEADER_TYPE.to_string(),
                value: "pong".to_string(),
            }],
            ..Default::default()
        }
    }

    pub fn ack(inbound: &Frame, ok: bool) -> Frame {
        Frame {
            method: inbound.method,
            service: inbound.service,
            headers: inbound.headers.clone(),
            payload: Some(
                format!(
                    r#"{{"code":{},"headers":null,"data":null}}"#,
                    if ok { 200 } else { 500 }
                )
                .into_bytes(),
            ),
            ..Default::default()
        }
    }

    pub fn marshal(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(64 + self.payload.as_ref().map_or(0, Vec::len));
        put_tag(&mut buf, 1, 0);
        put_varint(&mut buf, self.seq_id);
        put_tag(&mut buf, 2, 0);
        put_varint(&mut buf, self.log_id);
        put_tag(&mut buf, 3, 0);
        put_varint(&mut buf, self.service as u32 as u64);
        put_tag(&mut buf, 4, 0);
        put_varint(&mut buf, self.method as u32 as u64);
        for header in &self.headers {
            let mut inner = Vec::new();
            put_tag(&mut inner, 1, 2);
            put_bytes(&mut inner, header.key.as_bytes());
            put_tag(&mut inner, 2, 2);
            put_bytes(&mut inner, header.value.as_bytes());
            put_tag(&mut buf, 5, 2);
            put_bytes(&mut buf, &inner);
        }
        put_tag(&mut buf, 6, 2);
        put_bytes(&mut buf, self.payload_encoding.as_bytes());
        put_tag(&mut buf, 7, 2);
        put_bytes(&mut buf, self.payload_type.as_bytes());
        if let Some(payload) = &self.payload {
            put_tag(&mut buf, 8, 2);
            put_bytes(&mut buf, payload);
        }
        put_tag(&mut buf, 9, 2);
        put_bytes(&mut buf, self.log_id_new.as_bytes());
        buf
    }

    pub fn unmarshal(mut data: &[u8]) -> Result<Frame, String> {
        if data.is_empty() {
            return Err("空帧".to_string());
        }
        let mut frame = Frame::default();
        while !data.is_empty() {
            let (field, wire, consumed) = take_tag(data)?;
            data = &data[consumed..];
            match (field, wire) {
                (1, 0) => {
                    let (value, n) = take_varint(data)?;
                    frame.seq_id = value;
                    data = &data[n..];
                }
                (2, 0) => {
                    let (value, n) = take_varint(data)?;
                    frame.log_id = value;
                    data = &data[n..];
                }
                (3, 0) => {
                    let (value, n) = take_varint(data)?;
                    frame.service = value as u32 as i32;
                    data = &data[n..];
                }
                (4, 0) => {
                    let (value, n) = take_varint(data)?;
                    frame.method = value as u32 as i32;
                    data = &data[n..];
                }
                (5, 2) => {
                    let (value, n) = take_bytes(data)?;
                    frame.headers.push(unmarshal_header(value)?);
                    data = &data[n..];
                }
                (6, 2) => {
                    let (value, n) = take_bytes(data)?;
                    frame.payload_encoding = String::from_utf8_lossy(value).to_string();
                    data = &data[n..];
                }
                (7, 2) => {
                    let (value, n) = take_bytes(data)?;
                    frame.payload_type = String::from_utf8_lossy(value).to_string();
                    data = &data[n..];
                }
                (8, 2) => {
                    let (value, n) = take_bytes(data)?;
                    frame.payload = Some(value.to_vec());
                    data = &data[n..];
                }
                (9, 2) => {
                    let (value, n) = take_bytes(data)?;
                    frame.log_id_new = String::from_utf8_lossy(value).to_string();
                    data = &data[n..];
                }
                (_, 0) => {
                    let (_, n) = take_varint(data)?;
                    data = &data[n..];
                }
                (_, 1) => {
                    if data.len() < 8 {
                        return Err("截断的 fixed64".to_string());
                    }
                    data = &data[8..];
                }
                (_, 2) => {
                    let (_, n) = take_bytes(data)?;
                    data = &data[n..];
                }
                (_, 5) => {
                    if data.len() < 4 {
                        return Err("截断的 fixed32".to_string());
                    }
                    data = &data[4..];
                }
                _ => return Err("未知的 protobuf 线型".to_string()),
            }
        }
        Ok(frame)
    }
}

fn unmarshal_header(mut data: &[u8]) -> Result<Header, String> {
    let mut header = Header::default();
    while !data.is_empty() {
        let (field, wire, consumed) = take_tag(data)?;
        data = &data[consumed..];
        match (field, wire) {
            (1, 2) => {
                let (value, n) = take_bytes(data)?;
                header.key = String::from_utf8_lossy(value).to_string();
                data = &data[n..];
            }
            (2, 2) => {
                let (value, n) = take_bytes(data)?;
                header.value = String::from_utf8_lossy(value).to_string();
                data = &data[n..];
            }
            (_, 0) => {
                let (_, n) = take_varint(data)?;
                data = &data[n..];
            }
            (_, 2) => {
                let (_, n) = take_bytes(data)?;
                data = &data[n..];
            }
            (_, 1) => data = data.get(8..).ok_or("截断的 fixed64")?,
            (_, 5) => data = data.get(4..).ok_or("截断的 fixed32")?,
            _ => return Err("未知的 header 线型".to_string()),
        }
    }
    Ok(header)
}

fn put_tag(buf: &mut Vec<u8>, field: u64, wire: u64) {
    put_varint(buf, (field << 3) | wire);
}

fn put_varint(buf: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        buf.push((value as u8) | 0x80);
        value >>= 7;
    }
    buf.push(value as u8);
}

fn put_bytes(buf: &mut Vec<u8>, value: &[u8]) {
    put_varint(buf, value.len() as u64);
    buf.extend_from_slice(value);
}

fn take_varint(data: &[u8]) -> Result<(u64, usize), String> {
    let mut value = 0u64;
    let mut shift = 0u32;
    for (index, byte) in data.iter().enumerate() {
        value |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return Ok((value, index + 1));
        }
        shift += 7;
        if shift >= 64 {
            return Err("varint 过长".to_string());
        }
    }
    Err("截断的 varint".to_string())
}

fn take_tag(data: &[u8]) -> Result<(u64, u64, usize), String> {
    let (tag, n) = take_varint(data)?;
    Ok((tag >> 3, tag & 7, n))
}

fn take_bytes(data: &[u8]) -> Result<(&[u8], usize), String> {
    let (length, n) = take_varint(data)?;
    let length = length as usize;
    let end = n.checked_add(length).ok_or("长度溢出")?;
    if end > data.len() {
        return Err("截断的字节段".to_string());
    }
    Ok((&data[n..end], end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_data_frame() {
        let frame = Frame {
            seq_id: 7,
            service: 42,
            method: METHOD_DATA,
            headers: vec![
                Header {
                    key: "type".into(),
                    value: "event".into(),
                },
                Header {
                    key: "message_id".into(),
                    value: "m1".into(),
                },
            ],
            payload: Some(br#"{"hello":1}"#.to_vec()),
            ..Default::default()
        };
        let decoded = Frame::unmarshal(&frame.marshal()).unwrap();
        assert_eq!(decoded.seq_id, 7);
        assert_eq!(decoded.service, 42);
        assert_eq!(decoded.method, METHOD_DATA);
        assert_eq!(decoded.header_value("type"), "event");
        assert_eq!(decoded.header_value("message_id"), "m1");
        assert_eq!(decoded.payload.as_deref(), Some(&br#"{"hello":1}"#[..]));
    }

    #[test]
    fn ack_keeps_inbound_headers() {
        let inbound = Frame {
            service: 9,
            method: METHOD_DATA,
            headers: vec![Header {
                key: "message_id".into(),
                value: "abc".into(),
            }],
            ..Default::default()
        };
        let ack = Frame::ack(&inbound, true);
        assert_eq!(ack.header_value("message_id"), "abc");
        assert_eq!(
            String::from_utf8(ack.payload.unwrap()).unwrap(),
            r#"{"code":200,"headers":null,"data":null}"#
        );
    }
}
