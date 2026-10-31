use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanStep {
    pub id: String,
    pub text: String,
    pub status: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanArtifact {
    pub plan_id: String,
    pub revision: u32,
    pub title: String,
    pub summary: String,
    pub markdown: String,
    pub steps: Vec<PlanStep>,
    pub risks: Vec<String>,
    pub questions: Vec<String>,
    pub structured: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changed_files: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlanRecord {
    pub run_id: String,
    pub plan_id: String,
    pub revision: u32,
    pub seq: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlanActionRecord {
    pub plan_id: String,
    pub revision: u32,
    pub action: String,
    #[serde(default)]
    pub execution_run_status: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    AwaitingApproval,
    Approved,
    Executing,
    Executed,
    Failed,
    Rejected,
    Superseded,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlanState {
    pub plan_id: String,
    pub revision: u32,
    pub status: PlanStatus,
}

pub fn assemble_plan_artifact(
    plan_markdown: Option<&str>,
    final_text: &str,
    source_meta: Option<&Value>,
    changed_files: Vec<String>,
) -> PlanArtifact {
    let markdown = plan_markdown
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(final_text)
        .trim()
        .to_string();
    let steps = parse_markdown_steps(&markdown);
    let structured = !steps.is_empty();
    let refinement =
        source_meta.filter(|meta| meta.get("planAction").and_then(Value::as_str) == Some("refine"));
    let plan_id = refinement
        .and_then(|meta| meta.get("planId"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let revision = refinement
        .and_then(|meta| meta.get("revision"))
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(0)
        .saturating_add(1);
    PlanArtifact {
        plan_id,
        revision,
        title: markdown
            .lines()
            .find_map(|line| line.trim().strip_prefix('#').map(str::trim))
            .filter(|line| !line.is_empty())
            .unwrap_or_default()
            .to_string(),
        summary: final_text
            .split("\n\n")
            .map(str::trim)
            .find(|value| !value.is_empty())
            .unwrap_or_default()
            .to_string(),
        markdown,
        steps,
        risks: Vec::new(),
        questions: Vec::new(),
        structured,
        changed_files: (!changed_files.is_empty()).then_some(changed_files),
    }
}

pub fn recorded_plan_markdown(db: &Connection, run_id: &str, runtime_id: &str) -> Option<String> {
    let submitted = db
        .query_row(
            "SELECT json_extract(input_json, '$.plan') FROM run_events
             WHERE run_id=?1 AND event_type='tool_use' AND tool='ExitPlanMode'
               AND COALESCE(json_extract(input_json, '$.plan'), '') != ''
             ORDER BY seq DESC LIMIT 1",
            [run_id],
            |row| row.get(0),
        )
        .optional()
        .ok()
        .flatten();
    if submitted.is_some() {
        return submitted;
    }
    match runtime_id {
        "claude" | "codebuddy" => db
            .query_row(
                "SELECT json_extract(input_json, '$.content') FROM run_events
                 WHERE run_id=?1 AND event_type='tool_use' AND tool='Write'
                   AND (json_extract(input_json, '$.file_path') LIKE '%/.claude/plans/%.md'
                        OR json_extract(input_json, '$.file_path') LIKE '%\\.claude\\plans\\%.md')
                 ORDER BY seq DESC LIMIT 1",
                [run_id],
                |row| row.get(0),
            )
            .optional()
            .ok()
            .flatten(),
        "cursor" => db
            .query_row(
                "SELECT json_extract(input_json, '$.plan') FROM run_events
                 WHERE run_id=?1 AND event_type='tool_use' AND tool='CreatePlan'
                 ORDER BY seq DESC LIMIT 1",
                [run_id],
                |row| row.get(0),
            )
            .optional()
            .ok()
            .flatten(),
        _ => None,
    }
}

pub fn parse_markdown_steps(markdown: &str) -> Vec<PlanStep> {
    markdown
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let text = trimmed
                .strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix("* "))
                .or_else(|| {
                    let (number, text) = trimmed.split_once(". ")?;
                    number.chars().all(|ch| ch.is_ascii_digit()).then_some(text)
                })
                .or_else(|| {
                    trimmed
                        .strip_prefix("## ")
                        .or_else(|| trimmed.strip_prefix("### "))
                })?
                .trim();
            (!text.is_empty()).then_some(text.to_string())
        })
        .enumerate()
        .map(|(index, text)| PlanStep {
            id: format!("step-{}", index + 1),
            text,
            status: "pending".to_string(),
        })
        .collect()
}

pub fn derive_plan_states(plans: &[PlanRecord], actions: &[PlanActionRecord]) -> Vec<PlanState> {
    let latest_revision = plans.iter().fold(HashMap::new(), |mut latest, plan| {
        latest
            .entry(plan.plan_id.as_str())
            .and_modify(|revision: &mut u32| *revision = (*revision).max(plan.revision))
            .or_insert(plan.revision);
        latest
    });
    plans
        .iter()
        .map(|plan| {
            let action = actions
                .iter()
                .rev()
                .find(|action| action.plan_id == plan.plan_id && action.revision == plan.revision);
            let status = match action.map(|action| action.action.as_str()) {
                Some("approve") => {
                    match action.and_then(|item| item.execution_run_status.as_deref()) {
                        Some("queued" | "running") => PlanStatus::Executing,
                        Some("completed") => PlanStatus::Executed,
                        Some("failed" | "stopped") => PlanStatus::Failed,
                        _ => PlanStatus::Approved,
                    }
                }
                Some("reject") => PlanStatus::Rejected,
                Some("refine" | "supersede") => PlanStatus::Superseded,
                _ if latest_revision
                    .get(plan.plan_id.as_str())
                    .copied()
                    .unwrap_or(0)
                    > plan.revision =>
                {
                    PlanStatus::Superseded
                }
                _ => PlanStatus::AwaitingApproval,
            };
            PlanState {
                plan_id: plan.plan_id.clone(),
                revision: plan.revision,
                status,
            }
        })
        .collect()
}

pub fn latest_awaiting_plan(
    db: &Connection,
    session_id: &str,
) -> Result<Option<(String, String, u32, Value)>, String> {
    let mut statement = db
        .prepare(
            "SELECT run_id, seq, input_json FROM run_events
             WHERE session_id=?1 AND event_type='plan'
             ORDER BY created_at DESC, seq DESC",
        )
        .map_err(|error| error.to_string())?;
    let plans = statement
        .query_map([session_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, u32>(1)?,
                row.get::<_, Option<String>>(2)?.unwrap_or_default(),
            ))
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    let mut newer_plan_ids = std::collections::HashSet::new();
    for (run_id, seq, raw) in plans {
        let Ok(input) = serde_json::from_str::<Value>(&raw) else {
            continue;
        };
        let Some(plan_id) = input.get("planId").and_then(Value::as_str) else {
            continue;
        };
        let revision = input
            .get("revision")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or(1);
        if !newer_plan_ids.insert(plan_id.to_string()) {
            continue;
        }
        let action: Option<String> = db
            .query_row(
                "SELECT input_json FROM run_events
                 WHERE session_id=?1 AND event_type='plan_action'
                   AND json_extract(input_json, '$.planId')=?2
                   AND json_extract(input_json, '$.revision')=?3
                 ORDER BY seq DESC LIMIT 1",
                rusqlite::params![session_id, plan_id, revision],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if action.is_none() {
            return Ok(Some((run_id, plan_id.to_string(), revision, input)));
        }
        let _ = seq;
    }
    Ok(None)
}

pub fn artifact_input(artifact: &PlanArtifact) -> Value {
    serde_json::to_value(artifact).unwrap_or_else(|_| json!({}))
}

pub fn validate_plan_action(existing: &[String], requested: &str) -> Result<bool, String> {
    if existing.iter().any(|action| action == requested) {
        return Ok(true);
    }
    if existing.is_empty() {
        Ok(false)
    } else {
        Err(tr!("计划已处理"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refinement_keeps_plan_id_and_increments_revision() {
        let artifact = assemble_plan_artifact(
            None,
            "# Updated\n\n1. First\n2. Second",
            Some(&json!({"planAction":"refine","planId":"p1","revision":2})),
            vec!["src/main.rs".into()],
        );
        assert_eq!(artifact.plan_id, "p1");
        assert_eq!(artifact.revision, 3);
        assert_eq!(artifact.steps.len(), 2);
        assert_eq!(artifact.changed_files, Some(vec!["src/main.rs".into()]));
    }

    #[test]
    fn plan_states_follow_contract() {
        let plans = vec![
            PlanRecord {
                run_id: "r1".into(),
                plan_id: "p".into(),
                revision: 1,
                seq: 1,
            },
            PlanRecord {
                run_id: "r2".into(),
                plan_id: "p".into(),
                revision: 2,
                seq: 1,
            },
        ];
        let actions = vec![PlanActionRecord {
            plan_id: "p".into(),
            revision: 2,
            action: "approve".into(),
            execution_run_status: Some("running".into()),
        }];
        let states = derive_plan_states(&plans, &actions);
        assert_eq!(states[0].status, PlanStatus::Superseded);
        assert_eq!(states[1].status, PlanStatus::Executing);
    }

    #[test]
    fn plan_action_is_idempotent_but_actions_conflict() {
        assert!(validate_plan_action(&["approve".into()], "approve").unwrap());
        assert_eq!(
            validate_plan_action(&["approve".into()], "reject").unwrap_err(),
            "计划已处理"
        );
        assert!(!validate_plan_action(&[], "approve").unwrap());
    }

    #[test]
    fn reads_exact_runtime_plan_delivery_tools() {
        let db = Connection::open_in_memory().unwrap();
        crate::database::migrate(&db).unwrap();
        db.execute_batch(
            "INSERT INTO agents
               (id, name, avatar, description, instructions, runtime_id, model, max_concurrent_tasks)
             VALUES ('a', 'A', '', '', '', 'cursor', 'auto', 1);
             INSERT INTO chat_sessions
               (id, title, target_type, target_id, created_at, updated_at)
             VALUES ('s', 'S', 'agent', 'a', 'now', 'now');
             INSERT INTO agent_runs
               (id, session_id, agent_id, runtime_id, assignment, status, created_at, mode)
             VALUES ('r', 's', 'a', 'cursor', 'one', 'completed', 'now', 'plan');
             INSERT INTO run_events
               (id, run_id, session_id, seq, event_type, tool, input_json, created_at)
             VALUES
               ('e1', 'r', 's', 1, 'tool_use', 'CreatePlan',
                '{\"plan\":\"# Cursor plan\\n\\n1. Inspect\"}', 'now'),
               ('e2', 'r', 's', 2, 'tool_use', 'PlanSomethingElse',
                '{\"plan\":\"wrong\"}', 'now');",
        )
        .unwrap();
        assert_eq!(
            recorded_plan_markdown(&db, "r", "cursor").as_deref(),
            Some("# Cursor plan\n\n1. Inspect")
        );
        assert_eq!(recorded_plan_markdown(&db, "r", "opencode"), None);
        db.execute(
            "INSERT INTO run_events
             (id, run_id, session_id, seq, event_type, tool, input_json, created_at)
             VALUES ('e3', 'r', 's', 3, 'tool_use', 'ExitPlanMode',
                     '{\"plan\":\"# Auto plan\\n\\n1. Wait\"}', 'now')",
            [],
        )
        .unwrap();
        assert_eq!(
            recorded_plan_markdown(&db, "r", "claude").as_deref(),
            Some("# Auto plan\n\n1. Wait")
        );
    }

    #[test]
    fn only_latest_awaiting_plan_is_selected() {
        let db = Connection::open_in_memory().unwrap();
        crate::database::migrate(&db).unwrap();
        db.execute_batch(
            "INSERT INTO agents
               (id, name, avatar, description, instructions, runtime_id, model, max_concurrent_tasks)
             VALUES ('a', 'A', '', '', '', 'claude', 'auto', 1);
             INSERT INTO chat_sessions
               (id, title, target_type, target_id, created_at, updated_at)
             VALUES ('s', 'S', 'agent', 'a', 'now', 'now');
             INSERT INTO agent_runs
               (id, session_id, agent_id, runtime_id, assignment, status, created_at, mode)
             VALUES
               ('r1', 's', 'a', 'claude', 'one', 'completed', '1', 'plan'),
               ('r2', 's', 'a', 'claude', 'two', 'completed', '2', 'plan');
             INSERT INTO run_events
               (id, run_id, session_id, seq, event_type, input_json, created_at)
             VALUES
               ('e1', 'r1', 's', 1, 'plan',
                '{\"planId\":\"p1\",\"revision\":1,\"title\":\"旧\"}', '1'),
               ('e2', 'r2', 's', 1, 'plan',
                '{\"planId\":\"p2\",\"revision\":1,\"title\":\"新\"}', '2');",
        )
        .unwrap();
        let latest = latest_awaiting_plan(&db, "s").unwrap().unwrap();
        assert_eq!((latest.0.as_str(), latest.1.as_str()), ("r2", "p2"));
    }
}
