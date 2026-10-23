//! 「合并到其他分支」：预览、执行、冲突会话（手动或交给 AI 解决）、完成与放弃。
//!
//! 冲突时会建一个临时 Worktree，并把它登记成某个聊天会话的专属工作区（workspaces.session_id），
//! 这样工作台切到这个会话时，文件树、源码管理和 AI 改动都落在临时 Worktree 上，用户原来的工作区不受影响。

use std::path::PathBuf;

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use crate::{
    commands::{
        expand_user_path, get_git_workspace, get_workspace, off_main, setting_value,
        validate_chat_target,
    },
    database::AppState,
    git::output,
    git_merge::{self, MergePlan, MergeTarget},
    models::{ChatSession, Workspace},
};

pub fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(
        "
        -- Worktree 创建时的来源分支与提交，用于默认合并目标和「相比来源 +N 提交」
        CREATE TABLE IF NOT EXISTS workspace_origins (
          workspace_id TEXT PRIMARY KEY,
          base_ref TEXT NOT NULL,
          base_commit TEXT NOT NULL,
          created_at TEXT NOT NULL
        );
        -- 进行中的冲突合并；status: resolving / completed / aborted
        CREATE TABLE IF NOT EXISTS merge_sessions (
          id TEXT PRIMARY KEY,
          project_id TEXT NOT NULL,
          session_id TEXT NOT NULL,
          workspace_id TEXT NOT NULL,
          source_workspace_id TEXT NOT NULL,
          source_label TEXT NOT NULL,
          source_commit TEXT NOT NULL,
          target_branch TEXT NOT NULL,
          target_remote TEXT,
          push INTEGER NOT NULL,
          conflicts TEXT NOT NULL DEFAULT '[]',
          status TEXT NOT NULL,
          created_at TEXT NOT NULL
        );
        ",
    )
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeSession {
    pub id: String,
    pub project_id: String,
    pub session_id: String,
    pub workspace_id: String,
    pub source_workspace_id: String,
    pub source_label: String,
    pub target_branch: String,
    pub target_remote: Option<String>,
    pub push: bool,
    pub conflicts: Vec<String>,
    pub created_at: String,
}

impl MergeSession {
    fn target_display(&self) -> String {
        MergeTarget {
            branch: self.target_branch.clone(),
            remote: self.target_remote.clone(),
        }
        .display()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeOrigin {
    pub base_ref: String,
    pub base_commit: String,
    /// 当前分支相比来源多出的提交数。
    pub commits: u32,
    /// 当前分支相比来源改动的文件数。
    pub files: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeTargetPreview {
    pub branch: String,
    pub remote: Option<String>,
    pub display: String,
    /// 本地或远程（已获取的）目标分支是否存在。
    pub exists: bool,
    pub has_remote_branch: bool,
    /// 将合入的提交数与文件数（基于本地已获取的引用）。
    pub commits: u32,
    pub files: u32,
    /// fast_forward / merge / conflict / up_to_date / missing
    pub plan: String,
    /// 目标分支比当前多出的提交数。
    pub target_ahead: u32,
    pub conflict_files: Vec<String>,
    /// 目标分支正在某个目录检出时的路径。
    pub checked_out_at: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergePreview {
    pub source_label: String,
    pub source_commit: String,
    pub dirty: bool,
    pub has_remote: bool,
    pub origin: Option<MergeOrigin>,
    pub target: Option<MergeTargetPreview>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeIntoInput {
    pub workspace_id: String,
    pub target: String,
    pub push: bool,
    /// 有冲突时新建合并会话所用的执行者（智能体或小队）。
    pub chat_target_type: String,
    pub chat_target_id: String,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MergeIntoResult {
    /// up_to_date / merged / conflict
    pub status: &'static str,
    pub source_label: String,
    pub target_display: String,
    pub commit: Option<String>,
    pub fast_forward: bool,
    pub pushed: bool,
    pub local_updated: bool,
    pub notes: Vec<String>,
    pub conflicts: Vec<String>,
    pub merge_session: Option<MergeSession>,
    pub session: Option<ChatSession>,
    pub workspace: Option<Workspace>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeFinishResult {
    pub commit: String,
    pub source_label: String,
    pub target_display: String,
    pub pushed: bool,
    pub local_updated: bool,
    pub notes: Vec<String>,
}

/// 新建 Worktree 后记录它是从哪个分支、哪个提交迁出的。
pub fn record_origin(db: &Connection, workspace: &Workspace, base_ref: Option<&str>, git: &str) {
    if workspace.kind != "git" || workspace.isolation_mode != "worktree" {
        return;
    }
    let reference = base_ref.map(str::trim).filter(|value| !value.is_empty()).unwrap_or("HEAD");
    let label = if reference == "HEAD" {
        git_merge::head_label(git, &workspace.source_path)
    } else {
        reference.to_string()
    };
    let Some(commit) = git_merge::rev(git, &workspace.source_path, reference) else {
        return;
    };
    let _ = db.execute(
        "INSERT OR REPLACE INTO workspace_origins (workspace_id, base_ref, base_commit, created_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![workspace.id, label, commit, Utc::now().to_rfc3339()],
    );
}

fn load_origin(db: &Connection, workspace_id: &str) -> Option<(String, String)> {
    db.query_row(
        "SELECT base_ref, base_commit FROM workspace_origins WHERE workspace_id=?1",
        [workspace_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
    .ok()
    .flatten()
}

pub fn list_active(db: &Connection) -> Result<Vec<MergeSession>, String> {
    let mut statement = db
        .prepare(
            "SELECT m.id, m.project_id, m.session_id, m.workspace_id, m.source_workspace_id,
                    m.source_label, m.target_branch, m.target_remote, m.push, m.conflicts, m.created_at
             FROM merge_sessions m
             JOIN workspaces w ON w.id = m.workspace_id
             WHERE m.status = 'resolving'
             ORDER BY m.created_at",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], merge_session_from_row)
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

fn merge_session_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MergeSession> {
    let conflicts: String = row.get(9)?;
    Ok(MergeSession {
        id: row.get(0)?,
        project_id: row.get(1)?,
        session_id: row.get(2)?,
        workspace_id: row.get(3)?,
        source_workspace_id: row.get(4)?,
        source_label: row.get(5)?,
        target_branch: row.get(6)?,
        target_remote: row.get(7)?,
        push: row.get::<_, i64>(8)? != 0,
        conflicts: serde_json::from_str(&conflicts).unwrap_or_default(),
        created_at: row.get(10)?,
    })
}

fn load_merge(db: &Connection, merge_id: &str) -> Result<MergeSession, String> {
    db.query_row(
        "SELECT id, project_id, session_id, workspace_id, source_workspace_id,
                source_label, target_branch, target_remote, push, conflicts, created_at
         FROM merge_sessions WHERE id=?1 AND status='resolving'",
        [merge_id],
        merge_session_from_row,
    )
    .map_err(|_| "这次合并已经结束或不存在".to_string())
}

pub fn delete_for_project(db: &Connection, project_id: &str) -> Result<(), String> {
    db.execute("DELETE FROM merge_sessions WHERE project_id=?1", [project_id])
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn delete_origin(db: &Connection, workspace_id: &str) {
    let _ = db.execute("DELETE FROM workspace_origins WHERE workspace_id=?1", [workspace_id]);
}

fn ensure_session_idle(db: &Connection, session_id: &str) -> Result<(), String> {
    let active: u32 = db
        .query_row(
            "SELECT COUNT(*) FROM agent_runs WHERE session_id=?1 AND status IN ('queued', 'running')",
            [session_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if active > 0 {
        return Err("智能体还在处理这次合并，请先等它完成或停止运行".to_string());
    }
    Ok(())
}

fn merge_message(source_label: &str, target: &MergeTarget) -> String {
    format!("Merge branch '{source_label}' into {}", target.branch)
}

#[tauri::command]
pub async fn git_merge_preview(
    workspace_id: String,
    target: Option<String>,
    app: AppHandle,
) -> Result<MergePreview, String> {
    let _span = crate::diag::span("git_merge_preview");
    off_main(app, move |state| {
        let (workspace, git, origin) = {
            let db = state.lock_db()?;
            let workspace = get_git_workspace(&db, &workspace_id)?;
            let git = setting_value(&db, "gitExecutable", "git");
            let origin = load_origin(&db, &workspace_id);
            (workspace, git, origin)
        };
        let root = workspace.work_path.as_str();
        let head = git_merge::head_commit(&git, root)?;
        let dirty = !output(&git, root, &["status", "--porcelain"])?.trim().is_empty();
        let has_remote = crate::git::remotes(&git, root).is_ok_and(|items| !items.is_empty());
        let origin = origin.map(|(base_ref, base_commit)| {
            let (commits, files) = git_merge::compare(&git, root, &base_commit, &head);
            MergeOrigin {
                commits,
                files,
                base_commit: base_commit.chars().take(7).collect(),
                base_ref,
            }
        });
        let target = match target.map(|value| value.trim().to_string()).filter(|value| !value.is_empty()) {
            None => None,
            Some(value) => {
                let target = git_merge::resolve_target(&git, root, &value)?;
                let remote_commit = target
                    .remote_ref()
                    .and_then(|reference| git_merge::rev(&git, root, &reference));
                let local_commit = git_merge::rev(&git, root, &target.local_ref());
                let compare_to = remote_commit.clone().or(local_commit.clone());
                let (commits, files) = compare_to
                    .as_deref()
                    .map(|commit| git_merge::compare(&git, root, commit, &head))
                    .unwrap_or((0, 0));
                let outlook = match compare_to.as_deref() {
                    Some(commit) => git_merge::outlook(&git, root, commit, &head)?,
                    None => git_merge::MergeOutlook {
                        kind: "missing",
                        target_ahead: 0,
                        conflict_files: Vec::new(),
                    },
                };
                Some(MergeTargetPreview {
                    display: target.display(),
                    exists: compare_to.is_some(),
                    has_remote_branch: remote_commit.is_some(),
                    checked_out_at: git_merge::checked_out_path(&git, root, &target.branch),
                    branch: target.branch,
                    remote: target.remote,
                    commits,
                    files,
                    plan: outlook.kind.to_string(),
                    target_ahead: outlook.target_ahead,
                    conflict_files: outlook.conflict_files,
                })
            }
        };
        Ok(MergePreview {
            source_label: git_merge::head_label(&git, root),
            source_commit: head.chars().take(7).collect(),
            dirty,
            has_remote,
            origin,
            target,
        })
    })
    .await
}

#[tauri::command]
pub async fn git_merge_into_branch(
    input: MergeIntoInput,
    app: AppHandle,
) -> Result<MergeIntoResult, String> {
    let _span = crate::diag::span("git_merge_into_branch");
    let app_for_paths = app.clone();
    off_main(app, move |state| {
        validate_chat_target(&input.chat_target_type, &input.chat_target_id)?;
        let (workspace, git, isolation_base) = {
            let db = state.lock_db()?;
            let workspace = get_git_workspace(&db, &input.workspace_id)?;
            if workspace.session_id.is_some() {
                return Err("这是合并用的临时工作区，请在原来的工作区里发起合并".to_string());
            }
            let git = setting_value(&db, "gitExecutable", "git");
            let isolation_base = expand_user_path(&setting_value(&db, "isolationRoot", ""));
            (workspace, git, isolation_base)
        };
        let root = workspace.work_path.clone();
        let target = git_merge::resolve_target(&git, &root, &input.target)?;
        let source_label = git_merge::head_label(&git, &root);
        if source_label == target.branch {
            return Err("不能把分支合并到它自己".to_string());
        }
        if let Err(error) = git_merge::fetch_target(&git, &root, &target) {
            // 只更新本地时，拉不到远程也可以继续；要推送就必须以最新的远程为准。
            if input.push && !error.contains("couldn't find remote ref") {
                return Err(format!("获取 {} 失败：{error}", target.display()));
            }
        }
        let base = git_merge::target_base(&git, &root, &target, input.push)?;
        let head = git_merge::head_commit(&git, &root)?;
        let message = merge_message(&source_label, &target);
        let mut result = MergeIntoResult {
            source_label: source_label.clone(),
            target_display: target.display(),
            ..Default::default()
        };
        let commit = match git_merge::plan_merge(&git, &root, &base, &head, &message)? {
            MergePlan::UpToDate => {
                result.status = "up_to_date";
                return Ok(result);
            }
            MergePlan::FastForward(commit) => {
                result.fast_forward = true;
                commit
            }
            MergePlan::Merged(commit) => commit,
            MergePlan::Conflict(_) => {
                return start_conflict_session(
                    state,
                    &app_for_paths,
                    &input,
                    &workspace,
                    &git,
                    isolation_base,
                    ConflictStart {
                        target: &target,
                        base: &base,
                        head: &head,
                        source_label: &source_label,
                        message: &message,
                    },
                    result,
                );
            }
        };
        let outcome = git_merge::apply(&git, &root, &target, &commit, input.push)?;
        result.status = "merged";
        result.commit = Some(commit.chars().take(7).collect());
        result.pushed = outcome.pushed;
        result.local_updated = outcome.local_updated;
        result.notes = outcome.notes;
        Ok(result)
    })
    .await
}

struct ConflictStart<'a> {
    target: &'a MergeTarget,
    base: &'a str,
    head: &'a str,
    source_label: &'a str,
    message: &'a str,
}

#[allow(clippy::too_many_arguments)]
fn start_conflict_session(
    state: &AppState,
    app: &AppHandle,
    input: &MergeIntoInput,
    workspace: &Workspace,
    git: &str,
    isolation_base: Option<PathBuf>,
    start: ConflictStart<'_>,
    mut result: MergeIntoResult,
) -> Result<MergeIntoResult, String> {
    let merge_id = Uuid::new_v4().to_string();
    let workspace_id = Uuid::new_v4().to_string();
    let dir = isolation_base
        .map(Ok)
        .unwrap_or_else(|| app.path().app_data_dir().map_err(|error| error.to_string()))?
        .join("workspaces")
        .join(&workspace.project_id)
        .join(format!("merge-{}", &merge_id[..8]));
    let conflicts = git_merge::start_conflict_worktree(
        git,
        &workspace.source_path,
        &dir,
        start.base,
        start.head,
        start.message,
    )?;
    let dir_text = dir.to_string_lossy().to_string();
    let now = Utc::now().to_rfc3339();
    let session = ChatSession {
        id: Uuid::new_v4().to_string(),
        project_id: Some(workspace.project_id.clone()),
        project_ids: vec![workspace.project_id.clone()],
        title: format!("合并 {} → {}", start.source_label, start.target.branch),
        target_type: if input.chat_target_type == "agent" { "agent" } else { "squad" },
        target_id: input.chat_target_id.clone(),
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    let merge_workspace = Workspace {
        id: workspace_id,
        project_id: workspace.project_id.clone(),
        session_id: Some(session.id.clone()),
        source_workspace_id: Some(workspace.id.clone()),
        name: format!("{} · 合并到 {}", workspace.name, start.target.branch),
        kind: "git",
        source_path: workspace.source_path.clone(),
        work_path: dir_text.clone(),
        branch: None,
        isolation_mode: "worktree",
        created_at: now.clone(),
    };
    let merge = MergeSession {
        id: merge_id,
        project_id: workspace.project_id.clone(),
        session_id: session.id.clone(),
        workspace_id: merge_workspace.id.clone(),
        source_workspace_id: workspace.id.clone(),
        source_label: start.source_label.to_string(),
        target_branch: start.target.branch.clone(),
        target_remote: start.target.remote.clone(),
        push: input.push,
        conflicts: conflicts.clone(),
        created_at: now.clone(),
    };
    let saved = (|| -> Result<(), String> {
        let mut db = state.lock_db()?;
        let exists: bool = db
            .query_row(
                if session.target_type == "agent" {
                    "SELECT EXISTS(SELECT 1 FROM agents WHERE id=?1)"
                } else {
                    "SELECT EXISTS(SELECT 1 FROM squads WHERE id=?1)"
                },
                [&session.target_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if !exists {
            return Err("选择的智能体或小队不存在".to_string());
        }
        let transaction = db.transaction().map_err(|error| error.to_string())?;
        transaction
            .execute(
                "INSERT INTO chat_sessions
                 (id, project_id, title, target_type, target_id, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    session.id,
                    session.project_id,
                    session.title,
                    session.target_type,
                    session.target_id,
                    session.created_at,
                    session.updated_at
                ],
            )
            .map_err(|error| error.to_string())?;
        crate::session::replace_linked_projects(
            &transaction,
            &session.id,
            &session.project_ids,
        )?;
        transaction
            .execute(
                "INSERT INTO workspaces
                 (id, project_id, session_id, source_workspace_id, name, kind, source_path,
                  work_path, branch, isolation_mode, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, ?9, ?10)",
                params![
                    merge_workspace.id,
                    merge_workspace.project_id,
                    merge_workspace.session_id,
                    merge_workspace.source_workspace_id,
                    merge_workspace.name,
                    merge_workspace.kind,
                    merge_workspace.source_path,
                    merge_workspace.work_path,
                    merge_workspace.isolation_mode,
                    merge_workspace.created_at
                ],
            )
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "INSERT INTO merge_sessions
                 (id, project_id, session_id, workspace_id, source_workspace_id, source_label,
                  source_commit, target_branch, target_remote, push, conflicts, status, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'resolving', ?12)",
                params![
                    merge.id,
                    merge.project_id,
                    merge.session_id,
                    merge.workspace_id,
                    merge.source_workspace_id,
                    merge.source_label,
                    start.head,
                    merge.target_branch,
                    merge.target_remote,
                    merge.push as i64,
                    serde_json::to_string(&merge.conflicts).unwrap_or_else(|_| "[]".to_string()),
                    merge.created_at
                ],
            )
            .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())
    })();
    if let Err(error) = saved {
        git_merge::remove_worktree(git, &workspace.source_path, &dir_text);
        return Err(error);
    }
    result.status = "conflict";
    result.conflicts = conflicts;
    result.merge_session = Some(merge);
    result.session = Some(session);
    result.workspace = Some(merge_workspace);
    Ok(result)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergePullResult {
    /// up_to_date / updated / conflict
    pub status: &'static str,
    pub conflicts: Vec<String>,
}

#[tauri::command]
pub async fn git_pull_merge_session(
    merge_id: String,
    app: AppHandle,
) -> Result<MergePullResult, String> {
    let _span = crate::diag::span("git_pull_merge_session");
    off_main(app, move |state| {
        let (merge, workspace, git) = {
            let db = state.lock_db()?;
            let merge = load_merge(&db, &merge_id)?;
            ensure_session_idle(&db, &merge.session_id)?;
            let workspace = get_workspace(&db, &merge.workspace_id)?;
            (merge, workspace, setting_value(&db, "gitExecutable", "git"))
        };
        let target = MergeTarget {
            branch: merge.target_branch.clone(),
            remote: merge.target_remote.clone(),
        };
        let outcome = git_merge::incorporate_remote(
            &git,
            &workspace.work_path,
            &target,
            &merge_message(&merge.source_label, &target),
        )?;
        let (status, conflicts) = match outcome {
            git_merge::RemoteUpdate::UpToDate => ("up_to_date", Vec::new()),
            git_merge::RemoteUpdate::Updated => ("updated", Vec::new()),
            git_merge::RemoteUpdate::Conflict(files) => ("conflict", files),
        };
        Ok(MergePullResult { status, conflicts })
    })
    .await
}

#[tauri::command]
pub async fn git_finish_merge_session(
    merge_id: String,
    app: AppHandle,
) -> Result<MergeFinishResult, String> {
    let _span = crate::diag::span("git_finish_merge_session");
    off_main(app, move |state| {
        let (merge, workspace, git) = {
            let db = state.lock_db()?;
            let merge = load_merge(&db, &merge_id)?;
            ensure_session_idle(&db, &merge.session_id)?;
            let workspace = get_workspace(&db, &merge.workspace_id)?;
            (merge, workspace, setting_value(&db, "gitExecutable", "git"))
        };
        let target = MergeTarget {
            branch: merge.target_branch.clone(),
            remote: merge.target_remote.clone(),
        };
        let commit = git_merge::commit_resolution(
            &git,
            &workspace.work_path,
            &merge_message(&merge.source_label, &target),
        )?;
        let outcome = git_merge::apply(&git, &workspace.source_path, &target, &commit, merge.push)?;
        close_merge(state, &merge, &workspace, &git, "completed")?;
        Ok(MergeFinishResult {
            commit: commit.chars().take(7).collect(),
            source_label: merge.source_label.clone(),
            target_display: merge.target_display(),
            pushed: outcome.pushed,
            local_updated: outcome.local_updated,
            notes: outcome.notes,
        })
    })
    .await
}

#[tauri::command]
pub async fn git_abort_merge_session(merge_id: String, app: AppHandle) -> Result<(), String> {
    let _span = crate::diag::span("git_abort_merge_session");
    off_main(app, move |state| {
        let (merge, workspace, git) = {
            let db = state.lock_db()?;
            let merge = load_merge(&db, &merge_id)?;
            ensure_session_idle(&db, &merge.session_id)?;
            let workspace = get_workspace(&db, &merge.workspace_id)?;
            (merge, workspace, setting_value(&db, "gitExecutable", "git"))
        };
        let _ = crate::git::run_strings(&git, &workspace.work_path, &["merge", "--abort"]);
        close_merge(state, &merge, &workspace, &git, "aborted")
    })
    .await
}

/// 删除临时 Worktree 和它在数据库里的工作区记录；聊天会话保留，作为这次合并的记录。
fn close_merge(
    state: &AppState,
    merge: &MergeSession,
    workspace: &Workspace,
    git: &str,
    status: &str,
) -> Result<(), String> {
    git_merge::remove_worktree(git, &workspace.source_path, &workspace.work_path);
    let mut db = state.lock_db()?;
    let transaction = db.transaction().map_err(|error| error.to_string())?;
    for sql in [
        "DELETE FROM session_workspaces WHERE workspace_id=?1 OR source_workspace_id=?1",
        "DELETE FROM turn_file_snapshots WHERE workspace_id=?1",
        "DELETE FROM diff_chunk_states WHERE workspace_id=?1",
        "DELETE FROM file_review_baselines WHERE workspace_id=?1",
        "DELETE FROM workspaces WHERE id=?1",
    ] {
        transaction
            .execute(sql, [&workspace.id])
            .map_err(|error| error.to_string())?;
    }
    transaction
        .execute(
            "UPDATE merge_sessions SET status=?1 WHERE id=?2",
            params![status, merge.id],
        )
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())
}
