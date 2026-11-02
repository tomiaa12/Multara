//! 前端调用的命令。钥匙串、文件都是阻塞 IO，统一放到阻塞线程池里跑。

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

use serde::Serialize;

use super::{item::Ctx, keychain::SystemKeychain, registry, AccountStatus};
use crate::database::AppState;

const STORE_DIR: &str = "runtime-accounts";

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn ctx(keychain: &SystemKeychain) -> Result<Ctx<'_>, String> {
    let home = env_path("HOME")
        .or_else(|| env_path("USERPROFILE"))
        .ok_or_else(|| tr!("找不到用户主目录"))?;
    Ok(Ctx {
        codex_home: env_path("CODEX_HOME").unwrap_or_else(|| home.join(".codex")),
        kimi_home: env_path("KIMI_CODE_HOME").unwrap_or_else(|| home.join(".kimi-code")),
        user: std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_default(),
        home,
        keychain,
    })
}

fn builtin(runtime_id: &str) -> Result<(), String> {
    if runtime_id.starts_with("custom:") {
        return Err(tr!("自定义运行时不支持账号管理"));
    }
    Ok(())
}

async fn run<T, F>(app: AppHandle, work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&AppHandle, &Ctx, &Path) -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let dir = app
            .path()
            .app_data_dir()
            .map_err(|error| error.to_string())?
            .join(STORE_DIR);
        let keychain = SystemKeychain;
        let ctx = ctx(&keychain)?;
        work(&app, &ctx, &dir)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// 用同一个 CLI 的任务正在跑时不能换号：进程中途读到另一份 token 会出错。
/// 基于这个 CLI 的自定义运行时用的也是同一份全局登录，一起算。
fn ensure_idle(app: &AppHandle, runtime_id: &str) -> Result<(), String> {
    let state = app.state::<AppState>();
    let db = state.lock_db()?;
    let running: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM agent_runs WHERE status = 'running'
               AND (runtime_id = ?1
                    OR runtime_id IN (SELECT id FROM custom_runtimes WHERE provider = ?1))",
            [runtime_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if running > 0 {
        return Err(tr!("有任务正在使用这个 CLI，请等它结束或先停止再切换账号"));
    }
    Ok(())
}

fn detected(runtime_id: &str) -> Option<crate::models::Runtime> {
    crate::runtime::detect()
        .into_iter()
        .find(|runtime| runtime.id == runtime_id)
}

#[tauri::command]
pub async fn runtime_account_summaries(
    runtime_ids: Vec<String>,
    app: AppHandle,
) -> Result<Vec<AccountStatus>, String> {
    let _span = crate::diag::span("runtime_account_summaries");
    run(app, move |_, ctx, dir| {
        Ok(runtime_ids
            .iter()
            .filter(|id| !id.starts_with("custom:"))
            .map(|id| super::summary(ctx, dir, id))
            .collect())
    })
    .await
}

#[tauri::command]
pub async fn runtime_account_status(
    runtime_id: String,
    app: AppHandle,
) -> Result<AccountStatus, String> {
    builtin(&runtime_id)?;
    run(app, move |_, ctx, dir| super::status(ctx, dir, &runtime_id)).await
}

#[tauri::command]
pub async fn runtime_account_switch(
    runtime_id: String,
    account_id: String,
    app: AppHandle,
) -> Result<AccountStatus, String> {
    builtin(&runtime_id)?;
    run(app, move |app, ctx, dir| {
        ensure_idle(app, &runtime_id)?;
        let command = detected(&runtime_id)
            .map(|runtime| runtime.command)
            .unwrap_or_default();
        super::switch(ctx, dir, &runtime_id, &account_id, &command)
    })
    .await
}

#[tauri::command]
pub async fn runtime_account_remove(
    runtime_id: String,
    account_id: String,
    app: AppHandle,
) -> Result<AccountStatus, String> {
    builtin(&runtime_id)?;
    run(app, move |_, ctx, dir| super::remove(ctx, dir, &runtime_id, &account_id)).await
}

#[tauri::command]
pub async fn runtime_account_rename(
    runtime_id: String,
    account_id: String,
    name: String,
    app: AppHandle,
) -> Result<AccountStatus, String> {
    builtin(&runtime_id)?;
    run(app, move |_, ctx, dir| {
        super::rename(ctx, dir, &runtime_id, &account_id, &name)
    })
    .await
}

#[derive(Serialize)]
pub struct LoginCommand {
    /// 终端的工作目录（用户主目录）
    cwd: String,
    /// 在终端里输入的一行命令
    command: String,
}

/// 内嵌终端里要跑的登录命令。
#[tauri::command]
pub async fn runtime_account_login(runtime_id: String, app: AppHandle) -> Result<LoginCommand, String> {
    builtin(&runtime_id)?;
    run(app, move |_, ctx, dir| {
        let runtime = detected(&runtime_id).ok_or_else(|| tr!("找不到这个运行时"))?;
        // 先收下当前登录，免得被新登录覆盖；凭据读不出来时也要能去终端重新登录
        let _ = super::status(ctx, dir, &runtime_id);
        let program = runtime
            .path
            .ok_or_else(|| tr!("{command} 未检测到", command = runtime.command))?;
        Ok(LoginCommand {
            cwd: ctx.home.to_string_lossy().into_owned(),
            command: super::login::command_line(&program, registry::login_args(&runtime_id)),
        })
    })
    .await
}
