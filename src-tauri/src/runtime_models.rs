use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock, PoisonError},
    time::{Duration, Instant},
};

use serde::Serialize;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeModel {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeModels {
    /// 该运行时会不会收到 `--model`；为 false 时填了模型也会被忽略
    pub supported: bool,
    /// cli：从 CLI 实时查询；preset：CLI 不能列举，用官方别名；manual：只能手填
    pub source: &'static str,
    pub models: Vec<RuntimeModel>,
    pub error: Option<String>,
}

/// CLI 查询较慢（cursor 约 3 秒），结果按运行时缓存一段时间
const CACHE_TTL: Duration = Duration::from_secs(10 * 60);
/// 没登录或查询失败也短暂记住，免得每次挂载选择器都再起一次 CLI。
const FAILURE_CACHE_TTL: Duration = Duration::from_secs(30);
const LIST_TIMEOUT: Duration = Duration::from_secs(20);

fn cache() -> &'static Mutex<HashMap<String, (Instant, RuntimeModels)>> {
    static CACHE: OnceLock<Mutex<HashMap<String, (Instant, RuntimeModels)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// `since` 为 `Some` 时只要这之后写入的结果（刷新时等到的别人刚查完的那份）。
fn cached(runtime_id: &str, since: Option<Instant>) -> Option<RuntimeModels> {
    let cache = cache().lock().unwrap_or_else(PoisonError::into_inner);
    let (at, models) = cache.get(runtime_id)?;
    let ttl = if models.models.is_empty() {
        FAILURE_CACHE_TTL
    } else {
        CACHE_TTL
    };
    let fresh = at.elapsed() < ttl && since.map_or(true, |since| *at >= since);
    fresh.then(|| models.clone())
}

/// 这些运行时会把模型透传给 CLI；不能可靠枚举时允许手动填写。
pub fn list(runtime_id: &str, refresh: bool) -> RuntimeModels {
    match runtime_id {
        "cursor" => cursor_models(refresh),
        // Claude Code 没有列模型的命令；--model 接受这些别名（始终指向该系列最新版本），也接受完整模型名
        "claude" => RuntimeModels {
            supported: true,
            source: "preset",
            models: [
                ("sonnet", "Sonnet（最新）"),
                ("opus", "Opus（最新）"),
                ("haiku", "Haiku（最新）"),
            ]
            .into_iter()
            .map(|(id, label)| RuntimeModel {
                id: id.into(),
                label: label.into(),
            })
            .collect(),
            error: None,
        },
        "codex" | "gemini" | "qwen" | "opencode" => RuntimeModels {
            supported: true,
            source: "manual",
            models: Vec::new(),
            error: None,
        },
        _ => RuntimeModels {
            supported: false,
            source: "manual",
            models: Vec::new(),
            error: None,
        },
    }
}

fn cursor_models(refresh: bool) -> RuntimeModels {
    // 同一时间只起一个 `--list-models`；排队的调用者直接用前一个的结果。
    static LISTING: Mutex<()> = Mutex::new(());
    let requested = Instant::now();
    if let Some(hit) = cached("cursor", None).filter(|_| !refresh) {
        return hit;
    }
    let _listing = LISTING.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(hit) = cached("cursor", refresh.then_some(requested)) {
        return hit;
    }
    let models = query_cursor_models();
    cache()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert("cursor".into(), (Instant::now(), models.clone()));
    models
}

fn query_cursor_models() -> RuntimeModels {
    let result = crate::runtime::execution("cursor", "")
        .and_then(|(path, _, _)| run_with_timeout(path, &["--list-models"]))
        .map(|text| parse_cursor_models(&text));
    match result {
        Ok(models) if !models.is_empty() => RuntimeModels {
            supported: true,
            source: "cli",
            models,
            error: None,
        },
        Ok(_) => RuntimeModels {
            supported: true,
            source: "manual",
            models: Vec::new(),
            error: Some("Cursor CLI 没有返回模型列表，可能还没登录（cursor-agent login）".into()),
        },
        Err(error) => RuntimeModels {
            supported: true,
            source: "manual",
            models: Vec::new(),
            error: Some(error),
        },
    }
}

fn run_with_timeout(path: std::path::PathBuf, args: &[&str]) -> Result<String, String> {
    let mut command = crate::process::command(path);
    command.args(args);
    let output = crate::runtime::output_with_timeout(&mut command, LIST_TIMEOUT)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "查询模型列表超时".to_string())?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            "查询模型列表失败".into()
        } else {
            stderr
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            // CSI 序列：ESC [ ... 终止字节（@ 到 ~）
            if chars.peek() == Some(&'[') {
                chars.next();
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            }
            continue;
        }
        // 输出里夹带零宽字符（如 U+200B），去掉以免显示和比较出问题
        if matches!(ch, '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{feff}') {
            continue;
        }
        out.push(ch);
    }
    out
}

/// 解析 `cursor-agent --list-models`：每行 `id - 显示名`，首行是标题
fn parse_cursor_models(text: &str) -> Vec<RuntimeModel> {
    strip_ansi(text)
        .lines()
        .filter_map(|line| {
            let (id, label) = line.trim().split_once(" - ")?;
            let id = id.trim();
            if id.is_empty() || id.contains(char::is_whitespace) {
                return None;
            }
            let label = label
                .replace("(current, default)", "")
                .replace("(current)", "")
                .replace("(default)", "");
            let label = label.split_whitespace().collect::<Vec<_>>().join(" ");
            Some(RuntimeModel {
                id: id.to_string(),
                label: if label.is_empty() {
                    id.to_string()
                } else {
                    label
                },
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cursor_model_list() {
        let text = "Available models\n\nauto - Auto (current, default)\ngpt-5.2 - GPT-5.2\n\u{1b}[2mgrok-4.7-low-fast - Grok 4.7  Low Fast\u{200b}\u{200b}\u{1b}[0m\n";
        let models = parse_cursor_models(text);
        assert_eq!(models.len(), 3);
        assert_eq!(models[0].id, "auto");
        assert_eq!(models[0].label, "Auto");
        assert_eq!(models[2].id, "grok-4.7-low-fast");
        assert_eq!(models[2].label, "Grok 4.7 Low Fast");
    }

    #[test]
    fn only_model_aware_runtimes_are_supported() {
        assert!(list("claude", false).supported);
        assert!(list("codex", false).supported);
        assert!(list("gemini", false).supported);
        assert!(list("qwen", false).supported);
        assert!(list("opencode", false).supported);
    }
}
