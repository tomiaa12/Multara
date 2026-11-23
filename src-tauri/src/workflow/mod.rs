//! 工作流：在画布上用节点和连线编排智能体、脚本、人工审批、分支、循环和子流程。
//! 设计见 docs/工作流节点技术方案.md。

pub mod brief;
pub mod cli;
pub mod commands;
pub mod engine;
pub mod graph;
pub mod model;
pub mod runtime;
pub mod schema;
pub mod store;
pub mod template;
pub mod templates;
pub mod validate;

use std::collections::BTreeMap;

use rusqlite::Connection;

pub use engine::{is_plain_agent_node_session, is_workflow_session, WF_NODE_PREFIX, WF_RUN_PREFIX};

pub(crate) fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS workflows (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          slug TEXT NOT NULL,
          description TEXT NOT NULL DEFAULT '',
          icon TEXT NOT NULL DEFAULT '',
          draft_json TEXT NOT NULL,
          draft_hash TEXT NOT NULL DEFAULT '',
          draft_updated_at TEXT NOT NULL,
          published_version INTEGER,
          enabled INTEGER NOT NULL DEFAULT 1,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_workflows_slug ON workflows(slug);
        CREATE TABLE IF NOT EXISTS workflow_versions (
          workflow_id TEXT NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
          version INTEGER NOT NULL,
          definition_json TEXT NOT NULL,
          definition_hash TEXT NOT NULL,
          note TEXT NOT NULL DEFAULT '',
          created_at TEXT NOT NULL,
          PRIMARY KEY (workflow_id, version)
        );
        CREATE TABLE IF NOT EXISTS workflow_runs (
          id TEXT PRIMARY KEY,
          workflow_id TEXT NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
          version INTEGER NOT NULL,
          status TEXT NOT NULL CHECK(status IN ('queued','running','waiting','succeeded','failed','cancelled','skipped')),
          inputs_json TEXT NOT NULL DEFAULT '{}',
          outputs_json TEXT NOT NULL DEFAULT '{}',
          trigger_source TEXT NOT NULL CHECK(trigger_source IN ('manual','automation','channel','subflow','map','cli')),
          trigger_ref TEXT,
          parent_node_run_id TEXT,
          map_index INTEGER,
          extra_json TEXT NOT NULL DEFAULT '{}',
          overview_session_id TEXT NOT NULL,
          task_id TEXT,
          error_code TEXT,
          error TEXT,
          created_at TEXT NOT NULL,
          started_at TEXT,
          completed_at TEXT,
          content TEXT NOT NULL DEFAULT '',
          metadata_json TEXT NOT NULL DEFAULT '{}',
          next_node_id TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_workflow_runs_wf ON workflow_runs(workflow_id, created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_workflow_runs_status ON workflow_runs(status);
        CREATE INDEX IF NOT EXISTS idx_workflow_runs_parent ON workflow_runs(parent_node_run_id);
        CREATE INDEX IF NOT EXISTS idx_workflow_runs_overview ON workflow_runs(overview_session_id);
        CREATE TABLE IF NOT EXISTS workflow_run_workspaces (
          run_id TEXT NOT NULL REFERENCES workflow_runs(id) ON DELETE CASCADE,
          slot_key TEXT NOT NULL,
          workspace_id TEXT NOT NULL,
          base_commit TEXT,
          head_commit TEXT,
          owned INTEGER NOT NULL DEFAULT 1,
          PRIMARY KEY (run_id, slot_key)
        );
        CREATE TABLE IF NOT EXISTS workflow_workspace_locks (
          workspace_id TEXT PRIMARY KEY,
          node_run_id TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS workflow_node_runs (
          id TEXT PRIMARY KEY,
          run_id TEXT NOT NULL REFERENCES workflow_runs(id) ON DELETE CASCADE,
          node_id TEXT NOT NULL,
          kind TEXT NOT NULL,
          iteration INTEGER NOT NULL DEFAULT 0,
          attempt INTEGER NOT NULL DEFAULT 1,
          status TEXT NOT NULL CHECK(status IN ('pending','ready','running','waiting_human','waiting_children',
                                                'succeeded','failed','skipped','cancelled','reused')),
          inputs_json TEXT,
          fired_ports_json TEXT NOT NULL DEFAULT '[]',
          carried_json TEXT,
          state_json TEXT,
          session_id TEXT,
          repair_count INTEGER NOT NULL DEFAULT 0,
          workspace_before_json TEXT,
          workspace_after_json TEXT,
          fork_workspace_id TEXT,
          skip_reason TEXT,
          error_code TEXT,
          error TEXT,
          retry_at TEXT,
          deadline_at TEXT,
          created_at TEXT NOT NULL,
          started_at TEXT,
          completed_at TEXT,
          UNIQUE (run_id, node_id, iteration, attempt)
        );
        CREATE INDEX IF NOT EXISTS idx_wf_node_runs_run ON workflow_node_runs(run_id, node_id, iteration DESC, attempt DESC);
        CREATE INDEX IF NOT EXISTS idx_wf_node_runs_session ON workflow_node_runs(session_id);
        CREATE INDEX IF NOT EXISTS idx_wf_node_runs_timer ON workflow_node_runs(status, deadline_at, retry_at);
        CREATE TABLE IF NOT EXISTS workflow_values (
          id TEXT PRIMARY KEY,
          run_id TEXT NOT NULL REFERENCES workflow_runs(id) ON DELETE CASCADE,
          node_run_id TEXT NOT NULL REFERENCES workflow_node_runs(id) ON DELETE CASCADE,
          node_id TEXT NOT NULL,
          port TEXT NOT NULL,
          type TEXT NOT NULL,
          value_json TEXT,
          artifact_path TEXT,
          size INTEGER NOT NULL,
          source TEXT NOT NULL CHECK(source IN ('agent_cli','agent_text','script','engine','user')),
          created_at TEXT NOT NULL,
          UNIQUE (node_run_id, port)
        );
        CREATE INDEX IF NOT EXISTS idx_wf_values_run ON workflow_values(run_id);
        CREATE TABLE IF NOT EXISTS workflow_loop_counters (
          run_id TEXT NOT NULL REFERENCES workflow_runs(id) ON DELETE CASCADE,
          edge_id TEXT NOT NULL,
          count INTEGER NOT NULL DEFAULT 0,
          PRIMARY KEY (run_id, edge_id)
        );
        CREATE TABLE IF NOT EXISTS workflow_gate_decisions (
          node_run_id TEXT PRIMARY KEY REFERENCES workflow_node_runs(id) ON DELETE CASCADE,
          decision TEXT NOT NULL,
          comment TEXT NOT NULL DEFAULT '',
          form_json TEXT NOT NULL DEFAULT '{}',
          source TEXT NOT NULL CHECK(source IN ('ui','chat','feishu','timeout','cli','user')),
          decided_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS workflow_events (
          id TEXT PRIMARY KEY,
          run_id TEXT NOT NULL REFERENCES workflow_runs(id) ON DELETE CASCADE,
          node_run_id TEXT,
          seq INTEGER NOT NULL,
          kind TEXT NOT NULL,
          message TEXT NOT NULL,
          data_json TEXT NOT NULL DEFAULT '{}',
          created_at TEXT NOT NULL,
          UNIQUE (run_id, seq)
        );
        CREATE TABLE IF NOT EXISTS workflow_secrets (
          name TEXT PRIMARY KEY,
          description TEXT NOT NULL DEFAULT '',
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS workflow_node_templates (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          description TEXT NOT NULL DEFAULT '',
          template_json TEXT NOT NULL,
          created_at TEXT NOT NULL
        );
        ",
    )?;
    for sql in [
        "ALTER TABLE workflow_node_runs ADD COLUMN content TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE workflow_node_runs ADD COLUMN metadata_json TEXT NOT NULL DEFAULT '{}'",
        "ALTER TABLE workflow_node_runs ADD COLUMN next_node_id TEXT",
        "ALTER TABLE workflows ADD COLUMN linked_task_ids TEXT NOT NULL DEFAULT '[]'",
    ] {
        let _ = db.execute(sql, []);
    }
    let legacy: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM workflows WHERE draft_json LIKE '%sourcePort%' OR draft_json LIKE '%\"kind\":\"script\"%' OR draft_json LIKE '%\"kind\":\"note\"%')",
            [],
            |row| row.get(0),
        )
        .unwrap_or(false);
    if legacy {
        let _ = db.execute_batch(
            "
            DELETE FROM inbox_items WHERE session_id LIKE 'wf-run:%' OR session_id LIKE 'wf-node:%';
            DELETE FROM tasks WHERE id IN (SELECT task_id FROM workflow_runs WHERE task_id IS NOT NULL);
            DELETE FROM chat_sessions WHERE id LIKE 'wf-run:%' OR id LIKE 'wf-node:%';
            DELETE FROM workflows;
            ",
        );
    }
    Ok(())
}

/// 错误信息、日志写库前把已知密钥值替换掉
pub fn runtime_redact(text: &str, secrets: &BTreeMap<String, String>) -> String {
    let mut out = text.to_string();
    for (name, value) in secrets {
        if value.len() >= 4 {
            out = out.replace(value.as_str(), &format!("<secret:{name}>"));
        }
    }
    out
}

#[cfg(test)]
mod tests;
