//! 引擎单测：内存 SQLite + 假 Host，手动模拟 agent 完成和后台任务结果（技术方案 14.1）。

use std::cell::Cell;
use std::collections::BTreeMap;
use std::path::PathBuf;

use rusqlite::{params, Connection};
use serde_json::{json, Map, Value};

use super::engine::{self, Action, ForkRequest, Host, JobResult, PreparedSlot, SlotRequest, StartRun};
use super::model::WorkflowDef;
use super::store;

struct FakeHost {
    clock: Cell<i64>,
    commits: Cell<u32>,
}

impl FakeHost {
    fn new() -> Self {
        FakeHost { clock: Cell::new(0), commits: Cell::new(0) }
    }
    fn advance_secs(&self, secs: i64) {
        self.clock.set(self.clock.get() + secs * 1000);
    }
}

impl Host for FakeHost {
    fn now(&self) -> String {
        // 每次取时间都前进 1ms，保证先后顺序
        self.clock.set(self.clock.get() + 1);
        let base = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00+00:00").unwrap();
        (base + chrono::Duration::milliseconds(self.clock.get())).to_rfc3339()
    }
    fn artifacts_root(&self) -> Option<PathBuf> {
        None
    }
    fn secrets(&self) -> BTreeMap<String, String> {
        BTreeMap::new()
    }
    fn git_head(&self, _path: &str) -> Option<String> {
        Some(format!("c{}", self.commits.get()))
    }
    fn git_commit_all(&self, _path: &str, _message: &str) -> Result<Option<String>, String> {
        self.commits.set(self.commits.get() + 1);
        Ok(Some(format!("c{}", self.commits.get())))
    }
    fn git_reset_hard(&self, _path: &str, _commit: &str) -> Result<(), String> {
        Ok(())
    }
    fn prepare_slot(&self, db: &Connection, request: &SlotRequest<'_>) -> Result<PreparedSlot, String> {
        let id = format!("ws-{}", request.slot.key);
        db.execute(
            "INSERT OR IGNORE INTO workspaces (id, project_id, session_id, name, kind, source_path, work_path, branch, isolation_mode, created_at)
             VALUES (?1, ?2, ?3, 'repo', 'git', '/src', ?4, ?5, 'worktree', '1')",
            params![id, request.project_id, request.overview_session_id, format!("/tmp/{id}"), request.branch],
        )
        .map_err(|e| e.to_string())?;
        Ok(PreparedSlot { workspace_id: id, base_commit: Some("c0".into()) })
    }
    fn create_fork(&self, db: &Connection, request: &ForkRequest<'_>) -> Result<PreparedSlot, String> {
        let id = format!("fork-{}", request.branch);
        db.execute(
            "INSERT INTO workspaces (id, project_id, session_id, name, kind, source_path, work_path, branch, isolation_mode, created_at)
             SELECT ?1, project_id, ?2, 'fork', 'git', '/src', ?3, ?4, 'worktree', '2' FROM workspaces WHERE id=?5",
            params![id, request.overview_session_id, format!("/tmp/{id}"), request.branch, request.source_workspace_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(PreparedSlot { workspace_id: id, base_commit: None })
    }
}

fn db() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    crate::database::migrate(&db).unwrap();
    db.execute_batch(
        "INSERT INTO projects (id, name, created_at, opened_at) VALUES ('p1', '需求', 'now', 'now');
         INSERT OR IGNORE INTO agents (id, name, avatar, description, instructions, runtime_id, model, max_concurrent_tasks)
         VALUES ('coder', '编码', '', '', '', 'cursor', 'auto', 3),
                ('miku', 'Miku', '', '', '', 'cursor', 'auto', 3);",
    )
    .unwrap();
    db
}

fn create(db: &Connection, def: Value) -> String {
    let def: WorkflowDef = serde_json::from_value(def).unwrap();
    let diagnostics = super::validate::validate(&def, &store::DbLookup(db));
    let errors: Vec<_> = diagnostics.iter().filter(|d| d.severity == "error").collect();
    assert!(errors.is_empty(), "{errors:?}");
    let wf = store::create_workflow(db, "测试流程", "", &def, "2026-01-01T00:00:00+00:00").unwrap();
    store::publish(db, &wf.id, "", "2026-01-01T00:00:00+00:00").unwrap();
    wf.id
}

fn start(db: &Connection, host: &FakeHost, workflow_id: &str, inputs: Value) -> (String, Vec<Action>) {
    engine::start_run(db, host, StartRun {
        workflow_id,
        version: None,
        inputs,
        source: "manual",
        trigger_ref: None,
        parent_node_run_id: None,
        map_index: None,
        extra: json!({}),
        untrusted: false,
    })
    .unwrap()
}

fn current(db: &Connection, run_id: &str, node: &str) -> store::NodeRun {
    store::current_node_runs(db, run_id).unwrap().remove(node).unwrap_or_else(|| panic!("no node {node}"))
}

fn status(db: &Connection, run_id: &str) -> String {
    store::run(db, run_id).unwrap().status
}

fn runs_of(db: &Connection, run_id: &str, node: &str) -> Vec<store::NodeRun> {
    store::node_runs(db, run_id).unwrap().into_iter().filter(|n| n.node_id == node).collect()
}

/// 模拟 agent 跑完：提交输出、写回答、把 run 标成完成
fn finish_agent(db: &Connection, host: &FakeHost, run_id: &str, node: &str, outputs: &[(&str, Value)], text: &str) -> Vec<Action> {
    let nr = current(db, run_id, node);
    assert_eq!(nr.status, "running", "{node} should be running");
    let session = nr.session_id.clone().unwrap();
    for (port, value) in outputs {
        engine::submit_output(db, &nr.id, port, value.clone(), &host.now()).unwrap();
    }
    db.execute(
        "INSERT INTO messages (id, session_id, role, content, created_at) VALUES (?1, ?2, 'assistant', ?3, ?4)",
        params![uuid::Uuid::new_v4().to_string(), session, text, host.now()],
    )
    .unwrap();
    db.execute("UPDATE agent_runs SET status='completed' WHERE session_id=?1 AND status IN ('queued','running')", [&session])
        .unwrap();
    let mut acts = engine::on_session_idle(db, host, &session).unwrap();
    acts.extend(engine::advance(db, host, run_id).unwrap());
    acts
}

fn finish_job(db: &Connection, host: &FakeHost, run_id: &str, node: &str, outputs: Value, error: Option<(&str, &str)>) -> Vec<Action> {
    let nr = current(db, run_id, node);
    assert_eq!(nr.status, "running", "{node} should be running");
    let mut acts = engine::complete_job(db, host, &nr.id, JobResult {
        outputs: outputs.as_object().cloned().unwrap_or_default(),
        error: error.map(|(c, m)| (c.to_string(), m.to_string())),
    })
    .unwrap();
    acts.extend(engine::advance(db, host, run_id).unwrap());
    acts
}

fn script_result(code: i64) -> Value {
    json!({
        "exit_code": code, "ok": code == 0, "stdout": format!("exit {code}"), "stderr": "",
        "$result": { "exit_code": code, "ok": code == 0, "stdout": format!("exit {code}"), "stderr": "" }
    })
}

fn agent_node(id: &str, inputs: Value, outputs: Value, extra: Value) -> Value {
    let mut node = json!({
        "id": id, "kind": "agent", "title": id,
        "inputs": inputs, "outputs": outputs,
        "config": { "target": { "type": "agent", "id": "coder" }, "prompt": "do it", "maxRepairTurns": 1 }
    });
    if let Some(map) = extra.as_object() {
        for (k, v) in map {
            node[k] = v.clone();
        }
    }
    node
}

#[test]
fn linear_transforms_flow_values_to_outputs() {
    let db = db();
    let host = FakeHost::new();
    let wf = create(&db, json!({
        "inputs": [{ "name": "name", "type": "text", "required": true }],
        "outputs": [{ "name": "greeting", "type": "text" }],
        "nodes": [
            { "id": "start", "kind": "trigger" },
            { "id": "greet", "kind": "transform", "inputs": [{ "name": "name", "type": "text", "required": true }],
              "outputs": [{ "name": "text", "type": "text" }],
              "config": { "assignments": [{ "port": "text", "expr": "'hi ' ~ inputs.name ~ ' #' ~ run.inputs.name" }] } },
            { "id": "done", "kind": "output", "inputs": [{ "name": "greeting", "type": "text" }] }
        ],
        "edges": [
            { "id": "e1", "source": "start", "sourcePort": "name", "target": "greet", "targetPort": "name" },
            { "id": "e2", "source": "greet", "sourcePort": "text", "target": "done", "targetPort": "greeting" }
        ]
    }));
    let (run_id, acts) = start(&db, &host, &wf, json!({ "name": "Ada" }));
    let run = store::run(&db, &run_id).unwrap();
    assert_eq!(run.status, "succeeded");
    assert_eq!(run.outputs, json!({ "greeting": "hi Ada #Ada" }));
    assert!(acts.iter().any(|a| matches!(a, Action::RunFinished { status, .. } if status == "succeeded")));
    // 总览会话与任务卡
    let task_status: String = db.query_row("SELECT status FROM tasks WHERE id=?1", [run.task_id.unwrap()], |r| r.get(0)).unwrap();
    assert_eq!(task_status, "done");
}

#[test]
fn missing_required_run_input_is_rejected() {
    let db = db();
    let host = FakeHost::new();
    let wf = create(&db, json!({
        "inputs": [{ "name": "name", "type": "text", "required": true }],
        "nodes": [{ "id": "start", "kind": "trigger" }],
        "edges": []
    }));
    let result = engine::start_run(&db, &host, StartRun {
        workflow_id: &wf, version: None, inputs: json!({}), source: "manual", trigger_ref: None,
        parent_node_run_id: None, map_index: None, extra: json!({}), untrusted: false,
    });
    assert!(result.is_err());
}

#[test]
fn switch_skips_untaken_branch_and_join_still_runs() {
    let db = db();
    let host = FakeHost::new();
    let wf = create(&db, json!({
        "inputs": [{ "name": "n", "type": "number", "required": true }],
        "outputs": [{ "name": "path", "type": "text" }],
        "nodes": [
            { "id": "start", "kind": "trigger" },
            { "id": "pick", "kind": "switch", "inputs": [{ "name": "value", "type": "number", "required": true }],
              "config": { "cases": [{ "port": "big", "when": "inputs.value > 10" }], "defaultPort": "small" } },
            { "id": "big_path", "kind": "transform", "outputs": [{ "name": "path", "type": "text" }], "config": { "assignments": [{ "port": "path", "expr": "'big'" }] } },
            { "id": "small_path", "kind": "transform", "outputs": [{ "name": "path", "type": "text" }], "config": { "assignments": [{ "port": "path", "expr": "'small'" }] } },
            { "id": "after_big", "kind": "transform", "config": { "assignments": [] } },
            { "id": "done", "kind": "output", "join": "any", "inputs": [{ "name": "path", "type": "text", "multi": true }] }
        ],
        "edges": [
            { "id": "e1", "source": "start", "sourcePort": "n", "target": "pick", "targetPort": "value" },
            { "id": "e2", "source": "pick", "sourcePort": "big", "target": "big_path", "targetPort": "$in" },
            { "id": "e3", "source": "pick", "sourcePort": "small", "target": "small_path", "targetPort": "$in" },
            { "id": "e4", "source": "big_path", "sourcePort": "$out", "target": "after_big", "targetPort": "$in" },
            { "id": "e5", "source": "big_path", "sourcePort": "path", "target": "done", "targetPort": "path" },
            { "id": "e6", "source": "small_path", "sourcePort": "path", "target": "done", "targetPort": "path" }
        ]
    }));
    let (run_id, _) = start(&db, &host, &wf, json!({ "n": 3 }));
    assert_eq!(status(&db, &run_id), "succeeded");
    assert_eq!(current(&db, &run_id, "big_path").status, "skipped");
    assert_eq!(current(&db, &run_id, "after_big").status, "skipped");
    assert_eq!(store::run(&db, &run_id).unwrap().outputs, json!({ "path": ["small"] }));
}

#[test]
fn parallel_branches_join_all() {
    let db = db();
    let host = FakeHost::new();
    let wf = create(&db, json!({
        "nodes": [
            { "id": "start", "kind": "trigger" },
            { "id": "a", "kind": "script", "config": { "command": "echo a" } },
            { "id": "b", "kind": "script", "config": { "command": "echo b" } },
            { "id": "join", "kind": "transform", "config": { "assignments": [] } }
        ],
        "edges": [
            { "id": "e1", "source": "start", "sourcePort": "$out", "target": "a", "targetPort": "$in" },
            { "id": "e2", "source": "start", "sourcePort": "$out", "target": "b", "targetPort": "$in" },
            { "id": "e3", "source": "a", "sourcePort": "$out", "target": "join", "targetPort": "$in" },
            { "id": "e4", "source": "b", "sourcePort": "$out", "target": "join", "targetPort": "$in" }
        ]
    }));
    let (run_id, acts) = start(&db, &host, &wf, json!({}));
    let jobs = acts.iter().filter(|a| matches!(a, Action::Job(_))).count();
    assert_eq!(jobs, 2, "a and b start in parallel");
    finish_job(&db, &host, &run_id, "a", script_result(0), None);
    assert_eq!(current(&db, &run_id, "join").status, "pending");
    finish_job(&db, &host, &run_id, "b", script_result(0), None);
    assert_eq!(status(&db, &run_id), "succeeded");
}

fn loop_def() -> Value {
    json!({
        "workspaces": [{ "key": "app", "project": "p1", "isolation": "worktree_new" }],
        "nodes": [
            { "id": "start", "kind": "trigger" },
            agent_node("code", json!([{ "name": "feedback", "type": "any", "multi": true }]),
                json!([{ "name": "summary", "type": "markdown", "required": true }]),
                json!({ "workspaces": [{ "slot": "app", "access": "write" }] })),
            { "id": "test", "kind": "script", "config": { "command": "pnpm test", "cwd": { "slot": "app" }, "failOnNonZero": false } },
            { "id": "check", "kind": "switch", "inputs": [{ "name": "value", "type": "json", "required": true }],
              "config": { "cases": [{ "port": "pass", "when": "inputs.value.exit_code == 0" }], "defaultPort": "fail" } },
            { "id": "accept", "kind": "gate", "inputs": [{ "name": "summary", "type": "markdown" }],
              "config": { "title": "验收", "body": "{{ inputs.summary }}" } }
        ],
        "edges": [
            { "id": "e1", "source": "start", "sourcePort": "$out", "target": "code", "targetPort": "$in" },
            { "id": "e2", "source": "code", "sourcePort": "$out", "target": "test", "targetPort": "$in" },
            { "id": "e3", "source": "test", "sourcePort": "$result", "target": "check", "targetPort": "value" },
            { "id": "e4", "source": "check", "sourcePort": "fail", "target": "code", "targetPort": "feedback", "maxIterations": 2 },
            { "id": "e5", "source": "check", "sourcePort": "pass", "target": "accept", "targetPort": "$in" },
            { "id": "e6", "source": "code", "sourcePort": "summary", "target": "accept", "targetPort": "summary" },
            { "id": "e7", "source": "accept", "sourcePort": "changes", "target": "code", "targetPort": "feedback", "maxIterations": 3 }
        ]
    })
}

#[test]
fn test_failure_loops_back_with_feedback_then_gate_approves() {
    let db = db();
    let host = FakeHost::new();
    let wf = create(&db, loop_def());
    let (run_id, acts) = start(&db, &host, &wf, json!({}));
    assert!(acts.iter().any(|a| matches!(a, Action::DispatchAgents)));
    // 节点会话挂了槽位工作区
    let session = current(&db, &run_id, "code").session_id.unwrap();
    let linked: i64 = db.query_row("SELECT COUNT(*) FROM session_workspaces WHERE session_id=?1 AND workspace_id='ws-app'", [&session], |r| r.get(0)).unwrap();
    assert_eq!(linked, 1);
    let brief: String = db.query_row("SELECT content FROM messages WHERE session_id=?1 AND role='user'", [&session], |r| r.get(0)).unwrap();
    assert!(brief.contains("multara node output summary"));

    finish_agent(&db, &host, &run_id, "code", &[("summary", json!("v1"))], "done");
    assert_eq!(host.commits.get(), 1, "auto commit after write node");
    finish_job(&db, &host, &run_id, "test", script_result(1), None);
    // 回到 code 第二轮，带着反馈；同一会话续上
    let code = current(&db, &run_id, "code");
    assert_eq!(code.iteration, 1);
    assert_eq!(code.status, "running");
    assert_eq!(code.session_id.as_deref(), Some(session.as_str()));
    assert_eq!(code.inputs["feedback"][0]["exit_code"], json!(1));
    finish_agent(&db, &host, &run_id, "code", &[("summary", json!("v2"))], "done");
    finish_job(&db, &host, &run_id, "test", script_result(0), None);
    let gate = current(&db, &run_id, "accept");
    assert_eq!(gate.status, "waiting_human");
    assert_eq!(status(&db, &run_id), "waiting");
    let inbox: String = db.query_row("SELECT status FROM inbox_items WHERE session_id=?1", [format!("wf-run:{run_id}")], |r| r.get(0)).unwrap();
    assert_eq!(inbox, "needs_input");
    // 快捷操作带 action
    let actions: String = db.query_row(
        "SELECT quick_actions_json FROM messages WHERE session_id=?1 AND quick_actions_json != '[]'",
        [format!("wf-run:{run_id}")], |r| r.get(0)).unwrap();
    assert!(actions.contains("workflow.gate"));

    // 打回需要说明
    assert!(engine::decide_gate(&db, &host, &gate.id, "changes", "", Value::Null, "ui").is_err());
    // 聊天里打字打回
    engine::handle_overview_message(&db, &host, &format!("wf-run:{run_id}"), "打回：按钮颜色不对").unwrap();
    engine::advance(&db, &host, &run_id).unwrap();
    let code = current(&db, &run_id, "code");
    assert_eq!(code.iteration, 2);
    assert_eq!(code.inputs["feedback"][0]["comment"], json!("按钮颜色不对"));
    finish_agent(&db, &host, &run_id, "code", &[("summary", json!("v3"))], "done");
    finish_job(&db, &host, &run_id, "test", script_result(0), None);
    let gate = current(&db, &run_id, "accept");
    engine::decide_gate(&db, &host, &gate.id, "approve", "", Value::Null, "ui").unwrap();
    engine::advance(&db, &host, &run_id).unwrap();
    assert_eq!(status(&db, &run_id), "succeeded");
    assert_eq!(runs_of(&db, &run_id, "code").len(), 3);
}

#[test]
fn loop_exhausted_without_connection_fails_run() {
    let db = db();
    let host = FakeHost::new();
    let wf = create(&db, loop_def());
    let (run_id, _) = start(&db, &host, &wf, json!({}));
    for _ in 0..2 {
        finish_agent(&db, &host, &run_id, "code", &[("summary", json!("x"))], "done");
        finish_job(&db, &host, &run_id, "test", script_result(1), None);
    }
    finish_agent(&db, &host, &run_id, "code", &[("summary", json!("x"))], "done");
    finish_job(&db, &host, &run_id, "test", script_result(1), None);
    let run = store::run(&db, &run_id).unwrap();
    assert_eq!(run.status, "failed");
    assert_eq!(run.error_code.as_deref(), Some("loop_exhausted"));
    assert_eq!(store::loop_counts(&db, &run_id).unwrap().get("e4"), Some(&2));
}

#[test]
fn agent_output_invalid_gets_repair_turn_then_fails() {
    let db = db();
    let host = FakeHost::new();
    let wf = create(&db, json!({
        "nodes": [
            { "id": "start", "kind": "trigger" },
            agent_node("prd", json!([]), json!([{ "name": "spec", "type": "json", "required": true,
                "schema": { "type": "object", "required": ["title"] } }]), json!({}))
        ],
        "edges": [{ "id": "e1", "source": "start", "sourcePort": "$out", "target": "prd", "targetPort": "$in" }]
    }));
    let (run_id, _) = start(&db, &host, &wf, json!({}));
    let nr = current(&db, &run_id, "prd");
    // CLI 提交时就校验 schema
    assert!(engine::submit_output(&db, &nr.id, "spec", json!({ "x": 1 }), &host.now()).is_err());
    assert!(engine::submit_output(&db, &nr.id, "nope", json!({}), &host.now()).is_err());
    let acts = finish_agent(&db, &host, &run_id, "prd", &[], "I forgot");
    assert!(acts.iter().any(|a| matches!(a, Action::DispatchAgents)), "repair turn queued");
    assert_eq!(current(&db, &run_id, "prd").repair_count, 1);
    finish_agent(&db, &host, &run_id, "prd", &[], "still nothing");
    let run = store::run(&db, &run_id).unwrap();
    assert_eq!(run.status, "failed");
    assert_eq!(current(&db, &run_id, "prd").error_code.as_deref(), Some("output_invalid"));
}

#[test]
fn declared_failure_and_on_error_continue() {
    let db = db();
    let host = FakeHost::new();
    let wf = create(&db, json!({
        "nodes": [
            { "id": "start", "kind": "trigger" },
            agent_node("risky", json!([]), json!([]), json!({ "onError": "continue" })),
            { "id": "handle", "kind": "transform", "inputs": [{ "name": "err", "type": "json" }],
              "outputs": [{ "name": "code", "type": "text" }], "config": { "assignments": [{ "port": "code", "expr": "inputs.err.code" }] } },
            { "id": "ok_path", "kind": "transform", "config": { "assignments": [] } }
        ],
        "edges": [
            { "id": "e1", "source": "start", "sourcePort": "$out", "target": "risky", "targetPort": "$in" },
            { "id": "e2", "source": "risky", "sourcePort": "$error", "target": "handle", "targetPort": "err" },
            { "id": "e3", "source": "risky", "sourcePort": "$out", "target": "ok_path", "targetPort": "$in" }
        ]
    }));
    let (run_id, _) = start(&db, &host, &wf, json!({}));
    let nr = current(&db, &run_id, "risky");
    engine::declare_failure(&db, &nr.id, "做不到").unwrap();
    finish_agent(&db, &host, &run_id, "risky", &[], "sorry");
    assert_eq!(status(&db, &run_id), "succeeded");
    assert_eq!(current(&db, &run_id, "ok_path").status, "skipped");
    let handle = current(&db, &run_id, "handle");
    let outputs = super::values::outputs(&db, &handle.id).unwrap();
    assert_eq!(outputs["code"], json!("agent_declared_failure"));
}

#[test]
fn retry_policy_backs_off_and_tick_restarts() {
    let db = db();
    let host = FakeHost::new();
    let wf = create(&db, json!({
        "nodes": [
            { "id": "start", "kind": "trigger" },
            { "id": "flaky", "kind": "http", "config": { "url": "https://example.com" },
              "retry": { "maxAttempts": 2, "backoffSeconds": 10, "on": ["failed"] } }
        ],
        "edges": [{ "id": "e1", "source": "start", "sourcePort": "$out", "target": "flaky", "targetPort": "$in" }]
    }));
    let (run_id, _) = start(&db, &host, &wf, json!({}));
    finish_job(&db, &host, &run_id, "flaky", json!({}), Some(("http_status", "500")));
    let retry = current(&db, &run_id, "flaky");
    assert_eq!((retry.attempt, retry.status.as_str()), (2, "pending"));
    assert!(retry.retry_at.is_some());
    assert_eq!(status(&db, &run_id), "running");
    engine::tick(&db, &host).unwrap();
    assert_eq!(current(&db, &run_id, "flaky").status, "pending", "not yet due");
    host.advance_secs(11);
    engine::tick(&db, &host).unwrap();
    assert_eq!(current(&db, &run_id, "flaky").status, "running");
    finish_job(&db, &host, &run_id, "flaky", json!({ "status": 200, "ok": true }), None);
    assert_eq!(status(&db, &run_id), "succeeded");
}

#[test]
fn timeout_fails_node() {
    let db = db();
    let host = FakeHost::new();
    let wf = create(&db, json!({
        "nodes": [
            { "id": "start", "kind": "trigger" },
            { "id": "slow", "kind": "script", "timeoutMinutes": 1, "config": { "command": "sleep 100" } }
        ],
        "edges": [{ "id": "e1", "source": "start", "sourcePort": "$out", "target": "slow", "targetPort": "$in" }]
    }));
    let (run_id, _) = start(&db, &host, &wf, json!({}));
    host.advance_secs(61);
    let acts = engine::tick(&db, &host).unwrap();
    assert!(acts.iter().any(|a| matches!(a, Action::KillJob(_))));
    let run = store::run(&db, &run_id).unwrap();
    assert_eq!(run.status, "failed");
    assert_eq!(run.error_code.as_deref(), Some("timeout"));
}

#[test]
fn cancel_stops_active_nodes() {
    let db = db();
    let host = FakeHost::new();
    let wf = create(&db, loop_def());
    let (run_id, _) = start(&db, &host, &wf, json!({}));
    let acts = engine::cancel_run(&db, &host, &run_id, "user").unwrap();
    assert!(acts.iter().any(|a| matches!(a, Action::StopSession(_))));
    assert_eq!(status(&db, &run_id), "cancelled");
    assert_eq!(current(&db, &run_id, "code").status, "cancelled");
    assert_eq!(current(&db, &run_id, "test").status, "skipped");
}

#[test]
fn manual_retry_reruns_node_and_descendants_only() {
    let db = db();
    let host = FakeHost::new();
    let wf = create(&db, json!({
        "nodes": [
            { "id": "start", "kind": "trigger" },
            { "id": "a", "kind": "script", "config": { "command": "a" } },
            { "id": "b", "kind": "script", "config": { "command": "b" } }
        ],
        "edges": [
            { "id": "e1", "source": "start", "sourcePort": "$out", "target": "a", "targetPort": "$in" },
            { "id": "e2", "source": "a", "sourcePort": "$out", "target": "b", "targetPort": "$in" }
        ]
    }));
    let (run_id, _) = start(&db, &host, &wf, json!({}));
    finish_job(&db, &host, &run_id, "a", script_result(0), None);
    finish_job(&db, &host, &run_id, "b", json!({}), Some(("script_exit", "boom")));
    assert_eq!(status(&db, &run_id), "failed");
    engine::retry_node(&db, &host, &run_id, "b", false).unwrap();
    assert_eq!(status(&db, &run_id), "running");
    assert_eq!(runs_of(&db, &run_id, "a").len(), 1, "upstream not rerun");
    assert_eq!(current(&db, &run_id, "b").attempt, 2);
    finish_job(&db, &host, &run_id, "b", script_result(0), None);
    assert_eq!(status(&db, &run_id), "succeeded");
}

#[test]
fn write_lock_serializes_parallel_writers() {
    let db = db();
    let host = FakeHost::new();
    let wf = create(&db, json!({
        "workspaces": [{ "key": "app", "project": "p1", "isolation": "worktree_new" }],
        "nodes": [
            { "id": "start", "kind": "trigger" },
            { "id": "a", "kind": "script", "workspaces": [{ "slot": "app", "access": "write" }], "config": { "command": "a" } },
            { "id": "b", "kind": "script", "workspaces": [{ "slot": "app", "access": "write" }], "config": { "command": "b" } }
        ],
        "edges": [
            { "id": "e1", "source": "start", "sourcePort": "$out", "target": "a", "targetPort": "$in" },
            { "id": "e2", "source": "start", "sourcePort": "$out", "target": "b", "targetPort": "$in" }
        ]
    }));
    let (run_id, _) = start(&db, &host, &wf, json!({}));
    assert_eq!(current(&db, &run_id, "a").status, "running");
    assert_eq!(current(&db, &run_id, "b").status, "ready", "waits for the workspace lock");
    finish_job(&db, &host, &run_id, "a", script_result(0), None);
    assert_eq!(current(&db, &run_id, "b").status, "running");
    finish_job(&db, &host, &run_id, "b", script_result(0), None);
    assert_eq!(status(&db, &run_id), "succeeded");
}

#[test]
fn subflow_and_map_run_children() {
    let db = db();
    let host = FakeHost::new();
    let child = create(&db, json!({
        "inputs": [{ "name": "x", "type": "number", "required": true }],
        "outputs": [{ "name": "y", "type": "number" }],
        "nodes": [
            { "id": "start", "kind": "trigger" },
            { "id": "double", "kind": "transform", "inputs": [{ "name": "x", "type": "number" }],
              "outputs": [{ "name": "y", "type": "number" }], "config": { "assignments": [{ "port": "y", "expr": "inputs.x * 2" }] } },
            { "id": "done", "kind": "output", "inputs": [{ "name": "y", "type": "number" }] }
        ],
        "edges": [
            { "id": "e1", "source": "start", "sourcePort": "x", "target": "double", "targetPort": "x" },
            { "id": "e2", "source": "double", "sourcePort": "y", "target": "done", "targetPort": "y" }
        ]
    }));
    let parent = create(&db, json!({
        "outputs": [{ "name": "single", "type": "any" }, { "name": "many", "type": "json" }],
        "nodes": [
            { "id": "start", "kind": "trigger" },
            { "id": "one", "kind": "subflow", "outputs": [{ "name": "y", "type": "number" }], "config": { "workflowId": child, "inputs": { "x": "{{ 21 }}" } } },
            { "id": "all", "kind": "map", "config": { "workflowId": child, "items": "[1, 2, 3]", "itemInput": "x", "concurrency": 2 } },
            { "id": "done", "kind": "output", "inputs": [{ "name": "single", "type": "any" }, { "name": "many", "type": "json" }] }
        ],
        "edges": [
            { "id": "e1", "source": "start", "sourcePort": "$out", "target": "one", "targetPort": "$in" },
            { "id": "e2", "source": "one", "sourcePort": "y", "target": "all", "targetPort": "$in" },
            { "id": "e3", "source": "one", "sourcePort": "y", "target": "done", "targetPort": "single" },
            { "id": "e4", "source": "all", "sourcePort": "results", "target": "done", "targetPort": "many" }
        ]
    }));
    let (run_id, _) = start(&db, &host, &parent, json!({}));
    let run = store::run(&db, &run_id).unwrap();
    assert_eq!(run.status, "succeeded", "{:?}", run.error);
    assert_eq!(run.outputs["single"], json!(42));
    assert_eq!(run.outputs["many"], json!([{ "y": 2 }, { "y": 4 }, { "y": 6 }]));
    // 子运行不建任务卡
    let tasks: i64 = db.query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0)).unwrap();
    assert_eq!(tasks, 1);
}

#[test]
fn queue_concurrency_runs_one_at_a_time() {
    let db = db();
    let host = FakeHost::new();
    let wf = create(&db, json!({
        "nodes": [
            { "id": "start", "kind": "trigger" },
            { "id": "a", "kind": "script", "config": { "command": "a" } }
        ],
        "edges": [{ "id": "e1", "source": "start", "sourcePort": "$out", "target": "a", "targetPort": "$in" }],
        "settings": { "concurrency": "queue" }
    }));
    let (first, _) = start(&db, &host, &wf, json!({}));
    let (second, _) = start(&db, &host, &wf, json!({}));
    assert_eq!(status(&db, &second), "queued");
    finish_job(&db, &host, &first, "a", script_result(0), None);
    assert_eq!(status(&db, &first), "succeeded");
    assert_eq!(status(&db, &second), "running");
    // CLI 排队的运行由 tick 开始
    let third = engine::enqueue_run(&db, &wf, json!({}), "cli", &host.now()).unwrap();
    finish_job(&db, &host, &second, "a", script_result(0), None);
    assert_eq!(status(&db, &third), "running");
}

#[test]
fn plan_mode_agent_waits_for_human() {
    let db = db();
    let host = FakeHost::new();
    let wf = create(&db, json!({
        "nodes": [
            { "id": "start", "kind": "trigger" },
            agent_node("plan", json!([]), json!([]), json!({}))
        ],
        "edges": [{ "id": "e1", "source": "start", "sourcePort": "$out", "target": "plan", "targetPort": "$in" }]
    }));
    let (run_id, _) = start(&db, &host, &wf, json!({}));
    let nr = current(&db, &run_id, "plan");
    let session = nr.session_id.unwrap();
    // 还有 run 在跑：不判定
    let acts = engine::on_session_idle(&db, &host, &session).unwrap();
    assert!(acts.is_empty());
    assert_eq!(current(&db, &run_id, "plan").status, "running");
}

#[test]
fn overview_message_without_gate_is_ignored_and_bad_text_gets_hint() {
    let db = db();
    let host = FakeHost::new();
    let wf = create(&db, loop_def());
    let (run_id, _) = start(&db, &host, &wf, json!({}));
    let overview = format!("wf-run:{run_id}");
    assert_eq!(engine::handle_overview_message(&db, &host, &overview, "通过").unwrap().map(|a| a.len()), Some(0));
    finish_agent(&db, &host, &run_id, "code", &[("summary", json!("v1"))], "done");
    finish_job(&db, &host, &run_id, "test", script_result(0), None);
    engine::handle_overview_message(&db, &host, &overview, "嗯？").unwrap();
    assert_eq!(current(&db, &run_id, "accept").status, "waiting_human");
    engine::handle_overview_message(&db, &host, &overview, "通过").unwrap();
    engine::advance(&db, &host, &run_id).unwrap();
    assert_eq!(status(&db, &run_id), "succeeded");
}

#[test]
fn recover_fails_interrupted_jobs() {
    let db = db();
    let host = FakeHost::new();
    let wf = create(&db, json!({
        "nodes": [
            { "id": "start", "kind": "trigger" },
            { "id": "a", "kind": "script", "config": { "command": "a" } }
        ],
        "edges": [{ "id": "e1", "source": "start", "sourcePort": "$out", "target": "a", "targetPort": "$in" }]
    }));
    let (run_id, _) = start(&db, &host, &wf, json!({}));
    engine::recover(&db, &host).unwrap();
    let run = store::run(&db, &run_id).unwrap();
    assert_eq!(run.status, "failed");
    assert_eq!(current(&db, &run_id, "a").error_code.as_deref(), Some("app_restarted"));
}

#[test]
fn migration_upgrades_automation_target_check() {
    let db = Connection::open_in_memory().unwrap();
    crate::database::migrate(&db).unwrap();
    let sql: String = db
        .query_row("SELECT sql FROM sqlite_master WHERE name='automations'", [], |r| r.get(0))
        .unwrap();
    assert!(sql.contains("'workflow'"));
    // 再跑一次是幂等的
    crate::database::migrate(&db).unwrap();
    let _: Map<String, Value> = Map::new();
}
