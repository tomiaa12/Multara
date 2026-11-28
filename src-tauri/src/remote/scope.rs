use std::{
    collections::{HashMap, HashSet},
    path::{Component, Path, PathBuf},
};

use rusqlite::{Connection, OptionalExtension};
use serde_json::Value;

use super::permission::{DeviceCtx, Role};
use super::store::Group;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Access {
    Hidden,
    Read,
    Write,
}

pub fn project_in_scope(group: &Group, project_id: &str) -> bool {
    group.all_projects || group.project_ids.iter().any(|id| id == project_id)
}

pub fn agent_in_scope(group: &Group, target_id: &str) -> bool {
    group.all_agents || group.agent_ids.iter().any(|id| id == target_id)
}

fn err(error: rusqlite::Error) -> String {
    error.to_string()
}

/// 判定可见性所需的、跨多个会话复用的数据。
pub struct ScopeData {
    origins: HashMap<String, String>,
    device_groups: HashMap<String, String>,
}

impl ScopeData {
    pub fn load(db: &Connection) -> Result<Self, String> {
        let mut statement = db
            .prepare("SELECT id, origin_principal FROM chat_sessions WHERE origin_principal IS NOT NULL")
            .map_err(err)?;
        let origins = statement
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
            .map_err(err)?
            .collect::<rusqlite::Result<HashMap<_, _>>>()
            .map_err(err)?;
        let mut statement = db
            .prepare("SELECT id, group_id FROM remote_devices")
            .map_err(err)?;
        let device_groups = statement
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
            .map_err(err)?
            .collect::<rusqlite::Result<HashMap<_, _>>>()
            .map_err(err)?;
        Ok(Self {
            origins,
            device_groups,
        })
    }

    /// 规则见 docs/多设备-技术方案.md §4.5。
    pub fn session_access(&self, ctx: &DeviceCtx, session_id: &str, project_ids: &[String]) -> Access {
        let projects_ok = project_ids
            .iter()
            .all(|project_id| project_in_scope(&ctx.group, project_id));
        if !projects_ok {
            return Access::Hidden;
        }
        let origin = self.origins.get(session_id).map(String::as_str);
        let own_key = ctx.principal_key();
        if origin == Some(own_key.as_str()) {
            return Access::Write;
        }
        if ctx.role() == Role::Owner {
            return Access::Write;
        }
        let other_device = origin.and_then(|key| key.strip_prefix("device:"));
        if let Some(device_id) = other_device {
            let same_group = self
                .device_groups
                .get(device_id)
                .is_some_and(|group_id| *group_id == ctx.group.id);
            return if same_group && ctx.group.share_in_group {
                Access::Read
            } else {
                Access::Hidden
            };
        }
        if project_ids.is_empty() {
            return Access::Hidden;
        }
        if ctx.role() >= Role::Developer {
            Access::Write
        } else {
            Access::Read
        }
    }
}

pub fn session_access(db: &Connection, ctx: &DeviceCtx, session_id: &str) -> Result<Access, String> {
    let exists: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM chat_sessions WHERE id=?1)",
            [session_id],
            |row| row.get(0),
        )
        .map_err(err)?;
    if !exists {
        return Ok(Access::Hidden);
    }
    let project_ids = crate::session::linked_project_ids(db, session_id)?;
    Ok(ScopeData::load(db)?.session_access(ctx, session_id, &project_ids))
}

pub fn session_of_run(db: &Connection, run_id: &str) -> Result<Option<String>, String> {
    db.query_row(
        "SELECT session_id FROM agent_runs WHERE id=?1",
        [run_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(err)
}

pub fn session_of_task(db: &Connection, task_id: &str) -> Result<Option<String>, String> {
    db.query_row("SELECT session_id FROM tasks WHERE id=?1", [task_id], |row| {
        row.get(0)
    })
    .optional()
    .map_err(err)
}

pub fn session_of_message(db: &Connection, message_id: &str) -> Result<Option<String>, String> {
    db.query_row(
        "SELECT session_id FROM messages WHERE id=?1",
        [message_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(err)
}

pub fn session_of_attachment(db: &Connection, attachment_id: &str) -> Result<Option<String>, String> {
    db.query_row(
        "SELECT session_id FROM message_attachments WHERE id=?1",
        [attachment_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(err)
}

pub struct WorkspaceInfo {
    pub project_id: String,
    pub session_id: Option<String>,
    pub work_path: String,
    pub isolation_mode: String,
}

pub fn workspace_info(db: &Connection, workspace_id: &str) -> Result<Option<WorkspaceInfo>, String> {
    db.query_row(
        "SELECT project_id, session_id, work_path, isolation_mode FROM workspaces WHERE id=?1",
        [workspace_id],
        |row| {
            Ok(WorkspaceInfo {
                project_id: row.get(0)?,
                session_id: row.get(1)?,
                work_path: row.get(2)?,
                isolation_mode: row.get(3)?,
            })
        },
    )
    .optional()
    .map_err(err)
}

/// 工作区对设备的访问级别：需求在范围内；会话专属工作区还要求会话可见。
pub fn workspace_access(db: &Connection, ctx: &DeviceCtx, workspace_id: &str) -> Result<Access, String> {
    let Some(info) = workspace_info(db, workspace_id)? else {
        return Ok(Access::Hidden);
    };
    if !project_in_scope(&ctx.group, &info.project_id) {
        return Ok(Access::Hidden);
    }
    match info.session_id {
        Some(session_id) => session_access(db, ctx, &session_id),
        None => Ok(Access::Write),
    }
}

/// 所有工作区根目录（含会话专属 worktree）；终端 cwd 校验用。
pub fn visible_workspace_roots(db: &Connection, ctx: &DeviceCtx) -> Result<Vec<PathBuf>, String> {
    let mut statement = db
        .prepare("SELECT id FROM workspaces")
        .map_err(err)?;
    let ids = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(err)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(err)?;
    let mut roots = Vec::new();
    for id in ids {
        if workspace_access(db, ctx, &id)? == Access::Hidden {
            continue;
        }
        if let Some(info) = workspace_info(db, &id)? {
            if let Ok(root) = std::fs::canonicalize(&info.work_path) {
                roots.push(root);
            }
        }
    }
    Ok(roots)
}

/// 远程传来的工作区相对路径：拒绝绝对路径和 `..`，真实路径（解析符号链接后）必须仍在根目录内。
pub fn check_relative_path(root: &str, relative: &str) -> Result<(), String> {
    let relative = relative.trim();
    if relative.is_empty() {
        return Ok(());
    }
    let path = Path::new(relative);
    let escapes = path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    });
    if escapes || path.is_absolute() {
        return Err(tr!("路径不在工作区内"));
    }
    let root = std::fs::canonicalize(root).map_err(|_| tr!("工作区目录不存在"))?;
    let mut probe = root.join(path);
    // 目标可能还不存在（新建文件）：向上找到第一个已存在的祖先再校验。
    loop {
        if probe.exists() {
            let real = std::fs::canonicalize(&probe).map_err(|error| error.to_string())?;
            if !real.starts_with(&root) {
                return Err(tr!("路径不在工作区内"));
            }
            return Ok(());
        }
        if !probe.pop() || !probe.starts_with(&root) {
            return Ok(());
        }
    }
}

pub fn check_cwd(db: &Connection, ctx: &DeviceCtx, cwd: &str) -> Result<(), String> {
    let real = std::fs::canonicalize(cwd.trim()).map_err(|_| tr!("目录不存在"))?;
    let inside = visible_workspace_roots(db, ctx)?
        .iter()
        .any(|root| real.starts_with(root));
    if inside {
        Ok(())
    } else {
        Err(tr!("目录不在授权的工作区内"))
    }
}

fn array_mut<'a>(value: &'a mut Value, key: &str) -> Option<&'a mut Vec<Value>> {
    value.get_mut(key)?.as_array_mut()
}

fn str_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key)?.as_str()
}

fn string_list(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// 按设备授权裁剪返回给远程的数据。只依赖序列化后的 camelCase 字段。
pub struct Filter<'a> {
    ctx: &'a DeviceCtx,
    data: ScopeData,
    visible_sessions: HashSet<String>,
    visible_workspaces: HashSet<String>,
}

impl<'a> Filter<'a> {
    pub fn new(db: &Connection, ctx: &'a DeviceCtx) -> Result<Self, String> {
        let data = ScopeData::load(db)?;
        let mut statement = db
            .prepare("SELECT id FROM chat_sessions")
            .map_err(err)?;
        let session_ids = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(err)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(err)?;
        let mut visible_sessions = HashSet::new();
        for session_id in session_ids {
            let project_ids = crate::session::linked_project_ids(db, &session_id)?;
            if data.session_access(ctx, &session_id, &project_ids) != Access::Hidden {
                visible_sessions.insert(session_id);
            }
        }
        let mut statement = db
            .prepare("SELECT id, project_id, session_id FROM workspaces")
            .map_err(err)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })
            .map_err(err)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(err)?;
        let visible_workspaces = rows
            .into_iter()
            .filter(|(_, project_id, session_id)| {
                project_in_scope(&ctx.group, project_id)
                    && session_id
                        .as_ref()
                        .is_none_or(|session_id| visible_sessions.contains(session_id))
            })
            .map(|(id, _, _)| id)
            .collect();
        Ok(Self {
            ctx,
            data,
            visible_sessions,
            visible_workspaces,
        })
    }

    pub fn session_visible(&self, session_id: &str) -> bool {
        self.visible_sessions.contains(session_id)
    }

    pub fn session_access(&self, session: &Value) -> Access {
        let Some(id) = str_field(session, "id") else {
            return Access::Hidden;
        };
        self.data
            .session_access(self.ctx, id, &string_list(session, "projectIds"))
    }

    fn keep_projects(&self, items: &mut Vec<Value>) {
        items.retain(|item| {
            str_field(item, "id").is_some_and(|id| project_in_scope(&self.ctx.group, id))
        });
    }

    fn keep_workspaces(&self, items: &mut Vec<Value>) {
        items.retain(|item| {
            str_field(item, "id").is_some_and(|id| self.visible_workspaces.contains(id))
        });
    }

    fn keep_by_session(&self, items: &mut Vec<Value>, key: &str) {
        items.retain(|item| {
            str_field(item, key).is_some_and(|id| self.visible_sessions.contains(id))
        });
    }

    fn keep_agents(&self, items: &mut Vec<Value>) {
        items.retain(|item| {
            str_field(item, "id").is_some_and(|id| agent_in_scope(&self.ctx.group, id))
        });
    }

    /// 每个会话附上本设备对它的访问级别，界面据此禁用输入框。
    fn mark_sessions(&self, items: &mut [Value]) {
        for item in items.iter_mut() {
            let access = match self.session_access(item) {
                Access::Write => "write",
                Access::Read => "read",
                Access::Hidden => "hidden",
            };
            if let Some(object) = item.as_object_mut() {
                object.insert("remoteAccess".into(), Value::String(access.into()));
            }
        }
    }

    pub fn bootstrap(&self, mut value: Value) -> Value {
        if let Some(items) = array_mut(&mut value, "projects") {
            self.keep_projects(items);
        }
        if let Some(items) = array_mut(&mut value, "workspaces") {
            self.keep_workspaces(items);
        }
        if let Some(items) = array_mut(&mut value, "sessions") {
            self.keep_by_session(items, "id");
            self.mark_sessions(items);
        }
        if let Some(items) = array_mut(&mut value, "tasks") {
            self.keep_by_session(items, "sessionId");
        }
        if let Some(items) = array_mut(&mut value, "inbox") {
            self.keep_by_session(items, "sessionId");
        }
        if let Some(items) = array_mut(&mut value, "agents") {
            self.keep_agents(items);
        }
        if let Some(items) = array_mut(&mut value, "squads") {
            self.keep_agents(items);
        }
        if let Some(items) = array_mut(&mut value, "mergeSessions") {
            items.retain(|item| {
                str_field(item, "workspaceId").is_some_and(|id| self.visible_workspaces.contains(id))
            });
        }
        if let Some(object) = value.as_object_mut() {
            object.insert("automations".into(), Value::Array(Vec::new()));
            object.insert("resources".into(), Value::Array(Vec::new()));
            let settings = object
                .get("settings")
                .and_then(Value::as_object)
                .map(|settings| {
                    settings
                        .iter()
                        .filter(|(key, _)| SAFE_SETTINGS.contains(&key.as_str()))
                        .map(|(key, value)| (key.clone(), value.clone()))
                        .collect::<serde_json::Map<_, _>>()
                })
                .unwrap_or_default();
            object.insert("settings".into(), Value::Object(settings));
        }
        value
    }

    /// `(Vec<Project>, Vec<Workspace>)` 序列化为二元数组。
    pub fn projects_and_workspaces(&self, mut value: Value) -> Value {
        if let Some(pair) = value.as_array_mut() {
            if let Some(items) = pair.get_mut(0).and_then(Value::as_array_mut) {
                self.keep_projects(items);
            }
            if let Some(items) = pair.get_mut(1).and_then(Value::as_array_mut) {
                self.keep_workspaces(items);
            }
        }
        value
    }

    pub fn agents_and_squads(&self, mut value: Value) -> Value {
        if let Some(pair) = value.as_array_mut() {
            for index in 0..2 {
                if let Some(items) = pair.get_mut(index).and_then(Value::as_array_mut) {
                    self.keep_agents(items);
                }
            }
        }
        value
    }

    pub fn session_list(&self, mut value: Value, key: &str) -> Value {
        if let Some(items) = value.as_array_mut() {
            self.keep_by_session(items, key);
        }
        value
    }

    pub fn task_list(&self, value: Value) -> Value {
        self.session_list(value, "sessionId")
    }

    /// `TaskInboxSurfaces` 等对象：里面所有带 sessionId 的数组都裁剪。
    pub fn nested_sessions(&self, mut value: Value) -> Value {
        if let Some(object) = value.as_object_mut() {
            for item in object.values_mut() {
                if let Some(items) = item.as_array_mut() {
                    if items.iter().any(|entry| entry.get("sessionId").is_some()) {
                        self.keep_by_session(items, "sessionId");
                    }
                }
            }
        }
        value
    }

    pub fn active_diff(&self, mut value: Value) -> Value {
        if let Some(object) = value.as_object_mut() {
            for item in object.values_mut() {
                if let Some(items) = item.as_array_mut() {
                    items.retain(|entry| {
                        let session_ok = str_field(entry, "sessionId")
                            .is_none_or(|id| self.visible_sessions.contains(id));
                        let workspace_ok = str_field(entry, "workspaceId")
                            .is_none_or(|id| self.visible_workspaces.contains(id));
                        session_ok && workspace_ok
                    });
                }
            }
        }
        value
    }
}

/// 远程可以读到的设置：只保留影响界面展示、不含路径和密钥的项。
const SAFE_SETTINGS: &[&str] = &[
    "language",
    "theme",
    "fontSize",
    "editorFontSize",
    "editorWordWrap",
    "terminalFontSize",
    "uiDensity",
    "timeFormat",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_escaping_paths() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_string_lossy().to_string();
        std::fs::write(dir.path().join("a.txt"), "x").unwrap();
        assert!(check_relative_path(&root, "a.txt").is_ok());
        assert!(check_relative_path(&root, "new/file.txt").is_ok());
        assert!(check_relative_path(&root, "../secret").is_err());
        assert!(check_relative_path(&root, "sub/../../secret").is_err());
        #[cfg(windows)]
        assert!(check_relative_path(&root, "C:\\Windows\\win.ini").is_err());
        #[cfg(not(windows))]
        assert!(check_relative_path(&root, "/etc/passwd").is_err());
    }
}
