//! 引擎单测：开始把输入交给智能体，智能体的正文再交给结束。

use std::cell::Cell;
use std::collections::BTreeMap;
use std::path::PathBuf;

use rusqlite::{params, Connection};
use serde_json::{json, Value};

use super::engine::{self, Action, ForkRequest, Host, PreparedSlot, SlotRequest, StartRun};
use super::model::WorkflowDef;
use super::store;

struct FakeHost {
    clock: Cell<i64>,
}

impl FakeHost {
    fn new() -> Self {
        FakeHost { clock: Cell::new(0) }
    }
}

impl Host for FakeHost {
    fn now(&self) -> String {
        self.clock.set(self.clock.get() + 1);
        let base = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00+00:00").unwrap();
        (base + chrono::Duration::milliseconds(self.clock.get())).to_rfc3339()
    }
    fn artifacts_root(&self) -> Option<PathBuf> { None }
    fn secrets(&self) -> BTreeMap<String, String> { BTreeMap::new() }
    fn git_head(&self, _path: &str) -> Option<String> { Some("c0".into()) }
    fn git_commit_all(&self, _path: &str, _message: &str) -> Result<Option<String>, String> { Ok(None) }
    fn git_reset_hard(&self, _path: &str, _commit: &str) -> Result<(), String> { Ok(()) }
    fn prepare_slot(&self, _db: &Connection, _request: &SlotRequest<'_>) -> Result<PreparedSlot, String> {
        Ok(PreparedSlot { workspace_id: "ws".into(), base_commit: None })
    }
    fn create_fork(&self, _db: &Connection, _request: &ForkRequest<'_>) -> Result<PreparedSlot, String> {
        Ok(PreparedSlot { workspace_id: "fork".into(), base_commit: None })
    }
}

fn db() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    crate::database::migrate(&db).unwrap();
    db.execute_batch(
        "INSERT OR IGNORE INTO agents (id, name, avatar, description, instructions, runtime_id, model, max_concurrent_tasks)
         VALUES ('coder', '编码', '', '', '', 'cursor', 'auto', 3),
                ('miku', 'Miku', '', '', '', 'cursor', 'auto', 3);",
    )
    .unwrap();
    db
}

fn flow() -> Value {
    json!({
        "inputs": [{ "name": "question", "label": "问题", "required": true, "primary": true }],
        "nodes": [
            { "id": "start", "kind": "trigger", "title": "开始" },
            { "id": "ask", "kind": "agent", "title": "回答", "config": {
                "target": { "type": "agent", "id": "coder" },
                "prompt": "回答问题",
                "context": "previous",
                "route": "auto"
            }},
            { "id": "end", "kind": "output", "title": "结束" }
        ],
        "edges": [
            { "id": "e1", "source": "start", "target": "ask" },
            { "id": "e2", "source": "ask", "target": "end" }
        ]
    })
}

fn create(db: &Connection, def: Value) -> String {
    let def: WorkflowDef = serde_json::from_value(def).unwrap();
    let errors: Vec<_> = super::validate::validate(&def, &store::DbLookup(db)).into_iter().filter(|item| item.severity == "error").collect();
    assert!(errors.is_empty(), "{errors:?}");
    let wf = store::create_workflow(db, "测试流程", "", &def, "2026-01-01T00:00:00+00:00").unwrap();
    store::publish(db, &wf.id, "", "2026-01-01T00:00:00+00:00").unwrap();
    wf.id
}

fn current(db: &Connection, run_id: &str, node: &str) -> store::NodeRun {
    store::current_node_runs(db, run_id).unwrap().remove(node).expect(node)
}

#[test]
fn missing_required_run_input_is_rejected() {
    let db = db();
    let host = FakeHost::new();
    let id = create(&db, flow());
    let error = engine::start_run(&db, &host, StartRun {
        workflow_id: &id,
        version: None,
        inputs: json!({}),
        source: "manual",
        trigger_ref: None,
        parent_node_run_id: None,
        map_index: None,
        extra: json!({}),
        untrusted: false,
    }).unwrap_err();
    assert!(error.contains("question") || error.contains("问题"), "{error}");
}

#[test]
fn agent_text_is_handed_to_the_next_step() {
    let db = db();
    let host = FakeHost::new();
    let id = create(&db, flow());
    let (run_id, _) = engine::start_run(&db, &host, StartRun {
        workflow_id: &id,
        version: None,
        inputs: json!({ "question": "升级风险" }),
        source: "manual",
        trigger_ref: None,
        parent_node_run_id: None,
        map_index: None,
        extra: json!({}),
        untrusted: false,
    }).unwrap();
    let ask = current(&db, &run_id, "ask");
    assert_eq!(ask.status, "running");
    let session = ask.session_id.clone().unwrap();
    let brief: String = db.query_row("SELECT content FROM messages WHERE session_id=?1 AND role='user'", [&session], |row| row.get(0)).unwrap();
    assert!(brief.contains("升级风险"), "{brief}");
    assert!(brief.contains("回答问题"), "{brief}");
    db.execute(
        "INSERT INTO messages (id, session_id, role, content, created_at) VALUES (?1, ?2, 'assistant', ?3, ?4)",
        params![uuid::Uuid::new_v4().to_string(), session, "可以升级", host.now()],
    ).unwrap();
    db.execute("UPDATE agent_runs SET status='completed' WHERE session_id=?1", [&session]).unwrap();
    engine::on_session_idle(&db, &host, &session).unwrap();
    let end = current(&db, &run_id, "end");
    assert_eq!(end.content, "可以升级");
    assert_eq!(store::run(&db, &run_id).unwrap().status, "succeeded");
    let tasks: i64 = db.query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0)).unwrap();
    assert_eq!(tasks, 0);
}

#[test]
fn confirmation_form_keeps_the_step_waiting() {
    let db = db();
    let host = FakeHost::new();
    let id = create(&db, flow());
    let (run_id, _) = engine::start_run(&db, &host, StartRun {
        workflow_id: &id,
        version: None,
        inputs: json!({ "question": "建仓" }),
        source: "manual",
        trigger_ref: None,
        parent_node_run_id: None,
        map_index: None,
        extra: json!({}),
        untrusted: false,
    }).unwrap();
    let ask = current(&db, &run_id, "ask");
    let session = ask.session_id.clone().unwrap();
    let reply = "```multara-ui\n{\"type\":\"form\",\"title\":\"确认绑定仓库\",\"props\":{\"submitLabel\":\"确认仓库\",\"fields\":[]}}\n```";
    db.execute(
        "INSERT INTO messages (id, session_id, role, content, created_at) VALUES (?1, ?2, 'assistant', ?3, ?4)",
        params![uuid::Uuid::new_v4().to_string(), session, reply, host.now()],
    ).unwrap();
    db.execute("UPDATE agent_runs SET status='completed' WHERE session_id=?1", [&session]).unwrap();
    engine::on_session_idle(&db, &host, &session).unwrap();
    assert_eq!(current(&db, &run_id, "ask").status, "waiting_human");
    assert_eq!(current(&db, &run_id, "end").status, "pending");
    assert_eq!(store::run(&db, &run_id).unwrap().status, "waiting");
}

#[test]
fn a_follow_up_question_does_not_finish_the_step() {
    let db = db();
    let host = FakeHost::new();
    let id = create(&db, flow());
    let (run_id, _) = engine::start_run(&db, &host, StartRun {
        workflow_id: &id,
        version: None,
        inputs: json!({ "question": "建仓" }),
        source: "manual",
        trigger_ref: None,
        parent_node_run_id: None,
        map_index: None,
        extra: json!({}),
        untrusted: false,
    }).unwrap();
    let session = current(&db, &run_id, "ask").session_id.clone().unwrap();
    let form = "```multara-ui\n{\"type\":\"form\",\"title\":\"确认绑定仓库\"}\n```";
    db.execute(
        "INSERT INTO messages (id, session_id, role, content, created_at) VALUES (?1, ?2, 'assistant', ?3, ?4)",
        params![uuid::Uuid::new_v4().to_string(), &session, form, "2026-01-01T00:00:01+00:00"],
    ).unwrap();
    db.execute("UPDATE agent_runs SET status='completed' WHERE session_id=?1", [&session]).unwrap();
    engine::on_session_idle(&db, &host, &session).unwrap();
    db.execute(
        "INSERT INTO messages (id, session_id, role, content, created_at) VALUES (?1, ?2, 'user', ?3, ?4)",
        params![uuid::Uuid::new_v4().to_string(), &session, "这是什么，给我一个sit环境的地址", "2026-01-01T00:00:02+00:00"],
    ).unwrap();
    db.execute(
        "INSERT INTO messages (id, session_id, role, content, created_at) VALUES (?1, ?2, 'assistant', ?3, ?4)",
        params![uuid::Uuid::new_v4().to_string(), &session, "SIT 地址：https://sit.example/login", "2026-01-01T00:00:03+00:00"],
    ).unwrap();
    engine::on_session_idle(&db, &host, &session).unwrap();
    assert_eq!(current(&db, &run_id, "ask").status, "waiting_human");
    assert_eq!(current(&db, &run_id, "end").status, "pending");
}

#[test]
fn a_submitted_confirmation_lets_the_step_finish() {
    let db = db();
    let host = FakeHost::new();
    let id = create(&db, flow());
    let (run_id, _) = engine::start_run(&db, &host, StartRun {
        workflow_id: &id,
        version: None,
        inputs: json!({ "question": "建仓" }),
        source: "manual",
        trigger_ref: None,
        parent_node_run_id: None,
        map_index: None,
        extra: json!({}),
        untrusted: false,
    }).unwrap();
    let session = current(&db, &run_id, "ask").session_id.clone().unwrap();
    let form = "```multara-ui\n{\"type\":\"form\",\"title\":\"确认绑定仓库\"}\n```";
    db.execute(
        "INSERT INTO messages (id, session_id, role, content, created_at) VALUES (?1, ?2, 'assistant', ?3, ?4)",
        params![uuid::Uuid::new_v4().to_string(), &session, form, "2026-01-01T00:00:01+00:00"],
    ).unwrap();
    db.execute(
        "INSERT INTO messages (id, session_id, role, content, source_type, created_at) VALUES (?1, ?2, 'user', ?3, 'genui', ?4)",
        params![uuid::Uuid::new_v4().to_string(), &session, "[确认] submit", "2026-01-01T00:00:02+00:00"],
    ).unwrap();
    db.execute(
        "INSERT INTO messages (id, session_id, role, content, created_at) VALUES (?1, ?2, 'assistant', ?3, ?4)",
        params![uuid::Uuid::new_v4().to_string(), &session, "仓库已绑定。", "2026-01-01T00:00:03+00:00"],
    ).unwrap();
    db.execute("UPDATE agent_runs SET status='completed' WHERE session_id=?1", [&session]).unwrap();
    engine::on_session_idle(&db, &host, &session).unwrap();
    assert_eq!(current(&db, &run_id, "ask").status, "succeeded");
    assert_eq!(store::run(&db, &run_id).unwrap().status, "succeeded");
}

#[test]
fn overview_does_not_create_a_task() {
    let db = db();
    let host = FakeHost::new();
    let id = create(&db, flow());
    let (run_id, _) = engine::start_run(&db, &host, StartRun {
        workflow_id: &id,
        version: None,
        inputs: json!({ "question": "hi" }),
        source: "manual",
        trigger_ref: None,
        parent_node_run_id: None,
        map_index: None,
        extra: json!({}),
        untrusted: false,
    }).unwrap();
    let run = store::run(&db, &run_id).unwrap();
    assert!(run.task_id.is_none());
    let _ = Action::DispatchAgents;
}
