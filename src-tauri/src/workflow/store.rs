//! 工作流的数据库读写。表结构见 database.rs `migrate_workflows`。

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Serialize;
use serde_json::Value;

use super::model::WorkflowDef;

fn err(error: rusqlite::Error) -> String {
    error.to_string()
}

// ---------- 工作流 ----------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Workflow {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub icon: String,
    pub published_version: Option<u32>,
    pub enabled: bool,
    pub draft_updated_at: String,
    pub created_at: String,
    pub updated_at: String,
    /// 草稿与最新发布版本不同
    pub has_unpublished_changes: bool,
}

const WORKFLOW_COLUMNS: &str = "w.id, w.name, w.slug, w.description, w.icon, w.published_version,
    w.enabled, w.draft_updated_at, w.created_at, w.updated_at,
    COALESCE((SELECT v.definition_hash FROM workflow_versions v
              WHERE v.workflow_id=w.id AND v.version=w.published_version), '') != w.draft_hash";

fn map_workflow(row: &Row) -> rusqlite::Result<Workflow> {
    Ok(Workflow {
        id: row.get(0)?,
        name: row.get(1)?,
        slug: row.get(2)?,
        description: row.get(3)?,
        icon: row.get(4)?,
        published_version: row.get(5)?,
        enabled: row.get::<_, i64>(6)? != 0,
        draft_updated_at: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        has_unpublished_changes: row.get(10)?,
    })
}

pub fn hash_definition(def: &WorkflowDef) -> String {
    use sha2::{Digest, Sha256};
    let raw = serde_json::to_string(def).unwrap_or_default();
    let digest = Sha256::digest(raw.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn list_workflows(db: &Connection) -> Result<Vec<Workflow>, String> {
    let mut statement = db
        .prepare(&format!(
            "SELECT {WORKFLOW_COLUMNS} FROM workflows w ORDER BY w.created_at ASC, w.rowid ASC"
        ))
        .map_err(err)?;
    let rows = statement
        .query_map([], map_workflow)
        .map_err(err)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(err)?;
    Ok(rows)
}

pub fn linked_task_ids(db: &Connection, id: &str) -> Result<Vec<String>, String> {
    let raw: String = db
        .query_row("SELECT COALESCE(linked_task_ids, '[]') FROM workflows WHERE id=?1", [id], |row| row.get(0))
        .map_err(err)?;
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

pub fn set_linked_task_ids(db: &Connection, id: &str, ids: &[String]) -> Result<(), String> {
    let raw = serde_json::to_string(ids).map_err(|error| error.to_string())?;
    db.execute("UPDATE workflows SET linked_task_ids=?1 WHERE id=?2", params![raw, id])
        .map_err(err)?;
    Ok(())
}

pub fn workflow(db: &Connection, id: &str) -> Result<Workflow, String> {
    db.query_row(
        &format!("SELECT {WORKFLOW_COLUMNS} FROM workflows w WHERE w.id=?1"),
        [id],
        map_workflow,
    )
    .optional()
    .map_err(err)?
    .ok_or_else(|| tr!("工作流不存在"))
}

pub fn draft(db: &Connection, id: &str) -> Result<WorkflowDef, String> {
    let raw: String = db
        .query_row("SELECT draft_json FROM workflows WHERE id=?1", [id], |row| {
            row.get(0)
        })
        .optional()
        .map_err(err)?
        .ok_or_else(|| tr!("工作流不存在"))?;
    serde_json::from_str(&raw).map_err(|error| tr!("工作流定义无法解析：{error}", error = error))
}

pub fn create_workflow(
    db: &Connection,
    name: &str,
    description: &str,
    def: &WorkflowDef,
    now: &str,
) -> Result<Workflow, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(tr!("工作流名称不能为空"));
    }
    let id = uuid::Uuid::new_v4().to_string();
    let base = super::model::slugify(name);
    let mut slug = base.clone();
    let mut n = 2;
    while db
        .query_row("SELECT EXISTS(SELECT 1 FROM workflows WHERE slug=?1)", [&slug], |row| {
            row.get::<_, bool>(0)
        })
        .map_err(err)?
    {
        slug = format!("{base}-{n}");
        n += 1;
    }
    let raw = serde_json::to_string(def).map_err(|error| error.to_string())?;
    db.execute(
        "INSERT INTO workflows
         (id, name, slug, description, icon, draft_json, draft_hash, draft_updated_at, enabled, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, '', ?5, ?6, ?7, 1, ?7, ?7)",
        params![id, name, slug, description.trim(), raw, hash_definition(def), now],
    )
    .map_err(err)?;
    workflow(db, &id)
}

pub fn update_meta(
    db: &Connection,
    id: &str,
    name: &str,
    description: &str,
    icon: &str,
    now: &str,
) -> Result<Workflow, String> {
    if name.trim().is_empty() {
        return Err(tr!("工作流名称不能为空"));
    }
    db.execute(
        "UPDATE workflows SET name=?1, description=?2, icon=?3, updated_at=?4 WHERE id=?5",
        params![name.trim(), description.trim(), icon, now, id],
    )
    .map_err(err)?;
    workflow(db, id)
}

pub fn set_enabled(db: &Connection, id: &str, enabled: bool, now: &str) -> Result<(), String> {
    db.execute(
        "UPDATE workflows SET enabled=?1, updated_at=?2 WHERE id=?3",
        params![enabled as i64, now, id],
    )
    .map_err(err)?;
    Ok(())
}

/// 乐观锁：expected 与库里的 draft_updated_at 不一致时拒绝
pub fn save_draft(
    db: &Connection,
    id: &str,
    def: &WorkflowDef,
    expected_updated_at: Option<&str>,
    now: &str,
) -> Result<String, String> {
    let current: String = db
        .query_row(
            "SELECT draft_updated_at FROM workflows WHERE id=?1",
            [id],
            |row| row.get(0),
        )
        .optional()
        .map_err(err)?
        .ok_or_else(|| tr!("工作流不存在"))?;
    if let Some(expected) = expected_updated_at {
        if expected != current {
            return Err(format!("[conflict] {}", tr!("工作流已在别处修改，请刷新后再保存")));
        }
    }
    let raw = serde_json::to_string(def).map_err(|error| error.to_string())?;
    // 同一毫秒内连续保存时也要让时间戳前进，否则乐观锁分不出先后
    let stamp = if now <= current.as_str() {
        bump_stamp(&current)
    } else {
        now.to_string()
    };
    db.execute(
        "UPDATE workflows SET draft_json=?1, draft_hash=?2, draft_updated_at=?3, updated_at=?3 WHERE id=?4",
        params![raw, hash_definition(def), stamp, id],
    )
    .map_err(err)?;
    Ok(stamp)
}

fn bump_stamp(current: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(current)
        .map(|time| (time + chrono::Duration::milliseconds(1)).to_rfc3339())
        .unwrap_or_else(|_| format!("{current}+"))
}

pub fn publish(db: &Connection, id: &str, note: &str, now: &str) -> Result<u32, String> {
    let def = draft(db, id)?;
    let next: u32 = db
        .query_row(
            "SELECT COALESCE(MAX(version), 0) + 1 FROM workflow_versions WHERE workflow_id=?1 AND version > 0",
            [id],
            |row| row.get(0),
        )
        .map_err(err)?;
    insert_version(db, id, next, &def, note, now)?;
    db.execute(
        "UPDATE workflows SET published_version=?1, updated_at=?2 WHERE id=?3",
        params![next, now, id],
    )
    .map_err(err)?;
    Ok(next)
}

/// version 0：草稿试运行用的临时版本，每次覆盖
pub fn snapshot_draft_version(db: &Connection, id: &str, now: &str) -> Result<(), String> {
    let def = draft(db, id)?;
    db.execute(
        "DELETE FROM workflow_versions WHERE workflow_id=?1 AND version=0",
        [id],
    )
    .map_err(err)?;
    insert_version(db, id, 0, &def, "draft", now)
}

fn insert_version(
    db: &Connection,
    id: &str,
    version: u32,
    def: &WorkflowDef,
    note: &str,
    now: &str,
) -> Result<(), String> {
    let raw = serde_json::to_string(def).map_err(|error| error.to_string())?;
    db.execute(
        "INSERT INTO workflow_versions (workflow_id, version, definition_json, definition_hash, note, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, version, raw, hash_definition(def), note.trim(), now],
    )
    .map_err(err)?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionMeta {
    pub version: u32,
    pub note: String,
    pub created_at: String,
}

pub fn versions(db: &Connection, id: &str) -> Result<Vec<VersionMeta>, String> {
    let mut statement = db
        .prepare(
            "SELECT version, note, created_at FROM workflow_versions
             WHERE workflow_id=?1 AND version > 0 ORDER BY version DESC",
        )
        .map_err(err)?;
    let rows = statement
        .query_map([id], |row| {
            Ok(VersionMeta {
                version: row.get(0)?,
                note: row.get(1)?,
                created_at: row.get(2)?,
            })
        })
        .map_err(err)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(err)?;
    Ok(rows)
}

pub fn definition(db: &Connection, id: &str, version: u32) -> Result<WorkflowDef, String> {
    let raw: String = db
        .query_row(
            "SELECT definition_json FROM workflow_versions WHERE workflow_id=?1 AND version=?2",
            params![id, version],
            |row| row.get(0),
        )
        .optional()
        .map_err(err)?
        .ok_or_else(|| tr!("工作流版本不存在"))?;
    serde_json::from_str(&raw).map_err(|error| tr!("工作流定义无法解析：{error}", error = error))
}

pub fn latest_version(db: &Connection, id: &str) -> Result<Option<u32>, String> {
    db.query_row(
        "SELECT published_version FROM workflows WHERE id=?1",
        [id],
        |row| row.get(0),
    )
    .optional()
    .map_err(err)
    .map(Option::flatten)
}

pub fn delete_workflow(db: &Connection, id: &str) -> Result<(), String> {
    db.execute("DELETE FROM workflows WHERE id=?1", [id])
        .map_err(err)?;
    Ok(())
}

// ---------- 运行 ----------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub id: String,
    pub workflow_id: String,
    pub version: u32,
    pub status: String,
    pub inputs: Value,
    pub outputs: Value,
    pub trigger_source: String,
    pub trigger_ref: Option<String>,
    pub parent_node_run_id: Option<String>,
    pub map_index: Option<i64>,
    pub extra: Value,
    pub overview_session_id: String,
    pub task_id: Option<String>,
    pub error_code: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

impl Run {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status.as_str(),
            "succeeded" | "failed" | "cancelled" | "skipped"
        )
    }

    pub fn short_id(&self) -> String {
        self.id.chars().filter(|c| c.is_ascii_alphanumeric()).take(6).collect()
    }
}

const RUN_COLUMNS: &str = "id, workflow_id, version, status, inputs_json, outputs_json, trigger_source,
    trigger_ref, parent_node_run_id, map_index, extra_json, overview_session_id, task_id, error_code,
    error, created_at, started_at, completed_at";

fn json_col(raw: Option<String>) -> Value {
    raw.and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or(Value::Null)
}

fn map_run(row: &Row) -> rusqlite::Result<Run> {
    Ok(Run {
        id: row.get(0)?,
        workflow_id: row.get(1)?,
        version: row.get(2)?,
        status: row.get(3)?,
        inputs: json_col(row.get(4)?),
        outputs: json_col(row.get(5)?),
        trigger_source: row.get(6)?,
        trigger_ref: row.get(7)?,
        parent_node_run_id: row.get(8)?,
        map_index: row.get(9)?,
        extra: json_col(row.get(10)?),
        overview_session_id: row.get(11)?,
        task_id: row.get(12)?,
        error_code: row.get(13)?,
        error: row.get(14)?,
        created_at: row.get(15)?,
        started_at: row.get(16)?,
        completed_at: row.get(17)?,
    })
}

pub fn run(db: &Connection, id: &str) -> Result<Run, String> {
    db.query_row(
        &format!("SELECT {RUN_COLUMNS} FROM workflow_runs WHERE id=?1"),
        [id],
        map_run,
    )
    .optional()
    .map_err(err)?
    .ok_or_else(|| tr!("工作流运行不存在"))
}

pub fn run_by_overview_session(db: &Connection, session_id: &str) -> Result<Option<Run>, String> {
    db.query_row(
        &format!("SELECT {RUN_COLUMNS} FROM workflow_runs WHERE overview_session_id=?1"),
        [session_id],
        map_run,
    )
    .optional()
    .map_err(err)
}

pub fn list_runs(
    db: &Connection,
    workflow_id: Option<&str>,
    status: Option<&str>,
    limit: u32,
) -> Result<Vec<Run>, String> {
    let mut statement = db
        .prepare(&format!(
            "SELECT {RUN_COLUMNS} FROM workflow_runs
             WHERE (?1 IS NULL OR workflow_id=?1) AND (?2 IS NULL OR status=?2)
               AND parent_node_run_id IS NULL
             ORDER BY created_at DESC LIMIT ?3"
        ))
        .map_err(err)?;
    let rows = statement
        .query_map(params![workflow_id, status, limit.clamp(1, 500)], map_run)
        .map_err(err)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(err)?;
    Ok(rows)
}

pub fn child_runs(db: &Connection, parent_node_run_id: &str) -> Result<Vec<Run>, String> {
    let mut statement = db
        .prepare(&format!(
            "SELECT {RUN_COLUMNS} FROM workflow_runs WHERE parent_node_run_id=?1
             ORDER BY COALESCE(map_index, 0), created_at"
        ))
        .map_err(err)?;
    let rows = statement
        .query_map([parent_node_run_id], map_run)
        .map_err(err)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(err)?;
    Ok(rows)
}

pub fn active_run_ids(db: &Connection) -> Result<Vec<String>, String> {
    let mut statement = db
        .prepare("SELECT id FROM workflow_runs WHERE status IN ('running', 'waiting') ORDER BY created_at")
        .map_err(err)?;
    let rows = statement
        .query_map([], |row| row.get(0))
        .map_err(err)?
        .collect::<rusqlite::Result<Vec<String>>>()
        .map_err(err)?;
    Ok(rows)
}

pub struct NewRun<'a> {
    pub id: &'a str,
    pub workflow_id: &'a str,
    pub version: u32,
    pub status: &'a str,
    pub inputs: &'a Value,
    pub trigger_source: &'a str,
    pub trigger_ref: Option<&'a str>,
    pub parent_node_run_id: Option<&'a str>,
    pub map_index: Option<i64>,
    pub extra: &'a Value,
    pub overview_session_id: &'a str,
    pub now: &'a str,
}

pub fn insert_run(db: &Connection, run: NewRun<'_>) -> Result<(), String> {
    db.execute(
        "INSERT INTO workflow_runs
         (id, workflow_id, version, status, inputs_json, outputs_json, trigger_source, trigger_ref,
          parent_node_run_id, map_index, extra_json, overview_session_id, created_at, started_at)
         VALUES (?1, ?2, ?3, ?4, ?5, '{}', ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                 CASE WHEN ?4='running' THEN ?12 END)",
        params![
            run.id,
            run.workflow_id,
            run.version,
            run.status,
            run.inputs.to_string(),
            run.trigger_source,
            run.trigger_ref,
            run.parent_node_run_id,
            run.map_index,
            run.extra.to_string(),
            run.overview_session_id,
            run.now
        ],
    )
    .map_err(err)?;
    Ok(())
}

pub fn set_run_status(
    db: &Connection,
    id: &str,
    status: &str,
    error_code: Option<&str>,
    error: Option<&str>,
    now: &str,
) -> Result<(), String> {
    let terminal = matches!(status, "succeeded" | "failed" | "cancelled" | "skipped");
    db.execute(
        "UPDATE workflow_runs SET status=?1,
           error_code=COALESCE(?2, error_code), error=COALESCE(?3, error),
           started_at=CASE WHEN ?1 IN ('running','waiting') THEN COALESCE(started_at, ?4) ELSE started_at END,
           completed_at=CASE WHEN ?5 THEN ?4 ELSE NULL END
         WHERE id=?6",
        params![status, error_code, error, now, terminal, id],
    )
    .map_err(err)?;
    Ok(())
}

pub fn set_run_outputs(db: &Connection, id: &str, outputs: &Value) -> Result<(), String> {
    db.execute(
        "UPDATE workflow_runs SET outputs_json=?1 WHERE id=?2",
        params![outputs.to_string(), id],
    )
    .map_err(err)?;
    Ok(())
}

pub fn set_run_task(db: &Connection, id: &str, task_id: &str) -> Result<(), String> {
    db.execute(
        "UPDATE workflow_runs SET task_id=?1 WHERE id=?2",
        params![task_id, id],
    )
    .map_err(err)?;
    Ok(())
}

pub fn delete_run(db: &Connection, id: &str) -> Result<(), String> {
    for child in child_runs_all(db, id)? {
        delete_run(db, &child)?;
    }
    db.execute("DELETE FROM workflow_runs WHERE id=?1", [id])
        .map_err(err)?;
    Ok(())
}

fn child_runs_all(db: &Connection, run_id: &str) -> Result<Vec<String>, String> {
    let mut statement = db
        .prepare(
            "SELECT r.id FROM workflow_runs r
             JOIN workflow_node_runs n ON n.id = r.parent_node_run_id
             WHERE n.run_id=?1",
        )
        .map_err(err)?;
    let rows = statement
        .query_map([run_id], |row| row.get(0))
        .map_err(err)?
        .collect::<rusqlite::Result<Vec<String>>>()
        .map_err(err)?;
    Ok(rows)
}

// ---------- 节点运行 ----------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeRun {
    pub id: String,
    pub run_id: String,
    pub node_id: String,
    pub kind: String,
    pub iteration: i64,
    pub attempt: i64,
    pub status: String,
    pub inputs: Value,
    pub fired_ports: Vec<String>,
    pub carried: Value,
    pub state: Value,
    pub session_id: Option<String>,
    pub repair_count: i64,
    pub workspace_before: Value,
    pub workspace_after: Value,
    pub fork_workspace_id: Option<String>,
    pub skip_reason: Option<String>,
    pub error_code: Option<String>,
    pub error: Option<String>,
    pub retry_at: Option<String>,
    pub deadline_at: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub content: String,
    pub metadata: Value,
    pub next_node_id: Option<String>,
}

impl NodeRun {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status.as_str(),
            "succeeded" | "failed" | "skipped" | "cancelled" | "reused"
        )
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self.status.as_str(),
            "running" | "waiting_human" | "waiting_children"
        )
    }
}

const NODE_COLUMNS: &str = "id, run_id, node_id, kind, iteration, attempt, status, inputs_json,
    fired_ports_json, carried_json, state_json, session_id, repair_count, workspace_before_json,
    workspace_after_json, fork_workspace_id, skip_reason, error_code, error, retry_at, deadline_at,
    created_at, started_at, completed_at, content, metadata_json, next_node_id";

fn map_node_run(row: &Row) -> rusqlite::Result<NodeRun> {
    let fired: Option<String> = row.get(8)?;
    Ok(NodeRun {
        id: row.get(0)?,
        run_id: row.get(1)?,
        node_id: row.get(2)?,
        kind: row.get(3)?,
        iteration: row.get(4)?,
        attempt: row.get(5)?,
        status: row.get(6)?,
        inputs: json_col(row.get(7)?),
        fired_ports: fired
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default(),
        carried: json_col(row.get(9)?),
        state: json_col(row.get(10)?),
        session_id: row.get(11)?,
        repair_count: row.get(12)?,
        workspace_before: json_col(row.get(13)?),
        workspace_after: json_col(row.get(14)?),
        fork_workspace_id: row.get(15)?,
        skip_reason: row.get(16)?,
        error_code: row.get(17)?,
        error: row.get(18)?,
        retry_at: row.get(19)?,
        deadline_at: row.get(20)?,
        created_at: row.get(21)?,
        started_at: row.get(22)?,
        completed_at: row.get(23)?,
        content: row.get::<_, Option<String>>(24)?.unwrap_or_default(),
        metadata: json_col(row.get(25)?),
        next_node_id: row.get(26)?,
    })
}

pub fn node_run(db: &Connection, id: &str) -> Result<NodeRun, String> {
    db.query_row(
        &format!("SELECT {NODE_COLUMNS} FROM workflow_node_runs WHERE id=?1"),
        [id],
        map_node_run,
    )
    .optional()
    .map_err(err)?
    .ok_or_else(|| tr!("节点运行不存在"))
}

pub fn node_runs(db: &Connection, run_id: &str) -> Result<Vec<NodeRun>, String> {
    let mut statement = db
        .prepare(&format!(
            "SELECT {NODE_COLUMNS} FROM workflow_node_runs WHERE run_id=?1
             ORDER BY created_at, iteration, attempt"
        ))
        .map_err(err)?;
    let rows = statement
        .query_map([run_id], map_node_run)
        .map_err(err)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(err)?;
    Ok(rows)
}

/// 每个节点的当前节点运行（iteration 最大，其次 attempt 最大）
pub fn current_node_runs(db: &Connection, run_id: &str) -> Result<HashMap<String, NodeRun>, String> {
    let mut map: HashMap<String, NodeRun> = HashMap::new();
    for item in node_runs(db, run_id)? {
        let replace = match map.get(&item.node_id) {
            None => true,
            Some(existing) => (item.iteration, item.attempt) > (existing.iteration, existing.attempt),
        };
        if replace {
            map.insert(item.node_id.clone(), item);
        }
    }
    Ok(map)
}

/// 会话属于哪个节点运行（取最新、仍在进行的那条；没有就取最新一条）
pub fn node_run_by_session(db: &Connection, session_id: &str) -> Result<Option<NodeRun>, String> {
    db.query_row(
        &format!(
            "SELECT {NODE_COLUMNS} FROM workflow_node_runs WHERE session_id=?1
             ORDER BY CASE WHEN status IN ('running','waiting_human') THEN 0 ELSE 1 END,
                      created_at DESC LIMIT 1"
        ),
        [session_id],
        map_node_run,
    )
    .optional()
    .map_err(err)
}

pub struct NewNodeRun<'a> {
    pub run_id: &'a str,
    pub node_id: &'a str,
    pub kind: &'a str,
    pub iteration: i64,
    pub attempt: i64,
    pub carried: Option<&'a Value>,
    pub retry_at: Option<&'a str>,
    pub now: &'a str,
}

pub fn insert_node_run(db: &Connection, item: NewNodeRun<'_>) -> Result<String, String> {
    let id = uuid::Uuid::new_v4().to_string();
    db.execute(
        "INSERT INTO workflow_node_runs
         (id, run_id, node_id, kind, iteration, attempt, status, carried_json, retry_at, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending', ?7, ?8, ?9)",
        params![
            id,
            item.run_id,
            item.node_id,
            item.kind,
            item.iteration,
            item.attempt,
            item.carried.map(Value::to_string),
            item.retry_at,
            item.now
        ],
    )
    .map_err(err)?;
    Ok(id)
}

pub fn set_node_status(db: &Connection, id: &str, status: &str, now: &str) -> Result<(), String> {
    let terminal = matches!(status, "succeeded" | "failed" | "skipped" | "cancelled" | "reused");
    db.execute(
        "UPDATE workflow_node_runs SET status=?1,
           started_at=CASE WHEN ?1 IN ('running','waiting_human','waiting_children') THEN COALESCE(started_at, ?2) ELSE started_at END,
           completed_at=CASE WHEN ?3 THEN COALESCE(completed_at, ?2) ELSE completed_at END
         WHERE id=?4",
        params![status, now, terminal, id],
    )
    .map_err(err)?;
    Ok(())
}

pub fn set_node_field(db: &Connection, id: &str, column: &str, value: Option<&str>) -> Result<(), String> {
    const ALLOWED: &[&str] = &[
        "inputs_json",
        "fired_ports_json",
        "carried_json",
        "state_json",
        "session_id",
        "workspace_before_json",
        "workspace_after_json",
        "fork_workspace_id",
        "skip_reason",
        "error_code",
        "error",
        "retry_at",
        "deadline_at",
        "content",
        "metadata_json",
        "next_node_id",
    ];
    if !ALLOWED.contains(&column) {
        return Err(format!("unknown column {column}"));
    }
    db.execute(
        &format!("UPDATE workflow_node_runs SET {column}=?1 WHERE id=?2"),
        params![value, id],
    )
    .map_err(err)?;
    Ok(())
}

pub fn bump_repair(db: &Connection, id: &str) -> Result<(), String> {
    db.execute(
        "UPDATE workflow_node_runs SET repair_count=repair_count+1 WHERE id=?1",
        [id],
    )
    .map_err(err)?;
    Ok(())
}

// ---------- 循环计数、审批、锁、事件 ----------

pub fn loop_count(db: &Connection, run_id: &str, edge_id: &str) -> Result<u32, String> {
    db.query_row(
        "SELECT count FROM workflow_loop_counters WHERE run_id=?1 AND edge_id=?2",
        params![run_id, edge_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(err)
    .map(|value| value.unwrap_or(0))
}

pub fn loop_counts(db: &Connection, run_id: &str) -> Result<HashMap<String, u32>, String> {
    let mut statement = db
        .prepare("SELECT edge_id, count FROM workflow_loop_counters WHERE run_id=?1")
        .map_err(err)?;
    let rows = statement
        .query_map([run_id], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(err)?
        .collect::<rusqlite::Result<HashMap<String, u32>>>()
        .map_err(err)?;
    Ok(rows)
}

pub fn bump_loop(db: &Connection, run_id: &str, edge_id: &str) -> Result<(), String> {
    db.execute(
        "INSERT INTO workflow_loop_counters (run_id, edge_id, count) VALUES (?1, ?2, 1)
         ON CONFLICT(run_id, edge_id) DO UPDATE SET count=count+1",
        params![run_id, edge_id],
    )
    .map_err(err)?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GateDecisionRow {
    pub node_run_id: String,
    pub decision: String,
    pub comment: String,
    pub form: Value,
    pub source: String,
    pub decided_at: String,
}

pub fn insert_gate_decision(db: &Connection, row: &GateDecisionRow) -> Result<(), String> {
    db.execute(
        "INSERT INTO workflow_gate_decisions (node_run_id, decision, comment, form_json, source, decided_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            row.node_run_id,
            row.decision,
            row.comment,
            row.form.to_string(),
            row.source,
            row.decided_at
        ],
    )
    .map_err(|error| {
        if error.to_string().contains("UNIQUE") {
            tr!("这个关卡已经有人决定过了")
        } else {
            error.to_string()
        }
    })?;
    Ok(())
}

pub fn gate_decisions(db: &Connection, run_id: &str) -> Result<Vec<GateDecisionRow>, String> {
    let mut statement = db
        .prepare(
            "SELECT g.node_run_id, g.decision, g.comment, g.form_json, g.source, g.decided_at
             FROM workflow_gate_decisions g JOIN workflow_node_runs n ON n.id=g.node_run_id
             WHERE n.run_id=?1 ORDER BY g.decided_at",
        )
        .map_err(err)?;
    let rows = statement
        .query_map([run_id], |row| {
            Ok(GateDecisionRow {
                node_run_id: row.get(0)?,
                decision: row.get(1)?,
                comment: row.get(2)?,
                form: json_col(row.get(3)?),
                source: row.get(4)?,
                decided_at: row.get(5)?,
            })
        })
        .map_err(err)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(err)?;
    Ok(rows)
}

pub fn lock_holder(db: &Connection, workspace_id: &str) -> Result<Option<String>, String> {
    db.query_row(
        "SELECT l.node_run_id FROM workflow_workspace_locks l
         JOIN workflow_node_runs n ON n.id=l.node_run_id
         WHERE l.workspace_id=?1 AND n.status IN ('running','waiting_human','waiting_children')",
        [workspace_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(err)
}

pub fn take_lock(db: &Connection, workspace_id: &str, node_run_id: &str) -> Result<(), String> {
    db.execute(
        "INSERT INTO workflow_workspace_locks (workspace_id, node_run_id) VALUES (?1, ?2)
         ON CONFLICT(workspace_id) DO UPDATE SET node_run_id=excluded.node_run_id",
        params![workspace_id, node_run_id],
    )
    .map_err(err)?;
    Ok(())
}

pub fn release_locks(db: &Connection, node_run_id: &str) -> Result<(), String> {
    db.execute(
        "DELETE FROM workflow_workspace_locks WHERE node_run_id=?1",
        [node_run_id],
    )
    .map_err(err)?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunWorkspace {
    pub run_id: String,
    pub slot_key: String,
    pub workspace_id: String,
    pub path: String,
    pub branch: Option<String>,
    pub project_id: String,
    pub base_commit: Option<String>,
    pub head_commit: Option<String>,
    pub owned: bool,
}

pub fn run_workspaces(db: &Connection, run_id: &str) -> Result<Vec<RunWorkspace>, String> {
    let mut statement = db
        .prepare(
            "SELECT r.run_id, r.slot_key, r.workspace_id, w.work_path, w.branch, w.project_id,
                    r.base_commit, r.head_commit, r.owned
             FROM workflow_run_workspaces r JOIN workspaces w ON w.id=r.workspace_id
             WHERE r.run_id=?1 ORDER BY r.slot_key",
        )
        .map_err(err)?;
    let rows = statement
        .query_map([run_id], |row| {
            Ok(RunWorkspace {
                run_id: row.get(0)?,
                slot_key: row.get(1)?,
                workspace_id: row.get(2)?,
                path: row.get(3)?,
                branch: row.get(4)?,
                project_id: row.get(5)?,
                base_commit: row.get(6)?,
                head_commit: row.get(7)?,
                owned: row.get::<_, i64>(8)? != 0,
            })
        })
        .map_err(err)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(err)?;
    Ok(rows)
}

pub fn insert_run_workspace(
    db: &Connection,
    run_id: &str,
    slot: &str,
    workspace_id: &str,
    base_commit: Option<&str>,
    owned: bool,
) -> Result<(), String> {
    db.execute(
        "INSERT INTO workflow_run_workspaces (run_id, slot_key, workspace_id, base_commit, head_commit, owned)
         VALUES (?1, ?2, ?3, ?4, ?4, ?5)",
        params![run_id, slot, workspace_id, base_commit, owned as i64],
    )
    .map_err(err)?;
    Ok(())
}

pub fn set_slot_head(db: &Connection, run_id: &str, slot: &str, head: Option<&str>) -> Result<(), String> {
    db.execute(
        "UPDATE workflow_run_workspaces SET head_commit=?1 WHERE run_id=?2 AND slot_key=?3",
        params![head, run_id, slot],
    )
    .map_err(err)?;
    Ok(())
}

pub fn workspace_path(db: &Connection, workspace_id: &str) -> Result<Option<(String, Option<String>, String)>, String> {
    db.query_row(
        "SELECT work_path, branch, project_id FROM workspaces WHERE id=?1",
        [workspace_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )
    .optional()
    .map_err(err)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub id: String,
    pub run_id: String,
    pub node_run_id: Option<String>,
    pub seq: i64,
    pub kind: String,
    pub message: String,
    pub data: Value,
    pub created_at: String,
}

pub fn event(
    db: &Connection,
    run_id: &str,
    node_run_id: Option<&str>,
    kind: &str,
    message: &str,
    data: Value,
    now: &str,
) -> Result<(), String> {
    db.execute(
        "INSERT INTO workflow_events (id, run_id, node_run_id, seq, kind, message, data_json, created_at)
         VALUES (?1, ?2, ?3,
                 (SELECT COALESCE(MAX(seq), 0) + 1 FROM workflow_events WHERE run_id=?2),
                 ?4, ?5, ?6, ?7)",
        params![
            uuid::Uuid::new_v4().to_string(),
            run_id,
            node_run_id,
            kind,
            message,
            data.to_string(),
            now
        ],
    )
    .map_err(err)?;
    Ok(())
}

pub fn events(db: &Connection, run_id: &str, after_seq: i64) -> Result<Vec<Event>, String> {
    let mut statement = db
        .prepare(
            "SELECT id, run_id, node_run_id, seq, kind, message, data_json, created_at
             FROM workflow_events WHERE run_id=?1 AND seq > ?2 ORDER BY seq LIMIT 1000",
        )
        .map_err(err)?;
    let rows = statement
        .query_map(params![run_id, after_seq], |row| {
            Ok(Event {
                id: row.get(0)?,
                run_id: row.get(1)?,
                node_run_id: row.get(2)?,
                seq: row.get(3)?,
                kind: row.get(4)?,
                message: row.get(5)?,
                data: json_col(row.get(6)?),
                created_at: row.get(7)?,
            })
        })
        .map_err(err)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(err)?;
    Ok(rows)
}

// ---------- 密钥名 ----------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretName {
    pub name: String,
    pub description: String,
    pub updated_at: String,
}

pub fn secret_names(db: &Connection) -> Result<Vec<SecretName>, String> {
    let mut statement = db
        .prepare("SELECT name, description, updated_at FROM workflow_secrets ORDER BY name")
        .map_err(err)?;
    let rows = statement
        .query_map([], |row| {
            Ok(SecretName {
                name: row.get(0)?,
                description: row.get(1)?,
                updated_at: row.get(2)?,
            })
        })
        .map_err(err)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(err)?;
    Ok(rows)
}

pub fn upsert_secret_name(db: &Connection, name: &str, description: &str, now: &str) -> Result<(), String> {
    db.execute(
        "INSERT INTO workflow_secrets (name, description, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)
         ON CONFLICT(name) DO UPDATE SET description=excluded.description, updated_at=excluded.updated_at",
        params![name, description, now],
    )
    .map_err(err)?;
    Ok(())
}

pub fn delete_secret_name(db: &Connection, name: &str) -> Result<(), String> {
    db.execute("DELETE FROM workflow_secrets WHERE name=?1", [name])
        .map_err(err)?;
    Ok(())
}

// ---------- 校验用的查库实现 ----------

pub struct DbLookup<'a>(pub &'a Connection);

impl super::validate::Lookup for DbLookup<'_> {
    fn agent_exists(&self, id: &str) -> bool {
        self.0
            .query_row("SELECT EXISTS(SELECT 1 FROM agents WHERE id=?1)", [id], |row| row.get(0))
            .unwrap_or(false)
    }
    fn squad_exists(&self, id: &str) -> bool {
        self.0
            .query_row("SELECT EXISTS(SELECT 1 FROM squads WHERE id=?1)", [id], |row| row.get(0))
            .unwrap_or(false)
    }
    fn published(&self, workflow_id: &str, version: Option<u32>) -> Option<WorkflowDef> {
        let version = match version {
            Some(version) => version,
            None => latest_version(self.0, workflow_id).ok().flatten()?,
        };
        definition(self.0, workflow_id, version).ok()
    }
    fn secret_exists(&self, name: &str) -> bool {
        self.0
            .query_row("SELECT EXISTS(SELECT 1 FROM workflow_secrets WHERE name=?1)", [name], |row| row.get(0))
            .unwrap_or(false)
    }
}
