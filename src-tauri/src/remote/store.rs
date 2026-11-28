use chrono::{Duration as ChronoDuration, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::permission::Role;

pub const DEFAULT_GROUP_ID: &str = "remote-default";
pub const HOST_ENABLED_KEY: &str = "remoteHostEnabled";
pub const HOST_PORT_KEY: &str = "remoteHostPort";
pub const DEFAULT_PORT: u16 = 43130;
const AUDIT_RETENTION_DAYS: i64 = 90;
const AUDIT_MAX_ROWS: i64 = 200_000;

pub fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS remote_groups (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          role TEXT NOT NULL CHECK(role IN ('viewer','runner','developer','owner')),
          all_projects INTEGER NOT NULL DEFAULT 0,
          all_agents INTEGER NOT NULL DEFAULT 1,
          share_in_group INTEGER NOT NULL DEFAULT 0,
          require_approval INTEGER NOT NULL DEFAULT 0,
          max_running INTEGER NOT NULL DEFAULT 2,
          is_default INTEGER NOT NULL DEFAULT 0,
          created_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS remote_group_projects (
          group_id TEXT NOT NULL,
          project_id TEXT NOT NULL,
          PRIMARY KEY (group_id, project_id)
        );
        CREATE TABLE IF NOT EXISTS remote_group_agents (
          group_id TEXT NOT NULL,
          target_id TEXT NOT NULL,
          PRIMARY KEY (group_id, target_id)
        );
        CREATE TABLE IF NOT EXISTS remote_devices (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          owner_note TEXT NOT NULL DEFAULT '',
          fingerprint TEXT NOT NULL UNIQUE,
          group_id TEXT NOT NULL,
          paired_at TEXT NOT NULL,
          last_seen_at TEXT,
          last_address TEXT
        );
        CREATE TABLE IF NOT EXISTS remote_permission_requests (
          id TEXT PRIMARY KEY,
          device_id TEXT NOT NULL,
          permission TEXT NOT NULL,
          project_id TEXT,
          note TEXT NOT NULL DEFAULT '',
          status TEXT NOT NULL CHECK(status IN ('pending','approved','rejected','cancelled')),
          created_at TEXT NOT NULL,
          decided_at TEXT
        );
        CREATE TABLE IF NOT EXISTS remote_approvals (
          id TEXT PRIMARY KEY,
          device_id TEXT NOT NULL,
          cmd TEXT NOT NULL,
          args_hash TEXT NOT NULL,
          summary TEXT NOT NULL,
          status TEXT NOT NULL CHECK(status IN ('pending','approved','rejected','expired','used')),
          created_at TEXT NOT NULL,
          expires_at TEXT NOT NULL,
          decided_at TEXT
        );
        CREATE TABLE IF NOT EXISTS remote_audit_log (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          at TEXT NOT NULL,
          device_id TEXT,
          device_name TEXT NOT NULL,
          group_id TEXT,
          action TEXT NOT NULL,
          cmd TEXT,
          resource_kind TEXT,
          resource_id TEXT,
          outcome TEXT NOT NULL CHECK(outcome IN ('ok','denied','pending','error')),
          detail_json TEXT NOT NULL DEFAULT '{}'
        );
        CREATE INDEX IF NOT EXISTS idx_remote_audit_at ON remote_audit_log(at);
        CREATE INDEX IF NOT EXISTS idx_remote_audit_device ON remote_audit_log(device_id, at);
        CREATE TABLE IF NOT EXISTS remote_hosts (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          address TEXT NOT NULL,
          port INTEGER NOT NULL,
          fingerprint TEXT NOT NULL,
          created_at TEXT NOT NULL,
          last_connected_at TEXT
        );
        ",
    )?;
    db.execute(
        "INSERT OR IGNORE INTO remote_groups (id, name, role, is_default, created_at)
         VALUES (?1, ?2, 'viewer', 1, ?3)",
        params![DEFAULT_GROUP_ID, tr!("仅查看"), now()],
    )?;
    Ok(())
}

pub fn now() -> String {
    Utc::now().to_rfc3339()
}

fn err(error: rusqlite::Error) -> String {
    error.to_string()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: String,
    pub name: String,
    pub role: Role,
    pub all_projects: bool,
    pub all_agents: bool,
    pub share_in_group: bool,
    pub require_approval: bool,
    pub max_running: u32,
    pub is_default: bool,
    #[serde(default)]
    pub project_ids: Vec<String>,
    #[serde(default)]
    pub agent_ids: Vec<String>,
    #[serde(default)]
    pub device_count: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    pub name: String,
    pub owner_note: String,
    pub fingerprint: String,
    pub group_id: String,
    pub paired_at: String,
    pub last_seen_at: Option<String>,
    pub last_address: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Host {
    pub id: String,
    pub name: String,
    pub address: String,
    pub port: u16,
    pub fingerprint: String,
    pub created_at: String,
    pub last_connected_at: Option<String>,
}

fn ids(db: &Connection, sql: &str, key: &str) -> Result<Vec<String>, String> {
    let mut statement = db.prepare(sql).map_err(err)?;
    let rows = statement
        .query_map([key], |row| row.get(0))
        .map_err(err)?
        .collect::<rusqlite::Result<Vec<String>>>()
        .map_err(err)?;
    Ok(rows)
}

fn read_group(row: &rusqlite::Row<'_>) -> rusqlite::Result<Group> {
    let role: String = row.get(2)?;
    Ok(Group {
        id: row.get(0)?,
        name: row.get(1)?,
        role: Role::parse(&role).unwrap_or(Role::Viewer),
        all_projects: row.get::<_, i64>(3)? != 0,
        all_agents: row.get::<_, i64>(4)? != 0,
        share_in_group: row.get::<_, i64>(5)? != 0,
        require_approval: row.get::<_, i64>(6)? != 0,
        max_running: row.get::<_, i64>(7)?.max(1) as u32,
        is_default: row.get::<_, i64>(8)? != 0,
        project_ids: Vec::new(),
        agent_ids: Vec::new(),
        device_count: row.get::<_, i64>(9)?.max(0) as u32,
    })
}

const GROUP_COLUMNS: &str = "g.id, g.name, g.role, g.all_projects, g.all_agents, g.share_in_group,
    g.require_approval, g.max_running, g.is_default,
    (SELECT COUNT(*) FROM remote_devices d WHERE d.group_id = g.id)";

fn fill_group(db: &Connection, mut group: Group) -> Result<Group, String> {
    group.project_ids = ids(
        db,
        "SELECT project_id FROM remote_group_projects WHERE group_id=?1",
        &group.id,
    )?;
    group.agent_ids = ids(
        db,
        "SELECT target_id FROM remote_group_agents WHERE group_id=?1",
        &group.id,
    )?;
    Ok(group)
}

pub fn list_groups(db: &Connection) -> Result<Vec<Group>, String> {
    let mut statement = db
        .prepare(&format!(
            "SELECT {GROUP_COLUMNS} FROM remote_groups g ORDER BY g.is_default DESC, g.created_at"
        ))
        .map_err(err)?;
    let groups = statement
        .query_map([], read_group)
        .map_err(err)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(err)?;
    groups.into_iter().map(|group| fill_group(db, group)).collect()
}

pub fn get_group(db: &Connection, id: &str) -> Result<Option<Group>, String> {
    let group = db
        .query_row(
            &format!("SELECT {GROUP_COLUMNS} FROM remote_groups g WHERE g.id=?1"),
            [id],
            read_group,
        )
        .optional()
        .map_err(err)?;
    group.map(|group| fill_group(db, group)).transpose()
}

pub fn default_group_id(db: &Connection) -> Result<String, String> {
    db.query_row(
        "SELECT id FROM remote_groups WHERE is_default=1 ORDER BY created_at LIMIT 1",
        [],
        |row| row.get(0),
    )
    .optional()
    .map_err(err)
    .map(|id: Option<String>| id.unwrap_or_else(|| DEFAULT_GROUP_ID.to_string()))
}

pub fn save_group(db: &Connection, input: &Group) -> Result<Group, String> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(tr!("分组名称不能为空"));
    }
    let id = if input.id.trim().is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        input.id.clone()
    };
    let tx = db.unchecked_transaction().map_err(err)?;
    tx.execute(
        "INSERT INTO remote_groups
           (id, name, role, all_projects, all_agents, share_in_group, require_approval, max_running, is_default, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9)
         ON CONFLICT(id) DO UPDATE SET
           name=excluded.name, role=excluded.role, all_projects=excluded.all_projects,
           all_agents=excluded.all_agents, share_in_group=excluded.share_in_group,
           require_approval=excluded.require_approval, max_running=excluded.max_running",
        params![
            id,
            name,
            input.role.as_str(),
            input.all_projects as i64,
            input.all_agents as i64,
            input.share_in_group as i64,
            input.require_approval as i64,
            input.max_running.clamp(1, 6) as i64,
            now(),
        ],
    )
    .map_err(err)?;
    tx.execute("DELETE FROM remote_group_projects WHERE group_id=?1", [&id])
        .map_err(err)?;
    for project_id in &input.project_ids {
        tx.execute(
            "INSERT OR IGNORE INTO remote_group_projects (group_id, project_id) VALUES (?1, ?2)",
            params![id, project_id],
        )
        .map_err(err)?;
    }
    tx.execute("DELETE FROM remote_group_agents WHERE group_id=?1", [&id])
        .map_err(err)?;
    for target_id in &input.agent_ids {
        tx.execute(
            "INSERT OR IGNORE INTO remote_group_agents (group_id, target_id) VALUES (?1, ?2)",
            params![id, target_id],
        )
        .map_err(err)?;
    }
    tx.commit().map_err(err)?;
    get_group(db, &id)?.ok_or_else(|| tr!("分组不存在"))
}

pub fn delete_group(db: &Connection, id: &str) -> Result<(), String> {
    let group = get_group(db, id)?.ok_or_else(|| tr!("分组不存在"))?;
    if group.is_default {
        return Err(tr!("默认分组不能删除"));
    }
    if group.device_count > 0 {
        return Err(tr!("请先把组内设备移到其他分组"));
    }
    db.execute("DELETE FROM remote_group_projects WHERE group_id=?1", [id])
        .map_err(err)?;
    db.execute("DELETE FROM remote_group_agents WHERE group_id=?1", [id])
        .map_err(err)?;
    db.execute("DELETE FROM remote_groups WHERE id=?1", [id])
        .map_err(err)?;
    Ok(())
}

pub fn add_project_to_group(db: &Connection, group_id: &str, project_id: &str) -> Result<(), String> {
    db.execute(
        "INSERT OR IGNORE INTO remote_group_projects (group_id, project_id) VALUES (?1, ?2)",
        params![group_id, project_id],
    )
    .map_err(err)?;
    Ok(())
}

pub fn forget_project(db: &Connection, project_id: &str) -> Result<(), String> {
    db.execute(
        "DELETE FROM remote_group_projects WHERE project_id=?1",
        [project_id],
    )
    .map_err(err)?;
    Ok(())
}

fn read_device(row: &rusqlite::Row<'_>) -> rusqlite::Result<Device> {
    Ok(Device {
        id: row.get(0)?,
        name: row.get(1)?,
        owner_note: row.get(2)?,
        fingerprint: row.get(3)?,
        group_id: row.get(4)?,
        paired_at: row.get(5)?,
        last_seen_at: row.get(6)?,
        last_address: row.get(7)?,
    })
}

const DEVICE_COLUMNS: &str =
    "id, name, owner_note, fingerprint, group_id, paired_at, last_seen_at, last_address";

pub fn list_devices(db: &Connection) -> Result<Vec<Device>, String> {
    let mut statement = db
        .prepare(&format!(
            "SELECT {DEVICE_COLUMNS} FROM remote_devices ORDER BY paired_at"
        ))
        .map_err(err)?;
    let rows = statement
        .query_map([], read_device)
        .map_err(err)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(err)?;
    Ok(rows)
}

pub fn device_by_fingerprint(db: &Connection, fingerprint: &str) -> Result<Option<Device>, String> {
    db.query_row(
        &format!("SELECT {DEVICE_COLUMNS} FROM remote_devices WHERE fingerprint=?1"),
        [fingerprint],
        read_device,
    )
    .optional()
    .map_err(err)
}

pub fn get_device(db: &Connection, id: &str) -> Result<Option<Device>, String> {
    db.query_row(
        &format!("SELECT {DEVICE_COLUMNS} FROM remote_devices WHERE id=?1"),
        [id],
        read_device,
    )
    .optional()
    .map_err(err)
}

pub fn insert_device(
    db: &Connection,
    name: &str,
    owner_note: &str,
    fingerprint: &str,
    group_id: &str,
    address: &str,
) -> Result<Device, String> {
    let id = uuid::Uuid::new_v4().to_string();
    db.execute(
        "INSERT INTO remote_devices (id, name, owner_note, fingerprint, group_id, paired_at, last_address)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(fingerprint) DO UPDATE SET
           name=excluded.name, owner_note=excluded.owner_note, group_id=excluded.group_id,
           paired_at=excluded.paired_at, last_address=excluded.last_address",
        params![id, name, owner_note, fingerprint, group_id, now(), address],
    )
    .map_err(err)?;
    device_by_fingerprint(db, fingerprint)?.ok_or_else(|| tr!("设备不存在"))
}

pub fn update_device(
    db: &Connection,
    id: &str,
    name: &str,
    owner_note: &str,
    group_id: &str,
) -> Result<(), String> {
    if get_group(db, group_id)?.is_none() {
        return Err(tr!("分组不存在"));
    }
    db.execute(
        "UPDATE remote_devices SET name=?2, owner_note=?3, group_id=?4 WHERE id=?1",
        params![id, name.trim(), owner_note.trim(), group_id],
    )
    .map_err(err)?;
    Ok(())
}

pub fn touch_device(db: &Connection, id: &str, address: &str) {
    let _ = db.execute(
        "UPDATE remote_devices SET last_seen_at=?2, last_address=?3 WHERE id=?1",
        params![id, now(), address],
    );
}

pub fn delete_device(db: &Connection, id: &str) -> Result<(), String> {
    db.execute("DELETE FROM remote_devices WHERE id=?1", [id])
        .map_err(err)?;
    db.execute(
        "UPDATE remote_permission_requests SET status='cancelled', decided_at=?2
         WHERE device_id=?1 AND status='pending'",
        params![id, now()],
    )
    .map_err(err)?;
    db.execute(
        "UPDATE remote_approvals SET status='rejected', decided_at=?2
         WHERE device_id=?1 AND status IN ('pending','approved')",
        params![id, now()],
    )
    .map_err(err)?;
    Ok(())
}

fn read_host(row: &rusqlite::Row<'_>) -> rusqlite::Result<Host> {
    Ok(Host {
        id: row.get(0)?,
        name: row.get(1)?,
        address: row.get(2)?,
        port: row.get::<_, i64>(3)?.clamp(1, u16::MAX as i64) as u16,
        fingerprint: row.get(4)?,
        created_at: row.get(5)?,
        last_connected_at: row.get(6)?,
    })
}

const HOST_COLUMNS: &str = "id, name, address, port, fingerprint, created_at, last_connected_at";

pub fn list_hosts(db: &Connection) -> Result<Vec<Host>, String> {
    let mut statement = db
        .prepare(&format!("SELECT {HOST_COLUMNS} FROM remote_hosts ORDER BY created_at"))
        .map_err(err)?;
    let rows = statement
        .query_map([], read_host)
        .map_err(err)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(err)?;
    Ok(rows)
}

pub fn get_host(db: &Connection, id: &str) -> Result<Option<Host>, String> {
    db.query_row(
        &format!("SELECT {HOST_COLUMNS} FROM remote_hosts WHERE id=?1"),
        [id],
        read_host,
    )
    .optional()
    .map_err(err)
}

pub fn insert_host(
    db: &Connection,
    name: &str,
    address: &str,
    port: u16,
    fingerprint: &str,
) -> Result<Host, String> {
    let existing: Option<String> = db
        .query_row(
            "SELECT id FROM remote_hosts WHERE fingerprint=?1",
            [fingerprint],
            |row| row.get(0),
        )
        .optional()
        .map_err(err)?;
    let id = existing.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    db.execute(
        "INSERT INTO remote_hosts (id, name, address, port, fingerprint, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(id) DO UPDATE SET name=excluded.name, address=excluded.address, port=excluded.port",
        params![id, name, address, port as i64, fingerprint, now()],
    )
    .map_err(err)?;
    get_host(db, &id)?.ok_or_else(|| tr!("主机不存在"))
}

pub fn update_host(db: &Connection, id: &str, name: &str, address: &str, port: u16) -> Result<(), String> {
    db.execute(
        "UPDATE remote_hosts SET name=?2, address=?3, port=?4 WHERE id=?1",
        params![id, name.trim(), address.trim(), port as i64],
    )
    .map_err(err)?;
    Ok(())
}

pub fn touch_host(db: &Connection, id: &str) {
    let _ = db.execute(
        "UPDATE remote_hosts SET last_connected_at=?2 WHERE id=?1",
        params![id, now()],
    );
}

pub fn delete_host(db: &Connection, id: &str) -> Result<(), String> {
    db.execute("DELETE FROM remote_hosts WHERE id=?1", [id])
        .map_err(err)?;
    Ok(())
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRequest {
    pub id: String,
    pub device_id: String,
    pub device_name: String,
    pub permission: String,
    pub project_id: Option<String>,
    pub note: String,
    pub status: String,
    pub created_at: String,
    pub decided_at: Option<String>,
}

pub fn insert_request(
    db: &Connection,
    device_id: &str,
    permission: &str,
    project_id: Option<&str>,
    note: &str,
) -> Result<PermissionRequest, String> {
    let existing: Option<String> = db
        .query_row(
            "SELECT id FROM remote_permission_requests
             WHERE device_id=?1 AND permission=?2 AND COALESCE(project_id,'')=COALESCE(?3,'') AND status='pending'",
            params![device_id, permission, project_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(err)?;
    let id = match existing {
        Some(id) => {
            db.execute(
                "UPDATE remote_permission_requests SET note=?2 WHERE id=?1",
                params![id, note],
            )
            .map_err(err)?;
            id
        }
        None => {
            let id = uuid::Uuid::new_v4().to_string();
            db.execute(
                "INSERT INTO remote_permission_requests (id, device_id, permission, project_id, note, status, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6)",
                params![id, device_id, permission, project_id, note, now()],
            )
            .map_err(err)?;
            id
        }
    };
    get_request(db, &id)?.ok_or_else(|| tr!("申请不存在"))
}

const REQUEST_SELECT: &str = "SELECT r.id, r.device_id, COALESCE(d.name, ''), r.permission, r.project_id,
    r.note, r.status, r.created_at, r.decided_at
    FROM remote_permission_requests r LEFT JOIN remote_devices d ON d.id = r.device_id";

fn read_request(row: &rusqlite::Row<'_>) -> rusqlite::Result<PermissionRequest> {
    Ok(PermissionRequest {
        id: row.get(0)?,
        device_id: row.get(1)?,
        device_name: row.get(2)?,
        permission: row.get(3)?,
        project_id: row.get(4)?,
        note: row.get(5)?,
        status: row.get(6)?,
        created_at: row.get(7)?,
        decided_at: row.get(8)?,
    })
}

pub fn get_request(db: &Connection, id: &str) -> Result<Option<PermissionRequest>, String> {
    db.query_row(&format!("{REQUEST_SELECT} WHERE r.id=?1"), [id], read_request)
        .optional()
        .map_err(err)
}

pub fn list_requests(db: &Connection, device_id: Option<&str>) -> Result<Vec<PermissionRequest>, String> {
    let mut statement = db
        .prepare(&format!(
            "{REQUEST_SELECT} WHERE (?1 IS NULL OR r.device_id=?1) ORDER BY r.created_at DESC LIMIT 200"
        ))
        .map_err(err)?;
    let rows = statement
        .query_map([device_id], read_request)
        .map_err(err)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(err)?;
    Ok(rows)
}

pub fn set_request_status(db: &Connection, id: &str, status: &str) -> Result<(), String> {
    let changed = db
        .execute(
            "UPDATE remote_permission_requests SET status=?2, decided_at=?3 WHERE id=?1 AND status='pending'",
            params![id, status, now()],
        )
        .map_err(err)?;
    if changed == 0 {
        return Err(tr!("申请已处理"));
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Approval {
    pub id: String,
    pub device_id: String,
    pub device_name: String,
    pub cmd: String,
    pub args_hash: String,
    pub summary: String,
    pub status: String,
    pub created_at: String,
    pub expires_at: String,
    pub decided_at: Option<String>,
}

const APPROVAL_SELECT: &str = "SELECT a.id, a.device_id, COALESCE(d.name, ''), a.cmd, a.args_hash, a.summary,
    a.status, a.created_at, a.expires_at, a.decided_at
    FROM remote_approvals a LEFT JOIN remote_devices d ON d.id = a.device_id";

fn read_approval(row: &rusqlite::Row<'_>) -> rusqlite::Result<Approval> {
    Ok(Approval {
        id: row.get(0)?,
        device_id: row.get(1)?,
        device_name: row.get(2)?,
        cmd: row.get(3)?,
        args_hash: row.get(4)?,
        summary: row.get(5)?,
        status: row.get(6)?,
        created_at: row.get(7)?,
        expires_at: row.get(8)?,
        decided_at: row.get(9)?,
    })
}

pub fn expire_approvals(db: &Connection) -> Vec<Approval> {
    let current = now();
    let expiring = db
        .prepare(&format!(
            "{APPROVAL_SELECT} WHERE a.status IN ('pending','approved') AND a.expires_at < ?1"
        ))
        .and_then(|mut statement| {
            statement
                .query_map([&current], read_approval)?
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .unwrap_or_default();
    let _ = db.execute(
        "UPDATE remote_approvals SET status='expired' WHERE status IN ('pending','approved') AND expires_at < ?1",
        [&current],
    );
    expiring
}

pub fn get_approval(db: &Connection, id: &str) -> Result<Option<Approval>, String> {
    db.query_row(&format!("{APPROVAL_SELECT} WHERE a.id=?1"), [id], read_approval)
        .optional()
        .map_err(err)
}

pub fn insert_approval(
    db: &Connection,
    device_id: &str,
    cmd: &str,
    args_hash: &str,
    summary: &str,
    ttl_minutes: i64,
) -> Result<Approval, String> {
    expire_approvals(db);
    let existing: Option<String> = db
        .query_row(
            "SELECT id FROM remote_approvals
             WHERE device_id=?1 AND cmd=?2 AND args_hash=?3 AND status IN ('pending','approved')",
            params![device_id, cmd, args_hash],
            |row| row.get(0),
        )
        .optional()
        .map_err(err)?;
    let id = match existing {
        Some(id) => id,
        None => {
            let id = uuid::Uuid::new_v4().to_string();
            let expires = (Utc::now() + ChronoDuration::minutes(ttl_minutes)).to_rfc3339();
            db.execute(
                "INSERT INTO remote_approvals (id, device_id, cmd, args_hash, summary, status, created_at, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6, ?7)",
                params![id, device_id, cmd, args_hash, summary, now(), expires],
            )
            .map_err(err)?;
            id
        }
    };
    get_approval(db, &id)?.ok_or_else(|| tr!("审批不存在"))
}

pub fn list_approvals(db: &Connection, pending_only: bool) -> Result<Vec<Approval>, String> {
    expire_approvals(db);
    let mut statement = db
        .prepare(&format!(
            "{APPROVAL_SELECT} WHERE (?1 = 0 OR a.status='pending') ORDER BY a.created_at DESC LIMIT 200"
        ))
        .map_err(err)?;
    let rows = statement
        .query_map([pending_only as i64], read_approval)
        .map_err(err)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(err)?;
    Ok(rows)
}

pub fn decide_approval(db: &Connection, id: &str, approved: bool) -> Result<Approval, String> {
    expire_approvals(db);
    let status = if approved { "approved" } else { "rejected" };
    let changed = db
        .execute(
            "UPDATE remote_approvals SET status=?2, decided_at=?3 WHERE id=?1 AND status='pending'",
            params![id, status, now()],
        )
        .map_err(err)?;
    if changed == 0 {
        return Err(tr!("审批已处理或已过期"));
    }
    get_approval(db, id)?.ok_or_else(|| tr!("审批不存在"))
}

/// 校验并消耗一张审批票据：设备、命令、参数都一致，且已批准、未过期、未使用。
pub fn consume_approval(
    db: &Connection,
    id: &str,
    device_id: &str,
    cmd: &str,
    args_hash: &str,
) -> Result<(), String> {
    expire_approvals(db);
    let changed = db
        .execute(
            "UPDATE remote_approvals SET status='used'
             WHERE id=?1 AND device_id=?2 AND cmd=?3 AND args_hash=?4 AND status='approved'",
            params![id, device_id, cmd, args_hash],
        )
        .map_err(err)?;
    if changed == 0 {
        return Err(tr!("审批无效、已过期或参数已变化"));
    }
    Ok(())
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    pub id: i64,
    pub at: String,
    pub device_id: Option<String>,
    pub device_name: String,
    pub group_id: Option<String>,
    pub action: String,
    pub cmd: Option<String>,
    pub resource_kind: Option<String>,
    pub resource_id: Option<String>,
    pub outcome: String,
    pub detail: serde_json::Value,
}

pub fn insert_audit(db: &Connection, entry: &AuditEntry) {
    let _ = db.execute(
        "INSERT INTO remote_audit_log
           (at, device_id, device_name, group_id, action, cmd, resource_kind, resource_id, outcome, detail_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            now(),
            entry.device_id,
            entry.device_name,
            entry.group_id,
            entry.action,
            entry.cmd,
            entry.resource_kind,
            entry.resource_id,
            entry.outcome,
            entry.detail.to_string(),
        ],
    );
}

/// 记录一条设备相关审计，设备名和分组按当前值快照下来（设备移除后仍可读）。
pub fn audit_device(
    db: &Connection,
    device_id: &str,
    action: &str,
    cmd: Option<&str>,
    resource: Option<(&str, &str)>,
    outcome: &str,
    detail: serde_json::Value,
) {
    let device = get_device(db, device_id).ok().flatten();
    insert_audit(
        db,
        &AuditEntry {
            device_id: Some(device_id.to_string()),
            device_name: device.as_ref().map(|device| device.name.clone()).unwrap_or_default(),
            group_id: device.map(|device| device.group_id),
            action: action.to_string(),
            cmd: cmd.map(str::to_string),
            resource_kind: resource.map(|(kind, _)| kind.to_string()),
            resource_id: resource.map(|(_, id)| id.to_string()),
            outcome: outcome.to_string(),
            detail,
            ..AuditEntry::default()
        },
    );
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AuditQuery {
    pub device_id: Option<String>,
    pub group_id: Option<String>,
    pub action: Option<String>,
    pub outcome: Option<String>,
    pub before_id: Option<i64>,
    pub limit: Option<u32>,
}

pub fn query_audit(db: &Connection, query: &AuditQuery) -> Result<Vec<AuditEntry>, String> {
    let limit = query.limit.unwrap_or(100).clamp(1, 500) as i64;
    let mut statement = db
        .prepare(
            "SELECT id, at, device_id, device_name, group_id, action, cmd, resource_kind, resource_id, outcome, detail_json
             FROM remote_audit_log
             WHERE (?1 IS NULL OR device_id=?1) AND (?2 IS NULL OR group_id=?2)
               AND (?3 IS NULL OR action=?3) AND (?4 IS NULL OR outcome=?4)
               AND (?5 IS NULL OR id < ?5)
             ORDER BY id DESC LIMIT ?6",
        )
        .map_err(err)?;
    let rows = statement
        .query_map(
            params![
                query.device_id,
                query.group_id,
                query.action,
                query.outcome,
                query.before_id,
                limit
            ],
            |row| {
                let detail: String = row.get(10)?;
                Ok(AuditEntry {
                    id: row.get(0)?,
                    at: row.get(1)?,
                    device_id: row.get(2)?,
                    device_name: row.get(3)?,
                    group_id: row.get(4)?,
                    action: row.get(5)?,
                    cmd: row.get(6)?,
                    resource_kind: row.get(7)?,
                    resource_id: row.get(8)?,
                    outcome: row.get(9)?,
                    detail: serde_json::from_str(&detail).unwrap_or(serde_json::Value::Null),
                })
            },
        )
        .map_err(err)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(err)?;
    Ok(rows)
}

pub fn prune_audit(db: &Connection) {
    let cutoff = (Utc::now() - ChronoDuration::days(AUDIT_RETENTION_DAYS)).to_rfc3339();
    let _ = db.execute("DELETE FROM remote_audit_log WHERE at < ?1", [cutoff]);
    let _ = db.execute(
        "DELETE FROM remote_audit_log WHERE id <= (SELECT MAX(id) FROM remote_audit_log) - ?1",
        [AUDIT_MAX_ROWS],
    );
}

pub fn setting(db: &Connection, key: &str) -> Option<String> {
    db.query_row("SELECT value FROM app_settings WHERE key=?1", [key], |row| {
        row.get(0)
    })
    .optional()
    .ok()
    .flatten()
}

pub fn set_setting(db: &Connection, key: &str, value: &str) -> Result<(), String> {
    db.execute(
        "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![key, value],
    )
    .map_err(err)?;
    Ok(())
}

pub fn host_port(db: &Connection) -> u16 {
    setting(db, HOST_PORT_KEY)
        .and_then(|value| value.parse().ok())
        .filter(|port: &u16| *port > 0)
        .unwrap_or(DEFAULT_PORT)
}

pub fn host_enabled(db: &Connection) -> bool {
    setting(db, HOST_ENABLED_KEY).as_deref() == Some("true")
}
