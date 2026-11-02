//! 单个凭据项的读和写。值一律按字节原样保存，不解析、不改字段。

use std::{
    fs,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};

use serde_json::{Map, Value};

use super::{
    keychain::{self, Keychain},
    registry::{Item, KeyAccount},
};

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

    fn account<'s>(&'s self, account: &'s KeyAccount) -> &'s str {
        match account {
            KeyAccount::Fixed(name) => name,
            KeyAccount::CurrentUser => &self.user,
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
            Ok(ctx
                .keychain
                .read(service, ctx.account(account))?
                .map(String::into_bytes))
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
            Some(bytes) => ctx.keychain.write(
                service,
                ctx.account(account),
                &String::from_utf8_lossy(bytes),
            ),
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
    serde_json::from_slice(&bytes)
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
