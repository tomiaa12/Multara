use std::{
    fs::{self, File, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, OnceLock, PoisonError, TryLockError},
    thread,
    time::{Duration, Instant},
};

use chrono::Local;

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
