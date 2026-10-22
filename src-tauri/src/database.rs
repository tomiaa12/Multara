use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::{Child, ChildStdin},
    sync::{atomic::AtomicBool, mpsc::Sender, Arc, Mutex, MutexGuard},
    time::Duration,
};

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use tauri::{AppHandle, Manager};

pub struct ActiveRun {
    pub child: Arc<Mutex<Child>>,
    /// `spawn_run` 线程还在。线程退出（包括 panic）后变为 false，轮询线程负责收尸。
    pub supervisor_alive: Arc<AtomicBool>,
    pub session_id: String,
    pub provider: String,
    pub stdin: Arc<Mutex<Option<ChildStdin>>>,
    pub permissions: Arc<Mutex<HashMap<String, Sender<String>>>>,
}

pub struct AppState {
    pub db: Mutex<Connection>,
    pub active_runs: Mutex<HashMap<String, ActiveRun>>,
    pub project_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

pub fn open(app: &AppHandle) -> Result<AppState, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&data_dir).map_err(|error| error.to_string())?;
    crate::diag::init(&data_dir);
    let path: PathBuf = data_dir.join("multara.sqlite3");
    let connection = open_file(&path)?;
    recover_interrupted_runs(&connection).map_err(|error| error.to_string())?;
    Ok(AppState {
        db: Mutex::new(connection),
        active_runs: Mutex::new(HashMap::new()),
        project_locks: Mutex::new(HashMap::new()),
    })
}

pub fn open_file(path: &Path) -> Result<Connection, String> {
    let connection = Connection::open(path).map_err(|error| error.to_string())?;
    apply_pragmas(&connection).map_err(|error| error.to_string())?;
    migrate(&connection).map_err(|error| error.to_string())?;
    seed(&connection).map_err(|error| error.to_string())?;
    Ok(connection)
}

fn apply_pragmas(db: &Connection) -> rusqlite::Result<()> {
    db.busy_timeout(Duration::from_secs(5))?;
    db.pragma_update(None, "journal_mode", "WAL")?;
    db.pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

impl AppState {
    #[track_caller]
    pub fn lock_db(&self) -> Result<MutexGuard<'_, Connection>, String> {
        crate::diag::lock_mutex(&self.db, "db")
    }
}

pub(crate) fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(
        "
        PRAGMA foreign_keys = ON;
        CREATE TABLE IF NOT EXISTS projects (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          note TEXT NOT NULL DEFAULT '',
          created_at TEXT NOT NULL,
          opened_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS workspaces (
          id TEXT PRIMARY KEY,
          project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
          name TEXT NOT NULL,
          kind TEXT NOT NULL CHECK(kind IN ('git', 'folder')),
          source_path TEXT NOT NULL,
          work_path TEXT NOT NULL,
          branch TEXT,
          isolation_mode TEXT NOT NULL CHECK(isolation_mode IN ('worktree', 'copy', 'direct')),
          created_at TEXT NOT NULL,
          UNIQUE(project_id, work_path)
        );
        CREATE INDEX IF NOT EXISTS idx_workspaces_project ON workspaces(project_id);
        CREATE TABLE IF NOT EXISTS agents (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          avatar TEXT NOT NULL,
          description TEXT NOT NULL,
          instructions TEXT NOT NULL,
          runtime_id TEXT NOT NULL,
          model TEXT NOT NULL DEFAULT 'auto',
          max_concurrent_tasks INTEGER NOT NULL DEFAULT 1 CHECK(max_concurrent_tasks BETWEEN 1 AND 6),
          system_key TEXT
        );
        CREATE TABLE IF NOT EXISTS runtime_settings (
          runtime_id TEXT PRIMARY KEY,
          max_concurrent INTEGER NOT NULL CHECK(max_concurrent BETWEEN 1 AND 6)
        );
        CREATE TABLE IF NOT EXISTS custom_runtimes (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          command TEXT NOT NULL,
          args_json TEXT NOT NULL DEFAULT '[]'
        );
        CREATE TABLE IF NOT EXISTS agent_resources (
          agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
          resource_id TEXT NOT NULL,
          kind TEXT NOT NULL CHECK(kind IN ('skill', 'mcp')),
          PRIMARY KEY (agent_id, resource_id)
        );
        CREATE TABLE IF NOT EXISTS squads (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          description TEXT NOT NULL,
          leader_agent_id TEXT NOT NULL REFERENCES agents(id),
          instructions TEXT NOT NULL DEFAULT ''
        );
        CREATE TABLE IF NOT EXISTS squad_members (
          squad_id TEXT NOT NULL REFERENCES squads(id) ON DELETE CASCADE,
          agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
          PRIMARY KEY (squad_id, agent_id)
        );
        CREATE TABLE IF NOT EXISTS chat_sessions (
          id TEXT PRIMARY KEY,
          project_id TEXT REFERENCES projects(id) ON DELETE CASCADE,
          title TEXT NOT NULL,
          target_type TEXT NOT NULL CHECK(target_type IN ('agent', 'squad')),
          target_id TEXT NOT NULL,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS tasks (
          id TEXT PRIMARY KEY,
          session_id TEXT NOT NULL UNIQUE,
          title TEXT NOT NULL,
          description TEXT NOT NULL,
          agent_id TEXT NOT NULL,
          status TEXT NOT NULL CHECK(status IN (
            'backlog', 'todo', 'in_progress', 'in_review', 'done', 'blocked', 'cancelled'
          )),
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS messages (
          id TEXT PRIMARY KEY,
          session_id TEXT NOT NULL REFERENCES chat_sessions(id) ON DELETE CASCADE,
          role TEXT NOT NULL,
          content TEXT NOT NULL,
          run_id TEXT,
          failure_reason TEXT,
          elapsed_ms INTEGER,
          message_kind TEXT NOT NULL DEFAULT 'message',
          quick_actions_json TEXT NOT NULL DEFAULT '[]',
          created_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS message_attachments (
          id TEXT PRIMARY KEY,
          message_id TEXT NOT NULL,
          session_id TEXT NOT NULL,
          name TEXT NOT NULL,
          path TEXT NOT NULL,
          mime_type TEXT NOT NULL,
          size INTEGER NOT NULL,
          created_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS agent_runs (
          id TEXT PRIMARY KEY,
          session_id TEXT NOT NULL REFERENCES chat_sessions(id) ON DELETE CASCADE,
          agent_id TEXT NOT NULL REFERENCES agents(id),
          runtime_id TEXT NOT NULL,
          parent_run_id TEXT REFERENCES agent_runs(id),
          assignment TEXT NOT NULL,
          status TEXT NOT NULL,
          created_at TEXT NOT NULL,
          started_at TEXT,
          completed_at TEXT
        );
        CREATE TABLE IF NOT EXISTS run_events (
          id TEXT PRIMARY KEY,
          run_id TEXT NOT NULL,
          session_id TEXT NOT NULL,
          seq INTEGER NOT NULL,
          event_type TEXT NOT NULL CHECK(event_type IN ('text', 'thinking', 'tool_use', 'tool_result', 'error')),
          call_id TEXT,
          tool TEXT,
          content TEXT,
          input_json TEXT,
          output TEXT,
          created_at TEXT NOT NULL,
          UNIQUE(run_id, seq)
        );
        CREATE INDEX IF NOT EXISTS idx_run_events_run_seq ON run_events(run_id, seq);
        CREATE INDEX IF NOT EXISTS idx_message_attachments_message ON message_attachments(message_id);
        CREATE TABLE IF NOT EXISTS automations (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          project_id TEXT REFERENCES projects(id) ON DELETE CASCADE,
          target_type TEXT NOT NULL CHECK(target_type IN ('agent', 'squad')),
          target_id TEXT NOT NULL,
          prompt TEXT NOT NULL,
          trigger_type TEXT NOT NULL CHECK(trigger_type IN ('manual', 'interval', 'event')),
          interval_minutes INTEGER,
          event_name TEXT,
          enabled INTEGER NOT NULL DEFAULT 1,
          last_run_at TEXT,
          created_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS app_settings (
          key TEXT PRIMARY KEY,
          value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS automation_triggers (
          id TEXT PRIMARY KEY,
          automation_id TEXT NOT NULL REFERENCES automations(id) ON DELETE CASCADE,
          kind TEXT NOT NULL CHECK(kind IN ('schedule', 'webhook', 'api')),
          enabled INTEGER NOT NULL DEFAULT 1,
          cron_expression TEXT,
          timezone TEXT,
          next_run_at TEXT,
          last_fired_at TEXT,
          webhook_token TEXT UNIQUE,
          signing_secret TEXT,
          event_name TEXT,
          created_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS automation_runs (
          id TEXT PRIMARY KEY,
          automation_id TEXT NOT NULL REFERENCES automations(id) ON DELETE CASCADE,
          trigger_id TEXT REFERENCES automation_triggers(id) ON DELETE SET NULL,
          source TEXT NOT NULL CHECK(source IN ('schedule', 'manual', 'webhook', 'api')),
          status TEXT NOT NULL CHECK(status IN ('pending', 'running', 'skipped', 'completed', 'failed')),
          planned_at TEXT,
          triggered_at TEXT NOT NULL,
          completed_at TEXT,
          failure_reason TEXT,
          reason_code TEXT,
          trigger_payload TEXT,
          UNIQUE(trigger_id, planned_at)
        );
        CREATE TABLE IF NOT EXISTS webhook_deliveries (
          id TEXT PRIMARY KEY,
          trigger_id TEXT NOT NULL REFERENCES automation_triggers(id) ON DELETE CASCADE,
          dedupe_key TEXT NOT NULL,
          event TEXT,
          status TEXT NOT NULL CHECK(status IN ('queued', 'dispatched', 'rejected', 'ignored', 'failed')),
          automation_run_id TEXT REFERENCES automation_runs(id) ON DELETE SET NULL,
          reason_code TEXT,
          created_at TEXT NOT NULL,
          UNIQUE(trigger_id, dedupe_key)
        );
        CREATE INDEX IF NOT EXISTS idx_automation_runs_rule
          ON automation_runs(automation_id, triggered_at DESC);
        CREATE INDEX IF NOT EXISTS idx_automation_triggers_due
          ON automation_triggers(enabled, kind, next_run_at);
        CREATE TABLE IF NOT EXISTS agent_turns (
          id TEXT PRIMARY KEY,
          session_id TEXT NOT NULL,
          seq INTEGER NOT NULL,
          user_message_id TEXT NOT NULL,
          status TEXT NOT NULL CHECK(status IN ('running', 'completed', 'failed', 'cancelled')),
          created_at TEXT NOT NULL,
          completed_at TEXT,
          UNIQUE(session_id, seq)
        );
        CREATE TABLE IF NOT EXISTS session_workspaces (
          session_id TEXT NOT NULL,
          workspace_id TEXT NOT NULL,
          source_workspace_id TEXT NOT NULL,
          PRIMARY KEY(session_id, workspace_id)
        );
        CREATE INDEX IF NOT EXISTS idx_turns_session ON agent_turns(session_id, seq);
        CREATE TABLE IF NOT EXISTS turn_file_snapshots (
          id TEXT NOT NULL PRIMARY KEY,
          message_id TEXT NOT NULL,
          session_id TEXT NOT NULL,
          workspace_id TEXT NOT NULL,
          file_path TEXT NOT NULL,
          content BLOB NOT NULL,
          content_hash TEXT NOT NULL,
          snapshot_at INTEGER NOT NULL,
          UNIQUE(message_id, workspace_id, file_path)
        );
        CREATE INDEX IF NOT EXISTS idx_tfs_session ON turn_file_snapshots(session_id, snapshot_at);
        CREATE INDEX IF NOT EXISTS idx_tfs_message ON turn_file_snapshots(message_id);
        CREATE INDEX IF NOT EXISTS idx_tfs_file ON turn_file_snapshots(workspace_id, file_path);
        CREATE TABLE IF NOT EXISTS diff_chunk_states (
          id TEXT NOT NULL PRIMARY KEY,
          message_id TEXT NOT NULL,
          session_id TEXT NOT NULL,
          workspace_id TEXT NOT NULL,
          file_path TEXT NOT NULL,
          chunk_key TEXT NOT NULL,
          status TEXT NOT NULL DEFAULT 'pending',
          updated_at INTEGER NOT NULL,
          UNIQUE(message_id, workspace_id, file_path, chunk_key)
        );
        CREATE INDEX IF NOT EXISTS idx_dcs_session ON diff_chunk_states(session_id);
        CREATE INDEX IF NOT EXISTS idx_dcs_message ON diff_chunk_states(message_id, workspace_id, file_path);
        CREATE TABLE IF NOT EXISTS file_review_baselines (
          workspace_id TEXT NOT NULL,
          file_path TEXT NOT NULL,
          content BLOB NOT NULL,
          updated_at INTEGER NOT NULL,
          PRIMARY KEY (workspace_id, file_path)
        );
        ",
    )?;
    ensure_column(db, "agent_runs", "started_at", "TEXT")?;
    ensure_column(db, "agent_runs", "completed_at", "TEXT")?;
    ensure_column(db, "agent_runs", "input_tokens", "INTEGER NOT NULL DEFAULT 0")?;
    ensure_column(db, "agent_runs", "output_tokens", "INTEGER NOT NULL DEFAULT 0")?;
    ensure_column(db, "agent_runs", "cache_read_tokens", "INTEGER NOT NULL DEFAULT 0")?;
    ensure_column(db, "agent_runs", "cache_write_tokens", "INTEGER NOT NULL DEFAULT 0")?;
    ensure_column(db, "agent_runs", "cost_usd", "REAL NOT NULL DEFAULT 0")?;
    ensure_column(db, "agent_runs", "usage_model", "TEXT")?;
    ensure_column(db, "messages", "run_id", "TEXT")?;
    ensure_column(db, "messages", "failure_reason", "TEXT")?;
    ensure_column(db, "messages", "elapsed_ms", "INTEGER")?;
    ensure_column(
        db,
        "messages",
        "message_kind",
        "TEXT NOT NULL DEFAULT 'message'",
    )?;
    ensure_column(
        db,
        "messages",
        "quick_actions_json",
        "TEXT NOT NULL DEFAULT '[]'",
    )?;
    ensure_column(db, "run_events", "call_id", "TEXT")?;
    // 归档时间；NULL 表示未归档。
    ensure_column(db, "projects", "archived_at", "TEXT")?;
    crate::merge_flow::migrate(db)?;
    ensure_column(db, "workspaces", "session_id", "TEXT")?;
    ensure_column(db, "workspaces", "source_workspace_id", "TEXT")?;
    // 同一仓库的多个 Worktree 共用 source_path，唯一约束要落在检出目录上。
    migrate_workspace_checkout_key(db)?;
    ensure_column(db, "messages", "turn_id", "TEXT")?;
    ensure_column(db, "messages", "agent_id", "TEXT")?;
    ensure_column(db, "messages", "audience_type", "TEXT")?;
    ensure_column(db, "messages", "audience_id", "TEXT")?;
    ensure_column(db, "messages", "target_type", "TEXT")?;
    ensure_column(db, "messages", "target_id", "TEXT")?;
    ensure_column(db, "agent_runs", "turn_id", "TEXT")?;
    ensure_column(
        db,
        "chat_sessions",
        "project_id",
        "TEXT REFERENCES projects(id) ON DELETE CASCADE",
    )?;
    ensure_column(db, "agents", "system_key", "TEXT")?;
    ensure_column(db, "squads", "instructions", "TEXT NOT NULL DEFAULT ''")?;
    ensure_column(db, "custom_runtimes", "provider", "TEXT NOT NULL DEFAULT 'custom'")?;
    ensure_column(db, "custom_runtimes", "protocol", "TEXT NOT NULL DEFAULT ''")?;
    ensure_column(db, "custom_runtimes", "api_url", "TEXT NOT NULL DEFAULT ''")?;
    ensure_column(db, "custom_runtimes", "api_key", "TEXT NOT NULL DEFAULT ''")?;
    ensure_column(db, "custom_runtimes", "models_json", "TEXT NOT NULL DEFAULT '[]'")?;
    ensure_column(db, "custom_runtimes", "env_json", "TEXT NOT NULL DEFAULT '{}'")?;
    ensure_column(db, "custom_runtimes", "kind", "TEXT NOT NULL DEFAULT 'profile'")?;
    ensure_column(db, "custom_runtimes", "description", "TEXT NOT NULL DEFAULT ''")?;
    ensure_column(db, "custom_runtimes", "wire_api", "TEXT NOT NULL DEFAULT 'responses'")?;
    ensure_column(db, "custom_runtimes", "key_style", "TEXT NOT NULL DEFAULT 'bearer'")?;
    db.execute(
        "UPDATE custom_runtimes
         SET kind = 'gateway'
         WHERE kind = 'profile' AND (TRIM(api_url) != '' OR TRIM(api_key) != '')",
        [],
    )?;
    db.execute_batch(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_agents_system_key
         ON agents(system_key) WHERE system_key IS NOT NULL;",
    )?;
    db.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS session_runtime_chats (
          session_id TEXT NOT NULL,
          agent_id TEXT NOT NULL,
          runtime_id TEXT NOT NULL,
          conversation_id TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          PRIMARY KEY (session_id, agent_id, runtime_id)
        );
        ",
    )?;
    db.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS inbox_items (
          id TEXT PRIMARY KEY,
          session_id TEXT NOT NULL UNIQUE,
          project_id TEXT,
          task_id TEXT,
          title TEXT NOT NULL,
          preview TEXT NOT NULL,
          status TEXT NOT NULL,
          read INTEGER NOT NULL DEFAULT 0,
          read_message_id TEXT,
          created_at TEXT NOT NULL
        );
        ",
    )?;
    ensure_column(db, "inbox_items", "read_message_id", "TEXT")?;
    ensure_column(db, "inbox_items", "archived_at", "TEXT")?;
    ensure_column(db, "run_events", "parent_call_id", "TEXT")?;
    ensure_column(db, "run_events", "status", "TEXT")?;
    ensure_column(db, "messages", "feedback", "TEXT")?;
    ensure_column(db, "agent_runs", "model_override", "TEXT")?;
    ensure_column(db, "agent_runs", "mode", "TEXT")?;
    ensure_column(db, "agent_runs", "effort", "TEXT")?;
    ensure_column(db, "chat_sessions", "compact_summary", "TEXT")?;
    ensure_column(db, "chat_sessions", "compact_message_id", "TEXT")?;
    migrate_task_statuses(db)?;
    ensure_column(db, "tasks", "parent_task_id", "TEXT")?;
    ensure_column(db, "tasks", "priority", "TEXT NOT NULL DEFAULT 'none'")?;
    ensure_column(db, "tasks", "labels_json", "TEXT NOT NULL DEFAULT '[]'")?;
    ensure_column(db, "tasks", "checklist_json", "TEXT NOT NULL DEFAULT '[]'")?;
    ensure_column(db, "tasks", "pr_url", "TEXT NOT NULL DEFAULT ''")?;
    migrate_optional_automation_project(db)?;
    migrate_diff_chunk_keys(db)?;
    drop_legacy_review_tables(db)?;
    backfill_file_review_baselines(db)?;
    crate::snapshot_db::cleanup_old_snapshots(db).ok();
    // 旧自动化只建了隐藏会话；补成任务和收件箱，重启后就能在看板里看到。
    crate::ops::automation::backfill_results(db).ok();
    Ok(())
}

/// Runs only when the desktop app starts. CLI connections must not call this:
/// it would mark a live run as failed while the agent process is still going.
pub(crate) fn recover_interrupted_runs(db: &Connection) -> rusqlite::Result<()> {
    let now = Utc::now().to_rfc3339();
    let interrupted: Vec<(String, String, String)> = {
        let mut statement = db.prepare(
            "SELECT id, session_id, COALESCE(agent_id, '') FROM agent_runs WHERE status = 'running'",
        )?;
        let rows = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    let reason = "Multara 退出时这次运行还没写完，已中断。";
    for (run_id, session_id, agent_id) in interrupted {
        // 先把已流式落库的 run_events 收成 messages，否则重启后任务详情只剩标题/描述。
        let text = salvage_run_text(db, &run_id).unwrap_or_default();
        let content = if text.trim().is_empty() {
            reason.to_string()
        } else {
            text
        };
        let _ = db.execute(
            "INSERT INTO messages
             (id, session_id, role, content, run_id, agent_id, audience_type,
              failure_reason, message_kind, quick_actions_json, created_at)
             VALUES (?1, ?2, 'assistant', ?3, ?4, ?5, 'user', ?6, 'message', '[]', ?7)
             ON CONFLICT(id) DO UPDATE SET
               content=excluded.content,
               failure_reason=excluded.failure_reason",
            params![
                format!("run:{run_id}"),
                session_id,
                content,
                run_id,
                agent_id,
                reason,
                now
            ],
        );
        db.execute(
            "UPDATE agent_runs SET status='failed', completed_at=?1
             WHERE id=?2 AND status='running'",
            params![now, run_id],
        )?;
        let _ = crate::ops::task::sync_from_run(db, &session_id, "failed", &now);
    }
    db.execute(
        "UPDATE agent_runs SET status='failed', completed_at=?1
         WHERE status='queued' AND session_id LIKE 'automation-run:%'",
        [&now],
    )?;
    db.execute(
        "UPDATE automation_runs SET status='failed', completed_at=?1,
         reason_code='app_restarted', failure_reason='Multara 在任务完成前退出'
         WHERE status IN ('pending', 'running')",
        [&now],
    )?;
    Ok(())
}

fn salvage_run_text(db: &Connection, run_id: &str) -> rusqlite::Result<String> {
    let mut statement = db.prepare(
        "SELECT content FROM run_events
         WHERE run_id=?1 AND event_type='text' AND content IS NOT NULL AND trim(content) != ''
         ORDER BY seq",
    )?;
    let parts = statement
        .query_map([run_id], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    // 流式 text 多为替换而非追加；拼起来会碎句重复，取最长一条更接近最终可见正文。
    Ok(parts
        .into_iter()
        .max_by_key(|text| text.chars().count())
        .unwrap_or_default())
}

/// 旧数据只有按消息存的快照。每个仍待审阅的文件取最早一份，作为跟文件走的 baseline。
fn backfill_file_review_baselines(db: &Connection) -> rusqlite::Result<()> {
    let count: i64 = db.query_row(
        "SELECT COUNT(*) FROM file_review_baselines",
        [],
        |row| row.get(0),
    )?;
    if count > 0 {
        return Ok(());
    }
    let Ok(rows) = crate::snapshot_db::load_active_snapshots(db) else {
        return Ok(());
    };
    for row in rows {
        if crate::snapshot_db::ensure_review_baseline(
            db,
            &row.workspace_id,
            &row.file_path,
            &row.content,
        )
        .is_err()
        {
            continue;
        }
    }
    Ok(())
}

fn drop_legacy_review_tables(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(
        "DROP TABLE IF EXISTS turn_file_baselines;
         DROP TABLE IF EXISTS pending_tool_edits;
         DROP TABLE IF EXISTS review_bases;
         DROP TABLE IF EXISTS review_actions;
         DROP TABLE IF EXISTS checkpoint_snapshots;
         DROP TABLE IF EXISTS snapshot_entries;
         DROP TABLE IF EXISTS snapshots;
         DROP TABLE IF EXISTS content_objects;
         DROP TABLE IF EXISTS change_reviews;
         DROP TABLE IF EXISTS change_hunk_reviews;
         DROP TABLE IF EXISTS change_blocks;
         DROP TABLE IF EXISTS edit_operations;
         DROP TABLE IF EXISTS edit_sessions;
         DROP TABLE IF EXISTS checkpoints;
         DROP TABLE IF EXISTS checkpoint_git_commits;
         DROP TABLE IF EXISTS turn_file_changes;
         DELETE FROM app_settings WHERE key='reviewEngine';",
    )
}

fn migrate_diff_chunk_keys(db: &Connection) -> rusqlite::Result<()> {
    if !table_has_column(db, "diff_chunk_states", "chunk_index")? {
        return Ok(());
    }
    db.execute_batch(
        "DROP TABLE IF EXISTS diff_chunk_states;
         CREATE TABLE diff_chunk_states (
           id TEXT NOT NULL PRIMARY KEY,
           message_id TEXT NOT NULL,
           session_id TEXT NOT NULL,
           workspace_id TEXT NOT NULL,
           file_path TEXT NOT NULL,
           chunk_key TEXT NOT NULL,
           status TEXT NOT NULL DEFAULT 'pending',
           updated_at INTEGER NOT NULL,
           UNIQUE(message_id, workspace_id, file_path, chunk_key)
         );
         CREATE INDEX IF NOT EXISTS idx_dcs_session ON diff_chunk_states(session_id);
         CREATE INDEX IF NOT EXISTS idx_dcs_message ON diff_chunk_states(message_id, workspace_id, file_path);",
    )
}

fn table_has_column(db: &Connection, table: &str, column: &str) -> rusqlite::Result<bool> {
    let mut statement = db.prepare(&format!("PRAGMA table_info({table})"))?;
    let exists = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .iter()
        .any(|name| name == column);
    Ok(exists)
}

fn migrate_workspace_checkout_key(db: &Connection) -> rusqlite::Result<()> {
    let schema: Option<String> = db
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='workspaces'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if !schema
        .as_deref()
        .is_some_and(|sql| sql.contains("UNIQUE(project_id, source_path)"))
    {
        return Ok(());
    }
    db.execute_batch(
        "PRAGMA foreign_keys=OFF;
         CREATE TABLE workspaces_checkout_key (
           id TEXT PRIMARY KEY,
           project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
           name TEXT NOT NULL,
           kind TEXT NOT NULL CHECK(kind IN ('git', 'folder')),
           source_path TEXT NOT NULL,
           work_path TEXT NOT NULL,
           branch TEXT,
           isolation_mode TEXT NOT NULL CHECK(isolation_mode IN ('worktree', 'copy', 'direct')),
           created_at TEXT NOT NULL,
           session_id TEXT,
           source_workspace_id TEXT,
           UNIQUE(project_id, work_path)
         );
         INSERT INTO workspaces_checkout_key
           (id, project_id, name, kind, source_path, work_path, branch, isolation_mode,
            created_at, session_id, source_workspace_id)
         SELECT id, project_id, name, kind, source_path, work_path, branch, isolation_mode,
                created_at, session_id, source_workspace_id
         FROM workspaces;
         DROP TABLE workspaces;
         ALTER TABLE workspaces_checkout_key RENAME TO workspaces;
         CREATE INDEX IF NOT EXISTS idx_workspaces_project ON workspaces(project_id);
         PRAGMA foreign_keys=ON;",
    )
}

fn migrate_task_statuses(db: &Connection) -> rusqlite::Result<()> {
    let schema: Option<String> = db
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='tasks'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if schema
        .as_deref()
        .map(|sql| sql.contains("'backlog'"))
        .unwrap_or(true)
    {
        return Ok(());
    }
    db.execute_batch(
        "ALTER TABLE tasks RENAME TO tasks_legacy;
         CREATE TABLE tasks (
           id TEXT PRIMARY KEY,
           session_id TEXT NOT NULL UNIQUE,
           title TEXT NOT NULL,
           description TEXT NOT NULL,
           agent_id TEXT NOT NULL,
           status TEXT NOT NULL CHECK(status IN (
             'backlog', 'todo', 'in_progress', 'in_review', 'done', 'blocked', 'cancelled'
           )),
           created_at TEXT NOT NULL,
           updated_at TEXT NOT NULL
         );
         INSERT INTO tasks
           (id, session_id, title, description, agent_id, status, created_at, updated_at)
         SELECT id, session_id, title, description, agent_id,
           CASE status WHEN 'canceled' THEN 'cancelled' ELSE status END,
           created_at, updated_at
         FROM tasks_legacy;
         DROP TABLE tasks_legacy;",
    )
}

/// 已有库里 automations.project_id 是 NOT NULL。SQLite 不能直接去掉非空，重建表后允许不关联需求。
fn migrate_optional_automation_project(db: &Connection) -> rusqlite::Result<()> {
    let notnull: Option<i64> = db
        .query_row(
            "SELECT \"notnull\" FROM pragma_table_info('automations') WHERE name = 'project_id'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if notnull != Some(1) {
        return Ok(());
    }
    db.execute_batch(
        "PRAGMA foreign_keys=OFF;
         CREATE TABLE automations_optional_project (
           id TEXT PRIMARY KEY,
           name TEXT NOT NULL,
           project_id TEXT REFERENCES projects(id) ON DELETE CASCADE,
           target_type TEXT NOT NULL CHECK(target_type IN ('agent', 'squad')),
           target_id TEXT NOT NULL,
           prompt TEXT NOT NULL,
           trigger_type TEXT NOT NULL CHECK(trigger_type IN ('manual', 'interval', 'event')),
           interval_minutes INTEGER,
           event_name TEXT,
           enabled INTEGER NOT NULL DEFAULT 1,
           last_run_at TEXT,
           created_at TEXT NOT NULL
         );
         INSERT INTO automations_optional_project
           (id, name, project_id, target_type, target_id, prompt, trigger_type,
            interval_minutes, event_name, enabled, last_run_at, created_at)
         SELECT id, name, project_id, target_type, target_id, prompt, trigger_type,
                interval_minutes, event_name, enabled, last_run_at, created_at
         FROM automations;
         DROP TABLE automations;
         ALTER TABLE automations_optional_project RENAME TO automations;
         PRAGMA foreign_keys=ON;",
    )
}

fn ensure_column(
    db: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> rusqlite::Result<()> {
    if !table_has_column(db, table, column)? {
        db.execute_batch(&format!(
            "ALTER TABLE {table} ADD COLUMN {column} {definition}"
        ))?;
    }
    Ok(())
}

pub(crate) fn seed(db: &Connection) -> rusqlite::Result<()> {
    for (key, value) in [
        ("language", "system"),
        ("theme", "dark"),
        ("fileIconTheme", "material-icon-theme"),
        ("gitExecutable", "git"),
        ("isolationRoot", "~/.multara"),
        ("autoUpdate", "true"),
        ("shareDiagnostics", "false"),
        ("runTimeoutMinutes", "0"),
        ("webhookPort", "43127"),
        ("formatOnSave", "false"),
        ("stickyScroll", "true"),
    ] {
        db.execute(
            "INSERT OR IGNORE INTO app_settings (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
    }
    // 旧默认 30 会掐长任务；一次性迁到 0（不限）。用户之后若再设 30 会保留。
    let migrated = db.execute(
        "INSERT OR IGNORE INTO app_settings (key, value) VALUES ('migratedRunTimeoutUnlimited', '1')",
        [],
    )?;
    if migrated > 0 {
        db.execute(
            "UPDATE app_settings SET value = '0' WHERE key = 'runTimeoutMinutes' AND value = '30'",
            [],
        )?;
    }
    let runtime_id = crate::runtime::seed_runtime_id();
    db.execute(
        "INSERT OR IGNORE INTO agents
         (id, name, avatar, description, instructions, runtime_id, model, max_concurrent_tasks, system_key)
         VALUES (?1, ?2, ?3, ?4, '', ?5, 'auto', ?6, ?7)",
        params![
            crate::builtin_agents::MIKU_ID,
            crate::builtin_agents::MIKU_DEFAULT_NAME,
            crate::builtin_agents::MIKU_AVATAR,
            crate::builtin_agents::MIKU_DESCRIPTION,
            runtime_id,
            crate::builtin_agents::MIKU_MAX_CONCURRENT_TASKS,
            crate::builtin_agents::MIKU_SYSTEM_KEY,
        ],
    )?;
    db.execute(
        "UPDATE agents SET system_key=?1 WHERE id=?2 AND system_key IS NULL",
        params![
            crate::builtin_agents::MIKU_SYSTEM_KEY,
            crate::builtin_agents::MIKU_ID
        ],
    )?;
    // 仍是旧默认 emoji 时，升级为内置像素头像（用户自定义过的不覆盖）
    db.execute(
        "UPDATE agents SET avatar=?1 WHERE id=?2 AND avatar IN ('🦄', '🤖')",
        params![
            crate::builtin_agents::MIKU_AVATAR,
            crate::builtin_agents::MIKU_ID
        ],
    )?;
    let now = chrono::Utc::now().to_rfc3339();
    db.execute(
        "INSERT OR IGNORE INTO chat_sessions
         (id, project_id, title, target_type, target_id, created_at, updated_at)
         VALUES (?1, NULL, ?2, 'agent', ?3, ?4, ?4)",
        params![
            crate::builtin_agents::MIKU_WELCOME_SESSION_ID,
            crate::builtin_agents::MIKU_WELCOME_TITLE,
            crate::builtin_agents::MIKU_ID,
            now
        ],
    )?;
    db.execute(
        "INSERT OR IGNORE INTO messages
         (id, session_id, role, content, message_kind, quick_actions_json, created_at, agent_id, audience_type)
         VALUES ('welcome-opening', ?1, 'assistant', ?2, 'message', ?3, ?4, ?5, 'user')",
        params![
            crate::builtin_agents::MIKU_WELCOME_SESSION_ID,
            crate::builtin_agents::welcome_opening(crate::builtin_agents::MIKU_DEFAULT_NAME),
            crate::builtin_agents::welcome_quick_actions_json(),
            now,
            crate::builtin_agents::MIKU_ID,
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::migrate;
    use rusqlite::Connection;

    #[test]
    fn migrates_legacy_task_statuses_to_seven_state_board() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE tasks (
               id TEXT PRIMARY KEY, session_id TEXT NOT NULL UNIQUE, title TEXT NOT NULL,
               description TEXT NOT NULL, agent_id TEXT NOT NULL,
               status TEXT NOT NULL CHECK(status IN ('todo', 'in_progress', 'done', 'canceled')),
               created_at TEXT NOT NULL, updated_at TEXT NOT NULL
             );
             INSERT INTO tasks VALUES
               ('task', 'session', '任务', '', 'agent', 'canceled', 'now', 'now');",
        )
        .unwrap();

        migrate(&db).unwrap();

        let status: String = db
            .query_row("SELECT status FROM tasks WHERE id='task'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(status, "cancelled");
        db.execute(
            "INSERT INTO tasks VALUES ('blocked', 'session-2', '阻塞', '', 'agent', 'blocked', 'now', 'now')",
            [],
        )
        .unwrap();
    }

    #[test]
    fn migrates_legacy_messages_to_structured_answers() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE chat_sessions (
               id TEXT PRIMARY KEY, project_id TEXT, title TEXT, target_type TEXT, target_id TEXT,
               created_at TEXT, updated_at TEXT
             );
             CREATE TABLE messages (
               id TEXT PRIMARY KEY, session_id TEXT NOT NULL, role TEXT NOT NULL,
               content TEXT NOT NULL, created_at TEXT NOT NULL
             );",
        )
        .unwrap();

        migrate(&db).unwrap();

        let columns = db
            .prepare("PRAGMA table_info(messages)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        for column in [
            "run_id",
            "failure_reason",
            "elapsed_ms",
            "message_kind",
            "quick_actions_json",
        ] {
            assert!(columns.iter().any(|item| item == column), "缺少 {column}");
        }
        db.execute(
            "INSERT INTO run_events
             (id, run_id, session_id, seq, event_type, created_at)
             VALUES ('event', 'run', 'session', 1, 'thinking', 'now')",
            [],
        )
        .unwrap();
        assert!(db
            .execute(
                "INSERT INTO run_events
                 (id, run_id, session_id, seq, event_type, created_at)
                 VALUES ('duplicate', 'run', 'session', 1, 'text', 'now')",
                [],
            )
            .is_err());
    }

    #[test]
    fn migrates_session_tables_without_removing_existing_sessions() {
        let db = Connection::open_in_memory().unwrap();
        migrate(&db).unwrap();
        db.execute(
            "INSERT INTO chat_sessions
             (id, title, target_type, target_id, created_at, updated_at)
             VALUES ('session', '测试', 'agent', 'frontend', 'now', 'now')",
            [],
        )
        .unwrap();

        for table in ["agent_turns", "session_workspaces"] {
            let exists: bool = db
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(exists, "缺少 {table}");
        }
        let exists: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM chat_sessions WHERE id='session')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(exists, "迁移后会话丢失");
    }

    #[test]
    fn seed_is_idempotent_and_only_inserts_miku() {
        let db = Connection::open_in_memory().unwrap();
        migrate(&db).unwrap();
        super::seed(&db).unwrap();
        super::seed(&db).unwrap();
        let agents: u32 = db
            .query_row("SELECT COUNT(*) FROM agents", [], |row| row.get(0))
            .unwrap();
        assert_eq!(agents, 1);
        let key: String = db
            .query_row("SELECT system_key FROM agents WHERE id='miku'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(key, "miku");
        let squads: u32 = db
            .query_row("SELECT COUNT(*) FROM squads", [], |row| row.get(0))
            .unwrap();
        assert_eq!(squads, 0);
        let target: (String, String) = db
            .query_row(
                "SELECT target_type, target_id FROM chat_sessions WHERE id='welcome'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(target, ("agent".into(), "miku".into()));
        let opening: u32 = db
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE id='welcome-opening'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(opening, 1);
    }

    #[test]
    fn migrates_saved_api_fields_into_gateway_kind() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE custom_runtimes (
               id TEXT PRIMARY KEY,
               name TEXT NOT NULL,
               command TEXT NOT NULL,
               args_json TEXT NOT NULL DEFAULT '[]',
               provider TEXT NOT NULL DEFAULT 'custom',
               protocol TEXT NOT NULL DEFAULT '',
               api_url TEXT NOT NULL DEFAULT '',
               api_key TEXT NOT NULL DEFAULT '',
               models_json TEXT NOT NULL DEFAULT '[]',
               env_json TEXT NOT NULL DEFAULT '{}'
             );
             INSERT INTO custom_runtimes (id, name, command, api_url, api_key) VALUES
               ('custom:gw', 'Gateway', 'claude', 'https://gw.example/v1', 'sk'),
               ('custom:plain', 'Plain', 'cursor', '', '');",
        )
        .unwrap();
        migrate(&db).unwrap();
        let gateway: String = db
            .query_row("SELECT kind FROM custom_runtimes WHERE id='custom:gw'", [], |row| row.get(0))
            .unwrap();
        let plain: String = db
            .query_row("SELECT kind FROM custom_runtimes WHERE id='custom:plain'", [], |row| row.get(0))
            .unwrap();
        assert_eq!(gateway, "gateway");
        assert_eq!(plain, "profile");
    }
}
