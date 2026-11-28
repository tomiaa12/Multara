use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::store::{self, Device, Group};
use crate::channel::grant::ChannelGrant;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Viewer,
    Runner,
    Developer,
    Owner,
}

impl Role {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "viewer" => Some(Self::Viewer),
            "runner" => Some(Self::Runner),
            "developer" => Some(Self::Developer),
            "owner" => Some(Self::Owner),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Viewer => "viewer",
            Self::Runner => "runner",
            Self::Developer => "developer",
            Self::Owner => "owner",
        }
    }

    /// 角色到权限的唯一映射；矩阵见 docs/多设备-技术方案.md §4.2。
    pub fn allows(self, perm: Perm) -> bool {
        let minimum = match perm {
            Perm::ProjectRead | Perm::FileRead | Perm::TaskRead | Perm::GitRead => Role::Viewer,
            Perm::TaskCreate | Perm::AgentPlan | Perm::AgentRun => Role::Runner,
            Perm::AgentRunDirect
            | Perm::FileWrite
            | Perm::FileDelete
            | Perm::GitWrite
            | Perm::GitPush
            | Perm::ProjectWrite => Role::Developer,
            Perm::TaskManage | Perm::Terminal | Perm::ProjectDelete => Role::Owner,
        };
        self >= minimum
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Perm {
    ProjectRead,
    ProjectWrite,
    ProjectDelete,
    FileRead,
    FileWrite,
    FileDelete,
    TaskRead,
    TaskCreate,
    TaskManage,
    AgentPlan,
    AgentRun,
    AgentRunDirect,
    GitRead,
    GitWrite,
    GitPush,
    Terminal,
}

pub const ALL_PERMS: &[Perm] = &[
    Perm::ProjectRead,
    Perm::ProjectWrite,
    Perm::ProjectDelete,
    Perm::FileRead,
    Perm::FileWrite,
    Perm::FileDelete,
    Perm::TaskRead,
    Perm::TaskCreate,
    Perm::TaskManage,
    Perm::AgentPlan,
    Perm::AgentRun,
    Perm::AgentRunDirect,
    Perm::GitRead,
    Perm::GitWrite,
    Perm::GitPush,
    Perm::Terminal,
];

impl Perm {
    pub fn as_str(self) -> &'static str {
        match self {
            Perm::ProjectRead => "projectRead",
            Perm::ProjectWrite => "projectWrite",
            Perm::ProjectDelete => "projectDelete",
            Perm::FileRead => "fileRead",
            Perm::FileWrite => "fileWrite",
            Perm::FileDelete => "fileDelete",
            Perm::TaskRead => "taskRead",
            Perm::TaskCreate => "taskCreate",
            Perm::TaskManage => "taskManage",
            Perm::AgentPlan => "agentPlan",
            Perm::AgentRun => "agentRun",
            Perm::AgentRunDirect => "agentRunDirect",
            Perm::GitRead => "gitRead",
            Perm::GitWrite => "gitWrite",
            Perm::GitPush => "gitPush",
            Perm::Terminal => "terminal",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        ALL_PERMS.iter().copied().find(|perm| perm.as_str() == value)
    }

    /// 分组开启审批时，这些操作需要主机端逐次批准。
    pub fn high_risk(self) -> bool {
        matches!(
            self,
            Perm::GitPush | Perm::Terminal | Perm::FileDelete | Perm::ProjectDelete
        )
    }
}

/// 一台已连接设备的授权上下文，每个请求开始时从库里重新读取，权限变更立即生效。
#[derive(Clone, Debug)]
pub struct DeviceCtx {
    pub device: Device,
    pub group: Group,
}

impl DeviceCtx {
    pub fn load(db: &Connection, device_id: &str) -> Result<Option<Self>, String> {
        let Some(device) = store::get_device(db, device_id)? else {
            return Ok(None);
        };
        let group = match store::get_group(db, &device.group_id)? {
            Some(group) => group,
            None => store::get_group(db, &store::default_group_id(db)?)?
                .ok_or_else(|| tr!("分组不存在"))?,
        };
        Ok(Some(Self { device, group }))
    }

    pub fn role(&self) -> Role {
        self.group.role
    }

    pub fn can(&self, perm: Perm) -> bool {
        self.group.role.allows(perm)
    }

    pub fn principal_key(&self) -> String {
        format!("device:{}", self.device.id)
    }

    pub fn perms(&self) -> Vec<&'static str> {
        ALL_PERMS
            .iter()
            .filter(|perm| self.can(**perm))
            .map(|perm| perm.as_str())
            .collect()
    }
}

/// 运行中的操作发起者。`origin_principal` 列的格式：`device:<id>` / `channel:<installation>:<sender>`。
#[derive(Clone, Debug)]
pub enum Principal {
    Local,
    Device(DeviceCtx),
    Channel {
        installation_id: String,
        sender_id: String,
        grant: ChannelGrant,
    },
}

impl Principal {
    pub fn key(&self) -> Option<String> {
        match self {
            Principal::Local => None,
            Principal::Device(ctx) => Some(ctx.principal_key()),
            Principal::Channel {
                installation_id,
                sender_id,
                ..
            } => Some(format!("channel:{installation_id}:{sender_id}")),
        }
    }
}

fn principal_from_key(db: &Connection, key: &str) -> Result<Option<Principal>, String> {
    if let Some(device_id) = key.strip_prefix("device:") {
        // 设备已移除：按最严格处理，当作一个没有任何需求的 Viewer。
        return Ok(Some(match DeviceCtx::load(db, device_id)? {
            Some(ctx) => Principal::Device(ctx),
            None => Principal::Device(revoked_device(device_id)),
        }));
    }
    if let Some(rest) = key.strip_prefix("channel:") {
        let Some((installation_id, sender_id)) = rest.split_once(':') else {
            return Ok(None);
        };
        return Ok(Some(channel_principal(db, installation_id, sender_id)));
    }
    Ok(None)
}

fn revoked_device(device_id: &str) -> DeviceCtx {
    DeviceCtx {
        device: Device {
            id: device_id.to_string(),
            name: String::new(),
            owner_note: String::new(),
            fingerprint: String::new(),
            group_id: String::new(),
            paired_at: String::new(),
            last_seen_at: None,
            last_address: None,
        },
        group: Group {
            id: String::new(),
            name: String::new(),
            role: Role::Viewer,
            all_projects: false,
            all_agents: false,
            share_in_group: false,
            require_approval: true,
            max_running: 1,
            is_default: false,
            project_ids: Vec::new(),
            agent_ids: Vec::new(),
            device_count: 0,
            pages: Vec::new(),
        },
    }
}

fn channel_principal(db: &Connection, installation_id: &str, sender_id: &str) -> Principal {
    let grant = db
        .query_row(
            "SELECT permissions_json FROM channel_allowed_users WHERE installation_id=?1 AND channel_user_id=?2",
            [installation_id, sender_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .ok()
        .flatten()
        .flatten()
        .and_then(|json| serde_json::from_str::<ChannelGrant>(&json).ok())
        .unwrap_or_default();
    Principal::Channel {
        installation_id: installation_id.to_string(),
        sender_id: sender_id.to_string(),
        grant,
    }
}

/// 用户消息的来源 → 主体。设备消息带 `meta.deviceId`；渠道消息带 `installationId` / `senderId`。
fn principal_from_source(
    db: &Connection,
    _source_type: Option<&str>,
    meta_json: Option<&str>,
) -> Result<Option<Principal>, String> {
    let meta: serde_json::Value = meta_json
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default();
    if let Some(device_id) = meta.get("deviceId").and_then(|value| value.as_str()) {
        return principal_from_key(db, &format!("device:{device_id}"));
    }
    let installation = meta.get("installationId").and_then(|value| value.as_str());
    let sender = meta.get("senderId").and_then(|value| value.as_str());
    if let (Some(installation), Some(sender)) = (installation, sender) {
        return Ok(Some(channel_principal(db, installation, sender)));
    }
    Ok(None)
}

/// 一次运行的发起者：运行自身记录 → 触发它的用户消息 → 父运行 → 会话来源 → 本机。
pub fn run_principal(db: &Connection, run_id: &str) -> Result<Principal, String> {
    let mut current = Some(run_id.to_string());
    let mut depth = 0;
    while let Some(id) = current.take() {
        depth += 1;
        if depth > 16 {
            break;
        }
        let row: Option<(Option<String>, String, Option<String>, Option<String>)> = db
            .query_row(
                "SELECT origin_principal, session_id, turn_id, parent_run_id FROM agent_runs WHERE id=?1",
                [&id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        let Some((origin, session_id, turn_id, parent)) = row else {
            break;
        };
        if let Some(principal) = origin
            .as_deref()
            .map(|key| principal_from_key(db, key))
            .transpose()?
            .flatten()
        {
            return Ok(principal);
        }
        if let Some(turn_id) = turn_id {
            let source: Option<(Option<String>, Option<String>)> = db
                .query_row(
                    "SELECT source_type, source_meta_json FROM messages
                     WHERE turn_id=?1 AND role='user' ORDER BY created_at LIMIT 1",
                    [&turn_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(|error| error.to_string())?;
            if let Some((kind, meta)) = source {
                if let Some(principal) = principal_from_source(db, kind.as_deref(), meta.as_deref())? {
                    return Ok(principal);
                }
            }
        }
        if parent.is_none() {
            return session_principal(db, &session_id);
        }
        current = parent;
    }
    Ok(Principal::Local)
}

pub fn is_device_run(db: &Connection, run_id: &str) -> bool {
    matches!(run_principal(db, run_id), Ok(Principal::Device(_)))
}

pub fn session_principal(db: &Connection, session_id: &str) -> Result<Principal, String> {
    let origin: Option<String> = db
        .query_row(
            "SELECT origin_principal FROM chat_sessions WHERE id=?1",
            [session_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .flatten();
    if let Some(principal) = origin
        .as_deref()
        .map(|key| principal_from_key(db, key))
        .transpose()?
        .flatten()
    {
        return Ok(principal);
    }
    let source: Option<(Option<String>, Option<String>)> = db
        .query_row(
            "SELECT source_type, source_meta_json FROM messages
             WHERE session_id=?1 AND role='user' ORDER BY created_at DESC LIMIT 1",
            [session_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if let Some((kind, meta)) = source {
        if let Some(principal) = principal_from_source(db, kind.as_deref(), meta.as_deref())? {
            return Ok(principal);
        }
    }
    Ok(Principal::Local)
}

fn project_allowed(principal: &Principal, db: &Connection, project_id: &str) -> bool {
    let in_scope = match principal {
        Principal::Local => true,
        Principal::Device(ctx) => super::scope::project_in_scope(&ctx.group, project_id),
        Principal::Channel { grant, .. } => grant.allows_project(project_id),
    };
    in_scope && project_exists(db, project_id)
}

fn project_exists(db: &Connection, project_id: &str) -> bool {
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1)",
        [project_id],
        |row| row.get::<_, bool>(0),
    )
    .unwrap_or(false)
}

/// Multara CLI 在运行内执行命令前的主体校验（设备、IM 渠道用户）。本机运行直接放行。
pub fn ensure_cli_scope(
    db: &Connection,
    run_id: Option<&str>,
    resource: &str,
    verb: &str,
    project_ids: &[String],
) -> Result<(), String> {
    let Some(run_id) = run_id.map(str::trim).filter(|id| !id.is_empty()) else {
        return Ok(());
    };
    let principal = run_principal(db, run_id)?;
    let read_only = matches!(verb, "list" | "get" | "runs" | "show" | "status" | "read");
    match &principal {
        Principal::Local => return Ok(()),
        Principal::Device(ctx) => {
            let denied = matches!(
                (resource, verb),
                ("automation", _)
                    | ("workflow", _)
                    | ("channel", _)
                    | ("feishu", _)
                    | ("agent", "create" | "update" | "delete")
                    | ("squad", "create" | "update" | "member" | "delete")
                    | ("workspace", "add" | "detach")
                    | ("skill", "create" | "import" | "delete")
                    | ("mcp", _)
            ) && !(read_only && resource != "channel" && resource != "feishu");
            if denied {
                return Err(tr!("远程设备发起的运行不能执行这个操作"));
            }
            let needed = match (resource, verb) {
                ("task", "create") => Some(Perm::TaskCreate),
                ("task", _) if !read_only => Some(Perm::TaskCreate),
                ("project", "create" | "update") => Some(Perm::ProjectWrite),
                ("project", "delete" | "archive") => Some(Perm::ProjectDelete),
                _ => None,
            };
            if let Some(perm) = needed {
                if !ctx.can(perm) {
                    return Err(tr!("发起这次运行的设备没有这个权限"));
                }
            }
        }
        Principal::Channel { grant, .. } => {
            let allowed = match resource {
                "task" if !read_only => grant.create_task,
                "agent" if !read_only => grant.add_agent,
                "squad" if !read_only => grant.add_squad,
                "automation" if !read_only => grant.create_automation,
                "workflow" => grant.use_workflow,
                "workspace" if !read_only => grant.write_files,
                _ => true,
            };
            if !allowed {
                return Err(tr!("这个集成用户不允许使用这个能力"));
            }
        }
    }
    for project_id in project_ids {
        if !project_allowed(&principal, db, project_id) {
            return Err(tr!("发起这次运行的用户无权访问这个需求"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_matrix_is_monotonic() {
        for perm in ALL_PERMS {
            if Role::Viewer.allows(*perm) {
                assert!(Role::Runner.allows(*perm));
            }
            if Role::Runner.allows(*perm) {
                assert!(Role::Developer.allows(*perm));
            }
            if Role::Developer.allows(*perm) {
                assert!(Role::Owner.allows(*perm));
            }
        }
        assert!(!Role::Viewer.allows(Perm::TaskCreate));
        assert!(Role::Runner.allows(Perm::AgentRun));
        assert!(!Role::Runner.allows(Perm::AgentRunDirect));
        assert!(!Role::Developer.allows(Perm::Terminal));
        assert!(Role::Owner.allows(Perm::Terminal));
    }

    #[test]
    fn perm_names_round_trip() {
        for perm in ALL_PERMS {
            assert_eq!(Perm::parse(perm.as_str()), Some(*perm));
        }
    }
}
