use std::{
    fs::{self, File, OpenOptions},
    io::{BufWriter, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, OnceLock, PoisonError, TryLockError},
    thread,
    time::{Duration, Instant},
};

use chrono::Local;
use tauri::Manager;

const WAIT_WARN: Duration = Duration::from_secs(2);
const HEARTBEAT: Duration = Duration::from_secs(15);
/// 缓冲日志最多滞后这么久落盘；告警类的行立即落盘。
const FLUSH_INTERVAL: Duration = Duration::from_secs(1);
const MAX_LOG_BYTES: u64 = 8 * 1024 * 1024;

static LOGGER: OnceLock<Mutex<Logger>> = OnceLock::new();

struct Logger {
    file: BufWriter<File>,
    path: PathBuf,
    written: u64,
}

impl Logger {
    fn append(&mut self, line: &str, urgent: bool) {
        let _ = writeln!(self.file, "{line}");
        self.written += line.len() as u64 + 1;
        if self.written >= MAX_LOG_BYTES {
            self.rotate();
        } else if urgent {
            let _ = self.file.flush();
        }
    }

    fn rotate(&mut self) {
        let _ = self.file.flush();
        rotate_if_needed(&self.path);
        let Some(file) = open_log(&self.path) else {
            return;
        };
        let len = file.metadata().map(|meta| meta.len()).unwrap_or(0);
        // 改名失败时还是原文件，别每写一行都重试轮转。
        self.written = if len >= MAX_LOG_BYTES { 0 } else { len };
        self.file = BufWriter::new(file);
    }
}

fn open_log(path: &Path) -> Option<File> {
    OpenOptions::new().create(true).append(true).open(path).ok()
}

fn logger() -> Option<MutexGuard<'static, Logger>> {
    Some(LOGGER.get()?.lock().unwrap_or_else(PoisonError::into_inner))
}

pub fn init(data_dir: &Path) {
    let path = data_dir.join("multara.log");
    rotate_if_needed(&path);
    let Some(file) = open_log(&path) else {
        eprintln!("无法创建诊断日志：{}", path.display());
        return;
    };
    let written = file.metadata().map(|meta| meta.len()).unwrap_or(0);
    let _ = LOGGER.set(Mutex::new(Logger {
        file: BufWriter::new(file),
        path: path.clone(),
        written,
    }));
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        write(&format!("PANIC {info}"));
        previous(info);
    }));
    write(&format!(
        "==== Multara 启动 pid={} ====",
        std::process::id()
    ));
    write(&format!("日志文件 {}", path.display()));
    thread::Builder::new()
        .name("multara-diag".into())
        .spawn(|| {
            let mut last_heartbeat = Instant::now();
            loop {
                thread::sleep(FLUSH_INTERVAL);
                if last_heartbeat.elapsed() >= HEARTBEAT {
                    last_heartbeat = Instant::now();
                    write("HEARTBEAT");
                }
                if let Some(mut logger) = logger() {
                    let _ = logger.file.flush();
                }
            }
        })
        .ok();
}

pub fn write(message: &str) {
    let line = format_line(message);
    let urgent = should_echo(message);
    if urgent {
        eprintln!("{line}");
    }
    if let Some(mut logger) = logger() {
        logger.append(&line, urgent);
    }
}

fn format_line(message: &str) -> String {
    format!(
        "{} [{}] {}",
        Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
        format!("{:?}", thread::current().id())
            .replace("ThreadId(", "")
            .replace(')', ""),
        message
    )
}

fn should_echo(message: &str) -> bool {
    message.starts_with("====")
        || message.starts_with("日志文件")
        || message.starts_with("WAIT ")
        || message.starts_with("GOT  ")
        || message.starts_with("PANIC ")
        || message.starts_with("POISONED ")
        || message.starts_with("REAP ")
        || message.contains(" SLOW")
}

pub fn span(name: &'static str) -> Span {
    write(&format!("ENTER {name}"));
    Span {
        name,
        started: Instant::now(),
    }
}

pub struct Span {
    name: &'static str,
    started: Instant,
}

impl Drop for Span {
    fn drop(&mut self) {
        let elapsed = self.started.elapsed().as_secs_f64();
        if elapsed >= WAIT_WARN.as_secs_f64() {
            write(&format!("LEAVE {} {elapsed:.2}s SLOW", self.name));
        } else {
            write(&format!("LEAVE {} {elapsed:.2}s", self.name));
        }
    }
}

#[track_caller]
pub fn lock_mutex<'a, T>(mutex: &'a Mutex<T>, name: &str) -> Result<MutexGuard<'a, T>, String> {
    let location = std::panic::Location::caller();
    let at = format!("{}:{}", file_name(location.file()), location.line());
    let started = Instant::now();
    let guard = match mutex.try_lock() {
        Ok(guard) => return Ok(guard),
        Err(TryLockError::Poisoned(error)) => error.into_inner(),
        Err(TryLockError::WouldBlock) => {
            let guard = mutex.lock().unwrap_or_else(PoisonError::into_inner);
            let elapsed = started.elapsed();
            if elapsed >= WAIT_WARN {
                write(&format!(
                    "WAIT {name} {:.2}s at {at}",
                    elapsed.as_secs_f64()
                ));
            }
            guard
        }
    };
    // 持锁线程 panic 过；数据库连接本身仍可用，清掉中毒标记而不是让之后每次都失败。
    if mutex.is_poisoned() {
        mutex.clear_poison();
        write(&format!("POISONED {name} 已恢复 at {at}"));
    }
    Ok(guard)
}

fn file_name(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_report_keeps_failures_and_drops_heartbeats() {
        let dir = std::env::temp_dir().join(format!(
            "multara-error-log-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("multara.log"),
            "\
2026-11-12 09:00:00.000 [1] HEARTBEAT
2026-11-12 09:00:01.000 [1] LEAVE list_runtimes 4.39s SLOW
2026-11-12 09:00:02.000 [1] ENTER list_agents
2026-11-12 09:00:03.000 [1] webhook delivery abc dispatch failed: timeout
2026-11-12 09:00:04.000 [1] PANIC worker exploded
2026-11-12 09:00:05.000 [1] 监听工作区 ws 失败：denied
",
        )
        .unwrap();

        let report = recent_error_report(&dir, "9.9.9");
        assert!(report.starts_with("Multara error log\n"));
        assert!(report.contains("version: 9.9.9\n"));
        assert!(report.contains("SLOW"));
        assert!(report.contains("dispatch failed"));
        assert!(report.contains("PANIC worker exploded"));
        assert!(report.contains("失败"));
        assert!(!report.contains("HEARTBEAT"));
        assert!(!report.contains("ENTER list_agents"));
        let _ = fs::remove_dir_all(&dir);
    }
}

pub fn flush() {
    if let Some(mut logger) = logger() {
        let _ = logger.file.flush();
    }
}

const ERROR_TAIL_BYTES: u64 = 1024 * 1024;
const MAX_ERROR_LINES: usize = 800;
const MAX_ERROR_LINE_CHARS: usize = 1_000;
const LOG_FILE_LIMIT: usize = 2 * 1024 * 1024;

/// 从当前日志和上一份轮转日志的末尾抽出报错、卡顿、崩溃行。
pub fn recent_error_report(data_dir: &Path, version: &str) -> String {
    let current = data_dir.join("multara.log");
    let previous = current.with_extension("log.old");
    let mut matched = Vec::new();
    let mut missing = Vec::new();
    for path in [&previous, &current] {
        match read_tail(path, ERROR_TAIL_BYTES) {
            Ok(text) => {
                for line in text.lines() {
                    if is_error_line(line) {
                        matched.push(clip_error_line(line));
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(path.display().to_string());
            }
            Err(error) => matched.push(format!("# 无法读取 {}: {error}", path.display())),
        }
    }
    let dropped = matched.len().saturating_sub(MAX_ERROR_LINES);
    if dropped > 0 {
        matched.drain(0..dropped);
    }
    let mut report = String::new();
    report.push_str("Multara error log\n");
    report.push_str(&format!("version: {version}\n"));
    report.push_str(&format!(
        "exported: {}\n",
        Local::now().format("%Y-%m-%d %H:%M:%S")
    ));
    report.push_str(&format!("matched: {}\n", matched.len()));
    report.push_str(&format!("dropped: {dropped}\n"));
    if !missing.is_empty() {
        report.push_str(&format!("missing: {}\n", missing.join(", ")));
    }
    report.push_str(
        "filter: panic, error, failed, failure, wait, slow, reap, poisoned, stuck, 失败, 出错\n",
    );
    report.push_str("---\n");
    if matched.is_empty() {
        report.push_str("(no matching lines in the recent log tail)\n");
    } else {
        for line in &matched {
            report.push_str(line);
            report.push('\n');
        }
    }
    report
}

fn is_error_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("panic")
        || lower.contains("error")
        || lower.contains("failed")
        || lower.contains("failure")
        || lower.contains(" wait ")
        || lower.contains(" slow")
        || lower.contains("reap ")
        || lower.contains("poisoned")
        || lower.contains("stuck")
        || line.contains("失败")
        || line.contains("出错")
}

fn clip_error_line(line: &str) -> String {
    let mut chars = line.chars();
    let clipped: String = chars.by_ref().take(MAX_ERROR_LINE_CHARS).collect();
    if chars.next().is_none() {
        clipped
    } else {
        format!("{clipped}…")
    }
}

fn read_tail(path: &Path, max_bytes: u64) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    let skipped = if len > max_bytes {
        file.seek(SeekFrom::Start(len - max_bytes))?;
        true
    } else {
        false
    };
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if skipped {
        if let Some(index) = text.find('\n') {
            text.drain(..=index);
        }
    }
    Ok(text)
}

#[tauri::command]
pub async fn export_recent_error_log(app: tauri::AppHandle) -> Result<String, String> {
    let version = app.package_info().version.to_string();
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        flush();
        recent_error_report(&data_dir, &version)
    })
    .await
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn save_log_file(path: String, contents: String) -> Result<(), String> {
    let path = PathBuf::from(&path);
    if !path.is_absolute() {
        return Err(tr!("日志文件必须保存到绝对路径"));
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    if !extension.eq_ignore_ascii_case("log") && !extension.eq_ignore_ascii_case("txt") {
        return Err(tr!("日志文件必须是文本"));
    }
    if contents.len() > LOG_FILE_LIMIT {
        return Err(tr!("日志文件过大"));
    }
    if !contents.starts_with("Multara error log\n") {
        return Err(tr!("日志文件内容不正确"));
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
    }
    fs::write(path, contents).map_err(|error| error.to_string())
}

fn rotate_if_needed(path: &Path) {
    let Ok(meta) = fs::metadata(path) else {
        return;
    };
    if meta.len() < MAX_LOG_BYTES {
        return;
    }
    let old = path.with_extension("log.old");
    let _ = fs::remove_file(&old);
    let _ = fs::rename(path, old);
}
