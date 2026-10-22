use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{mpsc, Mutex, OnceLock},
    thread,
    time::Duration,
};

#[cfg(unix)]
use std::{io::Read, process::Command, time::Instant};

use crate::models::Runtime;

struct RuntimeSpec {
    id: &'static str,
    name: &'static str,
    provider: &'static str,
    commands: &'static [&'static str],
    default_concurrency: u8,
}

const RUNTIME_SPECS: &[RuntimeSpec] = &[
    RuntimeSpec {
        id: "claude",
        name: "Claude Code",
        provider: "claude",
        commands: &["claude"],
        default_concurrency: 2,
    },
    RuntimeSpec {
        id: "codex",
        name: "Codex",
        provider: "codex",
        commands: &["codex"],
        default_concurrency: 2,
    },
    RuntimeSpec {
        id: "cursor",
        name: "Cursor",
        provider: "cursor",
        commands: &["cursor-agent", "agent"],
        default_concurrency: 3,
    },
    RuntimeSpec {
        id: "copilot",
        name: "GitHub Copilot",
        provider: "copilot",
        commands: &["github-copilot-cli", "copilot"],
        default_concurrency: 2,
    },
    RuntimeSpec {
        id: "opencode",
        name: "OpenCode",
        provider: "opencode",
        commands: &["opencode"],
        default_concurrency: 2,
    },
    RuntimeSpec {
        id: "gemini",
        name: "Gemini CLI",
        provider: "gemini",
        commands: &["gemini"],
        default_concurrency: 2,
    },
    RuntimeSpec {
        id: "qwen",
        name: "Qwen Code",
        provider: "qwen",
        commands: &["qwen"],
        default_concurrency: 2,
    },
    RuntimeSpec {
        id: "kimi",
        name: "Kimi CLI",
        provider: "kimi",
        commands: &["kimi"],
        default_concurrency: 2,
    },
];

fn command_path_cache() -> &'static Mutex<HashMap<String, PathBuf>> {
    static CACHE: OnceLock<Mutex<HashMap<String, PathBuf>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

const MANAGED_ENV_KEYS: &[&str] = &[
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_MODEL",
    "OPENAI_BASE_URL",
    "OPENAI_API_KEY",
    "OPENAI_MODEL",
    "GOOGLE_GEMINI_BASE_URL",
    "GEMINI_API_KEY",
    "GEMINI_MODEL",
    "MULTARA_GATEWAY_KEY",
    "OPENCODE_CONFIG_CONTENT",
];

pub fn managed_env_keys() -> &'static [&'static str] {
    MANAGED_ENV_KEYS
}

const PROVIDERS: &[&str] = &[
    "claude",
    "codebuddy",
    "codex",
    "copilot",
    "opencode",
    "codearts",
    "deveco",
    "openclaw",
    "hermes",
    "pi",
    "cursor",
    "kimi",
    "reasonix",
    "dsh",
    "dim",
    "kiro",
    "antigravity",
    "qoder",
    "qoderclicn",
    "traecli",
    "grok",
    "gemini",
    "qwen",
    "qwenpaw",
    "mcode",
    "zeroclaw",
    "omp",
];

pub fn provider_supported(provider: &str) -> bool {
    PROVIDERS.contains(&provider)
}

/// Bidirectional stdio. The prompt is a protocol message, not a one-shot stdin write.
pub fn protocol_driver(provider: &str) -> Option<&'static str> {
    match provider {
        "hermes" | "kimi" | "reasonix" | "dim" | "kiro" | "qoder" | "qoderclicn" | "traecli"
        | "grok" | "qwenpaw" | "mcode" | "zeroclaw" => Some("acp"),
        "dsh" => Some("dsh"),
        _ => None,
    }
}

pub fn provider_gateway(provider: &str) -> bool {
    matches!(provider, "claude" | "codex" | "gemini" | "qwen" | "opencode")
}

pub fn protocol_for(provider: &str) -> &'static str {
    match provider {
        "claude" | "codebuddy" => "anthropic",
        "gemini" => "gemini",
        _ => "openai",
    }
}

pub fn prompt_via_stdin(provider: &str) -> bool {
    matches!(
        provider,
        "claude" | "codebuddy" | "cursor" | "qwen" | "opencode" | "codearts" | "pi" | "omp"
    )
}

pub fn detect() -> Vec<Runtime> {
    // A login shell snapshot goes stale after installers edit the profile.
    // Refresh it on every scan so "detect again" sees CLIs installed later.
    #[cfg(unix)]
    refresh_login_path_dirs();
    RUNTIME_SPECS.iter().map(detect_spec).collect()
}

pub fn seed_runtime_id() -> String {
    detect()
        .into_iter()
        .find(|runtime| runtime.status == "online")
        .map(|runtime| runtime.id)
        .unwrap_or_else(|| "cursor".to_string())
}

fn detect_spec(spec: &RuntimeSpec) -> Runtime {
    let detected = spec.commands.iter().find_map(|command| {
        resolve_runtime_command(command, spec.provider)
            .map(|path| ((*command).to_string(), path))
    });
    let (command, path, version, status) = match detected {
        Some((command, path)) => {
            let version = cli_version(&path);
            (
                command,
                Some(path.to_string_lossy().to_string()),
                version,
                "online",
            )
        }
        None => (spec.commands[0].to_string(), None, None, "not_found"),
    };
    Runtime {
        id: spec.id.to_string(),
        name: spec.name.to_string(),
        provider: spec.provider.to_string(),
        command,
        args: Vec::new(),
        path,
        version,
        status,
        max_concurrent: spec.default_concurrency.min(6),
        protocol: String::new(),
        api_url: String::new(),
        models: Vec::new(),
        kind: "builtin".into(),
        description: String::new(),
    }
}

pub fn detect_custom(
    id: String,
    name: String,
    command: String,
    args: Vec<String>,
    max_concurrent: u8,
    provider: String,
    protocol: String,
    api_url: String,
    models: Vec<String>,
    kind: String,
    description: String,
) -> Runtime {
    let provider = if provider.is_empty() {
        "custom".into()
    } else {
        provider
    };
    let path = resolve_runtime_command(&command, &provider);
    let version = path.as_deref().and_then(cli_version);
    Runtime {
        id,
        name,
        provider,
        command,
        args,
        version,
        path: path.as_ref().map(|path| path.to_string_lossy().to_string()),
        status: if path.is_some() {
            "online"
        } else {
            "not_found"
        },
        max_concurrent: max_concurrent.clamp(1, 6),
        protocol,
        api_url,
        models,
        kind: if kind == "gateway" {
            "gateway".into()
        } else {
            "profile".into()
        },
        description,
    }
}

pub fn trim_base_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

pub fn gateway_env(
    provider: &str,
    url: &str,
    key: &str,
    model: &str,
    key_style: &str,
) -> HashMap<String, String> {
    let mut env = HashMap::new();
    let base = trim_base_url(url);
    let token = key.trim();
    let model = model.trim();
    let model = (model != "auto").then_some(model).unwrap_or("");
    match provider {
        "claude" => {
            if !base.is_empty() {
                env.insert("ANTHROPIC_BASE_URL".into(), base);
            }
            if !token.is_empty() {
                let name = if key_style == "x-api-key" {
                    "ANTHROPIC_API_KEY"
                } else {
                    "ANTHROPIC_AUTH_TOKEN"
                };
                env.insert(name.into(), token.into());
            }
            if !model.is_empty() {
                env.insert("ANTHROPIC_MODEL".into(), model.into());
            }
        }
        "gemini" => {
            if !base.is_empty() {
                env.insert("GOOGLE_GEMINI_BASE_URL".into(), base);
            }
            if !token.is_empty() {
                env.insert("GEMINI_API_KEY".into(), token.into());
            }
            if !model.is_empty() {
                env.insert("GEMINI_MODEL".into(), model.into());
            }
        }
        "qwen" => {
            if !base.is_empty() {
                env.insert("OPENAI_BASE_URL".into(), base);
            }
            if !token.is_empty() {
                env.insert("OPENAI_API_KEY".into(), token.into());
            }
            if !model.is_empty() {
                env.insert("OPENAI_MODEL".into(), model.into());
            }
        }
        "codex" => {
            if !token.is_empty() {
                env.insert("MULTARA_GATEWAY_KEY".into(), token.into());
            }
        }
        "opencode" => {
            if !token.is_empty() {
                env.insert("OPENAI_API_KEY".into(), token.into());
            }
            if !base.is_empty() {
                env.insert("OPENAI_BASE_URL".into(), base.clone());
                env.insert(
                    "OPENCODE_CONFIG_CONTENT".into(),
                    opencode_config(&base, model),
                );
            }
        }
        _ => {}
    }
    env
}

fn opencode_config(base: &str, model: &str) -> String {
    let model_id = if model.is_empty() { "default" } else { model };
    serde_json::json!({
        "provider": {
            "multara": {
                "npm": "@ai-sdk/openai-compatible",
                "name": "Multara",
                "options": {
                    "baseURL": base,
                    "apiKey": "{env:OPENAI_API_KEY}"
                },
                "models": {
                    model_id: { "name": model_id }
                }
            }
        },
        "model": format!("multara/{model_id}")
    })
    .to_string()
}

pub fn gateway_args(provider: &str, url: &str, model: &str, wire_api: &str) -> Vec<String> {
    if provider != "codex" {
        return Vec::new();
    }
    let base = trim_base_url(url);
    if base.is_empty() {
        return Vec::new();
    }
    let wire = if wire_api == "chat" { "chat" } else { "responses" };
    let mut args = vec![
        "-c".into(),
        "model_provider=multara".into(),
        "-c".into(),
        format!("model_providers.multara.base_url=\"{base}\""),
        "-c".into(),
        "model_providers.multara.env_key=MULTARA_GATEWAY_KEY".into(),
        "-c".into(),
        format!("model_providers.multara.wire_api={wire}"),
    ];
    let model = model.trim();
    if !model.is_empty() && model != "auto" {
        args.push("-m".into());
        args.push(model.into());
    }
    args
}

pub fn compose_args(
    provider: &str,
    prefix: &[String],
    assignment: &str,
    gateway: bool,
    api_url: &str,
    model: &str,
    wire_api: &str,
) -> Vec<String> {
    let builtin = build_args(provider, assignment);
    let extra = if gateway {
        gateway_args(provider, api_url, model, wire_api)
    } else {
        Vec::new()
    };
    let mut args = prefix.to_vec();
    if builtin.last().is_some_and(|arg| arg == assignment) && !prompt_via_stdin(provider) {
        let prompt = builtin.last().cloned().unwrap_or_default();
        args.extend(builtin[..builtin.len() - 1].iter().cloned());
        args.extend(extra);
        args.push(prompt);
        return args;
    }
    args.extend(builtin);
    args.extend(extra);
    args
}

pub fn stdin_prompt(provider: &str, prompt: &str) -> Option<Vec<u8>> {
    if !prompt_via_stdin(provider) {
        return None;
    }
    if provider == "claude" || provider == "codebuddy" {
        let mut payload = serde_json::json!({
            "type": "user",
            "message": {
                "role": "user",
                "content": [{ "type": "text", "text": prompt }]
            }
        })
        .to_string()
        .into_bytes();
        payload.push(b'\n');
        return Some(payload);
    }
    Some(prompt.as_bytes().to_vec())
}

pub fn execution(runtime_id: &str, assignment: &str) -> Result<(PathBuf, Vec<String>, u8), String> {
    let spec = RUNTIME_SPECS
        .iter()
        .find(|item| item.id == runtime_id)
        .ok_or_else(|| format!("未知运行时：{runtime_id}"))?;
    let path = spec
        .commands
        .iter()
        .find_map(|command| resolve_runtime_command(command, spec.provider))
        .ok_or_else(|| format!("未找到 {} CLI", spec.name))?;
    Ok((
        path,
        build_args(runtime_id, assignment),
        spec.default_concurrency.min(6),
    ))
}

pub(crate) fn build_args(runtime_id: &str, assignment: &str) -> Vec<String> {
    match runtime_id {
        "claude" => vec![
            "-p".into(),
            "--output-format".into(),
            "stream-json".into(),
            "--input-format".into(),
            "stream-json".into(),
            "--verbose".into(),
            "--permission-mode".into(),
            "bypassPermissions".into(),
            "--disallowedTools".into(),
            "AskUserQuestion".into(),
        ],
        // 需求 / 任务 / 收件箱 / 右下角聊天都可能没有 Git（临时工作区或纯文件夹）。
        // Codex exec 默认要求仓库根，否则直接退出；由 Multara 管 cwd，统一跳过该检查。
        "codex" => vec![
            "exec".into(),
            "--json".into(),
            "--skip-git-repo-check".into(),
            assignment.into(),
        ],
        "cursor" => vec![
            "-p".into(),
            "--output-format".into(),
            "stream-json".into(),
            "--yolo".into(),
        ],
        "qwen" => vec![
            "--output-format".into(),
            "stream-json".into(),
            "--yolo".into(),
        ],
        "gemini" => vec![
            "-p".into(),
            assignment.into(),
            "--output-format".into(),
            "stream-json".into(),
        ],
        "copilot" => vec![
            "-p".into(),
            assignment.into(),
            "--output-format".into(),
            "json".into(),
            "--allow-all".into(),
            "--no-ask-user".into(),
        ],
        "opencode" => vec![
            "run".into(),
            "--format".into(),
            "json".into(),
            "--dangerously-skip-permissions".into(),
        ],
        "codebuddy" => vec![
            "-p".into(),
            "--output-format".into(),
            "stream-json".into(),
            "--input-format".into(),
            "stream-json".into(),
            "--verbose".into(),
            "--permission-mode".into(),
            "bypassPermissions".into(),
            "--disallowedTools".into(),
            "AskUserQuestion".into(),
            "--disallowedTools".into(),
            "EnterPlanMode".into(),
            "--disallowedTools".into(),
            "ExitPlanMode".into(),
        ],
        "codearts" => vec!["run".into(), "--format".into(), "json".into(), "--auto".into()],
        "deveco" => vec![
            "run".into(),
            "--format".into(),
            "json".into(),
            "--dangerously-skip-permissions".into(),
            assignment.into(),
        ],
        "openclaw" => vec![
            "agent".into(),
            "--local".into(),
            "--json".into(),
            "--session-id".into(),
            format!("multara-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)),
            "--message".into(),
            assignment.into(),
        ],
        "pi" | "omp" => vec!["-p".into(), "--mode".into(), "json".into()],
        "antigravity" => vec!["-p".into(), assignment.into()],
        "hermes" | "kimi" | "reasonix" | "dim" | "qwenpaw" | "mcode" | "zeroclaw" => {
            vec!["acp".into()]
        }
        "kiro" => vec!["acp".into(), "--trust-all-tools".into()],
        "qoder" | "qoderclicn" => vec!["--yolo".into(), "--acp".into()],
        "traecli" => vec!["acp".into(), "serve".into(), "--yolo".into()],
        "grok" => vec![
            "--no-auto-update".into(),
            "agent".into(),
            "--always-approve".into(),
            "stdio".into(),
        ],
        "dsh" => vec!["--profile".into(), "multica".into(), "--stdio".into()],
        _ => vec![assignment.into()],
    }
}

pub fn resolve_runtime_command(command: &str, provider: &str) -> Option<PathBuf> {
    let command = command.trim();
    if command.is_empty() {
        return None;
    }
    if provider == "codex" && command == "codex" {
        if let Some(path) = codex_override() {
            return path;
        }
    }
    if is_explicit_path(command) {
        let path = PathBuf::from(command);
        return is_executable(&path).then_some(path);
    }
    if let Ok(cache) = command_path_cache().lock() {
        if let Some(cached) = cache.get(command) {
            if is_executable(cached) {
                return Some(cached.clone());
            }
        }
    }
    let path = if path_lookup_disabled() {
        None
    } else {
        which::which(command)
            .ok()
            .or_else(|| resolve_from_login_path(command))
    }
    .or_else(|| {
        if provider == "codex" && command == "codex" {
            codex_bundle_paths()
                .into_iter()
                .find(|path| is_executable(path))
        } else {
            None
        }
    });
    if let Some(path) = &path {
        if let Ok(mut cache) = command_path_cache().lock() {
            cache.insert(command.to_string(), path.clone());
        }
    }
    path
}

/// Same rule as Multica's `detectCLIVersion`: run `<cli> --version` and keep
/// the first line that contains a semver token, such as `2.1.5 (Claude Code)`
/// or `codex-cli 0.118.0`.
fn cli_version(path: &Path) -> Option<String> {
    let key = path.to_string_lossy().to_string();
    let cache = version_cache();
    if let Some(cached) = cache.lock().ok().and_then(|cache| cache.get(&key).cloned()) {
        return cached;
    }
    let version = probe_cli_version(path);
    if let Ok(mut cache) = cache.lock() {
        cache.insert(key, version.clone());
    }
    version
}

fn version_cache() -> &'static Mutex<HashMap<String, Option<String>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn probe_cli_version(path: &Path) -> Option<String> {
    let path = path.to_path_buf();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let output = crate::process::command(&path)
            .arg("--version")
            .env("CI", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output();
        let _ = tx.send(output);
    });
    let output = rx.recv_timeout(Duration::from_secs(8)).ok()?.ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    extract_version_line(&stdout)
        .or_else(|| extract_version_line(&stderr))
        .or_else(|| {
            output.status.success().then(|| {
                let raw = stdout.trim();
                let raw = if raw.is_empty() { stderr.trim() } else { raw };
                (!raw.is_empty()).then(|| truncate_version(raw))
            })?
        })
}

fn extract_version_line(raw: &str) -> Option<String> {
    let pattern = regex::Regex::new(r"\d+\.\d+\.\d+").ok()?;
    raw.lines().map(str::trim).find(|line| !line.is_empty() && pattern.is_match(line)).map(truncate_version)
}

fn truncate_version(line: &str) -> String {
    let mut chars = line.chars();
    let truncated: String = chars.by_ref().take(200).collect();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

fn is_explicit_path(command: &str) -> bool {
    command.contains('/') || command.contains('\\')
}

fn is_executable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        return meta.permissions().mode() & 0o111 != 0;
    }
    #[cfg(not(unix))]
    true
}

/// `Some(None)` means the override is set but the file is missing: do not fall back.
fn codex_override() -> Option<Option<PathBuf>> {
    let raw = std::env::var("MULTARA_CODEX_PATH").ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let path = PathBuf::from(trimmed);
    Some(is_executable(&path).then_some(path))
}

fn codex_bundle_paths() -> Vec<PathBuf> {
    if let Some(paths) = bundle_override() {
        return paths;
    }
    #[cfg(target_os = "macos")]
    {
        let mut paths = vec![
            PathBuf::from("/Applications/ChatGPT.app/Contents/Resources/codex-cli/bin/codex"),
            PathBuf::from("/Applications/ChatGPT.app/Contents/Resources/codex"),
            PathBuf::from("/Applications/Codex.app/Contents/Resources/codex"),
        ];
        if let Some(home) = std::env::var_os("HOME") {
            let home = PathBuf::from(home).join("Applications");
            paths.push(home.join("ChatGPT.app/Contents/Resources/codex-cli/bin/codex"));
            paths.push(home.join("ChatGPT.app/Contents/Resources/codex"));
            paths.push(home.join("Codex.app/Contents/Resources/codex"));
        }
        return paths;
    }
    #[cfg(not(target_os = "macos"))]
    Vec::new()
}

fn bundle_override() -> Option<Vec<PathBuf>> {
    #[cfg(test)]
    {
        return CODEX_BUNDLE_OVERRIDE.lock().ok().and_then(|guard| guard.clone());
    }
    #[cfg(not(test))]
    None
}

fn path_lookup_disabled() -> bool {
    #[cfg(test)]
    {
        return PATH_LOOKUP_DISABLED
            .lock()
            .map(|guard| *guard)
            .unwrap_or(false);
    }
    #[cfg(not(test))]
    false
}

#[cfg(test)]
static CODEX_BUNDLE_OVERRIDE: Mutex<Option<Vec<PathBuf>>> = Mutex::new(None);

#[cfg(test)]
static PATH_LOOKUP_DISABLED: Mutex<bool> = Mutex::new(false);

#[cfg(test)]
static CODEX_FIXTURE: Mutex<()> = Mutex::new(());

/// Directories added to PATH after this process started. `which` only sees the
/// environment inherited at launch, so a CLI installed later is invisible
/// until the app restarts. These directories are read again on every lookup.
fn resolve_from_login_path(command: &str) -> Option<PathBuf> {
    which_in_dirs(command, &supplemental_path_dirs())
}

/// PATH for child processes launched from a GUI app: login-shell / registry dirs
/// plus the process PATH, so tools like `npx` resolve the same as in a terminal.
pub fn augmented_path_env() -> std::ffi::OsString {
    let mut dirs = supplemental_path_dirs();
    if let Some(path) = std::env::var_os("PATH") {
        for part in std::env::split_paths(&path) {
            if part.as_os_str().is_empty() {
                continue;
            }
            if !dirs.iter().any(|dir| dir == &part) {
                dirs.push(part);
            }
        }
    }
    std::env::join_paths(&dirs).unwrap_or_else(|_| std::env::var_os("PATH").unwrap_or_default())
}

fn which_in_dirs(command: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    if dirs.is_empty() {
        return None;
    }
    let path = std::env::join_paths(dirs).ok()?;
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    which::which_in(command, Some(path), cwd).ok()
}

fn supplemental_path_dirs() -> Vec<PathBuf> {
    if let Some(dirs) = supplemental_override() {
        return dirs;
    }
    native_path_dirs()
}

#[cfg(windows)]
fn native_path_dirs() -> Vec<PathBuf> {
    windows_registry_path_dirs()
}

#[cfg(unix)]
fn native_path_dirs() -> Vec<PathBuf> {
    let mut dirs = login_path_dirs();
    // GUI launches on macOS inherit a short PATH. The login shell usually
    // recovers the rest, but a slow or broken rc must not hide the directories
    // installers actually use. Append only, so an nvm/fnm Node stays ahead of
    // Homebrew.
    append_missing_dirs(&mut dirs, &well_known_bin_dirs());
    dirs
}

#[cfg(unix)]
fn well_known_bin_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ];
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return dirs;
    };
    dirs.push(home.join(".local/bin"));
    dirs.push(home.join(".volta/bin"));
    dirs.push(home.join(".bun/bin"));
    dirs.push(home.join(".cargo/bin"));
    dirs.push(home.join(".asdf/shims"));
    dirs.push(home.join(".local/share/mise/shims"));
    dirs.push(home.join(".mise/shims"));
    dirs.push(home.join(".fnm/aliases/default/bin"));
    dirs.push(home.join(".local/share/fnm/aliases/default/bin"));
    // nvm：取版本目录名最大的一个（通常是最新），避免扫全部。
    if let Ok(entries) = std::fs::read_dir(home.join(".nvm/versions/node")) {
        let mut versions: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();
        versions.sort();
        if let Some(latest) = versions.pop() {
            dirs.push(latest.join("bin"));
        }
    }
    dirs
}

#[cfg_attr(not(unix), allow(dead_code))]
fn append_missing_dirs(dirs: &mut Vec<PathBuf>, extras: &[PathBuf]) {
    for extra in extras {
        if !dirs.iter().any(|dir| dir == extra) {
            dirs.push(extra.clone());
        }
    }
}

#[cfg(not(any(windows, unix)))]
fn native_path_dirs() -> Vec<PathBuf> {
    Vec::new()
}

fn supplemental_override() -> Option<Vec<PathBuf>> {
    #[cfg(test)]
    {
        return SUPPLEMENTAL_PATH_OVERRIDE.lock().ok().and_then(|guard| guard.clone());
    }
    #[cfg(not(test))]
    None
}

#[cfg(test)]
static SUPPLEMENTAL_PATH_OVERRIDE: Mutex<Option<Vec<PathBuf>>> = Mutex::new(None);

/// User and machine PATH from the registry, not this process's environment.
/// Installers update the registry immediately; a running app keeps the old PATH.
#[cfg(windows)]
fn windows_registry_path_dirs() -> Vec<PathBuf> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;

    fn read(hive: winreg::HKEY, subkey: &str) -> Vec<PathBuf> {
        let key = RegKey::predef(hive).open_subkey(subkey).ok();
        let Some(key) = key else {
            return Vec::new();
        };
        let raw = key.get_value::<String, _>("Path").unwrap_or_default();
        expand_percent_vars(&raw)
            .split(';')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(PathBuf::from)
            .collect()
    }

    let mut dirs = read(HKEY_CURRENT_USER, "Environment");
    dirs.extend(read(
        HKEY_LOCAL_MACHINE,
        r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
    ));
    dirs
}

#[cfg_attr(not(windows), allow(dead_code))]
fn expand_percent_vars(raw: &str) -> String {
    let mut expanded = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(start) = rest.find('%') {
        expanded.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) if end > 0 => {
                let name = &after[..end];
                match std::env::var(name) {
                    Ok(value) => expanded.push_str(&value),
                    Err(_) => {
                        expanded.push('%');
                        expanded.push_str(name);
                        expanded.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            _ => {
                expanded.push('%');
                rest = after;
            }
        }
    }
    expanded.push_str(rest);
    expanded
}

#[cfg(unix)]
fn login_path_cache() -> &'static Mutex<Option<Vec<PathBuf>>> {
    static CACHE: OnceLock<Mutex<Option<Vec<PathBuf>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

#[cfg(unix)]
fn login_path_dirs() -> Vec<PathBuf> {
    if let Ok(guard) = login_path_cache().lock() {
        if let Some(dirs) = guard.clone() {
            return dirs;
        }
    }
    let dirs = load_login_path_dirs();
    if let Ok(mut guard) = login_path_cache().lock() {
        if guard.is_none() {
            *guard = Some(dirs.clone());
        } else if let Some(existing) = guard.clone() {
            return existing;
        }
    }
    dirs
}

#[cfg(unix)]
fn refresh_login_path_dirs() {
    let dirs = load_login_path_dirs();
    if let Ok(mut guard) = login_path_cache().lock() {
        *guard = Some(dirs);
    }
}

/// 与 VS Code `shellEnv.ts` 一致：用标记包住 PATH，避免 oh-my-zsh 等往 stdout 打横幅污染首行。
#[cfg(unix)]
const PATH_MARK: &str = "__MULTARA_PATH__";

#[cfg(unix)]
fn dump_path_command() -> String {
    format!("printf '{PATH_MARK}%s{PATH_MARK}\\n' \"$PATH\"")
}

#[cfg(unix)]
fn load_login_path_dirs() -> Vec<PathBuf> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let cmd = dump_path_command();
    // 必须优先 interactive+login（-ilc）。nvm/fnm 多半写在 .zshrc，纯 -lc 常「成功」
    // 但 PATH 很短；旧逻辑用 or_else，短 PATH 成功后永远不会再试 -lic。
    dump_login_path(&shell, &["-ilc", &cmd])
        .or_else(|| dump_login_path(&shell, &["-i", "-l", "-c", &cmd]))
        .or_else(|| dump_login_path(&shell, &["-lc", &cmd]))
        .unwrap_or_default()
        .split(':')
        .filter(|part| !part.is_empty())
        .map(PathBuf::from)
        .collect()
}

#[cfg(unix)]
fn extract_marked_path(text: &str) -> Option<String> {
    let start = text.find(PATH_MARK)? + PATH_MARK.len();
    let rest = &text[start..];
    let end = rest.find(PATH_MARK)?;
    let path = rest[..end].trim();
    (!path.is_empty()).then(|| path.to_string())
}

#[cfg(unix)]
fn dump_login_path(shell: &str, args: &[&str]) -> Option<String> {
    let mut child = Command::new(shell)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    // VS Code 默认 10s；nvm 初始化经常超过原先的 2s。
    if !wait_with_timeout(&mut child, Duration::from_secs(10)) {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    }
    let mut stdout = child.stdout.take()?;
    let mut text = String::new();
    stdout.read_to_string(&mut text).ok()?;
    extract_marked_path(&text)
}

#[cfg(unix)]
fn wait_with_timeout(child: &mut std::process::Child, timeout: Duration) -> bool {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) if started.elapsed() >= timeout => return false,
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, fs};
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    use super::{
        build_args, compose_args, detect, execution, extract_version_line,
        gateway_env, resolve_runtime_command,
        CODEX_BUNDLE_OVERRIDE, CODEX_FIXTURE, PATH_LOOKUP_DISABLED, RUNTIME_SPECS,
        SUPPLEMENTAL_PATH_OVERRIDE,
    };

    #[test]
    fn concurrency_never_exceeds_six() {
        assert!(detect().iter().all(|runtime| runtime.max_concurrent <= 6));
    }

    #[test]
    fn runtime_ids_are_unique() {
        let ids = RUNTIME_SPECS
            .iter()
            .map(|runtime| runtime.id)
            .collect::<HashSet<_>>();
        assert_eq!(ids.len(), RUNTIME_SPECS.len());
    }

    #[test]
    fn version_line_keeps_the_first_semver_line() {
        assert_eq!(
            extract_version_line("Active code page: 65001\n2.1.5 (Claude Code)\n"),
            Some("2.1.5 (Claude Code)".into())
        );
        assert_eq!(
            extract_version_line("codex-cli 0.118.0\n"),
            Some("codex-cli 0.118.0".into())
        );
        assert_eq!(extract_version_line("initializing plugins\n"), None);
    }

    #[test]
    fn cursor_and_qwen_read_prompts_from_stdin() {
        let cursor = build_args("cursor", "never put this prompt in argv");
        assert_eq!(cursor, ["-p", "--output-format", "stream-json", "--yolo"]);
        let qwen = build_args("qwen", "never put this prompt in argv");
        assert_eq!(qwen, ["--output-format", "stream-json", "--yolo"]);
    }

    #[test]
    fn codex_skips_git_repo_check_for_non_repo_chats() {
        let args = build_args("codex", "hello");
        assert_eq!(
            args,
            ["exec", "--json", "--skip-git-repo-check", "hello"]
        );
    }

    #[test]
    fn fixed_args_stay_in_front_of_builtin_protocol_args() {
        let args = compose_args(
            "cursor",
            &["--model".into(), "composer-2.5".into()],
            "never put this prompt in argv",
            false,
            "",
            "",
            "",
        );
        assert_eq!(&args[..2], ["--model", "composer-2.5"]);
        assert!(args.windows(2).any(|pair| pair == ["--output-format", "stream-json"]));
        assert!(!args.iter().any(|arg| arg == "never put this prompt in argv"));
    }

    #[test]
    fn codex_gateway_config_is_inserted_before_the_prompt() {
        let args = compose_args(
            "codex",
            &["--profile".into(), "team".into()],
            "do the task",
            true,
            "https://gw.example/v1/",
            "gpt-4.1",
            "chat",
        );
        assert_eq!(&args[..2], ["--profile", "team"]);
        let prompt_at = args.iter().position(|arg| arg == "do the task").unwrap();
        let wire_at = args
            .iter()
            .position(|arg| arg.contains("wire_api=chat"))
            .unwrap();
        assert!(wire_at < prompt_at);
        assert!(args.iter().any(|arg| arg.contains("https://gw.example/v1")));
        assert!(args.windows(2).any(|pair| pair == ["-m", "gpt-4.1"]));
    }

    #[test]
    fn claude_gateway_uses_auth_token_and_codex_does_not_set_openai_model() {
        let claude = gateway_env(
            "claude",
            "https://aigw.example/v1/",
            "sk-test",
            "sonnet",
            "bearer",
        );
        assert_eq!(claude.get("ANTHROPIC_BASE_URL").unwrap(), "https://aigw.example/v1");
        assert_eq!(claude.get("ANTHROPIC_AUTH_TOKEN").unwrap(), "sk-test");
        assert!(claude.get("ANTHROPIC_API_KEY").is_none());

        let x_api = gateway_env("claude", "https://aigw.example", "sk-test", "sonnet", "x-api-key");
        assert_eq!(x_api.get("ANTHROPIC_API_KEY").unwrap(), "sk-test");

        let codex = gateway_env("codex", "https://gw.example/v1", "sk-test", "gpt-4.1", "bearer");
        assert_eq!(codex.get("MULTARA_GATEWAY_KEY").unwrap(), "sk-test");
        assert!(codex.get("OPENAI_MODEL").is_none());
        assert!(codex.get("OPENAI_BASE_URL").is_none());
    }

    #[test]
    fn well_known_dirs_stay_behind_the_login_path() {
        let nvm = std::path::PathBuf::from("/Users/me/.nvm/versions/node/v22/bin");
        let mut dirs = vec![nvm.clone()];
        super::append_missing_dirs(
            &mut dirs,
            &[
                std::path::PathBuf::from("/opt/homebrew/bin"),
                nvm.clone(),
                std::path::PathBuf::from("/usr/local/bin"),
            ],
        );
        assert_eq!(
            dirs,
            vec![
                nvm,
                std::path::PathBuf::from("/opt/homebrew/bin"),
                std::path::PathBuf::from("/usr/local/bin"),
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn marked_path_ignores_shell_banner_lines() {
        let raw = "Welcome\n__MULTARA_PATH__/opt/homebrew/bin:/Users/me/.nvm/versions/node/v22/bin__MULTARA_PATH__\n";
        assert_eq!(
            super::extract_marked_path(raw).as_deref(),
            Some("/opt/homebrew/bin:/Users/me/.nvm/versions/node/v22/bin")
        );
    }

    #[test]
    fn percent_vars_in_path_are_expanded() {
        std::env::set_var("MULTARA_PATH_PROBE", "C:\\tools");
        assert_eq!(
            super::expand_percent_vars("%MULTARA_PATH_PROBE%\\bin;%MISSING%"),
            "C:\\tools\\bin;%MISSING%"
        );
        std::env::remove_var("MULTARA_PATH_PROBE");
    }

    #[test]
    fn augmented_path_env_includes_supplemental_dirs() {
        let dir = std::env::temp_dir().join(format!("multara-aug-path-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        *SUPPLEMENTAL_PATH_OVERRIDE.lock().unwrap() = Some(vec![dir.clone()]);
        let path = super::augmented_path_env();
        *SUPPLEMENTAL_PATH_OVERRIDE.lock().unwrap() = None;
        let dirs: Vec<_> = std::env::split_paths(&path).collect();
        assert!(dirs.iter().any(|entry| entry == &dir));
    }

    #[test]
    fn cli_installed_after_startup_is_found_via_supplemental_path() {
        let _guard = CODEX_FIXTURE.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("multara-supplemental-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let command = "multara-path-probe";
        #[cfg(windows)]
        let binary = dir.join("multara-path-probe.exe");
        #[cfg(not(windows))]
        let binary = dir.join(command);
        fs::write(&binary, b"#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        executable(&binary);
        *SUPPLEMENTAL_PATH_OVERRIDE.lock().unwrap() = Some(vec![dir.clone()]);
        *PATH_LOOKUP_DISABLED.lock().unwrap() = false;
        if let Ok(mut cache) = super::command_path_cache().lock() {
            cache.remove(command);
        }
        let found = resolve_runtime_command(command, "custom");
        *SUPPLEMENTAL_PATH_OVERRIDE.lock().unwrap() = None;
        if let Ok(mut cache) = super::command_path_cache().lock() {
            cache.remove(command);
        }
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(found, Some(binary));
    }

    #[test]
    fn unknown_runtime_does_not_scan_every_cli() {
        let error = execution("missing-runtime", "prompt").unwrap_err();
        assert!(error.contains("未知运行时"));
    }

    #[cfg(unix)]
    fn executable(path: &std::path::Path) {
        fs::write(path, b"#!/bin/sh\nexit 0\n").unwrap();
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn codex_bundle_is_used_when_path_lookup_misses() {
        let _guard = CODEX_FIXTURE.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("multara-codex-bundle-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let binary = dir.join("codex");
        executable(&binary);
        *CODEX_BUNDLE_OVERRIDE.lock().unwrap() = Some(vec![binary.clone()]);
        *PATH_LOOKUP_DISABLED.lock().unwrap() = true;
        std::env::remove_var("MULTARA_CODEX_PATH");
        if let Ok(mut cache) = super::command_path_cache().lock() {
            cache.remove("codex");
        }
        let found = resolve_runtime_command("codex", "codex");
        *CODEX_BUNDLE_OVERRIDE.lock().unwrap() = None;
        *PATH_LOOKUP_DISABLED.lock().unwrap() = false;
        if let Ok(mut cache) = super::command_path_cache().lock() {
            cache.remove("codex");
        }
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(found, Some(binary));
    }

    #[cfg(unix)]
    #[test]
    fn codex_override_does_not_fall_back_to_the_bundle() {
        let _guard = CODEX_FIXTURE.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("multara-codex-override-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let binary = dir.join("codex");
        executable(&binary);
        let missing = dir.join("missing-codex");
        *CODEX_BUNDLE_OVERRIDE.lock().unwrap() = Some(vec![binary]);
        *PATH_LOOKUP_DISABLED.lock().unwrap() = true;
        std::env::set_var("MULTARA_CODEX_PATH", &missing);
        if let Ok(mut cache) = super::command_path_cache().lock() {
            cache.remove("codex");
        }
        let found = resolve_runtime_command("codex", "codex");
        std::env::remove_var("MULTARA_CODEX_PATH");
        *CODEX_BUNDLE_OVERRIDE.lock().unwrap() = None;
        *PATH_LOOKUP_DISABLED.lock().unwrap() = false;
        if let Ok(mut cache) = super::command_path_cache().lock() {
            cache.remove("codex");
        }
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(found, None);
    }

    #[cfg(unix)]
    #[test]
    fn non_executable_bundle_is_ignored() {
        let _guard = CODEX_FIXTURE.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("multara-codex-plain-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let binary = dir.join("codex");
        fs::write(&binary, b"not executable").unwrap();
        let mut permissions = fs::metadata(&binary).unwrap().permissions();
        permissions.set_mode(0o644);
        fs::set_permissions(&binary, permissions).unwrap();
        *CODEX_BUNDLE_OVERRIDE.lock().unwrap() = Some(vec![binary]);
        *PATH_LOOKUP_DISABLED.lock().unwrap() = true;
        std::env::remove_var("MULTARA_CODEX_PATH");
        if let Ok(mut cache) = super::command_path_cache().lock() {
            cache.remove("codex");
        }
        let found = resolve_runtime_command("codex", "codex");
        *CODEX_BUNDLE_OVERRIDE.lock().unwrap() = None;
        *PATH_LOOKUP_DISABLED.lock().unwrap() = false;
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(found, None);
    }
}
