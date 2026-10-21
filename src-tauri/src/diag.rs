use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::Path,
    sync::{Mutex, MutexGuard, OnceLock, TryLockError},
    thread,
    time::{Duration, Instant},
};

use chrono::Local;

const WAIT_WARN: Duration = Duration::from_secs(2);
const HEARTBEAT: Duration = Duration::from_secs(15);
const MAX_LOG_BYTES: u64 = 8 * 1024 * 1024;

static LOGGER: OnceLock<Mutex<Logger>> = OnceLock::new();

struct Logger {
    file: File,
}

pub fn init(data_dir: &Path) {
    let path = data_dir.join("multara.log");
    rotate_if_needed(&path);
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok();
    let Some(file) = file else {
        eprintln!("无法创建诊断日志：{}", path.display());
        return;
    };
    let _ = LOGGER.set(Mutex::new(Logger { file }));
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
        .spawn(|| loop {
            thread::sleep(HEARTBEAT);
            write("HEARTBEAT");
        })
        .ok();
}

pub fn write(message: &str) {
    let line = format_line(message);
    if should_echo(message) {
        eprintln!("{line}");
    }
    if let Some(logger) = LOGGER.get() {
        if let Ok(mut logger) = logger.lock() {
            let _ = writeln!(logger.file, "{line}");
            let _ = logger.file.flush();
        }
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
    match mutex.try_lock() {
        Ok(guard) => return Ok(guard),
        Err(TryLockError::Poisoned(error)) => return Err(error.to_string()),
        Err(TryLockError::WouldBlock) => {}
    }

    let mut warned = Duration::ZERO;
    loop {
        match mutex.try_lock() {
            Ok(guard) => {
                let elapsed = started.elapsed();
                if elapsed >= WAIT_WARN {
                    write(&format!(
                        "GOT  {name} {:.2}s at {at}",
                        elapsed.as_secs_f64()
                    ));
                }
                return Ok(guard);
            }
            Err(TryLockError::Poisoned(error)) => return Err(error.to_string()),
            Err(TryLockError::WouldBlock) => {
                let elapsed = started.elapsed();
                if elapsed >= WAIT_WARN && elapsed.saturating_sub(warned) >= WAIT_WARN {
                    write(&format!(
                        "WAIT {name} {:.2}s at {at}",
                        elapsed.as_secs_f64()
                    ));
                    warned = elapsed;
                }
                thread::sleep(Duration::from_millis(25));
            }
        }
    }
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
