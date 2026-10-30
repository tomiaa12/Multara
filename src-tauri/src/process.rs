use std::{
    ffi::OsStr,
    io::{BufRead, ErrorKind},
    process::Command,
};

/// 子进程单行输出上限。超出部分直接丢弃，避免一行几百 MB 时整行进内存。
pub const MAX_LINE_BYTES: usize = 16 * 1024 * 1024;
pub const LINE_TRUNCATED_MARK: &str = "…（该行过长，已截断）";

/// 读一行（不含换行符），超过 `MAX_LINE_BYTES` 的部分丢弃并追加截断标记。
/// 非法 UTF-8 按 lossy 转换继续读，不会让读取提前结束；EOF 或读错误返回 `None`。
pub fn read_line_bounded<R: BufRead>(reader: &mut R) -> Option<String> {
    let mut line = Vec::new();
    let mut truncated = false;
    let mut read_any = false;
    loop {
        let available = match reader.fill_buf() {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        if available.is_empty() {
            break;
        }
        read_any = true;
        let newline = available.iter().position(|byte| *byte == b'\n');
        let segment = &available[..newline.unwrap_or(available.len())];
        let room = MAX_LINE_BYTES.saturating_sub(line.len());
        truncated |= segment.len() > room;
        line.extend_from_slice(&segment[..segment.len().min(room)]);
        let consumed = segment.len() + usize::from(newline.is_some());
        reader.consume(consumed);
        if newline.is_some() {
            break;
        }
    }
    if !read_any {
        return None;
    }
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    let mut text = String::from_utf8_lossy(&line).into_owned();
    if truncated {
        text.push_str(LINE_TRUNCATED_MARK);
    }
    Some(text)
}

pub struct BoundedLines<R>(pub R);

impl<R: BufRead> Iterator for BoundedLines<R> {
    type Item = String;

    fn next(&mut self) -> Option<String> {
        read_line_bounded(&mut self.0)
    }
}

/// 后台子进程：Windows 上不弹出 cmd / 控制台窗口。
pub fn command(program: impl AsRef<OsStr>) -> Command {
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
}

#[cfg(test)]
mod tests {
    use super::{read_line_bounded, BoundedLines, LINE_TRUNCATED_MARK, MAX_LINE_BYTES};
    use std::io::{BufReader, Cursor};

    #[test]
    fn reads_lines_and_keeps_going_after_invalid_utf8() {
        let data = b"one\r\ntw\xffo\nthree".to_vec();
        let lines: Vec<String> = BoundedLines(BufReader::new(Cursor::new(data))).collect();
        assert_eq!(lines, vec!["one", "tw\u{fffd}o", "three"]);
    }

    #[test]
    fn truncates_overlong_lines_and_reads_next() {
        let mut data = vec![b'x'; MAX_LINE_BYTES + 10];
        data.extend_from_slice(b"\nnext\n");
        let mut reader = BufReader::new(Cursor::new(data));
        let first = read_line_bounded(&mut reader).unwrap();
        assert!(first.ends_with(LINE_TRUNCATED_MARK));
        assert_eq!(first.len(), MAX_LINE_BYTES + LINE_TRUNCATED_MARK.len());
        assert_eq!(read_line_bounded(&mut reader).as_deref(), Some("next"));
        assert_eq!(read_line_bounded(&mut reader), None);
    }

    #[test]
    fn empty_line_is_not_eof() {
        let mut reader = BufReader::new(Cursor::new(b"\nlast".to_vec()));
        assert_eq!(read_line_bounded(&mut reader).as_deref(), Some(""));
        assert_eq!(read_line_bounded(&mut reader).as_deref(), Some("last"));
        assert_eq!(read_line_bounded(&mut reader), None);
    }
}
