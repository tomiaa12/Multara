use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
    thread,
    time::Duration,
};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

struct Session {
    writer: Box<dyn Write + Send>,
    master: Box<dyn portable_pty::MasterPty + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

static SESSIONS: LazyLock<Mutex<HashMap<String, Session>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Clone, Serialize)]
struct TerminalChunk {
    id: String,
    data: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TerminalExit {
    id: String,
    exit_code: u32,
}

/// 可选的 shell。`kind` 决定注入哪种 shell 集成脚本。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalProfile {
    pub id: String,
    pub name: String,
    pub path: String,
    pub args: Vec<String>,
    pub kind: String,
    pub is_default: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedTerminal {
    pub id: String,
    pub pid: u32,
    pub profile_id: String,
    pub shell: String,
    pub kind: String,
    pub integration: bool,
}

const EXIT_POLL_MS: u64 = 250;
/// 进程退出后再等一会儿，让读线程把最后的输出发完。
const EXIT_FLUSH_MS: u64 = 150;

pub fn open(app: AppHandle, app_data: Option<&Path>, cwd: &str, profile_id: Option<&str>) -> Result<OpenedTerminal, String> {
    let directory = Path::new(cwd);
    if !directory.is_dir() {
        return Err("终端目录不存在".to_string());
    }
    let all = profiles();
    let profile = profile_id
        .and_then(|wanted| all.iter().find(|item| item.id == wanted))
        .or_else(|| all.iter().find(|item| item.is_default))
        .or_else(|| all.first())
        .cloned()
        .ok_or_else(|| "没有可用的 shell".to_string())?;

    let system = native_pty_system();
    let pair = system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|error| error.to_string())?;
    let mut command = CommandBuilder::new(&profile.path);
    for arg in &profile.args {
        command.arg(arg);
    }
    let integration = app_data
        .map(|dir| apply_shell_integration(&mut command, &profile, dir))
        .unwrap_or(false);
    command.cwd(directory);
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    command.env("TERM_PROGRAM", "multara");
    if cfg!(target_os = "macos") && std::env::var_os("LANG").is_none() {
        command.env("LANG", "en_US.UTF-8");
    }
    let child = pair
        .slave
        .spawn_command(command)
        .map_err(|error| error.to_string())?;
    let pid = child.process_id().unwrap_or(0);
    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|error| error.to_string())?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|error| error.to_string())?;
    let id = Uuid::new_v4().to_string();
    let emit_id = id.clone();
    let data_app = app.clone();
    thread::spawn(move || {
        let mut buffer = [0_u8; 8192];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(size) => {
                    let _ = data_app.emit(
                        "terminal-data",
                        TerminalChunk {
                            id: emit_id.clone(),
                            data: STANDARD.encode(&buffer[..size]),
                        },
                    );
                }
            }
        }
    });
    sessions().insert(
        id.clone(),
        Session {
            writer,
            master: pair.master,
            child,
        },
    );
    watch_exit(app, id.clone());
    Ok(OpenedTerminal {
        id,
        pid,
        profile_id: profile.id,
        shell: profile.name,
        kind: profile.kind,
        integration,
    })
}

/// Windows 的 ConPTY 在子进程退出后读端不一定收到 EOF，所以单独轮询退出码。
/// 用户主动关闭的终端会先从表里移除，这里就不再发退出事件。
fn watch_exit(app: AppHandle, id: String) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(EXIT_POLL_MS));
        let status = {
            let mut sessions = sessions();
            let Some(session) = sessions.get_mut(&id) else {
                return;
            };
            match session.child.try_wait() {
                Ok(Some(status)) => Some(status.exit_code()),
                Ok(None) => None,
                Err(_) => Some(1),
            }
        };
        let Some(exit_code) = status else {
            continue;
        };
        thread::sleep(Duration::from_millis(EXIT_FLUSH_MS));
        let removed = sessions().remove(&id);
        if removed.is_some() {
            let _ = app.emit("terminal-exit", TerminalExit { id, exit_code });
        }
        return;
    });
}

pub fn write(id: &str, data: &[u8]) -> Result<(), String> {
    let mut sessions = sessions();
    let session = sessions.get_mut(id).ok_or_else(|| "终端已关闭".to_string())?;
    session
        .writer
        .write_all(data)
        .map_err(|error| error.to_string())?;
    session.writer.flush().map_err(|error| error.to_string())?;
    Ok(())
}

pub fn resize(id: &str, cols: u16, rows: u16) -> Result<(), String> {
    let sessions = sessions();
    let session = sessions.get(id).ok_or_else(|| "终端已关闭".to_string())?;
    session
        .master
        .resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|error| error.to_string())
}

pub fn close(id: &str) -> Result<(), String> {
    let removed = sessions().remove(id);
    drop(removed);
    Ok(())
}

// ---------------------------------------------------------------------------
// shell 配置

pub fn profiles() -> Vec<TerminalProfile> {
    let mut list = if cfg!(windows) { windows_profiles() } else { unix_profiles() };
    if let Some(first) = list.first_mut() {
        first.is_default = true;
    }
    list
}

fn profile(id: &str, name: &str, path: PathBuf, args: &[&str], kind: &str) -> TerminalProfile {
    TerminalProfile {
        id: id.to_string(),
        name: name.to_string(),
        path: path.to_string_lossy().to_string(),
        args: args.iter().map(|item| item.to_string()).collect(),
        kind: kind.to_string(),
        is_default: false,
    }
}

/// 顺序即默认优先级：和 VS Code 一样优先 PowerShell。
fn windows_profiles() -> Vec<TerminalProfile> {
    let system_root = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    let system32 = system_root.join("System32");
    let mut list = Vec::new();

    let pwsh = which::which("pwsh").ok().or_else(|| {
        ["ProgramFiles", "ProgramW6432"]
            .iter()
            .filter_map(|key| std::env::var_os(key))
            .map(|base| PathBuf::from(base).join("PowerShell").join("7").join("pwsh.exe"))
            .find(|path| path.is_file())
    });
    if let Some(path) = pwsh {
        list.push(profile("pwsh", "PowerShell", path, &[], "pwsh"));
    }
    let powershell = system32.join("WindowsPowerShell").join("v1.0").join("powershell.exe");
    if powershell.is_file() {
        list.push(profile("powershell", "Windows PowerShell", powershell, &[], "pwsh"));
    }
    let cmd = std::env::var_os("COMSPEC")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .unwrap_or_else(|| system32.join("cmd.exe"));
    list.push(profile("cmd", "命令提示符", cmd, &[], "cmd"));
    if let Some(path) = git_bash() {
        list.push(profile("git-bash", "Git Bash", path, &[], "git-bash"));
    }
    let wsl = system32.join("wsl.exe");
    if wsl.is_file() {
        list.push(profile("wsl", "WSL", wsl, &[], "other"));
    }
    list
}

fn git_bash() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"]
        .iter()
        .filter_map(|key| std::env::var_os(key))
        .map(|base| PathBuf::from(base).join("Git").join("bin").join("bash.exe"))
        .collect();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        candidates.push(PathBuf::from(local).join("Programs").join("Git").join("bin").join("bash.exe"));
    }
    if let Ok(git) = which::which("git") {
        // <Git>\cmd\git.exe 或 <Git>\bin\git.exe
        if let Some(root) = git.parent().and_then(Path::parent) {
            candidates.push(root.join("bin").join("bash.exe"));
        }
    }
    candidates.into_iter().find(|path| path.is_file())
}

fn unix_profiles() -> Vec<TerminalProfile> {
    let mut list = Vec::new();
    let mut seen = HashSet::new();
    let login: &[&str] = if cfg!(target_os = "macos") { &["-l"] } else { &[] };
    let mut push = |path: PathBuf, list: &mut Vec<TerminalProfile>| {
        let Ok(canonical) = fs::canonicalize(&path) else {
            return;
        };
        if !seen.insert(canonical) {
            return;
        }
        let name = path.file_name().map(|item| item.to_string_lossy().to_string()).unwrap_or_default();
        let kind = match name.as_str() {
            "zsh" | "bash" | "fish" => name.as_str(),
            _ => "other",
        };
        let args = if kind == "zsh" || kind == "bash" { login } else { &[] };
        let mut id = name.clone();
        let mut suffix = 2;
        while list.iter().any(|item| item.id == id) {
            id = format!("{name}-{suffix}");
            suffix += 1;
        }
        list.push(profile(&id, &name, path, args, kind));
    };
    if let Some(shell) = std::env::var_os("SHELL").map(PathBuf::from) {
        if shell.is_file() {
            push(shell, &mut list);
        }
    }
    for name in ["zsh", "bash", "fish", "sh"] {
        if let Ok(path) = which::which(name) {
            push(path, &mut list);
        }
    }
    if list.is_empty() {
        push(PathBuf::from("/bin/sh"), &mut list);
    }
    list
}

// ---------------------------------------------------------------------------
// shell 集成：用 OSC 633 报告提示符、命令开始/结束、退出码和当前目录（与 VS Code 同一套序列）。

const INTEGRATION_DIR: &str = "shell-integration";

const POWERSHELL_INTEGRATION: &str = r#"
if (-not $global:__multaraInstalled) {
  $global:__multaraInstalled = $true
  $global:__multaraStarted = $false
  $global:__multaraLastHistoryId = -1
  if ($function:prompt) { $global:__multaraPrompt = $function:prompt } else { $global:__multaraPrompt = { "PS $($executionContext.SessionState.Path.CurrentLocation)$('>' * ($nestedPromptLevel + 1)) " } }
  function global:prompt {
    $succeeded = $global:?
    $exitCode = $global:LASTEXITCODE
    $esc = [char]0x1b
    $bel = [char]0x07
    $out = ''
    if ($global:__multaraStarted) {
      $last = Get-History -Count 1
      if ($null -ne $last -and $last.Id -ne $global:__multaraLastHistoryId) {
        $global:__multaraLastHistoryId = $last.Id
        if ($succeeded) { $code = 0 } elseif ($exitCode) { $code = $exitCode } else { $code = 1 }
        $out += "$esc]633;D;$code$bel"
      } else {
        $out += "$esc]633;D$bel"
      }
    } else {
      $last = Get-History -Count 1
      if ($null -ne $last) { $global:__multaraLastHistoryId = $last.Id }
    }
    $global:__multaraStarted = $true
    $location = $executionContext.SessionState.Path.CurrentLocation
    if ($location.Provider.Name -eq 'FileSystem') { $out += "$esc]633;P;Cwd=$($location.ProviderPath)$bel" }
    $out += "$esc]633;A$bel"
    $out += ((& $global:__multaraPrompt) -join '')
    $out += "$esc]633;B$bel"
    $global:LASTEXITCODE = $exitCode
    $out
  }
  if (Get-Command PSConsoleHostReadLine -ErrorAction SilentlyContinue) {
    $global:__multaraReadLine = $function:PSConsoleHostReadLine
    function global:PSConsoleHostReadLine {
      $line = & $global:__multaraReadLine
      [Console]::Write("$([char]0x1b)]633;C$([char]0x07)")
      $line
    }
  }
}
"#;

const BASH_INTEGRATION: &str = r#"
if [ -n "$MULTARA_SHELL_LOGIN" ]; then
  unset MULTARA_SHELL_LOGIN
  [ -r /etc/profile ] && . /etc/profile
  if [ -r ~/.bash_profile ]; then . ~/.bash_profile
  elif [ -r ~/.bash_login ]; then . ~/.bash_login
  elif [ -r ~/.profile ]; then . ~/.profile
  fi
else
  [ -r ~/.bashrc ] && . ~/.bashrc
fi
if [ -z "$__multara_installed" ]; then
  __multara_installed=1
  __multara_started=
  __multara_precmd() {
    local code=$?
    if [ -n "$__multara_started" ]; then printf '\033]633;D;%s\007' "$code"; fi
    __multara_started=1
    printf '\033]633;P;Cwd=%s\007' "$PWD"
    return $code
  }
  __multara_wrap_prompt() {
    local code=$?
    case "$PS1" in
      *'633;A'*) ;;
      *) PS1='\[\033]633;A\007\]'"$PS1"'\[\033]633;B\007\]' ;;
    esac
    return $code
  }
  PS0=$'\033]633;C\007'"$PS0"
  if [[ "$(declare -p PROMPT_COMMAND 2>/dev/null)" == "declare -a"* ]]; then
    PROMPT_COMMAND=(__multara_precmd "${PROMPT_COMMAND[@]}" __multara_wrap_prompt)
  else
    PROMPT_COMMAND=$'__multara_precmd\n'"$PROMPT_COMMAND"$'\n__multara_wrap_prompt'
  fi
fi
"#;

const ZSH_ENV: &str = r#"
__multara_dir=$ZDOTDIR
ZDOTDIR=${USER_ZDOTDIR:-$HOME}
[[ -r "$ZDOTDIR/.zshenv" ]] && . "$ZDOTDIR/.zshenv"
USER_ZDOTDIR=$ZDOTDIR
ZDOTDIR=$__multara_dir
"#;

const ZSH_PROFILE: &str = r#"
ZDOTDIR=$USER_ZDOTDIR
[[ -r "$ZDOTDIR/.zprofile" ]] && . "$ZDOTDIR/.zprofile"
USER_ZDOTDIR=$ZDOTDIR
ZDOTDIR=$__multara_dir
"#;

const ZSH_RC: &str = r#"
ZDOTDIR=$USER_ZDOTDIR
[[ -r "$ZDOTDIR/.zshrc" ]] && . "$ZDOTDIR/.zshrc"
unset __multara_dir
if [[ -z $__multara_installed ]]; then
  __multara_installed=1
  __multara_started=
  __multara_precmd() {
    local code=$?
    [[ -n $__multara_started ]] && printf '\033]633;D;%s\007' $code
    __multara_started=1
    printf '\033]633;P;Cwd=%s\007' "$PWD"
    return $code
  }
  __multara_wrap_prompt() {
    [[ $PS1 == *'633;A'* ]] || PS1=$'%{\033]633;A\007%}'"$PS1"$'%{\033]633;B\007%}'
  }
  __multara_preexec() { printf '\033]633;C\007' }
  precmd_functions=(__multara_precmd $precmd_functions __multara_wrap_prompt)
  preexec_functions+=(__multara_preexec)
fi
"#;

const FISH_INTEGRATION: &str = r#"
if not set -q __multara_installed
  set -g __multara_installed 1
  function __multara_preexec --on-event fish_preexec
    printf '\e]633;C\a'
  end
  function __multara_postexec --on-event fish_postexec
    printf '\e]633;D;%s\a' $status
  end
  functions -c fish_prompt __multara_original_prompt
  function fish_prompt
    printf '\e]633;P;Cwd=%s\a\e]633;A\a' $PWD
    __multara_original_prompt
    printf '\e]633;B\a'
  end
end
"#;

/// cmd 没有执行前钩子，只能通过 PROMPT 报告目录和提示符边界，没有退出码。
const CMD_PROMPT: &str = "$E]633;D$E\\$E]633;P;Cwd=$P$E\\$E]633;A$E\\$P$G$E]633;B$E\\";

fn apply_shell_integration(command: &mut CommandBuilder, profile: &TerminalProfile, app_data: &Path) -> bool {
    let dir = app_data.join(INTEGRATION_DIR);
    match profile.kind.as_str() {
        "pwsh" => {
            command.args(["-NoLogo", "-NoExit", "-EncodedCommand", &encode_powershell(POWERSHELL_INTEGRATION)]);
            true
        }
        "bash" | "git-bash" => {
            let Ok(script) = write_script(&dir, "bash-integration.sh", BASH_INTEGRATION) else {
                return false;
            };
            // `--login` 会让 bash 跳过 --init-file，登录配置改由脚本自己读取。
            let args: Vec<String> = profile.args.iter().filter(|arg| *arg != "-l" && *arg != "--login").cloned().collect();
            let mut rebuilt = CommandBuilder::new(&profile.path);
            if profile.kind == "git-bash" || args.len() != profile.args.len() {
                rebuilt.env("MULTARA_SHELL_LOGIN", "1");
            }
            rebuilt.args(args);
            rebuilt.arg("--init-file");
            rebuilt.arg(script.to_string_lossy().replace('\\', "/"));
            rebuilt.arg("-i");
            *command = rebuilt;
            true
        }
        "zsh" => {
            let zsh_dir = dir.join("zsh");
            let written = write_script(&zsh_dir, ".zshenv", ZSH_ENV)
                .and(write_script(&zsh_dir, ".zprofile", ZSH_PROFILE))
                .and(write_script(&zsh_dir, ".zshrc", ZSH_RC));
            if written.is_err() {
                return false;
            }
            let user = std::env::var_os("ZDOTDIR")
                .or_else(|| std::env::var_os("HOME"))
                .unwrap_or_default();
            command.env("USER_ZDOTDIR", user);
            command.env("ZDOTDIR", &zsh_dir);
            true
        }
        "fish" => {
            let Ok(script) = write_script(&dir, "fish-integration.fish", FISH_INTEGRATION) else {
                return false;
            };
            command.arg("--init-command");
            command.arg(format!("source '{}'", script.to_string_lossy()));
            true
        }
        "cmd" => {
            command.env("PROMPT", CMD_PROMPT);
            true
        }
        _ => false,
    }
}

fn write_script(dir: &Path, name: &str, content: &str) -> Result<PathBuf, String> {
    fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let path = dir.join(name);
    if fs::read_to_string(&path).ok().as_deref() != Some(content) {
        fs::write(&path, content).map_err(|error| error.to_string())?;
    }
    Ok(path)
}

fn encode_powershell(script: &str) -> String {
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    STANDARD.encode(bytes)
}

// ---------------------------------------------------------------------------
// 文件链接与剪贴板

const RESOLVE_LIMIT: usize = 64;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedPath {
    pub path: String,
    pub is_directory: bool,
}

/// 把终端输出里出现的路径按 cwd 解析成绝对路径，不存在的返回 None。
pub fn resolve_paths(cwd: &str, candidates: &[String]) -> Vec<Option<ResolvedPath>> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    candidates
        .iter()
        .take(RESOLVE_LIMIT)
        .map(|raw| {
            let path = if let Some(rest) = raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\")) {
                home.as_ref()?.join(rest)
            } else {
                let candidate = PathBuf::from(raw);
                if candidate.is_absolute() {
                    candidate
                } else {
                    Path::new(cwd).join(candidate)
                }
            };
            let metadata = fs::metadata(&path).ok()?;
            let path = fs::canonicalize(&path).map(strip_verbatim).unwrap_or(path);
            Some(ResolvedPath {
                path: path.to_string_lossy().to_string(),
                is_directory: metadata.is_dir(),
            })
        })
        .collect()
}

/// Windows 的 canonicalize 会得到 `\\?\C:\...`，前端只认普通路径。
fn strip_verbatim(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => path,
    }
}

#[cfg(windows)]
pub fn clipboard_text() -> Result<String, String> {
    clipboard_win::get_clipboard_string().map_err(|error| error.to_string())
}

#[cfg(not(windows))]
pub fn clipboard_text() -> Result<String, String> {
    let attempts: &[(&str, &[&str])] = if cfg!(target_os = "macos") {
        &[("pbpaste", &[])]
    } else {
        &[
            ("wl-paste", &["--no-newline"]),
            ("xclip", &["-selection", "clipboard", "-o"]),
            ("xsel", &["-ob"]),
        ]
    };
    for (program, args) in attempts {
        if let Ok(output) = crate::process::command(program).args(*args).output() {
            if output.status.success() {
                return Ok(String::from_utf8_lossy(&output.stdout).to_string());
            }
        }
    }
    Err("无法读取剪贴板".to_string())
}

// ---------------------------------------------------------------------------
// 命令名补全

const COMMAND_NAME_LIMIT: usize = 8_000;

pub fn command_names() -> Vec<String> {
    static CACHE: LazyLock<Vec<String>> = LazyLock::new(|| {
        let Some(path) = std::env::var_os("PATH") else {
            return Vec::new();
        };
        let extensions = executable_extensions();
        let mut names = HashSet::new();
        let mut folded = HashSet::new();
        'outer: for directory in std::env::split_paths(&path) {
            let Ok(entries) = fs::read_dir(directory) else {
                continue;
            };
            for entry in entries.flatten() {
                let file = entry.file_name().to_string_lossy().to_string();
                let Some(name) = command_name(&file, extensions.as_deref()) else {
                    continue;
                };
                if extensions.is_some() && !folded.insert(name.to_lowercase()) {
                    continue;
                }
                names.insert(name);
                if names.len() >= COMMAND_NAME_LIMIT {
                    break 'outer;
                }
            }
        }
        let mut list: Vec<String> = names.into_iter().collect();
        list.sort();
        list
    });
    CACHE.clone()
}

/// Windows 下按 PATHEXT 过滤，返回 None 表示不过滤扩展名。
fn executable_extensions() -> Option<Vec<String>> {
    if !cfg!(windows) {
        return None;
    }
    let raw = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD;.PS1".to_string());
    Some(
        raw.split(';')
            .map(|item| item.trim().to_lowercase())
            .filter(|item| item.starts_with('.'))
            .collect(),
    )
}

fn command_name(file: &str, extensions: Option<&[String]>) -> Option<String> {
    if file.starts_with('.') || file.contains(' ') {
        return None;
    }
    let Some(extensions) = extensions else {
        return Some(file.to_string());
    };
    let lower = file.to_lowercase();
    let extension = extensions.iter().find(|ext| lower.ends_with(ext.as_str()))?;
    let stem = &file[..file.len() - extension.len()];
    (!stem.is_empty()).then(|| stem.to_string())
}

// ---------------------------------------------------------------------------
// 给智能体读的终端快照

const SNAPSHOT_DIR: &str = "terminals";

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TerminalSnapshotMeta {
    id: String,
    title: String,
    cwd: String,
    line_offset: usize,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct TerminalSnapshotIndex {
    active_id: String,
    sessions: Vec<TerminalSnapshotMeta>,
}

pub fn publish_snapshot(
    app_data: &Path,
    id: &str,
    title: &str,
    cwd: &str,
    line_offset: usize,
    text: &str,
    active: bool,
) -> Result<(), String> {
    let _guard = snapshot_io();
    let dir = app_data.join(SNAPSHOT_DIR);
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    fs::write(dir.join(format!("{id}.txt")), text).map_err(|error| error.to_string())?;
    let mut index = read_index(&dir);
    index.sessions.retain(|item| item.id != id);
    index.sessions.push(TerminalSnapshotMeta {
        id: id.to_string(),
        title: title.to_string(),
        cwd: cwd.to_string(),
        line_offset,
    });
    if active || index.active_id.is_empty() || index.active_id == id {
        index.active_id = id.to_string();
    }
    write_index(&dir, &index)
}

pub fn remove_snapshot(app_data: &Path, id: &str) -> Result<(), String> {
    let _guard = snapshot_io();
    let dir = app_data.join(SNAPSHOT_DIR);
    let _ = fs::remove_file(dir.join(format!("{id}.txt")));
    let mut index = read_index(&dir);
    index.sessions.retain(|item| item.id != id);
    if index.active_id == id {
        index.active_id = index.sessions.first().map(|item| item.id.clone()).unwrap_or_default();
    }
    if index.sessions.is_empty() {
        let _ = fs::remove_file(dir.join("index.json"));
        return Ok(());
    }
    write_index(&dir, &index)
}

pub fn list_snapshots(app_data: &Path) -> Result<serde_json::Value, String> {
    let index = read_index(&app_data.join(SNAPSHOT_DIR));
    let sessions: Vec<serde_json::Value> = index
        .sessions
        .iter()
        .map(|item| {
            serde_json::json!({
                "id": item.id,
                "title": item.title,
                "cwd": item.cwd,
                "active": item.id == index.active_id,
            })
        })
        .collect();
    Ok(serde_json::Value::Array(sessions))
}

pub fn read_snapshot(
    app_data: &Path,
    id: Option<&str>,
    start: Option<usize>,
    end: Option<usize>,
) -> Result<serde_json::Value, String> {
    let dir = app_data.join(SNAPSHOT_DIR);
    let index = read_index(&dir);
    let meta = match id {
        Some(id) => index.sessions.iter().find(|item| item.id == id),
        None => index.sessions.iter().find(|item| item.id == index.active_id).or_else(|| index.sessions.first()),
    }
    .ok_or_else(|| "没有打开的终端".to_string())?;
    let text = fs::read_to_string(dir.join(format!("{}.txt", meta.id))).unwrap_or_default();
    let (start_line, end_line, slice) = slice_terminal_text(&text, meta.line_offset, start, end)?;
    Ok(serde_json::json!({
        "id": meta.id,
        "title": meta.title,
        "cwd": meta.cwd,
        "startLine": start_line,
        "endLine": end_line,
        "text": slice,
    }))
}

pub fn slice_terminal_text(
    text: &str,
    line_offset: usize,
    start: Option<usize>,
    end: Option<usize>,
) -> Result<(usize, usize, String), String> {
    let lines: Vec<&str> = if text.is_empty() { Vec::new() } else { text.split('\n').collect() };
    let offset = line_offset.max(1);
    let last = offset + lines.len().saturating_sub(1);
    let start_line = start.unwrap_or(offset).max(offset);
    let end_line = end.unwrap_or(last).min(if lines.is_empty() { offset } else { last });
    if let Some(start) = start {
        if start > last {
            return Err(format!("起始行 {start} 超出终端内容（最后一行 {last}）"));
        }
    }
    if end_line < start_line {
        return Ok((start_line, start_line, String::new()));
    }
    let from = start_line - offset;
    let to = (end_line - offset).saturating_add(1).min(lines.len());
    Ok((start_line, end_line, lines.get(from..to).unwrap_or(&[]).join("\n")))
}

fn snapshot_io() -> std::sync::MutexGuard<'static, ()> {
    static IO: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
    IO.lock().unwrap_or_else(|error| error.into_inner())
}

fn read_index(dir: &Path) -> TerminalSnapshotIndex {
    let Ok(bytes) = fs::read(dir.join("index.json")) else {
        return TerminalSnapshotIndex::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

fn write_index(dir: &Path, index: &TerminalSnapshotIndex) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(index).map_err(|error| error.to_string())?;
    let path = dir.join("index.json");
    let temporary = dir.join("index.json.tmp");
    fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
    fs::rename(&temporary, &path).map_err(|error| error.to_string())
}

// ---------------------------------------------------------------------------
// 端口与子进程

#[derive(Clone, Serialize)]
pub struct TerminalService {
    pub id: String,
    pub ports: Vec<u16>,
}

/// 每个还活着的终端壳进程，以及它的子进程正在监听的 TCP 端口。
pub fn services() -> Vec<TerminalService> {
    let alive = alive_sessions();
    if alive.is_empty() {
        return Vec::new();
    }
    let children = process_children();
    let listening = listen_ports();
    alive
        .into_iter()
        .map(|(id, pid)| {
            let mut ports = Vec::new();
            for member in descendant_pids(pid, &children) {
                let Some(found) = listening.get(&member) else {
                    continue;
                };
                for port in found {
                    if !ports.contains(port) {
                        ports.push(*port);
                    }
                }
            }
            ports.sort_unstable();
            TerminalService { id, ports }
        })
        .collect()
}

/// 结束正在监听该端口的进程，返回被结束的进程数。
pub fn free_port(port: u16) -> Result<usize, String> {
    let owners: Vec<u32> = listen_ports()
        .into_iter()
        .filter(|(_, ports)| ports.contains(&port))
        .map(|(pid, _)| pid)
        .filter(|pid| *pid != 0 && *pid != std::process::id())
        .collect();
    if owners.is_empty() {
        return Err(format!("没有进程在监听端口 {port}"));
    }
    for pid in &owners {
        let status = if cfg!(windows) {
            crate::process::command("taskkill").args(["/PID", &pid.to_string(), "/T", "/F"]).status()
        } else {
            crate::process::command("kill").args(["-9", &pid.to_string()]).status()
        };
        status.map_err(|error| error.to_string())?;
    }
    Ok(owners.len())
}

fn alive_sessions() -> Vec<(String, u32)> {
    let mut sessions = sessions();
    let mut alive = Vec::new();
    for (id, session) in sessions.iter_mut() {
        if session.child.try_wait().ok().flatten().is_some() {
            continue;
        }
        let Some(pid) = session.child.process_id() else {
            continue;
        };
        alive.push((id.clone(), pid));
    }
    alive
}

#[cfg(windows)]
fn process_children() -> HashMap<u32, Vec<u32>> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
        System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS},
    };
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    // SAFETY: 标准的 Toolhelp 快照遍历，句柄在函数末尾关闭。
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return children;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snapshot, &mut entry) != 0 {
            loop {
                if entry.th32ProcessID != 0 {
                    children.entry(entry.th32ParentProcessID).or_default().push(entry.th32ProcessID);
                }
                if Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snapshot);
    }
    children
}

#[cfg(not(windows))]
fn process_children() -> HashMap<u32, Vec<u32>> {
    let output = crate::process::command(ps_program())
        .args(["-ax", "-o", "pid=,ppid="])
        .output();
    let Ok(output) = output else {
        return HashMap::new();
    };
    if !output.status.success() {
        return HashMap::new();
    }
    parse_process_tree(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(windows)]
fn listen_ports() -> HashMap<u32, Vec<u16>> {
    let output = crate::process::command("netstat").args(["-ano", "-p", "TCP"]).output();
    let mut ports = output
        .ok()
        .filter(|output| output.status.success())
        .map(|output| parse_netstat(&String::from_utf8_lossy(&output.stdout)))
        .unwrap_or_default();
    if let Ok(output) = crate::process::command("netstat").args(["-ano", "-p", "TCPv6"]).output() {
        if output.status.success() {
            for (pid, list) in parse_netstat(&String::from_utf8_lossy(&output.stdout)) {
                let merged = ports.entry(pid).or_default();
                for port in list {
                    if !merged.contains(&port) {
                        merged.push(port);
                    }
                }
                merged.sort_unstable();
            }
        }
    }
    ports
}

#[cfg(not(windows))]
fn listen_ports() -> HashMap<u32, Vec<u16>> {
    let output = crate::process::command(lsof_program())
        .args(["-nP", "-iTCP", "-sTCP:LISTEN", "-F", "pn"])
        .output();
    let Ok(output) = output else {
        return HashMap::new();
    };
    if !output.status.success() {
        return HashMap::new();
    }
    parse_listen_ports(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(not(windows))]
fn ps_program() -> &'static str {
    if cfg!(target_os = "macos") {
        "/bin/ps"
    } else {
        "ps"
    }
}

#[cfg(not(windows))]
fn lsof_program() -> &'static str {
    if cfg!(target_os = "macos") {
        "/usr/sbin/lsof"
    } else {
        "lsof"
    }
}

#[cfg_attr(windows, allow(dead_code))]
fn parse_process_tree(output: &str) -> HashMap<u32, Vec<u32>> {
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for line in output.lines() {
        let mut parts = line.split_whitespace();
        let Some(pid) = parts.next().and_then(|value| value.parse().ok()) else {
            continue;
        };
        let Some(parent) = parts.next().and_then(|value| value.parse().ok()) else {
            continue;
        };
        children.entry(parent).or_default().push(pid);
    }
    children
}

fn descendant_pids(root: u32, children: &HashMap<u32, Vec<u32>>) -> HashSet<u32> {
    let mut seen = HashSet::new();
    let mut stack = vec![root];
    while let Some(pid) = stack.pop() {
        if !seen.insert(pid) {
            continue;
        }
        let Some(kids) = children.get(&pid) else {
            continue;
        };
        stack.extend(kids.iter().copied());
    }
    seen
}

#[cfg_attr(windows, allow(dead_code))]
fn parse_listen_ports(output: &str) -> HashMap<u32, Vec<u16>> {
    let mut ports: HashMap<u32, Vec<u16>> = HashMap::new();
    let mut current = None;
    for line in output.lines() {
        let Some(kind) = line.as_bytes().first().copied() else {
            continue;
        };
        let rest = line.get(1..).unwrap_or("");
        if kind == b'p' {
            current = rest.trim().parse().ok();
            continue;
        }
        if kind != b'n' {
            continue;
        }
        let Some(pid) = current else {
            continue;
        };
        let Some(port) = rest.rsplit(':').next().and_then(|value| value.parse().ok()) else {
            continue;
        };
        let list = ports.entry(pid).or_default();
        if !list.contains(&port) {
            list.push(port);
        }
    }
    for list in ports.values_mut() {
        list.sort_unstable();
    }
    ports
}

/// 解析 `netstat -ano`。状态列会被本地化，所以用"远端地址端口为 0"识别监听行。
#[cfg_attr(not(windows), allow(dead_code))]
fn parse_netstat(output: &str) -> HashMap<u32, Vec<u16>> {
    let mut ports: HashMap<u32, Vec<u16>> = HashMap::new();
    for line in output.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 5 || !parts[0].eq_ignore_ascii_case("TCP") {
            continue;
        }
        if !parts[2].ends_with(":0") {
            continue;
        }
        let Some(port) = parts[1].rsplit(':').next().and_then(|value| value.parse::<u16>().ok()) else {
            continue;
        };
        let Some(pid) = parts[parts.len() - 1].parse::<u32>().ok() else {
            continue;
        };
        let list = ports.entry(pid).or_default();
        if !list.contains(&port) {
            list.push(port);
        }
    }
    for list in ports.values_mut() {
        list.sort_unstable();
    }
    ports
}

fn sessions() -> std::sync::MutexGuard<'static, HashMap<String, Session>> {
    SESSIONS.lock().unwrap_or_else(|error| error.into_inner())
}

#[cfg(test)]
mod tests {
    use super::{command_name, descendant_pids, parse_listen_ports, parse_netstat, parse_process_tree, slice_terminal_text};

    #[test]
    fn slice_uses_one_based_lines_and_the_stored_offset() {
        let (start, end, text) = slice_terminal_text("a\nb\nc", 11, Some(12), Some(13)).unwrap();
        assert_eq!((start, end, text.as_str()), (12, 13, "b\nc"));
    }

    #[test]
    fn slice_rejects_a_start_past_the_buffer() {
        let error = slice_terminal_text("only", 1, Some(4), None).unwrap_err();
        assert!(error.contains("超出"));
    }

    #[test]
    fn process_tree_groups_children_by_parent() {
        let children = parse_process_tree("  10  1\n20 10\n21 10\n30 20\n");
        let family = descendant_pids(10, &children);
        assert!(family.contains(&10));
        assert!(family.contains(&20));
        assert!(family.contains(&21));
        assert!(family.contains(&30));
        assert!(!family.contains(&1));
    }

    #[test]
    fn listen_ports_keep_numeric_ports_for_each_pid() {
        let ports = parse_listen_ports("p42\nn127.0.0.1:8080\nn*:5173\np7\nn[::1]:3000\n");
        assert_eq!(ports.get(&42).map(Vec::as_slice), Some([5173, 8080].as_slice()));
        assert_eq!(ports.get(&7).map(Vec::as_slice), Some([3000].as_slice()));
    }

    #[test]
    fn netstat_keeps_only_listening_rows() {
        let output = "\n活动连接\n\n  协议  本地地址          外部地址        状态           PID\n  TCP    0.0.0.0:5173           0.0.0.0:0              LISTENING       42\n  TCP    127.0.0.1:5173         127.0.0.1:50000        ESTABLISHED     42\n  TCP    [::]:3000              [::]:0                 侦听            7\n";
        let ports = parse_netstat(output);
        assert_eq!(ports.get(&42).map(Vec::as_slice), Some([5173].as_slice()));
        assert_eq!(ports.get(&7).map(Vec::as_slice), Some([3000].as_slice()));
    }

    #[test]
    fn windows_command_names_follow_pathext() {
        let exts = vec![".exe".to_string(), ".cmd".to_string()];
        assert_eq!(command_name("Git.EXE", Some(&exts)).as_deref(), Some("Git"));
        assert_eq!(command_name("npm.cmd", Some(&exts)).as_deref(), Some("npm"));
        assert_eq!(command_name("kernel32.dll", Some(&exts)), None);
        assert_eq!(command_name("ls", None).as_deref(), Some("ls"));
    }
}
