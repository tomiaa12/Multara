use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub note: String,
    pub workspace_count: u32,
    pub created_at: String,
    pub opened_at: String,
    pub archived_at: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    pub project_id: String,
    pub session_id: Option<String>,
    pub source_workspace_id: Option<String>,
    pub name: String,
    pub kind: &'static str,
    pub source_path: String,
    pub work_path: String,
    pub branch: Option<String>,
    pub isolation_mode: &'static str,
    pub created_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCandidate {
    pub path: String,
    pub name: String,
    pub kind: &'static str,
    pub branches: Vec<String>,
    pub current_branch: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    pub depth: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symlink_target: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileContent {
    pub workspace_id: String,
    pub path: String,
    pub content: String,
    pub language: String,
    pub size: u64,
    pub read_only: bool,
    /// 空字符串表示文本。image / audio / video 时 content 是 data URL，只读预览。
    #[serde(default)]
    pub media: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitFileStatus {
    pub path: String,
    pub index_status: String,
    pub worktree_status: String,
    pub staged: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitIndexFile {
    pub in_repo: bool,
    pub tracked: bool,
    pub binary: bool,
    pub content: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitLineBlame {
    pub committed: bool,
    pub hash: String,
    pub author_name: String,
    pub author_email: String,
    pub author_time: i64,
    pub subject: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHistoryItem {
    pub hash: String,
    pub parents: Vec<String>,
    pub subject: String,
    pub author: String,
    pub committed_at: String,
    pub refs: Vec<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHistoryPage {
    pub items: Vec<GitHistoryItem>,
    pub has_more: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitCommitFile {
    pub path: String,
    pub status: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitRepositoryStatus {
    pub workspace_id: String,
    pub name: String,
    pub path: String,
    pub branch: String,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub has_remote: bool,
    pub merging: bool,
    pub changes: Vec<GitFileStatus>,
    pub branches: Vec<String>,
    pub remote_branches: Vec<String>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitCommitInput {
    pub workspace_id: String,
    pub message: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Runtime {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub command: String,
    pub args: Vec<String>,
    pub path: Option<String>,
    pub version: Option<String>,
    pub status: &'static str,
    pub max_concurrent: u8,
    #[serde(default)]
    pub protocol: String,
    #[serde(default)]
    pub api_url: String,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomRuntimeInput {
    pub id: Option<String>,
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub max_concurrent: u8,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub protocol: String,
    #[serde(default)]
    pub api_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub wire_api: String,
    #[serde(default)]
    pub key_style: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomRuntimeForm {
    pub id: String,
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub max_concurrent: u8,
    pub provider: String,
    pub protocol: String,
    pub api_url: String,
    pub api_key: String,
    pub models: Vec<String>,
    pub env: std::collections::HashMap<String, String>,
    pub kind: String,
    pub description: String,
    pub wire_api: String,
    pub key_style: String,
    pub has_api_key: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub avatar: String,
    pub description: String,
    pub instructions: String,
    pub runtime_id: String,
    pub model: String,
    pub max_concurrent_tasks: u8,
    pub skill_ids: Vec<String>,
    pub mcp_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_instructions: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentInput {
    pub id: Option<String>,
    pub name: String,
    pub avatar: String,
    pub description: String,
    pub instructions: String,
    pub runtime_id: String,
    pub model: String,
    pub max_concurrent_tasks: u8,
    pub skill_ids: Vec<String>,
    pub mcp_ids: Vec<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Squad {
    pub id: String,
    pub name: String,
    pub description: String,
    /// 队长处理该小队任务时注入到 prompt 的指引。
    pub instructions: String,
    pub leader_agent_id: String,
    pub member_agent_ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SquadInput {
    pub id: Option<String>,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub instructions: String,
    pub leader_agent_id: String,
    pub member_agent_ids: Vec<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSession {
    pub id: String,
    pub project_id: Option<String>,
    pub project_ids: Vec<String>,
    pub title: String,
    pub target_type: &'static str,
    pub target_id: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskChecklistItem {
    pub id: String,
    pub text: String,
    pub done: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub session_id: String,
    pub title: String,
    pub description: String,
    pub agent_id: String,
    pub status: &'static str,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_task_id: Option<String>,
    #[serde(default)]
    pub priority: String,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub checklist: Vec<TaskChecklistItem>,
    #[serde(default)]
    pub pr_url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskInput {
    pub title: String,
    pub description: String,
    pub agent_id: String,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub project_ids: Vec<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub source_session_id: Option<String>,
    #[serde(default)]
    pub parent_task_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSessionInput {
    pub project_id: Option<String>,
    pub target_type: String,
    pub target_id: String,
    pub title: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineItem {
    pub seq: u32,
    pub event_type: &'static str,
    pub call_id: Option<String>,
    pub tool: Option<String>,
    pub content: Option<String>,
    pub input: Option<serde_json::Value>,
    pub output: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub replaces_final_text: bool,
    /// 子代理（Claude Task/Agent）里的调用指向发起它的父调用。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_call_id: Option<String>,
    /// 工具调用结果：`ok` 或 `error`。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickAction {
    pub label: String,
    pub prompt: String,
    #[serde(default)]
    pub primary: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageAttachment {
    pub id: String,
    pub name: String,
    pub path: String,
    pub mime_type: String,
    pub size: u64,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageAttachmentInput {
    pub name: String,
    pub path: String,
    pub mime_type: String,
    pub size: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: String,
    pub session_id: String,
    pub turn_id: Option<String>,
    pub role: &'static str,
    pub content: String,
    pub run_id: Option<String>,
    pub agent_id: Option<String>,
    pub audience_type: Option<&'static str>,
    pub audience_id: Option<String>,
    pub target_type: Option<&'static str>,
    pub target_id: Option<String>,
    pub failure_reason: Option<String>,
    pub elapsed_ms: Option<u64>,
    pub message_kind: &'static str,
    pub quick_actions: Vec<QuickAction>,
    pub timeline: Vec<TimelineItem>,
    pub attachments: Vec<MessageAttachment>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feedback: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunEvent {
    pub run_id: String,
    pub session_id: String,
    pub agent_id: String,
    pub status: &'static str,
    pub chunk: Option<String>,
    pub error: Option<String>,
    pub timeline_item: Option<TimelineItem>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub replaces_final_text: bool,
    pub elapsed_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_kind: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub affected_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalResource {
    pub id: String,
    pub kind: &'static str,
    pub owner: String,
    /// global 或 workspace。
    pub scope: &'static str,
    /// 来源的显示名，例如 "Claude Code" 或 "项目名 · .github/skills"。
    pub source_label: String,
    pub name: String,
    pub description: String,
    pub source_path: String,
    pub enabled: bool,
    /// 技能取 SKILL.md 的修改时间，MCP 取配置文件的修改时间。
    pub updated_at: String,
    /// MCP 传输：stdio、http 或 sse。技能为空。
    pub transport: String,
    /// 命令或 URL，列表里不放密钥。
    pub summary: String,
    pub needs_auth: bool,
    /// ready、needs_auth、needs_input、disabled、incomplete。技能为空。
    pub config_status: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceSource {
    pub owner: String,
    pub kind: &'static str,
    pub label: String,
    pub scope: &'static str,
    pub path: String,
    pub exists: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptFile {
    pub path: String,
    pub name: String,
    pub description: String,
    pub source_label: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpInputs {
    pub ids: Vec<String>,
    pub values: serde_json::Map<String, serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpInput {
    pub name: String,
    #[serde(default)]
    pub previous_name: Option<String>,
    /// 写入哪个来源，默认 multara。
    #[serde(default)]
    pub owner: Option<String>,
    pub transport: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub env_file: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub env_text: String,
    #[serde(default)]
    pub headers_text: String,
    #[serde(default)]
    pub needs_auth: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpForm {
    pub name: String,
    pub owner: String,
    pub transport: String,
    pub command: String,
    pub cwd: String,
    pub env_file: String,
    pub args: Vec<String>,
    pub url: String,
    pub env_text: String,
    pub headers_text: String,
    pub needs_auth: bool,
    pub enabled: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardStats {
    pub running: u32,
    pub queued: u32,
    pub completed_today: u32,
    pub failed_today: u32,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Automation {
    pub id: String,
    pub name: String,
    pub project_id: Option<String>,
    pub target_type: String,
    pub target_id: String,
    pub prompt: String,
    pub trigger_type: String,
    pub interval_minutes: Option<u32>,
    pub event_name: Option<String>,
    pub enabled: bool,
    pub last_run_at: Option<String>,
    pub created_at: String,
    pub trigger_id: Option<String>,
    pub cron_expression: Option<String>,
    pub timezone: Option<String>,
    pub next_run_at: Option<String>,
    pub webhook_token: Option<String>,
    pub last_run_status: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationInput {
    pub id: Option<String>,
    pub name: String,
    pub project_id: Option<String>,
    pub target_type: String,
    pub target_id: String,
    pub prompt: String,
    pub trigger_type: String,
    pub interval_minutes: Option<u32>,
    pub event_name: Option<String>,
    pub enabled: bool,
    pub cron_expression: Option<String>,
    pub timezone: Option<String>,
    pub signing_secret: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationRun {
    pub id: String,
    pub automation_id: String,
    pub source: String,
    pub status: String,
    pub planned_at: Option<String>,
    pub triggered_at: String,
    pub completed_at: Option<String>,
    pub failure_reason: Option<String>,
    pub reason_code: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebhookInput {
    pub token: String,
    pub event: Option<String>,
    pub dedupe_key: String,
    pub payload: String,
    pub signature: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebhookDelivery {
    pub id: String,
    pub dedupe_key: String,
    pub event: Option<String>,
    pub status: String,
    pub automation_run_id: Option<String>,
    pub reason_code: Option<String>,
    pub created_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatisticGroup {
    /// 按智能体分组时是 agent_id（前端据此取头像）；按运行时分组时为空
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    pub total: u32,
    pub completed: u32,
    pub failed: u32,
    pub seconds: f64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub cost_usd: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageDay {
    pub date: String,
    pub total: u32,
    pub failed: u32,
    pub seconds: f64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub cost_usd: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunStatistics {
    pub total: u32,
    pub completed: u32,
    pub failed: u32,
    pub stopped: u32,
    pub success_rate: f64,
    pub average_duration_seconds: f64,
    pub total_seconds: f64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub cost_usd: f64,
    pub daily: Vec<UsageDay>,
    pub by_runtime: Vec<StatisticGroup>,
    pub by_agent: Vec<StatisticGroup>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentActivityRecord {
    pub id: String,
    pub status: String,
    pub assignment: String,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub session_id: String,
    pub session_title: String,
    pub task_id: Option<String>,
    pub project_id: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxItem {
    pub id: String,
    pub session_id: String,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    pub title: String,
    pub preview: String,
    pub status: String,
    pub read: bool,
    pub read_message_id: Option<String>,
    pub unread_count: i64,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxInput {
    pub session_id: String,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    pub title: String,
    pub preview: String,
    pub status: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapData {
    pub projects: Vec<Project>,
    pub workspaces: Vec<Workspace>,
    pub runtimes: Vec<Runtime>,
    pub agents: Vec<Agent>,
    pub squads: Vec<Squad>,
    pub sessions: Vec<ChatSession>,
    pub tasks: Vec<Task>,
    pub resources: Vec<GlobalResource>,
    pub stats: DashboardStats,
    pub automations: Vec<Automation>,
    pub inbox: Vec<InboxItem>,
    pub settings: std::collections::HashMap<String, String>,
    pub merge_sessions: Vec<crate::merge_flow::MergeSession>,
}
