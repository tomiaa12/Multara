//! Canvas（L2）的数据与规则：manifest 校验、目录白名单、实例增删改查。
//! CLI 直连 SQLite 和应用内控制通道（`ctl.rs`）走同一套函数，行为一致。

use std::{
    fs,
    path::{Path, PathBuf},
};

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

/// 每个会话最多同时存在的实例数
pub const MAX_INSTANCES_PER_SESSION: usize = 8;
/// action 等待页面结果的默认超时（秒）
pub const DEFAULT_ACTION_TIMEOUT_SECS: u64 = 15;
const MANIFEST: &str = "canvas.json";
const MAX_MANIFEST_BYTES: u64 = 256 * 1024;
const MAX_INPUT_BYTES: usize = 64 * 1024;
const MAX_STATUS_CHARS: usize = 200;

/// 预期内的错误：`code` 给智能体判断，`message` 给人看。
#[derive(Debug, Clone, PartialEq)]
pub struct CanvasError {
    pub code: &'static str,
    pub message: String,
}

impl CanvasError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// CLI 退出码：1 参数错误，2 运行环境问题，3 Canvas 错误（方案 §5.6）
    pub fn exit_code(&self) -> i32 {
        match self.code {
            "canvas.invalid_args" | "canvas.invalid_manifest" | "canvas.invalid_input"
            | "canvas.dir_not_allowed" | "canvas.unknown_action" => 1,
            "canvas.disabled" | "canvas.app_not_running" | "canvas.no_session" => 2,
            _ => 3,
        }
    }

    pub fn to_json(&self) -> Value {
        json!({ "ok": false, "code": self.code, "message": self.message })
    }
}

impl From<rusqlite::Error> for CanvasError {
    fn from(error: rusqlite::Error) -> Self {
        Self::new("canvas.internal", error.to_string())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    pub name: String,
    pub description: String,
    pub input_schema: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Manifest {
    pub id: String,
    pub title: String,
    pub description: String,
    pub entry: String,
    pub input_schema: Option<Value>,
    pub actions: Vec<Action>,
}

/// 是谁在操作：会话来自控制通道 token（或 CLI 的 MULTARA_SESSION_ID），不信任请求体。
#[derive(Debug, Clone, Default)]
pub struct Caller {
    pub session_id: String,
    pub run_id: Option<String>,
    pub agent_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct OpenRequest {
    /// 绝对路径（CLI 先把相对路径转成绝对路径再发）
    pub dir: Option<String>,
    /// 重开已注册的定义
    pub id: Option<String>,
    pub instance: Option<String>,
    pub title: Option<String>,
    pub input: Option<Value>,
}

pub fn valid_canvas_id(id: &str) -> bool {
    (3..=64).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

pub fn valid_instance_id(id: &str) -> bool {
    (1..=128).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

/// 会话级 Canvas 的根目录：`<app-data>/canvases/<session-id>/`（spawn_run 注入为 MULTARA_CANVAS_DIR）
pub fn session_root(app_data: &Path, session_id: &str) -> PathBuf {
    app_data.join("canvases").join(session_id)
}

/// 私有持久数据目录：`<app-data>/canvas-data/<canvas-id>/`（所有会话共用）
pub fn data_root(app_data: &Path, canvas_id: &str) -> PathBuf {
    app_data.join("canvas-data").join(canvas_id)
}

// ---------- manifest ----------

pub fn read_manifest(dir: &Path) -> Result<(Manifest, String, Vec<String>), CanvasError> {
    let path = dir.join(MANIFEST);
    let size = fs::metadata(&path)
        .map_err(|_| CanvasError::new("canvas.invalid_manifest", tr!("目录下没有 canvas.json")))?
        .len();
    if size > MAX_MANIFEST_BYTES {
        return Err(CanvasError::new("canvas.invalid_manifest", tr!("canvas.json 太大")));
    }
    let raw = fs::read_to_string(&path)
        .map_err(|error| CanvasError::new("canvas.invalid_manifest", error.to_string()))?;
    let (manifest, warnings) = parse_manifest(&raw)?;
    if crate::ui_host::resolve_path(dir, &format!("/{}", manifest.entry), "index.html").is_none() {
        return Err(CanvasError::new(
            "canvas.invalid_manifest",
            tr!("入口文件不存在：{entry}", entry = manifest.entry),
        ));
    }
    Ok((manifest, raw, warnings))
}

pub fn parse_manifest(raw: &str) -> Result<(Manifest, Vec<String>), CanvasError> {
    let invalid = |message: String| CanvasError::new("canvas.invalid_manifest", message);
    let value: Value = serde_json::from_str(raw).map_err(|error| invalid(error.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| invalid(tr!("canvas.json 必须是对象")))?;
    let text = |key: &str| object.get(key).and_then(Value::as_str).map(str::trim).unwrap_or("");
    let id = text("id").to_string();
    if !valid_canvas_id(&id) {
        return Err(invalid(tr!("id 只能是 3–64 位小写字母、数字和连字符")));
    }
    let title = text("title").to_string();
    if title.is_empty() {
        return Err(invalid(tr!("缺少 title")));
    }
    let entry = match text("entry") {
        "" => "index.html".to_string(),
        entry => entry.trim_start_matches('/').to_string(),
    };
    let mut warnings = Vec::new();
    let input_schema = object.get("inputSchema").filter(|value| !value.is_null()).cloned();
    if let Some(schema) = &input_schema {
        check_schema(schema, "inputSchema", &mut warnings).map_err(invalid)?;
    }
    let mut actions = Vec::new();
    for (index, item) in object
        .get("actions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .enumerate()
    {
        let name = item.get("name").and_then(Value::as_str).unwrap_or("").trim().to_string();
        if name.is_empty() || !valid_instance_id(&name) {
            return Err(invalid(tr!("第 {index} 个动作名无效", index = index + 1)));
        }
        if name.starts_with("canvas.") {
            return Err(invalid(tr!("动作名不能以 canvas. 开头：{name}", name = name)));
        }
        if actions.iter().any(|action: &Action| action.name == name) {
            return Err(invalid(tr!("动作名重复：{name}", name = name)));
        }
        let input_schema = item.get("inputSchema").filter(|value| !value.is_null()).cloned();
        if let Some(schema) = &input_schema {
            check_schema(schema, &format!("actions.{name}.inputSchema"), &mut warnings)
                .map_err(invalid)?;
        }
        actions.push(Action {
            name,
            description: item
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string(),
            input_schema,
        });
    }
    Ok((
        Manifest {
            id,
            title,
            description: text("description").to_string(),
            entry,
            input_schema,
            actions,
        },
        warnings,
    ))
}

const SCHEMA_KEYWORDS: &[&str] = &[
    "type", "properties", "required", "enum", "items", "minimum", "maximum", "description", "title",
    "default", "$schema",
];

/// 只支持 JSON Schema 子集；子集外的关键字忽略并记一条提示（方案 §5.2）。
fn check_schema(schema: &Value, at: &str, warnings: &mut Vec<String>) -> Result<(), String> {
    let Some(object) = schema.as_object() else {
        return Err(tr!("{at} 必须是对象", at = at));
    };
    for key in object.keys() {
        if !SCHEMA_KEYWORDS.contains(&key.as_str()) {
            warnings.push(tr!("{at} 里的 {key} 不支持，已忽略", at = at, key = key));
        }
    }
    if let Some(properties) = object.get("properties").and_then(Value::as_object) {
        for (name, child) in properties {
            check_schema(child, &format!("{at}.properties.{name}"), warnings)?;
        }
    }
    if let Some(items) = object.get("items") {
        check_schema(items, &format!("{at}.items"), warnings)?;
    }
    Ok(())
}

/// 按子集校验值：type / properties / required / enum / items / minimum / maximum
pub fn validate_value(schema: &Value, value: &Value, at: &str) -> Result<(), String> {
    let Some(schema) = schema.as_object() else {
        return Ok(());
    };
    if let Some(kind) = schema.get("type").and_then(Value::as_str) {
        let ok = match kind {
            "object" => value.is_object(),
            "array" => value.is_array(),
            "string" => value.is_string(),
            "number" => value.is_number(),
            "integer" => value.as_f64().is_some_and(|number| number.fract() == 0.0),
            "boolean" => value.is_boolean(),
            "null" => value.is_null(),
            _ => true,
        };
        if !ok {
            return Err(tr!("{at} 应为 {kind}", at = at, kind = kind));
        }
    }
    if let Some(options) = schema.get("enum").and_then(Value::as_array) {
        if !options.contains(value) {
            return Err(tr!("{at} 不在允许的取值里", at = at));
        }
    }
    if let Some(number) = value.as_f64() {
        if schema.get("minimum").and_then(Value::as_f64).is_some_and(|min| number < min) {
            return Err(tr!("{at} 小于最小值", at = at));
        }
        if schema.get("maximum").and_then(Value::as_f64).is_some_and(|max| number > max) {
            return Err(tr!("{at} 大于最大值", at = at));
        }
    }
    if let Some(object) = value.as_object() {
        for name in schema
            .get("required")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if !object.contains_key(name) {
                return Err(tr!("{at} 缺少 {name}", at = at, name = name));
            }
        }
        if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
            for (name, child) in properties {
                if let Some(item) = object.get(name) {
                    validate_value(child, item, &format!("{at}.{name}"))?;
                }
            }
        }
    }
    if let (Some(items), Some(array)) = (schema.get("items"), value.as_array()) {
        for (index, item) in array.iter().enumerate() {
            validate_value(items, item, &format!("{at}[{index}]"))?;
        }
    }
    Ok(())
}

/// 打开时的输入：声明了对象 schema 就必须传对象（`{}` 可以，`null` 不行）
pub fn check_open_input(manifest: &Manifest, input: &Value) -> Result<(), CanvasError> {
    if input.to_string().len() > MAX_INPUT_BYTES {
        return Err(CanvasError::new("canvas.invalid_input", tr!("输入太大")));
    }
    match &manifest.input_schema {
        None => Ok(()),
        Some(schema) => validate_value(schema, input, "input")
            .map_err(|message| CanvasError::new("canvas.invalid_input", message)),
    }
}

/// action 的名字和输入按 manifest 校验
pub fn check_action(manifest: &Manifest, name: &str, input: &Value) -> Result<(), CanvasError> {
    let action = manifest
        .actions
        .iter()
        .find(|action| action.name == name)
        .ok_or_else(|| {
            CanvasError::new("canvas.unknown_action", tr!("没有这个动作：{name}", name = name))
        })?;
    if input.to_string().len() > MAX_INPUT_BYTES {
        return Err(CanvasError::new("canvas.invalid_input", tr!("输入太大")));
    }
    match &action.input_schema {
        None if input.is_null() || input.as_object().is_some_and(|object| object.is_empty()) => Ok(()),
        None => Err(CanvasError::new(
            "canvas.invalid_input",
            tr!("动作 {name} 不接受输入", name = name),
        )),
        Some(schema) => validate_value(schema, input, "input")
            .map_err(|message| CanvasError::new("canvas.invalid_input", message)),
    }
}

// ---------- 目录白名单 ----------

/// 智能体能托管的目录：会话 Canvas 根，或会话关联需求的工作区（方案 R5）。
pub fn allowed_roots(db: &Connection, app_data: &Path, session_id: &str) -> Vec<PathBuf> {
    let mut roots = vec![session_root(app_data, session_id)];
    if let Ok(mut statement) = db.prepare(
        "SELECT w.work_path FROM workspaces w
         JOIN session_projects sp ON sp.project_id = w.project_id
         WHERE sp.session_id = ?1",
    ) {
        if let Ok(rows) = statement.query_map([session_id], |row| row.get::<_, String>(0)) {
            roots.extend(rows.flatten().map(PathBuf::from));
        }
    }
    roots
}

/// `canonicalize` 之后必须落在某个允许的根之下（不能就是根本身以外的祖先目录）。
pub fn resolve_dir(requested: &Path, roots: &[PathBuf]) -> Result<PathBuf, CanvasError> {
    let denied = || {
        CanvasError::new(
            "canvas.dir_not_allowed",
            tr!("只能托管 $MULTARA_CANVAS_DIR 或当前工作区里的目录"),
        )
    };
    if !requested.is_absolute() {
        return Err(denied());
    }
    let target = requested.canonicalize().map_err(|_| {
        CanvasError::new("canvas.invalid_args", tr!("目录不存在：{dir}", dir = requested.display()))
    })?;
    if !target.is_dir() {
        return Err(CanvasError::new("canvas.invalid_args", tr!("不是目录：{dir}", dir = requested.display())));
    }
    let allowed = roots
        .iter()
        .filter_map(|root| root.canonicalize().ok())
        .any(|root| target.starts_with(&root));
    if allowed {
        Ok(target)
    } else {
        Err(denied())
    }
}

// ---------- 实例 ----------

fn now() -> String {
    Utc::now().to_rfc3339()
}

pub fn instance_json(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let input: String = row.get("input")?;
    Ok(json!({
        "instanceId": row.get::<_, String>("instance_id")?,
        "sessionId": row.get::<_, String>("session_id")?,
        "canvasId": row.get::<_, String>("canvas_id")?,
        "title": row.get::<_, String>("title")?,
        "status": row.get::<_, String>("status")?,
        "input": serde_json::from_str::<Value>(&input).unwrap_or(Value::Null),
        "hasContext": row.get::<_, Option<String>>("context")?.is_some(),
        "createdByAgent": row.get::<_, Option<String>>("created_by_agent")?,
        "visible": row.get::<_, i64>("visible")? != 0,
        "pinned": row.get::<_, i64>("pinned")? != 0,
        "scope": row.get::<_, String>("scope")?,
        "createdAt": row.get::<_, String>("created_at")?,
        "updatedAt": row.get::<_, String>("updated_at")?,
    }))
}

/// 会话里的实例。`include_pinned` 时再加上其它会话固定（pin）的实例——
/// 只给界面列表用；智能体（控制通道）只看得到自己会话的。
pub fn list(db: &Connection, session_id: &str, include_pinned: bool) -> Result<Value, CanvasError> {
    let sql = if include_pinned {
        "SELECT * FROM canvas_instances WHERE session_id=?1 OR pinned=1
         ORDER BY session_id<>?1, created_at, instance_id"
    } else {
        "SELECT * FROM canvas_instances WHERE session_id=?1 ORDER BY created_at, instance_id"
    };
    let mut statement = db.prepare(sql)?;
    let rows = statement
        .query_map([session_id], instance_json)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Value::Array(rows))
}

pub fn get(db: &Connection, session_id: &str, instance_id: &str) -> Result<Value, CanvasError> {
    db.query_row(
        "SELECT * FROM canvas_instances WHERE session_id=?1 AND instance_id=?2",
        params![session_id, instance_id],
        instance_json,
    )
    .optional()?
    .ok_or_else(|| not_found(instance_id))
}

fn not_found(instance_id: &str) -> CanvasError {
    CanvasError::new("canvas.not_found", tr!("没有这个 Canvas：{id}", id = instance_id))
}

/// 托管一个实例需要的信息（每次请求现读，智能体改了定义立即生效）
#[derive(Debug, Clone)]
pub struct HostingView {
    pub dir: PathBuf,
    pub manifest: Manifest,
    pub canvas_id: String,
    pub title: String,
    pub input: Value,
}

pub fn hosting_view(db: &Connection, session_id: &str, instance_id: &str) -> Result<HostingView, CanvasError> {
    let (canvas_id, scope, owner_id, title, input): (String, String, String, String, String) = db
        .query_row(
            "SELECT canvas_id, scope, owner_id, title, input FROM canvas_instances
             WHERE session_id=?1 AND instance_id=?2",
            params![session_id, instance_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .optional()?
        .ok_or_else(|| not_found(instance_id))?;
    let (dir, manifest_raw): (String, String) = db
        .query_row(
            "SELECT dir, manifest FROM canvases WHERE scope=?1 AND owner_id=?2 AND id=?3",
            params![scope, owner_id, canvas_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| not_found(instance_id))?;
    let (manifest, _) = parse_manifest(&manifest_raw)?;
    Ok(HostingView {
        dir: PathBuf::from(dir),
        manifest,
        canvas_id,
        title,
        input: serde_json::from_str(&input).unwrap_or(Value::Null),
    })
}

pub struct Opened {
    pub instance: Value,
    pub warnings: Vec<String>,
    /// 同一实例重开（不是新建）
    pub reopened: bool,
}

/// 打开或重开（幂等）：同一 instanceId 刷新定义和输入，不新建。
pub fn open(
    db: &Connection,
    app_data: &Path,
    caller: &Caller,
    request: &OpenRequest,
) -> Result<Opened, CanvasError> {
    if caller.session_id.is_empty() {
        return Err(CanvasError::new("canvas.no_session", tr!("不知道当前是哪个会话")));
    }
    let session_exists: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM chat_sessions WHERE id=?1)",
        [&caller.session_id],
        |row| row.get(0),
    )?;
    if !session_exists {
        return Err(CanvasError::new("canvas.no_session", tr!("聊天不存在")));
    }
    let mut scope = "session".to_string();
    let mut owner = caller.session_id.clone();
    let (manifest, manifest_raw, dir, warnings) = match (&request.dir, &request.id) {
        (Some(dir), _) => {
            let roots = allowed_roots(db, app_data, &caller.session_id);
            let dir = resolve_dir(Path::new(dir), &roots)?;
            let (manifest, raw, warnings) = read_manifest(&dir)?;
            (manifest, raw, dir, warnings)
        }
        (None, Some(id)) => {
            // 先找本会话注册过的，再找会话关联工作区里的 .multara/canvases/<id>/（五期）
            let session_dir: Option<String> = db
                .query_row(
                    "SELECT dir FROM canvases WHERE scope='session' AND owner_id=?1 AND id=?2",
                    params![caller.session_id, id],
                    |row| row.get(0),
                )
                .optional()?;
            let dir = match session_dir {
                Some(dir) => PathBuf::from(dir),
                None => {
                    let found = scan_workspace_canvases(db, &session_workspaces(db, &caller.session_id))
                        .into_iter()
                        .find(|definition| &definition.manifest.id == id)
                        .ok_or_else(|| {
                            CanvasError::new("canvas.not_found", tr!("没有注册过这个 Canvas：{id}", id = id))
                        })?;
                    scope = "workspace".to_string();
                    owner = found.workspace_id;
                    found.dir
                }
            };
            let (manifest, raw, warnings) = read_manifest(&dir)?;
            (manifest, raw, dir, warnings)
        }
        (None, None) => {
            return Err(CanvasError::new("canvas.invalid_args", tr!("需要 --dir 或 --id")));
        }
    };
    let scope = scope.as_str();
    let owner = owner.as_str();
    let instance_id = request.instance.clone().unwrap_or_else(|| manifest.id.clone());
    if !valid_instance_id(&instance_id) {
        return Err(CanvasError::new("canvas.invalid_args", tr!("实例 id 无效")));
    }
    let input = request.input.clone().unwrap_or_else(|| {
        if manifest.input_schema.is_some() { json!({}) } else { Value::Null }
    });
    check_open_input(&manifest, &input)?;
    let now = now();
    db.execute(
        "INSERT INTO canvases (id, scope, owner_id, dir, title, description, manifest, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(scope, owner_id, id) DO UPDATE SET
           dir=excluded.dir, title=excluded.title, description=excluded.description,
           manifest=excluded.manifest, updated_at=excluded.updated_at",
        params![
            manifest.id,
            scope,
            owner,
            dir.to_string_lossy(),
            manifest.title,
            manifest.description,
            manifest_raw,
            now
        ],
    )?;
    let existing: Option<String> = db
        .query_row(
            "SELECT canvas_id FROM canvas_instances WHERE session_id=?1 AND instance_id=?2",
            params![caller.session_id, instance_id],
            |row| row.get(0),
        )
        .optional()?;
    let title = request
        .title
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .unwrap_or(&manifest.title)
        .to_string();
    let reopened = existing.is_some();
    if reopened {
        db.execute(
            "UPDATE canvas_instances SET canvas_id=?3, scope=?4, owner_id=?5, title=?6, input=?7,
               visible=1, updated_at=?8
             WHERE session_id=?1 AND instance_id=?2",
            params![
                caller.session_id,
                instance_id,
                manifest.id,
                scope,
                owner,
                title,
                input.to_string(),
                now
            ],
        )?;
    } else {
        let count: i64 = db.query_row(
            "SELECT COUNT(*) FROM canvas_instances WHERE session_id=?1",
            [&caller.session_id],
            |row| row.get(0),
        )?;
        if count as usize >= MAX_INSTANCES_PER_SESSION {
            return Err(CanvasError::new(
                "canvas.limit",
                tr!("一个会话最多开 {max} 个 Canvas，先关掉不用的", max = MAX_INSTANCES_PER_SESSION),
            ));
        }
        db.execute(
            "INSERT INTO canvas_instances
               (instance_id, session_id, canvas_id, scope, owner_id, title, input,
                created_by_run, created_by_agent, visible, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1, ?10, ?10)",
            params![
                instance_id,
                caller.session_id,
                manifest.id,
                scope,
                owner,
                title,
                input.to_string(),
                caller.run_id,
                caller.agent_id,
                now
            ],
        )?;
    }
    Ok(Opened {
        instance: get(db, &caller.session_id, &instance_id)?,
        warnings,
        reopened,
    })
}

pub fn close(db: &Connection, session_id: &str, instance_id: &str) -> Result<(), CanvasError> {
    let removed = db.execute(
        "DELETE FROM canvas_instances WHERE session_id=?1 AND instance_id=?2",
        params![session_id, instance_id],
    )?;
    if removed == 0 {
        return Err(not_found(instance_id));
    }
    Ok(())
}

pub fn set_status(db: &Connection, session_id: &str, instance_id: &str, text: &str) -> Result<Value, CanvasError> {
    let text: String = text.trim().chars().take(MAX_STATUS_CHARS).collect();
    let changed = db.execute(
        "UPDATE canvas_instances SET status=?3, updated_at=?4 WHERE session_id=?1 AND instance_id=?2",
        params![session_id, instance_id, text, now()],
    )?;
    if changed == 0 {
        return Err(not_found(instance_id));
    }
    get(db, session_id, instance_id)
}

/// 关标签只改可见性，实例和服务都保留
pub fn set_visible(db: &Connection, session_id: &str, instance_id: &str, visible: bool) -> Result<(), CanvasError> {
    let changed = db.execute(
        "UPDATE canvas_instances SET visible=?3, updated_at=?4 WHERE session_id=?1 AND instance_id=?2",
        params![session_id, instance_id, visible as i64, now()],
    )?;
    if changed == 0 {
        return Err(not_found(instance_id));
    }
    Ok(())
}

/// 页面最新一次 `__multara/context`（入库，重启后智能体仍能读到）
pub fn set_context(db: &Connection, session_id: &str, instance_id: &str, context: &Value) -> Result<(), CanvasError> {
    if context.to_string().len() > MAX_INPUT_BYTES {
        return Err(CanvasError::new("canvas.invalid_input", tr!("上下文太大")));
    }
    let at = now();
    let changed = db.execute(
        "UPDATE canvas_instances SET context=?3, context_at=?4, updated_at=?4
         WHERE session_id=?1 AND instance_id=?2",
        params![session_id, instance_id, context.to_string(), at],
    )?;
    if changed == 0 {
        return Err(not_found(instance_id));
    }
    Ok(())
}

pub fn get_context(db: &Connection, session_id: &str, instance_id: &str) -> Result<Value, CanvasError> {
    let (context, at): (Option<String>, Option<String>) = db
        .query_row(
            "SELECT context, context_at FROM canvas_instances WHERE session_id=?1 AND instance_id=?2",
            params![session_id, instance_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| not_found(instance_id))?;
    Ok(json!({
        "context": context.and_then(|raw| serde_json::from_str::<Value>(&raw).ok()),
        "updatedAt": at,
    }))
}

/// 会话删除：删实例和会话级定义，删 `<app-data>/canvases/<session-id>/`；canvas-data 按 canvas id 共享，不删
pub fn delete_session(db: &Connection, app_data: Option<&Path>, session_id: &str) -> Result<(), String> {
    db.execute("DELETE FROM canvas_instances WHERE session_id=?1", [session_id])
        .map_err(|error| error.to_string())?;
    db.execute(
        "DELETE FROM canvases WHERE scope='session' AND owner_id=?1",
        [session_id],
    )
    .map_err(|error| error.to_string())?;
    if let Some(app_data) = app_data {
        let root = session_root(app_data, session_id);
        if root.exists() {
            let _ = fs::remove_dir_all(root);
        }
    }
    Ok(())
}

/// 已注册、无必填输入、本会话还没开的定义：「添加上下文 → Canvas...」的「可打开」组。
/// 包括本会话注册过的，以及会话关联工作区里 `.multara/canvases/` 下的（五期）。
pub fn available(db: &Connection, session_id: &str) -> Result<Value, CanvasError> {
    let mut statement = db.prepare(
        "SELECT c.id, c.title, c.description, c.manifest FROM canvases c
         WHERE c.scope='session' AND c.owner_id=?1
           AND NOT EXISTS (SELECT 1 FROM canvas_instances i
                           WHERE i.session_id=?1 AND i.canvas_id=c.id)
         ORDER BY c.title",
    )?;
    let rows = statement
        .query_map([session_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut seen = std::collections::HashSet::new();
    let mut items: Vec<Value> = rows
        .into_iter()
        .filter(|(_, _, _, raw)| {
            parse_manifest(raw).is_ok_and(|(manifest, _)| !requires_input(&manifest))
        })
        .filter(|(id, ..)| seen.insert(id.clone()))
        .map(|(id, title, description, _)| {
            json!({ "canvasId": id, "title": title, "description": description, "scope": "session" })
        })
        .collect();
    let opened: std::collections::HashSet<String> = list(db, session_id, false)?
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item["canvasId"].as_str().map(str::to_string))
        .collect();
    for definition in scan_workspace_canvases(db, &session_workspaces(db, session_id)) {
        if requires_input(&definition.manifest)
            || opened.contains(&definition.manifest.id)
            || !seen.insert(definition.manifest.id.clone())
        {
            continue;
        }
        items.push(definition_json(&definition));
    }
    Ok(Value::Array(items))
}

// ---------- 工作区级 Canvas（五期） ----------

/// 工作区里的 Canvas：`<workspace>/.multara/canvases/<canvas-id>/canvas.json`，可随仓库提交，Multara 不清理
#[derive(Debug, Clone)]
pub struct WorkspaceCanvas {
    pub workspace_id: String,
    pub dir: PathBuf,
    pub manifest: Manifest,
}

fn definition_json(definition: &WorkspaceCanvas) -> Value {
    json!({
        "canvasId": definition.manifest.id,
        "title": definition.manifest.title,
        "description": definition.manifest.description,
        "scope": "workspace",
        "workspaceId": definition.workspace_id,
        "requiresInput": requires_input(&definition.manifest),
    })
}

pub fn session_workspaces(db: &Connection, session_id: &str) -> Vec<(String, PathBuf)> {
    workspace_rows(
        db,
        "SELECT w.id, w.work_path FROM workspaces w
         JOIN session_projects sp ON sp.project_id = w.project_id
         WHERE sp.session_id = ?1",
        session_id,
    )
}

pub fn project_workspaces(db: &Connection, project_id: &str) -> Vec<(String, PathBuf)> {
    workspace_rows(
        db,
        "SELECT id, work_path FROM workspaces WHERE project_id = ?1",
        project_id,
    )
}

fn workspace_rows(db: &Connection, sql: &str, key: &str) -> Vec<(String, PathBuf)> {
    let Ok(mut statement) = db.prepare(sql) else {
        return Vec::new();
    };
    statement
        .query_map([key], |row| Ok((row.get::<_, String>(0)?, PathBuf::from(row.get::<_, String>(1)?))))
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
}

/// 扫描工作区里的定义并登记进 canvases 表（scope=workspace，owner=workspace id）。
/// 坏掉的定义跳过；登记失败不影响列出。
pub fn scan_workspace_canvases(db: &Connection, workspaces: &[(String, PathBuf)]) -> Vec<WorkspaceCanvas> {
    let mut found = Vec::new();
    for (workspace_id, root) in workspaces {
        let base = root.join(".multara").join("canvases");
        let Ok(entries) = fs::read_dir(&base) else {
            continue;
        };
        let Ok(base) = base.canonicalize() else {
            continue;
        };
        let mut dirs: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).filter(|path| path.is_dir()).collect();
        dirs.sort();
        for dir in dirs {
            // 符号链接指到工作区外的不要
            let Ok(dir) = dir.canonicalize() else { continue };
            if !dir.starts_with(&base) {
                continue;
            }
            let Ok((manifest, raw, _)) = read_manifest(&dir) else {
                continue;
            };
            let _ = db.execute(
                "INSERT INTO canvases (id, scope, owner_id, dir, title, description, manifest, updated_at)
                 VALUES (?1, 'workspace', ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(scope, owner_id, id) DO UPDATE SET
                   dir=excluded.dir, title=excluded.title, description=excluded.description,
                   manifest=excluded.manifest, updated_at=excluded.updated_at",
                params![
                    manifest.id,
                    workspace_id,
                    dir.to_string_lossy(),
                    manifest.title,
                    manifest.description,
                    raw,
                    now()
                ],
            );
            found.push(WorkspaceCanvas {
                workspace_id: workspace_id.clone(),
                dir,
                manifest,
            });
        }
    }
    found
}

/// 新建聊天菜单的 Canvas 子菜单：某个需求工作区里可直接打开的定义
pub fn project_definitions(db: &Connection, project_id: &str) -> Value {
    let mut seen = std::collections::HashSet::new();
    Value::Array(
        scan_workspace_canvases(db, &project_workspaces(db, project_id))
            .iter()
            .filter(|definition| !requires_input(&definition.manifest))
            .filter(|definition| seen.insert(definition.manifest.id.clone()))
            .map(definition_json)
            .collect(),
    )
}

/// 固定（pin）：固定的实例在所有会话的 Canvas 列表里都出现，操作仍然归属原会话
pub fn set_pinned(db: &Connection, session_id: &str, instance_id: &str, pinned: bool) -> Result<(), CanvasError> {
    let changed = db.execute(
        "UPDATE canvas_instances SET pinned=?3, updated_at=?4 WHERE session_id=?1 AND instance_id=?2",
        params![session_id, instance_id, pinned as i64, now()],
    )?;
    if changed == 0 {
        return Err(not_found(instance_id));
    }
    Ok(())
}

fn requires_input(manifest: &Manifest) -> bool {
    manifest.input_schema.as_ref().is_some_and(|schema| {
        schema
            .get("required")
            .and_then(Value::as_array)
            .is_some_and(|required| !required.is_empty())
    })
}

pub fn canvases_enabled(db: &Connection) -> bool {
    crate::commands::genui_enabled(db)
        && crate::commands::setting_value(db, "canvasesEnabled", "false") == "true"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        crate::database::migrate(&db).unwrap();
        db.execute_batch(
            "INSERT INTO chat_sessions (id, title, target_type, target_id, created_at, updated_at) VALUES
               ('s1', 'A', 'agent', 'miku', 'now', 'now'),
               ('s2', 'B', 'agent', 'miku', 'now', 'now');",
        )
        .unwrap();
        db
    }

    fn write_canvas(root: &Path, id: &str, extra: &str) -> PathBuf {
        let dir = root.join(id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("index.html"), "<p>ok</p>").unwrap();
        fs::write(
            dir.join("canvas.json"),
            format!(r#"{{"id":"{id}","title":"Board {id}"{extra}}}"#),
        )
        .unwrap();
        dir
    }

    fn caller(session: &str) -> Caller {
        Caller {
            session_id: session.into(),
            run_id: Some("r1".into()),
            agent_id: Some("miku".into()),
        }
    }

    #[test]
    fn workspace_canvases_are_discovered_and_opened_by_id() {
        let app = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        let db = db();
        db.execute_batch(&format!(
            "INSERT INTO projects (id, name, created_at, opened_at) VALUES ('p1', '需求', 'now', 'now');
             INSERT INTO workspaces (id, project_id, name, kind, source_path, work_path, isolation_mode, created_at)
               VALUES ('w1', 'p1', 'repo', 'folder', '{root}', '{root}', 'direct', 'now');
             INSERT INTO session_projects (session_id, project_id, sort_order) VALUES ('s1', 'p1', 0);",
            root = repo.path().display()
        ))
        .unwrap();
        let base = repo.path().join(".multara").join("canvases");
        write_canvas(&base, "release-board", "");
        write_canvas(&base, "needs-input", r#","inputSchema":{"type":"object","required":["v"]}"#);
        fs::create_dir_all(base.join("broken")).unwrap();
        fs::write(base.join("broken").join("canvas.json"), "{oops").unwrap();

        // 需求下可直接打开的：坏的和要必填输入的都不列
        let defs = project_definitions(&db, "p1");
        let ids: Vec<&str> = defs.as_array().unwrap().iter().map(|item| item["canvasId"].as_str().unwrap()).collect();
        assert_eq!(ids, vec!["release-board"]);
        assert_eq!(defs[0]["scope"], "workspace");

        let available_ids = |db: &Connection| {
            available(db, "s1")
                .unwrap()
                .as_array()
                .unwrap()
                .iter()
                .map(|item| item["canvasId"].as_str().unwrap().to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(available_ids(&db), vec!["release-board"]);
        let opened = open(&db, app.path(), &caller("s1"), &OpenRequest {
            id: Some("release-board".into()),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(opened.instance["scope"], "workspace");
        // 开过的不再出现在「可打开」里
        assert!(available_ids(&db).is_empty());
        // 会话删除不动工作区定义（两份合法定义都登记过，坏的那份没有）
        delete_session(&db, Some(app.path()), "s1").unwrap();
        let left: i64 = db
            .query_row("SELECT COUNT(*) FROM canvases WHERE scope='workspace'", [], |row| row.get(0))
            .unwrap();
        assert_eq!(left, 2);
        assert!(base.join("release-board").join("canvas.json").exists());
    }

    #[test]
    fn pinned_instances_show_up_in_other_sessions_for_the_ui_only() {
        let app = tempfile::tempdir().unwrap();
        let db = db();
        let dir = write_canvas(&session_root(app.path(), "s1"), "board", "");
        open(&db, app.path(), &caller("s1"), &OpenRequest {
            dir: Some(dir.to_string_lossy().into()),
            ..Default::default()
        })
        .unwrap();
        assert!(list(&db, "s2", true).unwrap().as_array().unwrap().is_empty());
        set_pinned(&db, "s1", "board", true).unwrap();
        let shown = list(&db, "s2", true).unwrap();
        assert_eq!(shown[0]["sessionId"], "s1");
        assert_eq!(shown[0]["pinned"], true);
        // 智能体（控制通道）只看自己会话的
        assert!(list(&db, "s2", false).unwrap().as_array().unwrap().is_empty());
        assert_eq!(set_pinned(&db, "s2", "board", false).unwrap_err().code, "canvas.not_found");
    }

    #[test]
    fn manifest_rules() {
        assert!(parse_manifest(r#"{"id":"ab","title":"x"}"#).is_err());
        assert!(parse_manifest(r#"{"id":"Board","title":"x"}"#).is_err());
        assert!(parse_manifest(r#"{"id":"board","title":""}"#).is_err());
        let err = parse_manifest(r#"{"id":"board","title":"x","actions":[{"name":"canvas.go"}]}"#).unwrap_err();
        assert_eq!(err.code, "canvas.invalid_manifest");
        assert!(parse_manifest(r#"{"id":"board","title":"x","actions":[{"name":"a"},{"name":"a"}]}"#).is_err());
        let (manifest, warnings) = parse_manifest(
            r#"{"id":"board","title":"x","inputSchema":{"type":"object","pattern":"x"},"actions":[{"name":"set","inputSchema":{"type":"object"}}]}"#,
        )
        .unwrap();
        assert_eq!(manifest.entry, "index.html");
        assert_eq!(manifest.actions[0].name, "set");
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn schema_subset_validation() {
        let schema = json!({
            "type": "object",
            "required": ["item", "ok"],
            "properties": {
                "item": { "type": "string", "enum": ["db", "cache"] },
                "ok": { "type": "boolean" },
                "n": { "type": "integer", "minimum": 1, "maximum": 3 },
                "tags": { "type": "array", "items": { "type": "string" } }
            }
        });
        assert!(validate_value(&schema, &json!({"item":"db","ok":true}), "input").is_ok());
        assert!(validate_value(&schema, &json!({"item":"db"}), "input").is_err());
        assert!(validate_value(&schema, &json!({"item":"x","ok":true}), "input").is_err());
        assert!(validate_value(&schema, &json!({"item":"db","ok":true,"n":5}), "input").is_err());
        assert!(validate_value(&schema, &json!({"item":"db","ok":true,"n":1.5}), "input").is_err());
        assert!(validate_value(&schema, &json!({"item":"db","ok":true,"tags":[1]}), "input").is_err());
        assert!(validate_value(&schema, &Value::Null, "input").is_err());
    }

    #[test]
    fn dir_whitelist() {
        let app = tempfile::tempdir().unwrap();
        let db = db();
        let roots = allowed_roots(&db, app.path(), "s1");
        let inside = write_canvas(&session_root(app.path(), "s1"), "board", "");
        assert!(resolve_dir(&inside, &roots).is_ok());
        let outside = write_canvas(app.path(), "elsewhere", "");
        assert_eq!(resolve_dir(&outside, &roots).unwrap_err().code, "canvas.dir_not_allowed");
        assert_eq!(resolve_dir(Path::new("relative"), &roots).unwrap_err().code, "canvas.dir_not_allowed");
        let home = std::env::var("HOME").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("/"));
        assert_eq!(resolve_dir(&home, &roots).unwrap_err().code, "canvas.dir_not_allowed");
        // 另一个会话的根不在白名单
        let other = write_canvas(&session_root(app.path(), "s2"), "board", "");
        assert_eq!(resolve_dir(&other, &roots).unwrap_err().code, "canvas.dir_not_allowed");
        #[cfg(unix)]
        {
            let link = session_root(app.path(), "s1").join("link");
            std::os::unix::fs::symlink(&outside, &link).unwrap();
            assert_eq!(resolve_dir(&link, &roots).unwrap_err().code, "canvas.dir_not_allowed");
        }
    }

    #[test]
    fn open_is_idempotent_and_limited() {
        let app = tempfile::tempdir().unwrap();
        let db = db();
        let dir = write_canvas(&session_root(app.path(), "s1"), "board", "");
        let request = OpenRequest {
            dir: Some(dir.to_string_lossy().into()),
            ..Default::default()
        };
        let first = open(&db, app.path(), &caller("s1"), &request).unwrap();
        assert!(!first.reopened);
        assert_eq!(first.instance["instanceId"], "board");
        assert_eq!(first.instance["createdByAgent"], "miku");
        set_visible(&db, "s1", "board", false).unwrap();
        let again = open(&db, app.path(), &caller("s1"), &OpenRequest {
            input: Some(json!({ "v": 1 })),
            ..request.clone()
        })
        .unwrap();
        assert!(again.reopened);
        assert_eq!(again.instance["visible"], true);
        assert_eq!(again.instance["input"], json!({ "v": 1 }));
        assert_eq!(list(&db, "s1", false).unwrap().as_array().unwrap().len(), 1);
        // 重开已注册的
        let by_id = open(&db, app.path(), &caller("s1"), &OpenRequest {
            id: Some("board".into()),
            instance: Some("second".into()),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(by_id.instance["canvasId"], "board");
        for index in 2..MAX_INSTANCES_PER_SESSION {
            open(&db, app.path(), &caller("s1"), &OpenRequest {
                instance: Some(format!("i{index}")),
                ..request.clone()
            })
            .unwrap();
        }
        let over = open(&db, app.path(), &caller("s1"), &OpenRequest {
            instance: Some("one-too-many".into()),
            ..request.clone()
        });
        assert_eq!(over.err().unwrap().code, "canvas.limit");
    }

    #[test]
    fn input_schema_is_enforced_on_open() {
        let app = tempfile::tempdir().unwrap();
        let db = db();
        let dir = write_canvas(
            &session_root(app.path(), "s1"),
            "board",
            r#","inputSchema":{"type":"object","required":["v"]}"#,
        );
        let request = OpenRequest {
            dir: Some(dir.to_string_lossy().into()),
            ..Default::default()
        };
        assert_eq!(open(&db, app.path(), &caller("s1"), &request).err().unwrap().code, "canvas.invalid_input");
        assert!(open(&db, app.path(), &caller("s1"), &OpenRequest {
            input: Some(json!({ "v": "1" })),
            ..request
        })
        .is_ok());
    }

    #[test]
    fn sessions_cannot_touch_each_other() {
        let app = tempfile::tempdir().unwrap();
        let db = db();
        let dir = write_canvas(&session_root(app.path(), "s1"), "board", "");
        open(&db, app.path(), &caller("s1"), &OpenRequest {
            dir: Some(dir.to_string_lossy().into()),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(close(&db, "s2", "board").unwrap_err().code, "canvas.not_found");
        assert_eq!(set_status(&db, "s2", "board", "x").unwrap_err().code, "canvas.not_found");
        assert!(list(&db, "s2", false).unwrap().as_array().unwrap().is_empty());
        assert_eq!(set_status(&db, "s1", "board", "跑到第 2 步").unwrap()["status"], "跑到第 2 步");
        set_context(&db, "s1", "board", &json!({ "text": "hi" })).unwrap();
        assert_eq!(get_context(&db, "s1", "board").unwrap()["context"]["text"], "hi");
    }

    #[test]
    fn deleting_a_session_cleans_up() {
        let app = tempfile::tempdir().unwrap();
        let db = db();
        let root = session_root(app.path(), "s1");
        let dir = write_canvas(&root, "board", "");
        open(&db, app.path(), &caller("s1"), &OpenRequest {
            dir: Some(dir.to_string_lossy().into()),
            ..Default::default()
        })
        .unwrap();
        fs::create_dir_all(data_root(app.path(), "board")).unwrap();
        delete_session(&db, Some(app.path()), "s1").unwrap();
        assert!(list(&db, "s1", false).unwrap().as_array().unwrap().is_empty());
        let left: i64 = db.query_row("SELECT COUNT(*) FROM canvases", [], |row| row.get(0)).unwrap();
        assert_eq!(left, 0);
        assert!(!root.exists());
        assert!(data_root(app.path(), "board").exists());
    }

    #[test]
    fn actions_are_checked_against_the_manifest() {
        let (manifest, _) = parse_manifest(
            r#"{"id":"board","title":"x","actions":[{"name":"set","inputSchema":{"type":"object","required":["ok"]}},{"name":"reset"}]}"#,
        )
        .unwrap();
        assert!(check_action(&manifest, "set", &json!({ "ok": true })).is_ok());
        assert_eq!(check_action(&manifest, "set", &json!({})).unwrap_err().code, "canvas.invalid_input");
        assert_eq!(check_action(&manifest, "nope", &json!({})).unwrap_err().code, "canvas.unknown_action");
        assert!(check_action(&manifest, "reset", &Value::Null).is_ok());
        assert_eq!(check_action(&manifest, "reset", &json!({ "x": 1 })).unwrap_err().code, "canvas.invalid_input");
    }

    #[test]
    fn exit_codes() {
        assert_eq!(CanvasError::new("canvas.invalid_args", "").exit_code(), 1);
        assert_eq!(CanvasError::new("canvas.app_not_running", "").exit_code(), 2);
        assert_eq!(CanvasError::new("canvas.timeout", "").exit_code(), 3);
    }
}
