use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::database::AppState;

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
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelInstallationInput {
    pub id: Option<String>,
    pub name: String,
    pub channel_type: String,
    pub target_type: String,
    pub target_id: String,
    pub project_id: Option<String>,
    pub app_id: String,
    pub app_secret: Option<String>,
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
        group_enabled: installation.config.group_enabled,
        pairing_code: installation.config.pairing_code.clone(),
        enabled: installation.enabled,
        status: installation.status.clone(),
        last_error: installation.last_error.clone(),
        has_secret: secret::load(app, &installation.id).is_some(),
    }
}

fn generate_pairing_code() -> String {
    let uuid = uuid::Uuid::new_v4();
    let digits: String = uuid
        .as_bytes()
        .iter()
        .map(|byte| char::from(b'0' + (byte % 10)))
        .collect();
    digits.chars().take(6).collect()
}

#[tauri::command]
pub fn list_channel_installations(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ChannelInstallationView>, String> {
    let db = state.lock_db()?;
    let installations = store::list_installations(&db)?;
    Ok(installations.iter().map(|item| view(&app, item)).collect())
}

#[tauri::command]
pub fn save_channel_installation(
    input: ChannelInstallationInput,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<ChannelInstallationView, String> {
    let name = input.name.trim();
    let app_id = input.app_id.trim();
    if name.is_empty() {
        return Err("请填写名称".to_string());
    }
    if app_id.is_empty() {
        return Err("请填写 App ID".to_string());
    }
    if !matches!(input.target_type.as_str(), "agent" | "squad") {
        return Err("目标类型无效".to_string());
    }
    if !matches!(
        input.channel_type.as_str(),
        super::TYPE_FEISHU | super::TYPE_LARK
    ) {
        return Err("暂不支持这个渠道类型".to_string());
    }
    let db = state.lock_db()?;
    if input.target_type == "agent" {
        let exists: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM agents WHERE id=?1)",
                [&input.target_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if !exists {
            return Err("选择的智能体不存在".to_string());
        }
    } else {
        let exists: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM squads WHERE id=?1)",
                [&input.target_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if !exists {
            return Err("选择的小队不存在".to_string());
        }
    }
    if let Some(project_id) = input.project_id.as_deref().filter(|value| !value.is_empty()) {
        let exists: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1)",
                [project_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if !exists {
            return Err("选择的需求不存在".to_string());
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
    let app_changed = config.app_id != app_id;
    config.app_id = app_id.to_string();
    if let Some(region) = input.region.as_deref().filter(|value| !value.is_empty()) {
        config.region = if region == super::TYPE_LARK {
            super::TYPE_LARK.to_string()
        } else {
            super::TYPE_FEISHU.to_string()
        };
    }
    config.group_enabled = input.group_enabled;
    if config.pairing_code.is_empty() {
        config.pairing_code = generate_pairing_code();
    }
    if app_changed {
        config.bot_open_id = None;
        config.bot_name = None;
    }

    if let Some(app_secret) = input.app_secret.as_deref().map(str::trim) {
        if !app_secret.is_empty() {
            secret::store(&app, &id, app_secret)?;
        }
    }
    if secret::load(&app, &id).is_none() {
        return Err("请填写 App Secret".to_string());
    }

    let installation = Installation {
        id: id.clone(),
        channel_type: input.channel_type.clone(),
        name: name.to_string(),
        target_type: input.target_type.clone(),
        target_id: input.target_id.clone(),
        project_id: input.project_id.clone().filter(|value| !value.is_empty()),
        config,
        enabled: input.enabled,
        status: existing
            .as_ref()
            .map(|item| item.status.clone())
            .unwrap_or_else(|| "disconnected".to_string()),
        last_error: None,
    };
    store::save_installation(&db, &installation)?;
    drop(db);
    supervisor::reload(&app);
    let db = state.lock_db()?;
    let saved = store::get_installation(&db, &id)?.ok_or_else(|| "保存失败".to_string())?;
    Ok(view(&app, &saved))
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
pub fn test_channel_installation(
    id: String,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<HashMap<String, String>, String> {
    let (installation, api) = super::feishu::api_for(&app, &id)?;
    let (open_id, name) = api.bot_info()?;
    let mut config: ChannelConfig = installation.config.clone();
    config.bot_open_id = (!open_id.is_empty()).then(|| open_id.clone());
    config.bot_name = (!name.is_empty()).then(|| name.clone());
    {
        let db = state.lock_db()?;
        store::set_config(&db, &id, &config)?;
    }
    let mut result = HashMap::new();
    result.insert("openId".to_string(), open_id);
    result.insert("name".to_string(), name);
    Ok(result)
}

#[tauri::command]
pub fn list_channel_allowed_users(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ChannelUserView>, String> {
    let db = state.lock_db()?;
    Ok(store::list_allowed_users(&db, &id)?
        .into_iter()
        .map(|user| ChannelUserView {
            channel_user_id: user.channel_user_id,
            display_name: user.display_name,
            role: user.role,
        })
        .collect())
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

#[tauri::command]
pub fn rotate_channel_pairing_code(
    id: String,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let db = state.lock_db()?;
    let installation = store::get_installation(&db, &id)?.ok_or_else(|| "渠道连接不存在".to_string())?;
    let mut config = installation.config.clone();
    config.pairing_code = generate_pairing_code();
    store::set_config(&db, &id, &config)?;
    drop(db);
    supervisor::reload(&app);
    Ok(config.pairing_code)
}
