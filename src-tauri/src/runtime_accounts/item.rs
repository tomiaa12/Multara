//! 单个凭据项的读和写。值一律按字节原样保存，不解析、不改字段。

use std::{
    borrow::Cow,
    fs,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};

use serde_json::{Map, Value};

use super::{
    keychain::{self, Keychain},
    registry::{Item, KeyAccount},
};

/// GitHub CLI `hosts.yml` 里 `github.com` 段的当前 `user`。
pub fn gh_hosts_user(hosts_yml: &str) -> Option<String> {
    let mut in_github = false;
    let mut user = None;
    for line in hosts_yml.lines() {
        if line.starts_with("github.com:") {
            in_github = true;
            continue;
        }
        if !in_github {
            continue;
        }
        if !line.is_empty() && !line.starts_with([' ', '\t', '#']) {
            break;
        }
        let trimmed = line.trim();
        let Some(value) = trimmed.strip_prefix("user:") else {
            continue;
        };
        let value = value.trim().trim_matches(['"', '\'']);
        if !value.is_empty() {
            user = Some(value.to_string());
        }
    }
    user
}

/// Copilot `config.json` 的 `lastLoggedInUser` → `(host, login)`。支持整行 `//` 注释。
pub fn copilot_last_user(config_text: &str) -> Option<(String, String)> {
    let root: Value = serde_json::from_str(&strip_json_line_comments(config_text)).ok()?;
    let user = root.get("lastLoggedInUser")?;
    let login = user.get("login")?.as_str()?.trim();
    if login.is_empty() {
        return None;
    }
    let host = user
        .get("host")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("https://github.com");
    Some((host.to_string(), login.to_string()))
}

fn strip_json_line_comments(text: &str) -> String {
    text.lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn copilot_cli_account(ctx: &Ctx<'_>, github_suffix: bool) -> Result<String, String> {
    let path = ctx.path("~/.copilot/config.json");
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Err(tr!("读不到 Copilot 当前用户（~/.copilot/config.json）"));
        }
        Err(error) => return Err(error.to_string()),
    };
    let (host, login) = copilot_last_user(&text)
        .ok_or_else(|| tr!("读不到 Copilot 当前用户（config.json 里没有 lastLoggedInUser）"))?;
    if github_suffix {
        Ok(format!("{host}:{login}:github"))
    } else {
        Ok(format!("{host}:{login}"))
    }
}

pub struct Ctx<'a> {
    pub home: PathBuf,
    pub codex_home: PathBuf,
    pub kimi_home: PathBuf,
    pub user: String,
    pub keychain: &'a dyn Keychain,
}

impl Ctx<'_> {
    pub fn path(&self, raw: &str) -> PathBuf {
        let roots = [
            ("$CODEX/", &self.codex_home),
            ("$KIMI/", &self.kimi_home),
            ("~/", &self.home),
        ];
        roots
            .iter()
            .find_map(|(prefix, root)| raw.strip_prefix(prefix).map(|rest| root.join(rest)))
            .unwrap_or_else(|| PathBuf::from(raw))
    }

    fn account<'a>(&'a self, account: &'a KeyAccount) -> Result<Cow<'a, str>, String> {
        match account {
            KeyAccount::Fixed(name) => Ok(Cow::Borrowed(name)),
            KeyAccount::CurrentUser => Ok(Cow::Borrowed(&self.user)),
            KeyAccount::GhHostsUser => {
                let path = self.path("~/.config/gh/hosts.yml");
                let text = match fs::read_to_string(&path) {
                    Ok(text) => text,
                    Err(error) if error.kind() == ErrorKind::NotFound => {
                        return Err(tr!("读不到 GitHub CLI 当前用户（~/.config/gh/hosts.yml）"));
                    }
                    Err(error) => return Err(error.to_string()),
                };
                gh_hosts_user(&text)
                    .map(Cow::Owned)
                    .ok_or_else(|| tr!("读不到 GitHub CLI 当前用户（hosts.yml 里没有 user）"))
            }
            KeyAccount::CopilotCliUser => copilot_cli_account(self, false).map(Cow::Owned),
            KeyAccount::CopilotCliUserGithub => copilot_cli_account(self, true).map(Cow::Owned),
        }
    }
}

pub fn read(ctx: &Ctx, item: &Item) -> Result<Option<Vec<u8>>, String> {
    match item {
        Item::File(raw) => read_file(&ctx.path(raw)),
        Item::JsonField { file, pointer } => {
            let path = ctx.path(file);
            let Some(root) = read_json(&path)? else {
                return Ok(None);
            };
            root.pointer(pointer)
                .map(|value| serde_json::to_vec(value).map_err(|error| error.to_string()))
                .transpose()
        }
        Item::Keychain { service, account } => {
            if !keychain::supported() {
                return Ok(None);
            }
            let Ok(name) = ctx.account(account) else {
                // 还没登录时 hosts.yml / user 不存在，当成未登录而不是报错
                return Ok(None);
            };
            Ok(ctx.keychain.read(service, &name)?.map(String::into_bytes))
        }
    }
}

/// `None` 表示这一项在该账号下不存在：文件会被删掉；钥匙串条目不删，避免替 CLI 登出。
pub fn write(ctx: &Ctx, item: &Item, value: Option<&[u8]>) -> Result<(), String> {
    match item {
        Item::File(raw) => {
            let path = ctx.path(raw);
            match value {
                Some(bytes) => write_private(&path, bytes),
                None => remove_file(&path),
            }
        }
        Item::JsonField { file, pointer } => {
            let path = ctx.path(file);
            let existing = read_json(&path)?;
            if existing.is_none() && value.is_none() {
                return Ok(());
            }
            let mut root = existing.unwrap_or_else(|| Value::Object(Map::new()));
            match value {
                Some(bytes) => {
                    let field: Value =
                        serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
                    set_pointer(&mut root, pointer, field)?;
                }
                None => remove_pointer(&mut root, pointer),
            }
            let mut text = serde_json::to_vec_pretty(&root).map_err(|error| error.to_string())?;
            text.push(b'\n');
            write_private(&path, &text)
        }
        Item::Keychain { service, account } => match value {
            Some(bytes) => {
                let name = ctx.account(account)?;
                ctx.keychain
                    .write(service, &name, &String::from_utf8_lossy(bytes))
            }
            None => Ok(()),
        },
    }
}

fn read_file(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(tr!(
            "读取 {path} 失败：{error}",
            path = path.display(),
            error = error
        )),
    }
}

fn read_json(path: &Path) -> Result<Option<Value>, String> {
    let Some(bytes) = read_file(path)? else {
        return Ok(None);
    };
    let text = String::from_utf8_lossy(&bytes);
    serde_json::from_str(&strip_json_line_comments(&text))
        .map(Some)
        .map_err(|_| tr!("{path} 不是标准 JSON，无法读取账号", path = path.display()))
}

fn remove_file(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != ErrorKind::NotFound => Err(error.to_string()),
        _ => Ok(()),
    }
}

/// 同目录临时文件再替换：已有文件沿用原权限，新文件只给本人读写。
pub fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or_else(|| tr!("无效文件路径"))?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
    temporary
        .write_all(bytes)
        .map_err(|error| error.to_string())?;
    temporary.flush().map_err(|error| error.to_string())?;
    match fs::metadata(path) {
        Ok(metadata) => fs::set_permissions(temporary.path(), metadata.permissions())
            .map_err(|error| error.to_string())?,
        Err(_) => restrict(temporary.path())?,
    }
    temporary
        .persist(path)
        .map_err(|error| tr!("保存文件失败：{error}", error = error.error))?;
    Ok(())
}

#[cfg(unix)]
fn restrict(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|error| error.to_string())
}

#[cfg(not(unix))]
fn restrict(_path: &Path) -> Result<(), String> {
    Ok(())
}

fn pointer_parts(pointer: &str) -> Vec<&str> {
    pointer.split('/').filter(|part| !part.is_empty()).collect()
}

fn set_pointer(root: &mut Value, pointer: &str, field: Value) -> Result<(), String> {
    let parts = pointer_parts(pointer);
    let Some((last, parents)) = parts.split_last() else {
        *root = field;
        return Ok(());
    };
    let mut node = root;
    for part in parents {
        let object = node
            .as_object_mut()
            .ok_or_else(|| tr!("配置结构和预期不同，无法写入账号"))?;
        node = object
            .entry(part.to_string())
            .or_insert_with(|| Value::Object(Map::new()));
    }
    node.as_object_mut()
        .ok_or_else(|| tr!("配置结构和预期不同，无法写入账号"))?
        .insert(last.to_string(), field);
    Ok(())
}

fn remove_pointer(root: &mut Value, pointer: &str) {
    let parts = pointer_parts(pointer);
    let Some((last, parents)) = parts.split_last() else {
        return;
    };
    let parent = parents
        .iter()
        .try_fold(root, |node, part| node.get_mut(*part));
    if let Some(object) = parent.and_then(Value::as_object_mut) {
        object.remove(*last);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_accounts::keychain::memory::MemoryKeychain;

    fn ctx<'a>(home: &Path, keychain: &'a MemoryKeychain) -> Ctx<'a> {
        Ctx {
            home: home.to_path_buf(),
            codex_home: home.join(".codex"),
            kimi_home: home.join(".kimi-code"),
            user: "tester".into(),
            keychain,
        }
    }

    #[test]
    fn json_field_only_touches_its_own_key() {
        let home = tempfile::tempdir().unwrap();
        let keychain = MemoryKeychain::default();
        let ctx = ctx(home.path(), &keychain);
        let path = home.path().join(".cursor/cli-config.json");
        write_private(&path, br#"{"editor":{"vimMode":true},"authInfo":{"email":"a@x.com"}}"#)
            .unwrap();
        let item = Item::JsonField {
            file: "~/.cursor/cli-config.json",
            pointer: "/authInfo",
        };

        write(&ctx, &item, Some(br#"{"email":"b@x.com"}"#)).unwrap();
        let root: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(root["authInfo"]["email"], "b@x.com");
        assert_eq!(root["editor"]["vimMode"], true);

        write(&ctx, &item, None).unwrap();
        let root: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert!(root.get("authInfo").is_none());
        assert_eq!(root["editor"]["vimMode"], true);
    }

    #[test]
    fn nested_pointer_creates_parents() {
        let mut root = Value::Object(Map::new());
        set_pointer(&mut root, "/security/auth", serde_json::json!({"selectedType":"oauth"}))
            .unwrap();
        assert_eq!(root["security"]["auth"]["selectedType"], "oauth");
        remove_pointer(&mut root, "/security/auth");
        assert!(root["security"].get("auth").is_none());
    }

    #[test]
    fn missing_file_reads_as_none_and_none_removes_file() {
        let home = tempfile::tempdir().unwrap();
        let keychain = MemoryKeychain::default();
        let ctx = ctx(home.path(), &keychain);
        let item = Item::File("$CODEX/auth.json");
        assert_eq!(read(&ctx, &item).unwrap(), None);
        write(&ctx, &item, Some(b"{}")).unwrap();
        assert_eq!(read(&ctx, &item).unwrap(), Some(b"{}".to_vec()));
        write(&ctx, &item, None).unwrap();
        assert_eq!(read(&ctx, &item).unwrap(), None);
    }

    #[test]
    fn gh_hosts_user_reads_active_login() {
        assert_eq!(
            gh_hosts_user("github.com:\n    users:\n        alice:\n    user: alice\n").as_deref(),
            Some("alice")
        );
        assert_eq!(gh_hosts_user("enterprise.example:\n    user: bob\n"), None);
    }

    #[test]
    fn gh_hosts_keychain_uses_active_user() {
        let home = tempfile::tempdir().unwrap();
        let keychain = MemoryKeychain::default();
        let ctx = ctx(home.path(), &keychain);
        let hosts = home.path().join(".config/gh/hosts.yml");
        write_private(
            &hosts,
            b"github.com:\n    users:\n        alice:\n    user: alice\n",
        )
        .unwrap();
        keychain
            .write("gh:github.com", "alice", "token-a")
            .unwrap();
        let item = Item::Keychain {
            service: "gh:github.com",
            account: KeyAccount::GhHostsUser,
        };
        assert_eq!(read(&ctx, &item).unwrap(), Some(b"token-a".to_vec()));
        write(&ctx, &item, Some(b"token-b")).unwrap();
        assert_eq!(
            keychain.read("gh:github.com", "alice").unwrap().as_deref(),
            Some("token-b")
        );
    }

    #[test]
    fn copilot_cli_keychain_uses_last_logged_in_user() {
        let home = tempfile::tempdir().unwrap();
        let keychain = MemoryKeychain::default();
        let ctx = ctx(home.path(), &keychain);
        write_private(
            &home.path().join(".copilot/config.json"),
            br#"// header
{"lastLoggedInUser":{"host":"https://github.com","login":"alice"}}
"#,
        )
        .unwrap();
        keychain
            .write("copilot-cli", "https://github.com:alice", "token-a")
            .unwrap();
        let item = Item::Keychain {
            service: "copilot-cli",
            account: KeyAccount::CopilotCliUser,
        };
        assert_eq!(read(&ctx, &item).unwrap(), Some(b"token-a".to_vec()));
        assert_eq!(
            copilot_last_user("// x\n{\"lastLoggedInUser\":{\"login\":\"bob\"}}\n"),
            Some(("https://github.com".into(), "bob".into()))
        );
    }

    #[cfg(unix)]
    #[test]
    fn new_files_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("a/b.json");
        write_private(&path, b"{}").unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
