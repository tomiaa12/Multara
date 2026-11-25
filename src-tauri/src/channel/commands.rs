use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::database::AppState;

use super::feishu::api::ContactLookup;
use super::{secret, store, supervisor, ChannelConfig, Installation};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelInstallationView {
    id: String,
    channel_type: String,
    name: String,
    target_type: String,
    target_id: String,
    project_id: Option<String>,
    app_id: String,
    region: String,
    bot_name: Option<String>,
    bot_open_id: Option<String>,
    bot_avatar: Option<String>,
    group_enabled: bool,
    pairing_code: String,
    enabled: bool,
    status: String,
    last_error: Option<String>,
    has_secret: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelUserView {
    channel_user_id: String,
    display_name: Option<String>,
    role: String,
    /// 没拿到姓名时的原因，界面上提示用户去开权限。
    name_error: Option<String>,
    /// 授权（配对成功）时间
    created_at: String,
    note: Option<String>,
    target_type: Option<String>,
    target_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelInstallationInput {
    pub id: Option<String>,
    pub name: String,
    pub channel_type: String,
    /// 安装级目标已废弃，改由授权用户各自配置；保留字段兼容旧客户端。
    #[serde(default)]
    pub target_type: String,
    #[serde(default)]
    pub target_id: String,
    pub project_id: Option<String>,
    pub app_id: String,
    pub app_secret: Option<String>,
    /// Slack 的 App-level Token（xapp-）。其他渠道不用。
    pub app_token: Option<String>,
    pub region: Option<String>,
    #[serde(default)]
    pub group_enabled: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

fn view(app: &AppHandle, installation: &Installation) -> ChannelInstallationView {
    ChannelInstallationView {
        id: installation.id.clone(),
        channel_type: installation.channel_type.clone(),
        name: installation.name.clone(),
        target_type: installation.target_type.clone(),
        target_id: installation.target_id.clone(),
        project_id: installation.project_id.clone(),
        app_id: installation.config.app_id.clone(),
        region: installation.config.region().to_string(),
        bot_name: installation.config.bot_name.clone(),
        bot_open_id: installation.config.bot_open_id.clone(),
        bot_avatar: installation.config.bot_avatar.clone(),
        group_enabled: installation.config.group_enabled,
        pairing_code: installation.config.pairing_code.clone(),
        enabled: installation.enabled,
        status: installation.status.clone(),
        last_error: installation.last_error.clone(),
        has_secret: secret::load(app, &installation.id).is_some(),
    }
}

pub(crate) fn generate_pairing_code() -> String {
    let uuid = uuid::Uuid::new_v4();
    let digits: String = uuid
        .as_bytes()
        .iter()
        .map(|byte| char::from(b'0' + (byte % 10)))
        .collect();
    digits.chars().take(6).collect()
}

/// 钥匙串访问（`security` 子进程，可能弹授权框）不能在主线程，也不能持有数据库锁。
#[tauri::command]
pub async fn list_channel_installations(
    app: AppHandle,
) -> Result<Vec<ChannelInstallationView>, String> {
    crate::commands::off_main(app.clone(), move |state| {
        let installations = store::list_installations(&*state.lock_db()?)?;
        Ok(installations.iter().map(|item| view(&app, item)).collect())
    })
    .await
}

#[tauri::command]
pub async fn save_channel_installation(
    input: ChannelInstallationInput,
    app: AppHandle,
) -> Result<ChannelInstallationView, String> {
    crate::commands::off_main(app.clone(), move |state| {
        save_installation_blocking(input, &app, state)
    })
    .await
}

fn apply_credentials(
    channel_type: &str,
    app_id: &mut String,
    config: &mut super::ChannelConfig,
    fresh_secret: Option<String>,
    fresh_token: Option<String>,
    existing_secret: Option<String>,
) -> Result<Option<String>, String> {
    if channel_type == super::TYPE_WEIXIN {
        if app_id.is_empty() {
            *app_id = config.app_id.clone();
        }
        config.app_id = app_id.clone();
        return Ok(None);
    }
    if channel_type == super::TYPE_SLACK {
        let encoded = match (fresh_secret, fresh_token) {
            (Some(bot), Some(app_token)) => {
                if !bot.starts_with("xoxb-") || !app_token.starts_with("xapp-") {
                    return Err(tr!("Bot Token 以 xoxb- 开头，App Token 以 xapp- 开头"));
                }
                Some(super::slack::encode_secret(&bot, &app_token))
            }
            (None, None) => None,
            _ => return Err(tr!("请同时填写 Bot Token 和 App Token")),
        };
        if let Some(secret) = encoded {
            let (id, name) = super::slack::probe(&secret)?;
            *app_id = id;
            config.app_id = app_id.clone();
            config.bot_open_id = Some(app_id.clone());
            config.bot_name = Some(name.clone());
            config.bot_username = name;
            config.credential_rev = config.credential_rev.saturating_add(1);
            return Ok(Some(secret));
        }
        if existing_secret.is_none() {
            return Err(tr!("请填写 Bot Token 和 App Token"));
        }
        if app_id.is_empty() {
            *app_id = config.app_id.clone();
        }
        config.app_id = app_id.clone();
        return Ok(None);
    }
    if channel_type == super::TYPE_TELEGRAM {
        let Some(token) = fresh_secret else {
            if existing_secret.is_none() {
                return Err(tr!("请填写 Bot Token"));
            }
            if app_id.is_empty() {
                *app_id = config.app_id.clone();
            }
            config.app_id = app_id.clone();
            return Ok(None);
        };
        let (id, name) = super::telegram::probe(&token)?;
        *app_id = id.clone();
        config.app_id = id.clone();
        config.bot_open_id = Some(id);
        config.bot_name = Some(name);
        config.credential_rev = config.credential_rev.saturating_add(1);
        return Ok(Some(token));
    }
    if app_id.is_empty() {
        return Err(match channel_type {
            super::TYPE_DINGTALK => tr!("请填写 AppKey"),
            super::TYPE_WECOM => tr!("请填写 Bot ID"),
            super::TYPE_QQ => tr!("请填写 AppID"),
            _ => tr!("请填写 App ID"),
        });
    }
    if config.app_id != *app_id {
        config.bot_open_id = None;
        config.bot_name = None;
        config.bot_avatar = None;
    }
    config.app_id = app_id.clone();
    let Some(secret) = fresh_secret else {
        if existing_secret.is_none() {
            return Err(tr!("请填写密钥"));
        }
        return Ok(None);
    };
    if channel_type == super::TYPE_DINGTALK {
        super::dingtalk::probe(app_id, &secret)?;
    } else if channel_type == super::TYPE_WECOM {
        super::wecom::probe(app_id, &secret)?;
    } else if channel_type == super::TYPE_QQ {
        let (id, name) = super::qq::probe(app_id, &secret)?;
        config.bot_open_id = Some(id);
        config.bot_name = Some(name);
        let avatar = super::qq::cached_avatar(app_id);
        if !avatar.is_empty() {
            config.bot_avatar = Some(avatar);
        }
    }
    config.credential_rev = config.credential_rev.saturating_add(1);
    Ok(Some(secret))
}

fn save_installation_blocking(
    input: ChannelInstallationInput,
    app: &AppHandle,
    state: &AppState,
) -> Result<ChannelInstallationView, String> {
    let name = input.name.trim();
    let mut app_id = input.app_id.trim().to_string();
    if name.is_empty() {
        return Err(tr!("请填写名称"));
    }
    if !super::is_known(&input.channel_type) {
        return Err(tr!("暂不支持这个渠道类型"));
    }
    if input.channel_type == super::TYPE_WEIXIN && input.id.is_none() {
        return Err(tr!("微信请用扫码连接"));
    }
    let db = state.lock_db()?;
    if let Some(project_id) = input
        .project_id
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        let exists: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1)",
                [project_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if !exists {
            return Err(tr!("选择的需求不存在"));
        }
    }

    let existing = input
        .id
        .as_deref()
        .map(|id| store::get_installation(&db, id))
        .transpose()?
        .flatten();
    let id = existing
        .as_ref()
        .map(|item| item.id.clone())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let mut config = existing
        .as_ref()
        .map(|item| item.config.clone())
        .unwrap_or_default();
    if super::is_feishu(&input.channel_type) {
        if let Some(region) = input.region.as_deref().filter(|value| !value.is_empty()) {
            config.region = if region == super::TYPE_LARK {
                super::TYPE_LARK.to_string()
            } else {
                super::TYPE_FEISHU.to_string()
            };
        }
    }
    config.group_enabled = input.group_enabled && super::supports_group(&input.channel_type);
    if config.pairing_code.is_empty() {
        config.pairing_code = generate_pairing_code();
    }
    drop(db);
    let fresh_secret = input
        .app_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let fresh_token = input
        .app_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let stored_secret = apply_credentials(
        &input.channel_type,
        &mut app_id,
        &mut config,
        fresh_secret,
        fresh_token,
        secret::load(app, &id),
    )?;
    if let Some(secret) = stored_secret {
        secret::store(app, &id, &secret)?;
    }
    if secret::load(app, &id).is_none() {
        return Err(tr!("请填写凭据"));
    }
    if app_id.is_empty() {
        return Err(tr!("请填写应用标识"));
    }
    // 安装级目标不再使用；新建留空，编辑保留旧值以免误清已回填到用户上的历史数据。
    let target_type = existing
        .as_ref()
        .map(|item| item.target_type.clone())
        .filter(|value| matches!(value.as_str(), "agent" | "squad"))
        .unwrap_or_else(|| "agent".to_string());
    let target_id = existing
        .as_ref()
        .map(|item| item.target_id.clone())
        .unwrap_or_default();

    let installation = Installation {
        id: id.clone(),
        channel_type: input.channel_type.clone(),
        name: name.to_string(),
        target_type,
        target_id,
        project_id: input.project_id.clone().filter(|value| !value.is_empty()),
        config,
        enabled: input.enabled,
        status: existing
            .as_ref()
            .map(|item| item.status.clone())
            .unwrap_or_else(|| "disconnected".to_string()),
        last_error: None,
    };
    store::save_installation(&*state.lock_db()?, &installation)?;
    supervisor::reload(app);
    let saved = store::get_installation(&*state.lock_db()?, &id)?.ok_or_else(|| tr!("保存失败"))?;
    Ok(view(app, &saved))
}

#[tauri::command]
pub fn delete_channel_installation(
    id: String,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    {
        let db = state.lock_db()?;
        store::delete_installation(&db, &id)?;
    }
    secret::delete(&app, &id);
    supervisor::reload(&app);
    Ok(())
}

#[tauri::command]
pub fn set_channel_enabled(
    id: String,
    enabled: bool,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    {
        let db = state.lock_db()?;
        db.execute(
            "UPDATE channel_installations SET enabled=?1, updated_at=?2 WHERE id=?3",
            rusqlite::params![enabled as i64, chrono::Utc::now().to_rfc3339(), id],
        )
        .map_err(|error| error.to_string())?;
        if !enabled {
            let _ = store::set_status(&db, &id, "disconnected", None);
        }
    }
    supervisor::reload(&app);
    Ok(())
}

#[tauri::command]
pub async fn test_channel_installation(
    id: String,
    app: AppHandle,
) -> Result<HashMap<String, String>, String> {
    crate::commands::off_main(app.clone(), move |state| {
        let channel_type = store::get_installation(&*state.lock_db()?, &id)?
            .ok_or_else(|| tr!("渠道连接不存在"))?
            .channel_type;
        let (installation, open_id, name, avatar) = if super::is_feishu(&channel_type) {
            let (installation, api) = super::feishu::api_for(&app, &id)?;
            let bot = api.bot_info()?;
            (installation, bot.open_id, bot.name, bot.avatar_url)
        } else {
            let (open_id, name) = super::link::probe(&app, &id)?;
            let installation = store::get_installation(&*state.lock_db()?, &id)?
                .ok_or_else(|| tr!("渠道连接不存在"))?;
            let avatar = if installation.channel_type == super::TYPE_DINGTALK {
                super::dingtalk::cached_avatar(&installation.config.app_id)
            } else if installation.channel_type == super::TYPE_QQ {
                super::qq::cached_avatar(&installation.config.app_id)
            } else {
                String::new()
            };
            (installation, open_id, name, avatar)
        };
        let mut config: ChannelConfig = installation.config.clone();
        // 钉钉的探测 id 是 AppKey，不能盖掉消息里记下的 chatbotUserId。
        // 没读到名字或头像时保留已有值，避免一次失败把飞书以外的资料清掉。
        if super::is_feishu(&installation.channel_type) {
            config.bot_open_id = (!open_id.is_empty()).then(|| open_id.clone());
            config.bot_name = (!name.is_empty()).then(|| name.clone());
            config.bot_avatar = (!avatar.is_empty()).then_some(avatar);
        } else {
            if installation.channel_type != super::TYPE_DINGTALK && !open_id.is_empty() {
                config.bot_open_id = Some(open_id.clone());
            }
            if !name.is_empty() {
                config.bot_name = Some(name.clone());
            }
            if !avatar.is_empty() {
                config.bot_avatar = Some(avatar);
            }
        }
        store::set_config(&*state.lock_db()?, &id, &config)?;
        let mut result = HashMap::new();
        result.insert("openId".to_string(), open_id);
        result.insert("name".to_string(), name);
        Ok(result)
    })
    .await
}

#[tauri::command]
pub fn list_channel_allowed_users(
    id: String,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ChannelUserView>, String> {
    let mut users = {
        let db = state.lock_db()?;
        store::list_allowed_users(&db, &id)?
    };
    let name_errors = fill_missing_names(&app, &state, &id, &mut users);
    Ok(users
        .into_iter()
        .map(|user| ChannelUserView {
            name_error: name_errors.get(&user.channel_user_id).cloned(),
            channel_user_id: user.channel_user_id,
            display_name: user.display_name,
            role: user.role,
            created_at: user.created_at,
            note: user.note,
            target_type: user.target_type,
            target_id: user.target_id,
        })
        .collect())
}

fn fill_missing_names(
    app: &AppHandle,
    state: &tauri::State<'_, AppState>,
    installation_id: &str,
    users: &mut [store::AllowedUser],
) -> HashMap<String, String> {
    let mut errors = HashMap::new();
    let missing = users.iter().any(|user| saved_name(user).is_none());
    if !missing {
        return errors;
    }
    let feishu = state
        .lock_db()
        .ok()
        .and_then(|db| store::get_installation(&db, installation_id).ok().flatten())
        .is_some_and(|item| super::is_feishu(&item.channel_type));
    if !feishu {
        return errors;
    }
    let api = match super::feishu::api_for(app, installation_id) {
        Ok((_, api)) => api,
        Err(error) => {
            for user in users.iter().filter(|user| saved_name(user).is_none()) {
                errors.insert(user.channel_user_id.clone(), error.clone());
            }
            return errors;
        }
    };
    for user in users.iter_mut() {
        if saved_name(user).is_some() {
            continue;
        }
        let name = match api.lookup_user_name(&user.channel_user_id) {
            ContactLookup::Name(name) => name,
            ContactLookup::MissingNamePermission => {
                errors.insert(
                    user.channel_user_id.clone(),
                    tr!("飞书没有返回姓名：应用需开通「获取用户基本信息」权限（contact:user.base:readonly），并创建新版本发布后生效"),
                );
                continue;
            }
            ContactLookup::Failed(error) => {
                errors.insert(
                    user.channel_user_id.clone(),
                    tr!("查询飞书用户姓名失败：{error}", error = error),
                );
                continue;
            }
        };
        if let Ok(db) = state.lock_db() {
            let _ =
                store::set_allowed_user_name(&db, installation_id, &user.channel_user_id, &name);
        }
        user.display_name = Some(name);
    }
    errors
}

fn saved_name(user: &store::AllowedUser) -> Option<&str> {
    user.display_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
}

#[tauri::command]
pub fn remove_channel_allowed_user(
    id: String,
    user_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let db = state.lock_db()?;
    store::remove_allowed_user(&db, &id, &user_id)
}

/// 列表上的群聊开关：只改 `group_enabled`。连接线程按配置指纹重启后生效。
#[tauri::command]
pub fn set_channel_group_enabled(
    id: String,
    enabled: bool,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    {
        let db = state.lock_db()?;
        let installation =
            store::get_installation(&db, &id)?.ok_or_else(|| tr!("渠道连接不存在"))?;
        if enabled && !super::supports_group(&installation.channel_type) {
            return Err(tr!("微信只接收扫码账号的私聊"));
        }
        let mut config = installation.config.clone();
        config.group_enabled = enabled;
        store::set_config(&db, &id, &config)?;
    }
    supervisor::reload(&app);
    Ok(())
}

#[tauri::command]
pub fn set_channel_user_note(
    id: String,
    user_id: String,
    note: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let note = note.trim();
    let db = state.lock_db()?;
    store::set_allowed_user_note(&db, &id, &user_id, (!note.is_empty()).then_some(note))
}

#[tauri::command]
pub fn set_channel_user_target(
    id: String,
    user_id: String,
    target_type: Option<String>,
    target_id: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let target_type = target_type
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let target_id = target_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    match (target_type, target_id) {
        (None, None) => {
            let db = state.lock_db()?;
            store::set_allowed_user_target(&db, &id, &user_id, None, None)
        }
        (Some(kind), Some(target)) if matches!(kind, "agent" | "squad") => {
            let db = state.lock_db()?;
            let exists: bool = if kind == "agent" {
                db.query_row(
                    "SELECT EXISTS(SELECT 1 FROM agents WHERE id=?1)",
                    [target],
                    |row| row.get(0),
                )
            } else {
                db.query_row(
                    "SELECT EXISTS(SELECT 1 FROM squads WHERE id=?1)",
                    [target],
                    |row| row.get(0),
                )
            }
            .map_err(|error| error.to_string())?;
            if !exists {
                return Err(if kind == "agent" {
                    tr!("选择的智能体不存在")
                } else {
                    tr!("选择的小队不存在")
                });
            }
            store::set_allowed_user_target(&db, &id, &user_id, Some(kind), Some(target))
        }
        _ => Err(tr!("目标类型无效")),
    }
}

#[tauri::command]
pub fn rotate_channel_pairing_code(
    id: String,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let db = state.lock_db()?;
    let installation = store::get_installation(&db, &id)?.ok_or_else(|| tr!("渠道连接不存在"))?;
    let mut config = installation.config.clone();
    config.pairing_code = generate_pairing_code();
    store::set_config(&db, &id, &config)?;
    drop(db);
    supervisor::reload(&app);
    Ok(config.pairing_code)
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeixinLoginInput {
    pub name: String,
    pub project_id: Option<String>,
}

#[tauri::command]
pub async fn begin_weixin_login(
    input: WeixinLoginInput,
    app: AppHandle,
) -> Result<super::weixin::LoginView, String> {
    crate::commands::off_main(app, move |_state| {
        super::weixin::begin(&input.name, input.project_id)
    })
    .await
}

#[tauri::command]
pub async fn poll_weixin_login(
    session_id: String,
    app: AppHandle,
) -> Result<super::weixin::LoginView, String> {
    crate::commands::off_main(app.clone(), move |_state| {
        super::weixin::poll(&app, &session_id)
    })
    .await
}

#[tauri::command]
pub async fn submit_weixin_verify_code(session_id: String, code: String) -> Result<(), String> {
    super::weixin::submit_code(&session_id, &code)
}
