//! 内置运行时（官方 CLI）的账号：显示当前登录，在多个账号之间切换。
//!
//! 做法是「收录当前登录 → 存进 Multara 的账号库 → 切换时写回」，登录本身仍由 CLI 完成。
//! 切换改的是该 CLI 的全局登录，终端里的同一个 CLI 也会跟着变。几条规矩（同 cc-switch）：
//! - 切换前先把当前登录收回来：CLI 可能刷新过 token，已收录的账号用最新内容更新，
//!   不认识的自动收录，切走也不会丢。
//! - 写完再读一遍核对身份，不一致就恢复原登录。
//! - 从列表删除只删 Multara 的副本，不替 CLI 登出；不自动换号。
//!
//! 详见 `docs/官方运行时账号管理技术方案.md`。

pub mod commands;
pub mod identity;
pub mod item;
pub mod keychain;
pub mod login;
pub mod registry;
pub mod store;

use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
    thread,
    time::{Duration, SystemTime},
};

use serde::Serialize;

use identity::{identify, Identity};
use item::Ctx;
use registry::{Item, Spec, Tier};
use store::{Account, Store};

/// 账号库读改写和切换整个过程串行，避免两次点击交错写凭据
static LOCK: Mutex<()> = Mutex::new(());

const KIMI_LOCK_STALE: Duration = Duration::from_secs(5);
const KIMI_LOCK_RETRY: Duration = Duration::from_millis(500);
const KIMI_LOCK_RETRIES: u32 = 120;
const ACCOUNT_ID_CHARS: usize = 12;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountStatus {
    pub runtime_id: String,
    pub tier: Tier,
    pub switchable: bool,
    /// 不能切换的原因
    pub reason: Option<String>,
    /// 设置了会覆盖登录的环境变量时，变量名
    pub override_env: Option<String>,
    pub live: LiveAccount,
    pub accounts: Vec<AccountView>,
    /// 切换后检测到外部还开着这个 CLI
    pub running_elsewhere: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveAccount {
    /// `None`：凭据位置未知，判断不了
    pub signed_in: Option<bool>,
    pub label: Option<String>,
    /// 当前登录是库里的哪个账号
    pub account_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountView {
    pub id: String,
    pub label: Option<String>,
    pub name: Option<String>,
    pub captured_at: i64,
    pub current: bool,
}

pub fn tags(spec: &Spec) -> Vec<String> {
    spec.items.iter().map(Item::tag).collect()
}

fn read_live(ctx: &Ctx, spec: &Spec, with_keychain: bool) -> Result<Vec<Option<Vec<u8>>>, String> {
    spec.items
        .iter()
        .map(|item| match item {
            Item::Keychain { .. } if !with_keychain => Ok(None),
            _ => item::read(ctx, item),
        })
        .collect()
}

fn unsupported(ctx: &Ctx, spec: &Spec) -> Option<String> {
    if spec.mac_only && !keychain::supported() {
        return Some(tr!("当前系统暂不支持切换这个 CLI 的账号"));
    }
    if spec.runtime == "codex" && codex_uses_keyring(ctx) {
        return Some(tr!(
            "Codex 设置为把登录存进系统钥匙串（cli_auth_credentials_store），暂不支持切换"
        ));
    }
    None
}

fn codex_uses_keyring(ctx: &Ctx) -> bool {
    let text = fs::read_to_string(ctx.codex_home.join("config.toml")).unwrap_or_default();
    let store = toml::from_str::<toml::Table>(&text)
        .ok()
        .and_then(|table| table.get("cli_auth_credentials_store")?.as_str().map(str::to_string));
    matches!(store.as_deref(), Some("keyring" | "auto"))
}

fn override_env(ctx: &Ctx, spec: &Spec) -> Option<String> {
    let from_process = spec
        .env_overrides
        .iter()
        .find(|name| std::env::var(name).is_ok_and(|value| !value.trim().is_empty()));
    if let Some(name) = from_process {
        return Some(name.to_string());
    }
    if spec.runtime != "claude" {
        return None;
    }
    let text = fs::read(ctx.home.join(".claude/settings.json")).ok()?;
    let settings: serde_json::Value = serde_json::from_slice(&text).ok()?;
    let env = settings.get("env")?;
    spec.env_overrides
        .iter()
        .find(|name| env.get(**name).and_then(|value| value.as_str()).is_some_and(|value| !value.trim().is_empty()))
        .map(|name| name.to_string())
}

fn unknown_status(runtime: &str) -> AccountStatus {
    AccountStatus {
        runtime_id: runtime.to_string(),
        tier: Tier::D,
        switchable: false,
        reason: None,
        override_env: None,
        live: LiveAccount {
            signed_in: None,
            label: None,
            account_id: None,
        },
        accounts: Vec::new(),
        running_elsewhere: false,
    }
}

fn build_status(
    ctx: &Ctx,
    spec: &Spec,
    store: &Store,
    live: Option<&Identity>,
) -> AccountStatus {
    let reason = unsupported(ctx, spec);
    let current = live.and_then(|identity| find_by_key(store, &identity.key));
    AccountStatus {
        runtime_id: spec.runtime.to_string(),
        tier: spec.tier,
        switchable: reason.is_none(),
        reason,
        override_env: override_env(ctx, spec),
        live: LiveAccount {
            signed_in: Some(live.is_some()),
            label: live.and_then(|identity| identity.label.clone()),
            account_id: current.map(|account| account.id.clone()),
        },
        accounts: store
            .accounts
            .iter()
            .map(|account| AccountView {
                id: account.id.clone(),
                label: account.label.clone(),
                name: account.name.clone(),
                captured_at: account.captured_at,
                current: current.is_some_and(|item| item.id == account.id),
            })
            .collect(),
        running_elsewhere: false,
    }
}

fn find_by_key<'s>(store: &'s Store, key: &str) -> Option<&'s Account> {
    store.accounts.iter().find(|account| account.identity_key == key)
}

/// 已收录就更新内容，没收录就新建；返回账号 id。内容没变时不动，免得「保存于」一直是刚刚。
fn upsert(store: &mut Store, tags: &[String], identity: &Identity, values: &[Option<Vec<u8>>]) -> String {
    let items = store::encode(tags, values);
    let now = chrono::Utc::now().timestamp_millis();
    if let Some(account) = store
        .accounts
        .iter_mut()
        .find(|account| account.identity_key == identity.key)
    {
        if account.items == items {
            return account.id.clone();
        }
        account.items = items;
        account.label = identity.label.clone().or(account.label.take());
        account.captured_at = now;
        return account.id.clone();
    }
    let id: String = uuid::Uuid::new_v4()
        .simple()
        .to_string()
        .chars()
        .take(ACCOUNT_ID_CHARS)
        .collect();
    store.accounts.push(Account {
        id: id.clone(),
        identity_key: identity.key.clone(),
        label: identity.label.clone(),
        name: None,
        captured_at: now,
        items,
    });
    id
}

/// 运行时列表里的账号区。钥匙串和文件混存的 CLI（Claude、Cursor）只读文件、不收录，
/// 免得一打开页面就弹钥匙串授权；当前账号按显示名对上。其余的和 `status` 一样。
pub fn summary(ctx: &Ctx, dir: &Path, runtime: &str) -> AccountStatus {
    let Some(spec) = registry::spec(runtime) else {
        return unknown_status(runtime);
    };
    let is_keychain = |item: &Item| matches!(item, Item::Keychain { .. });
    let mixed = spec.items.iter().any(is_keychain) && !spec.items.iter().all(is_keychain);
    if !mixed {
        if let Ok(status) = status(ctx, dir, runtime) {
            return status;
        }
    }
    let store = store::load(&store::path(dir, runtime)).unwrap_or_default();
    let mut status = build_status(ctx, spec, &store, None);
    let live = mixed
        .then(|| read_live(ctx, spec, false).ok())
        .flatten()
        .map(|values| identify(&values));
    let Some(live) = live else {
        status.live.signed_in = None;
        return status;
    };
    status.live.signed_in = Some(live.is_some());
    status.live.label = live.and_then(|identity| identity.label);
    let current = status.live.label.as_ref().and_then(|label| {
        store
            .accounts
            .iter()
            .find(|account| account.label.as_ref() == Some(label))
            .map(|account| account.id.clone())
    });
    for account in &mut status.accounts {
        account.current = current.as_ref() == Some(&account.id);
    }
    status.live.account_id = current;
    status
}

/// 读当前登录并自动收进账号库：新账号新建，已收录的用最新凭据更新。
pub fn status(ctx: &Ctx, dir: &Path, runtime: &str) -> Result<AccountStatus, String> {
    let Some(spec) = registry::spec(runtime) else {
        return Ok(unknown_status(runtime));
    };
    let _guard = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let path = store::path(dir, runtime);
    let mut store = store::load(&path)?;
    let values = read_live(ctx, spec, true)?;
    let live = identify(&values);
    if let Some(identity) = live.as_ref().filter(|_| unsupported(ctx, spec).is_none()) {
        let before = serde_json::to_vec(&store).ok();
        store.current = Some(upsert(&mut store, &tags(spec), identity, &values));
        if serde_json::to_vec(&store).ok() != before {
            store::save(&path, &mut store)?;
        }
    }
    Ok(build_status(ctx, spec, &store, live.as_ref()))
}

pub fn switch(ctx: &Ctx, dir: &Path, runtime: &str, account_id: &str, command: &str) -> Result<AccountStatus, String> {
    let spec = registry::spec(runtime).ok_or_else(|| tr!("这个 CLI 暂不支持账号切换"))?;
    if let Some(reason) = unsupported(ctx, spec) {
        return Err(reason);
    }
    let _guard = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let _kimi = spec
        .kimi_lock
        .map(|name| kimi_lock(&ctx.kimi_home, name))
        .transpose()?;
    let tags = tags(spec);
    let path = store::path(dir, runtime);
    let mut store = store::load(&path)?;
    let target = store
        .accounts
        .iter()
        .find(|account| account.id == account_id)
        .cloned()
        .ok_or_else(|| tr!("账号不存在，可能已被删除"))?;

    let previous = read_live(ctx, spec, true)?;
    if let Some(identity) = identify(&previous) {
        if identity.key == target.identity_key {
            store.current = Some(target.id.clone());
            store::save(&path, &mut store)?;
            return Ok(build_status(ctx, spec, &store, Some(&identity)));
        }
        upsert(&mut store, &tags, &identity, &previous);
        store::save(&path, &mut store)?;
    }

    let wanted = target.values(&tags);
    if let Err(error) = write_all(ctx, spec, &wanted) {
        let _ = write_all(ctx, spec, &previous);
        return Err(error);
    }
    let after = identify(&read_live(ctx, spec, true)?);
    if after.as_ref().map(|identity| identity.key.as_str()) != Some(target.identity_key.as_str()) {
        let _ = write_all(ctx, spec, &previous);
        return Err(tr!("切换后读到的不是目标账号，已恢复原来的登录"));
    }
    store.current = Some(target.id.clone());
    store::save(&path, &mut store)?;
    let mut status = build_status(ctx, spec, &store, after.as_ref());
    status.running_elsewhere = login::process_running(command);
    Ok(status)
}

fn write_all(ctx: &Ctx, spec: &Spec, values: &[Option<Vec<u8>>]) -> Result<(), String> {
    spec.items
        .iter()
        .zip(values)
        .try_for_each(|(item, value)| item::write(ctx, item, value.as_deref()))
}

pub fn remove(ctx: &Ctx, dir: &Path, runtime: &str, account_id: &str) -> Result<AccountStatus, String> {
    let spec = registry::spec(runtime).ok_or_else(|| tr!("这个 CLI 暂不支持账号切换"))?;
    let _guard = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let path = store::path(dir, runtime);
    let mut store = store::load(&path)?;
    store.accounts.retain(|account| account.id != account_id);
    if store.current.as_deref() == Some(account_id) {
        store.current = None;
    }
    store::save(&path, &mut store)?;
    let live = identify(&read_live(ctx, spec, true)?);
    Ok(build_status(ctx, spec, &store, live.as_ref()))
}

pub fn rename(ctx: &Ctx, dir: &Path, runtime: &str, account_id: &str, name: &str) -> Result<AccountStatus, String> {
    let spec = registry::spec(runtime).ok_or_else(|| tr!("这个 CLI 暂不支持账号切换"))?;
    let _guard = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let path = store::path(dir, runtime);
    let mut store = store::load(&path)?;
    let account = store
        .accounts
        .iter_mut()
        .find(|account| account.id == account_id)
        .ok_or_else(|| tr!("账号不存在，可能已被删除"))?;
    let name = name.trim();
    account.name = (!name.is_empty()).then(|| name.to_string());
    store::save(&path, &mut store)?;
    let live = identify(&read_live(ctx, spec, true)?);
    Ok(build_status(ctx, spec, &store, live.as_ref()))
}

/// 和 Kimi Code 抢同一把 proper-lockfile 锁（`$KIMI/oauth/<name>.lock` 目录），
/// 避免覆盖它刚轮换的 refresh token。Kimi 在 Windows 上和设了 `KIMI_DISABLE_OAUTH_LOCK=1`
/// 时不加锁，这里也跳过。
struct KimiLock(PathBuf);

impl Drop for KimiLock {
    fn drop(&mut self) {
        let _ = fs::remove_dir(&self.0);
    }
}

fn kimi_lock(home: &Path, name: &str) -> Result<Option<KimiLock>, String> {
    if cfg!(windows)
        || std::env::var("KIMI_DISABLE_OAUTH_LOCK").as_deref() == Ok("1")
        || !home.exists()
    {
        return Ok(None);
    }
    let oauth = home.join("oauth");
    fs::create_dir_all(&oauth).map_err(|error| error.to_string())?;
    let target = oauth.join(name);
    if !target.exists() {
        fs::write(&target, b"").map_err(|error| error.to_string())?;
    }
    let dir = oauth.join(format!("{name}.lock"));
    for _ in 0..KIMI_LOCK_RETRIES {
        match fs::create_dir(&dir) {
            Ok(()) => return Ok(Some(KimiLock(dir))),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                let stale = fs::metadata(&dir)
                    .and_then(|metadata| metadata.modified())
                    .ok()
                    .and_then(|modified| SystemTime::now().duration_since(modified).ok())
                    .is_some_and(|age| age > KIMI_LOCK_STALE);
                if stale {
                    let _ = fs::remove_dir(&dir);
                    continue;
                }
                thread::sleep(KIMI_LOCK_RETRY);
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    Err(tr!("Kimi Code 正在刷新登录，请稍后再试"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use keychain::{memory::MemoryKeychain, Keychain};

    fn ctx<'a>(home: &Path, keychain: &'a MemoryKeychain) -> Ctx<'a> {
        Ctx {
            home: home.to_path_buf(),
            codex_home: home.join(".codex"),
            kimi_home: home.join(".kimi-code"),
            user: "tester".into(),
            keychain,
        }
    }

    fn jwt(sub: &str, email: &str) -> String {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
        let claims = format!(r#"{{"sub":"{sub}","email":"{email}"}}"#);
        format!("e30.{}.sig", URL_SAFE_NO_PAD.encode(claims))
    }

    fn codex_auth(sub: &str, email: &str) -> String {
        format!(r#"{{"tokens":{{"id_token":"{}"}}}}"#, jwt(sub, email))
    }

    #[test]
    fn status_saves_logins_and_switch_absorbs_unseen_ones() {
        let home = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let keychain = MemoryKeychain::default();
        let ctx = ctx(home.path(), &keychain);
        let auth = home.path().join(".codex/auth.json");

        item::write_private(&auth, codex_auth("a", "a@x.com").as_bytes()).unwrap();
        let first = status(&ctx, data.path(), "codex").unwrap();
        let account_a = first.live.account_id.clone().unwrap();
        assert_eq!(first.accounts.len(), 1);

        // 用户在终端登录了另一个号、还没打开面板：切回 A 时 B 会被收录
        item::write_private(&auth, codex_auth("b", "b@x.com").as_bytes()).unwrap();
        let switched = switch(&ctx, data.path(), "codex", &account_a, "codex-test-missing").unwrap();
        assert_eq!(switched.live.label.as_deref(), Some("a@x.com"));
        assert_eq!(switched.accounts.len(), 2);
        assert!(fs::read_to_string(&auth).unwrap().contains(&jwt("a", "a@x.com")));

        let account_b = switched
            .accounts
            .iter()
            .find(|account| !account.current)
            .unwrap()
            .id
            .clone();
        let back = switch(&ctx, data.path(), "codex", &account_b, "codex-test-missing").unwrap();
        assert_eq!(back.live.label.as_deref(), Some("b@x.com"));
    }

    #[test]
    fn status_refreshes_known_account_with_latest_tokens() {
        let home = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let keychain = MemoryKeychain::default();
        let ctx = ctx(home.path(), &keychain);
        let auth = home.path().join(".grok/auth.json");

        item::write_private(&auth, format!(r#"{{"sub":"g","refresh":"old"}}"#).as_bytes()).unwrap();
        let id = status(&ctx, data.path(), "grok").unwrap().live.account_id.unwrap();
        rename(&ctx, data.path(), "grok", &id, "工作号").unwrap();
        let captured_at = status(&ctx, data.path(), "grok").unwrap().accounts[0].captured_at;
        thread::sleep(Duration::from_millis(5));
        assert_eq!(status(&ctx, data.path(), "grok").unwrap().accounts[0].captured_at, captured_at);
        item::write_private(&auth, br#"{"sub":"g","refresh":"new"}"#).unwrap();
        let refreshed = status(&ctx, data.path(), "grok").unwrap();
        assert_eq!(refreshed.accounts.len(), 1);
        assert_eq!(refreshed.accounts[0].name.as_deref(), Some("工作号"));

        let saved = store::load(&store::path(data.path(), "grok")).unwrap();
        let values = saved.accounts[0].values(&tags(registry::spec("grok").unwrap()));
        assert_eq!(values[0].as_deref(), Some(br#"{"sub":"g","refresh":"new"}"#.as_slice()));
    }

    #[test]
    fn keychain_and_json_field_switch_together() {
        let home = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let keychain = MemoryKeychain::default();
        let ctx = ctx(home.path(), &keychain);
        let config = home.path().join(".cursor/cli-config.json");
        let set = |token: &str, email: &str, id: u32| {
            keychain.write("cursor-access-token", "cursor-user", token).unwrap();
            keychain.write("cursor-refresh-token", "cursor-user", token).unwrap();
            let root = format!(r#"{{"editor":{{"vimMode":true}},"authInfo":{{"email":"{email}","userId":{id}}}}}"#);
            item::write_private(&config, root.as_bytes()).unwrap();
        };
        if !keychain::supported() {
            return;
        }
        set("t1", "one@x.com", 1);
        let one = status(&ctx, data.path(), "cursor").unwrap().live.account_id.unwrap();
        set("t2", "two@x.com", 2);
        assert_eq!(status(&ctx, data.path(), "cursor").unwrap().accounts.len(), 2);

        switch(&ctx, data.path(), "cursor", &one, "cursor-test-missing").unwrap();
        assert_eq!(
            keychain.read("cursor-access-token", "cursor-user").unwrap().as_deref(),
            Some("t1")
        );
        let root: serde_json::Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
        assert_eq!(root["authInfo"]["email"], "one@x.com");
        assert_eq!(root["editor"]["vimMode"], true);

        // 列表上不读钥匙串，靠显示名认出当前账号
        let listed = summary(&ctx, data.path(), "cursor");
        assert_eq!(listed.live.account_id.as_deref(), Some(one.as_str()));
        assert_eq!(listed.accounts.iter().filter(|account| account.current).count(), 1);
    }

    #[test]
    fn signed_out_saves_nothing_and_unregistered_cli_is_unknown() {
        let home = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let keychain = MemoryKeychain::default();
        let ctx = ctx(home.path(), &keychain);
        let signed_out = status(&ctx, data.path(), "grok").unwrap();
        assert_eq!(signed_out.live.signed_in, Some(false));
        assert!(signed_out.accounts.is_empty());
        let unknown = status(&ctx, data.path(), "qoder").unwrap();
        assert_eq!(unknown.tier, Tier::D);
        assert_eq!(unknown.live.signed_in, None);
    }

    #[test]
    fn codex_keyring_mode_is_not_switchable() {
        let home = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let keychain = MemoryKeychain::default();
        let ctx = ctx(home.path(), &keychain);
        item::write_private(
            &home.path().join(".codex/config.toml"),
            b"cli_auth_credentials_store = \"keyring\"\n",
        )
        .unwrap();
        let status = status(&ctx, data.path(), "codex").unwrap();
        assert!(!status.switchable);
        assert!(status.reason.is_some());
    }
}
