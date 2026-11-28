use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use super::{
    client, gateway,
    frame::RemoteError,
    identity, pairing,
    server::{self, HostStatus},
    store, terminals,
};
use crate::database::AppState;

fn off_main<T: Send + 'static>(
    app: AppHandle,
    task: impl FnOnce(&AppHandle) -> Result<T, String> + Send + 'static,
) -> impl std::future::Future<Output = Result<T, String>> {
    async move {
        tauri::async_runtime::spawn_blocking(move || task(&app))
            .await
            .map_err(|error| error.to_string())?
    }
}

fn devices_in_group(db: &rusqlite::Connection, group_id: &str) -> Vec<String> {
    store::list_devices(db)
        .unwrap_or_default()
        .into_iter()
        .filter(|device| device.group_id == group_id)
        .map(|device| device.id)
        .collect()
}

// ---------- 主机端 ----------

#[tauri::command]
pub async fn remote_host_get_status(app: AppHandle) -> Result<HostStatus, String> {
    off_main(app, server::status).await
}

#[tauri::command]
pub async fn remote_host_set_enabled(enabled: bool, app: AppHandle) -> Result<HostStatus, String> {
    off_main(app, move |app| {
        {
            let state = app.state::<AppState>();
            let db = state.lock_db()?;
            store::set_setting(&db, store::HOST_ENABLED_KEY, if enabled { "true" } else { "false" })?;
        }
        if enabled {
            server::listen(app)?;
        } else {
            server::stop();
            pairing::cancel_code();
        }
        server::status(app)
    })
    .await
}

#[tauri::command]
pub async fn remote_host_set_port(port: u16, app: AppHandle) -> Result<HostStatus, String> {
    off_main(app, move |app| {
        if port == 0 {
            return Err(tr!("端口无效"));
        }
        let enabled = {
            let state = app.state::<AppState>();
            let db = state.lock_db()?;
            store::set_setting(&db, store::HOST_PORT_KEY, &port.to_string())?;
            store::host_enabled(&db)
        };
        if enabled {
            server::listen(app)?;
        }
        server::status(app)
    })
    .await
}

#[tauri::command]
pub async fn remote_pairing_create(app: AppHandle) -> Result<pairing::PairingCode, String> {
    off_main(app, |app| {
        let status = server::status(app)?;
        if !status.running {
            return Err(tr!("请先开启主机模式"));
        }
        pairing::create_code(&status.fingerprint, &status.addresses, status.port)
    })
    .await
}

#[tauri::command]
pub fn remote_pairing_cancel() {
    pairing::cancel_code();
}

#[tauri::command]
pub fn remote_pairing_pending() -> Vec<pairing::PendingPairing> {
    pairing::list_pending()
}

#[tauri::command]
pub async fn remote_pairing_confirm(
    id: String,
    accept: bool,
    name: String,
    owner_note: String,
    group_id: String,
    app: AppHandle,
) -> Result<Option<store::Device>, String> {
    off_main(app, move |app| {
        let device = pairing::confirm(app, &id, accept, &name, &owner_note, &group_id)?;
        let _ = app.emit(server::DEVICES_CHANGED_EVENT, ());
        Ok(device)
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceView {
    #[serde(flatten)]
    pub device: store::Device,
    pub online: bool,
    pub running: u32,
    pub total_tokens: i64,
    pub cost_usd: f64,
}

#[tauri::command]
pub async fn remote_devices_list(app: AppHandle) -> Result<Vec<DeviceView>, String> {
    off_main(app, |app| {
        let online = server::online_device_ids();
        let state = app.state::<AppState>();
        let db = state.lock_db()?;
        store::list_devices(&db)?
            .into_iter()
            .map(|device| {
                let (running, total_tokens, cost_usd) = db
                    .query_row(
                        "SELECT COALESCE(SUM(status IN ('queued', 'running')), 0),
                                COALESCE(SUM(input_tokens + output_tokens + cache_read_tokens + cache_write_tokens), 0),
                                COALESCE(SUM(cost_usd), 0)
                         FROM agent_runs WHERE origin_principal=?1",
                        [format!("device:{}", device.id)],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .unwrap_or((0, 0, 0.0));
                Ok(DeviceView {
                    online: online.contains(&device.id),
                    running,
                    total_tokens,
                    cost_usd,
                    device,
                })
            })
            .collect()
    })
    .await
}

#[tauri::command]
pub async fn remote_device_update(
    id: String,
    name: String,
    owner_note: String,
    group_id: String,
    app: AppHandle,
) -> Result<(), String> {
    off_main(app, move |app| {
        {
            let state = app.state::<AppState>();
            let db = state.lock_db()?;
            store::update_device(&db, &id, &name, &owner_note, &group_id)?;
        }
        gateway::notify_permissions_changed(&[id]);
        let _ = app.emit(server::DEVICES_CHANGED_EVENT, ());
        Ok(())
    })
    .await
}

#[tauri::command]
pub fn remote_device_disconnect(id: String) {
    server::disconnect_device(&id);
}

#[tauri::command]
pub async fn remote_device_remove(id: String, app: AppHandle) -> Result<(), String> {
    off_main(app, move |app| {
        {
            let state = app.state::<AppState>();
            let db = state.lock_db()?;
            store::audit_device(&db, &id, "remove", None, None, "ok", Value::Null);
            store::delete_device(&db, &id)?;
        }
        server::disconnect_device(&id);
        terminals::close_all(app, &id);
        let _ = app.emit(server::DEVICES_CHANGED_EVENT, ());
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn remote_groups_list(app: AppHandle) -> Result<Vec<store::Group>, String> {
    off_main(app, |app| store::list_groups(&*app.state::<AppState>().lock_db()?)).await
}

#[tauri::command]
pub async fn remote_group_save(group: store::Group, app: AppHandle) -> Result<store::Group, String> {
    off_main(app, move |app| {
        let (saved, devices) = {
            let state = app.state::<AppState>();
            let db = state.lock_db()?;
            let saved = store::save_group(&db, &group)?;
            let devices = devices_in_group(&db, &saved.id);
            (saved, devices)
        };
        gateway::notify_permissions_changed(&devices);
        Ok(saved)
    })
    .await
}

#[tauri::command]
pub async fn remote_group_delete(id: String, app: AppHandle) -> Result<(), String> {
    off_main(app, move |app| store::delete_group(&*app.state::<AppState>().lock_db()?, &id)).await
}

#[tauri::command]
pub async fn remote_requests_list(app: AppHandle) -> Result<Vec<store::PermissionRequest>, String> {
    off_main(app, |app| store::list_requests(&*app.state::<AppState>().lock_db()?, None)).await
}

/// 处理权限申请：`move` 把设备移到 `group_id`；`addProject` 把申请的需求加入设备当前分组；`reject` 拒绝。
#[tauri::command]
pub async fn remote_request_decide(
    id: String,
    action: String,
    group_id: Option<String>,
    app: AppHandle,
) -> Result<(), String> {
    off_main(app, move |app| {
        let (request, affected) = {
            let state = app.state::<AppState>();
            let db = state.lock_db()?;
            let request = store::get_request(&db, &id)?.ok_or_else(|| tr!("申请不存在"))?;
            let device = store::get_device(&db, &request.device_id)?.ok_or_else(|| tr!("设备不存在"))?;
            let affected = match action.as_str() {
                "move" => {
                    let target = group_id.as_deref().filter(|id| !id.is_empty()).ok_or_else(|| tr!("请选择分组"))?;
                    store::update_device(&db, &device.id, &device.name, &device.owner_note, target)?;
                    vec![device.id.clone()]
                }
                "addProject" => {
                    let project_id = request.project_id.as_deref().ok_or_else(|| tr!("这个申请没有指定需求"))?;
                    store::add_project_to_group(&db, &device.group_id, project_id)?;
                    devices_in_group(&db, &device.group_id)
                }
                "reject" => Vec::new(),
                _ => return Err(tr!("无效的操作")),
            };
            let status = if action == "reject" { "rejected" } else { "approved" };
            store::set_request_status(&db, &id, status)?;
            store::audit_device(&db, &device.id, "request_decided", None, None, "ok", serde_json::json!({ "action": action }));
            (store::get_request(&db, &id)?.ok_or_else(|| tr!("申请不存在"))?, affected)
        };
        gateway::notify_request(&request.device_id, &request);
        gateway::notify_permissions_changed(&affected);
        let _ = app.emit(gateway::REQUESTS_CHANGED_EVENT, ());
        let _ = app.emit(server::DEVICES_CHANGED_EVENT, ());
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn remote_approvals_list(pending_only: bool, app: AppHandle) -> Result<Vec<store::Approval>, String> {
    off_main(app, move |app| store::list_approvals(&*app.state::<AppState>().lock_db()?, pending_only)).await
}

#[tauri::command]
pub async fn remote_approval_decide(id: String, approve: bool, app: AppHandle) -> Result<(), String> {
    off_main(app, move |app| {
        let approval = {
            let state = app.state::<AppState>();
            let db = state.lock_db()?;
            let approval = store::decide_approval(&db, &id, approve)?;
            store::audit_device(
                &db,
                &approval.device_id,
                "approval_decided",
                Some(&approval.cmd),
                None,
                "ok",
                serde_json::json!({ "approved": approve }),
            );
            approval
        };
        gateway::notify_approval(&approval.device_id, &approval);
        let _ = app.emit(gateway::APPROVALS_CHANGED_EVENT, ());
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn remote_audit_query(query: store::AuditQuery, app: AppHandle) -> Result<Vec<store::AuditEntry>, String> {
    off_main(app, move |app| {
        let state = app.state::<AppState>();
        let db = state.lock_db()?;
        store::prune_audit(&db);
        store::query_audit(&db, &query)
    })
    .await
}

// ---------- 控制端 ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostView {
    #[serde(flatten)]
    pub host: store::Host,
    pub status: Option<client::HostConnStatus>,
}

#[tauri::command]
pub async fn remote_hosts_list(app: AppHandle) -> Result<Vec<HostView>, String> {
    off_main(app, |app| {
        let statuses = client::statuses();
        let hosts = store::list_hosts(&*app.state::<AppState>().lock_db()?)?;
        Ok(hosts
            .into_iter()
            .map(|host| HostView {
                status: statuses.iter().find(|status| status.host_id == host.id).cloned(),
                host,
            })
            .collect())
    })
    .await
}

#[tauri::command]
pub fn remote_identity(app: AppHandle) -> Result<String, String> {
    identity::load_or_create(&server::app_data(&app)?).map(|identity| identity.fingerprint)
}

#[tauri::command]
pub async fn remote_host_pair(
    address: String,
    port: u16,
    code: String,
    fingerprint: Option<String>,
    device_name: String,
    app: AppHandle,
) -> Result<store::Host, String> {
    off_main(app, move |app| {
        let fingerprint = fingerprint.map(|value| value.trim().to_ascii_lowercase()).filter(|value| !value.is_empty());
        let host = client::pair(app, &address, port, &code, fingerprint, &device_name)?;
        client::connect(app, &host.id);
        Ok(host)
    })
    .await
}

#[tauri::command]
pub async fn remote_host_update(id: String, name: String, address: String, port: u16, app: AppHandle) -> Result<(), String> {
    off_main(app, move |app| {
        store::update_host(&*app.state::<AppState>().lock_db()?, &id, &name, &address, port)?;
        client::disconnect(&id);
        client::connect(app, &id);
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn remote_host_remove(id: String, app: AppHandle) -> Result<(), String> {
    off_main(app, move |app| {
        client::disconnect(&id);
        store::delete_host(&*app.state::<AppState>().lock_db()?, &id)
    })
    .await
}

#[tauri::command]
pub fn remote_host_connect(id: String, app: AppHandle) {
    client::connect(&app, &id);
}

#[tauri::command]
pub fn remote_host_disconnect(id: String) {
    client::disconnect(&id);
}

#[tauri::command]
pub async fn remote_invoke(
    host_id: String,
    cmd: String,
    args: Option<Value>,
    approval: Option<String>,
    app: AppHandle,
) -> Result<Value, RemoteError> {
    tauri::async_runtime::spawn_blocking(move || {
        client::invoke(&app, &host_id, &cmd, args.unwrap_or(Value::Object(Default::default())), approval)
    })
    .await
    .map_err(|error| RemoteError::new("internal", error.to_string()))?
}
