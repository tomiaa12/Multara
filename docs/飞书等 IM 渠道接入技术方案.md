# 飞书等 IM 渠道接入技术方案

## 1. 目标

让用户在飞书（以及后续的 Slack、钉钉、企业微信、Telegram）里直接和 Multara 的智能体或小队对话：

1. 在飞书私聊或群里 @机器人 发消息，Multara 在本机启动对应智能体运行。
2. 运行结束后把回答发回飞书。运行中用表情（Typing）或卡片显示进度。
3. 一个飞书会话（私聊、群、群话题）对应一个 Multara `chat_sessions`，多轮对话可以续接。
4. 飞书里发来的消息和智能体回复都出现在 Multara 聊天界面里，和本地聊天是同一份数据。
5. 支持 `/new`、`/stop`、`/status` 等控制指令，以及图片和文件输入。
6. 需要用户确认的权限请求和 `multara-questions` 提问，可以在飞书卡片里回答。
7. 渠道层按平台无关的接口设计，飞书只是第一个实现。

本文是实现方案，不表示这些能力已经完成。

---

## 2. Multica 的做法（参考）

Multica（`../multica`）已经接入了飞书 / Lark、Slack、钉钉、企业微信和 Telegram。下面只列和 Multara 设计有关的结论。

### 2.1 渠道抽象

代码在 `server/internal/integrations/channel/`：

- `Channel` 接口只有 5 个方法：`Type()`、`Connect(ctx)`（阻塞跑接收循环，出错返回给 supervisor 重连）、`Disconnect`、`Send(OutboundMessage)`、`Capabilities()`。
- 入站**不在接口上**：适配器把消息推给统一的 `InboundHandler(ctx, InboundMessage)`。
- `InboundMessage` 是平台无关的结构：`EventID / MessageID / Source{ChatID, ChatType p2p|group, SenderID, ThreadID} / Text / CommandText / MediaRefs / ReplyTo / AddressedToBot`。
- `Capability` 位掩码：`Text | RichCard | ThreadReply | QuoteReply | Attachment | TypingIndicator | MessageEdit`。
- `engine.Router` 是所有平台共用的入站管线：解析 `/new` `/clear` → 去重 Claim → 群聊 @ 过滤 → 身份校验 → 绑定会话、写消息 → Typing 表情 → 3 秒防抖合并为一次运行（`batcher.go`）。
- 每个平台只实现一组 `ResolverSet`（安装解析、身份、去重、会话绑定、媒体、回复、Typing）。

### 2.2 飞书接入细节

- **入站只用长连接 WebSocket**，没有 Webhook 回调地址，所以不需要公网 URL，也没有签名校验和 `encrypt_key` 解密。
  - `POST /callback/ws/endpoint` 拿一次性 wss 地址 → protobuf `Frame` 二进制帧 → 每个数据帧回 ACK `{code:200}`（回 500 飞书会重投）→ 分片拼包 → 应用层 ping/pong。
  - 只处理 `im.message.receive_v1`，消息类型：text、post、image、file、audio、media、video、merge_forward。
  - 没有依赖 `oapi-sdk-go`，协议是手写的。
- **安装**用设备码扫码流程（RFC 8628），不是手填 app_id：
  - `POST https://accounts.feishu.cn/oauth/v1/app/registration`，`action=begin`、`archetype=PersonalAgent`，返回二维码地址；
  - 轮询 `action=poll` 拿到 `client_id`（app_id）和 `client_secret`，同时拿到扫码人的 `open_id`，安装人自动绑定；
  - `tenant_brand` 区分飞书 / Lark，决定 API 域名 `open.feishu.cn` / `open.larksuite.com`。
- **会话映射**：私聊和普通群以 `chat_id` 为键；群话题以 `chat_id:thread_id` 为键，每个话题一个会话。`/new` 换新会话，`/clear` 清上下文。
- **出站不流式**：运行结束后发一条消息。纯文本用 `msg_type=text`，含 Markdown 用 `interactive` 卡片（`schema 2.0` + `markdown` 元素）。
  - 话题内 `reply_in_thread` 回复，普通群引用触发消息回复，私聊直接发。回复目标失效时降级为直接发到会话。
  - 进度只用 Typing 表情：收到消息加 `emoji_type: "Typing"`，运行结束删掉。
- **去重**：`(installation_id, message_id)` 两阶段 Claim / Mark，避免飞书重投导致重复运行。
- **身份**：`open_id` 绑定 Multica 用户，未绑定的人收到绑定卡片。群里没 @ 机器人的消息直接丢弃。
- **路由快照**：入队时把回复目标（chat_id、message_id、thread_id、sender）冻结到 `channel_task_delivery`。回复只看这个快照，Web 端输入的消息不会回发到飞书。

### 2.3 哪些不能照搬

| Multica | Multara | 结论 |
|---|---|---|
| 服务端多副本，需要 WS 租约（Postgres / Redis CAS） | 单机单进程 | 不需要租约，但要防止**同一应用开两个实例**同时连 |
| 多用户、工作区成员、绑定页面 | 单用户本机应用 | 身份简化为 open_id 白名单 |
| 任务交给远端 daemon 执行 | 本机直接 spawn CLI | 直接复用 `executor` 队列 |
| Go + goroutine + context | Rust + `std::thread` + `mpsc`，无 tokio | 用同步 WebSocket 库，每个连接一个线程 |
| 服务一直在线 | 窗口一关进程就退出 | 需要托盘常驻 |

---

## 3. Multara 当前实现与缺口

### 3.1 已有基础

- **运行队列在数据库里**：`executor::start_poller`（`executor.rs:67`）每 2 秒检查 `agent_runs` 里 `queued` 的行并启动。任何地方插入一条 queued run 都会被执行。
- **发消息主流程** `commands.rs::send_message_inner`（`:3742`）：
  1. 解析 `@mention`；
  2. 有运行在跑且 stdin 开着时走 `executor::steer` 追加输入；
  3. 否则 `session::ensure_session_idle`，写 `agent_turns`、`messages`、`message_attachments`；
  4. `dispatch()`（`:4285`）→ `ops::run::enqueue` 插入 queued run → `executor::notify_run` + `executor::dispatch`。
- **运行结束钩子** `executor::finish`（`:1473`）：更新 `agent_runs`，`save_terminal_message` 写入 `messages`（id `run:<runId>`），最后发出带完整 `chunk` 的 `run-event`。**这里就是回发飞书的挂载点。**
- **权限确认**：ACP 运行时的 `session/request_permission` 会变成 `eventType: "permission"` 的时间线项，`executor::resolve_permission`（`:577`）可以在 UI 之外调用。
- **入站服务器先例**：`webhook.rs` 用 `tiny_http` 在 `127.0.0.1:43127` 接自动化 Webhook，带 HMAC 校验和 `webhook_deliveries` 去重。
- **出站 HTTP**：`http_client.rs` 封装了系统 `curl`（`request` / `get` / `request_stream`）。
- **共享逻辑层** `src-tauri/src/ops/`（chat、run、session……）已经被 CLI 和 command 共用。

### 3.2 主要缺口

1. 发消息逻辑写在 Tauri command 里，**没有可以在后台线程直接调用的“给某会话发一条消息”函数**。
2. 没有 WebSocket 库，没有 protobuf 解码，没有 tokio。
3. `webhook.rs` 只监听 `127.0.0.1`，飞书访问不到。即使监听公网，用户本机也没有公网地址，所以**必须走长连接**。
4. 没有托盘，窗口关闭（Windows）或退出后渠道就断了。
5. `messages` 没有来源字段，区分不了消息来自本地还是飞书，也不知道回复该发去哪。
6. 设置存在 `app_settings` 明文，`settings_map` 会把所有值返回前端。没有钥匙串。
7. 大多数运行时以跳过权限的方式启动（claude `bypassPermissions`、codex `--dangerously-bypass-approvals-and-sandbox`、cursor `--yolo` 等）。**从飞书发来的一句话等于在用户电脑上执行任意命令**，必须有严格的发送者白名单。
8. `SettingsView.vue` 发送的 `permissionMode` 不在 `save_settings` 白名单里，会被静默丢弃，`ask` 模式可能从未生效。远程审批依赖这个开关，需要先修。

---

## 4. 总体设计

```
┌───────────────────────── Multara 进程 ─────────────────────────┐
│                                                                │
│  channel::supervisor（线程）                                     │
│    ├─ FeishuChannel #1 ──WS 线程── 飞书长连接网关                 │
│    ├─ FeishuChannel #2                                         │
│    └─ （后续）Slack / 钉钉 / 企业微信 / Telegram                  │
│             │ InboundMessage                                   │
│             ▼                                                  │
│  channel::router                                               │
│    去重 → 白名单 → 群聊 @ 过滤 → 指令 → 防抖合并                    │
│             │                                                  │
│             ▼                                                  │
│  ops::chat::send_message(source = Channel{…})   ← 抽出来的公共逻辑  │
│    写 messages / attachments → dispatch → agent_runs(queued)    │
│    写 channel_run_deliveries（回复路由快照）                       │
│             │                                                  │
│  executor（不变）spawn CLI → run_events → run-event → UI          │
│             │ finish()                                         │
│             ▼                                                  │
│  channel::outbound::on_run_finished(run_id)                    │
│    查 channel_run_deliveries → Channel.send / reply → 去 Typing   │
└────────────────────────────────────────────────────────────────┘
```

原则：

- **执行路径不分叉**。飞书消息和本地输入走同一个 `send_message` 逻辑和同一个 `executor`，UI 自然能看到。
- **渠道层不碰 CLI**。只做“收消息 → 变成一次会话输入”和“运行结束 → 把结果发回去”。
- **回复路由在入队时冻结**，和 Multica 的 `channel_task_delivery` 一样。用户在 UI 里给同一会话发消息，不会被发到飞书。

---

## 5. 概念与数据模型

### 5.1 概念

| 概念 | 含义 |
|---|---|
| 渠道连接 `ChannelInstallation` | 一个飞书自建应用（app_id + app_secret），绑定到一个智能体或小队 |
| 外部会话 `ChannelChat` | 飞书的私聊、群或群话题 |
| 会话绑定 `ChannelSessionBinding` | 外部会话 ↔ Multara `chat_sessions` |
| 授权用户 `ChannelAllowedUser` | 允许触发运行的飞书 open_id |
| 回复路由 `ChannelRunDelivery` | 某次运行结束后回复到哪条消息、哪个话题 |

一个飞书应用只对应一个目标（智能体或小队），和 Multica 的“一个 bot 对应一个 agent”一致。想接多个智能体就建多个飞书应用。在飞书里用 `/agent` 临时切换目标放在后续阶段。

### 5.2 数据库

在 `database.rs` 迁移中新增（字段按 Multara 现有 TEXT 主键 + `created_at` 风格）：

```sql
-- 渠道连接
CREATE TABLE channel_installations (
  id TEXT PRIMARY KEY,
  channel_type TEXT NOT NULL,              -- feishu | lark | slack | dingtalk | wecom | telegram
  name TEXT NOT NULL,
  target_type TEXT NOT NULL CHECK (target_type IN ('agent','squad')),
  target_id TEXT NOT NULL,
  project_id TEXT,                         -- 可选：新会话默认挂到哪个需求（决定 cwd）
  config_json TEXT NOT NULL,               -- app_id, region, bot_open_id, bot_name, group_policy …（不含密钥）
  secret_ref TEXT,                         -- 钥匙串条目名；回退时为加密后的密文
  enabled INTEGER NOT NULL DEFAULT 1,
  status TEXT NOT NULL DEFAULT 'disconnected', -- connected | connecting | disconnected | error
  last_error TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_channel_installations_app
  ON channel_installations(channel_type, json_extract(config_json, '$.app_id'));

-- 授权用户（白名单）
CREATE TABLE channel_allowed_users (
  installation_id TEXT NOT NULL,
  channel_user_id TEXT NOT NULL,           -- 飞书 open_id
  display_name TEXT,
  role TEXT NOT NULL DEFAULT 'owner',      -- owner | member（member 只能问答，见 §9）
  created_at TEXT NOT NULL,
  PRIMARY KEY (installation_id, channel_user_id)
);

-- 外部会话 ↔ Multara 会话
CREATE TABLE channel_session_bindings (
  id TEXT PRIMARY KEY,
  installation_id TEXT NOT NULL,
  route_key TEXT NOT NULL,                 -- chat_id 或 chat_id:thread_id
  chat_id TEXT NOT NULL,
  chat_type TEXT NOT NULL CHECK (chat_type IN ('p2p','group')),
  thread_id TEXT,
  session_id TEXT NOT NULL,
  retired_at TEXT,                         -- /new 后旧绑定退役，保留历史
  created_at TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_channel_binding_active
  ON channel_session_bindings(installation_id, route_key) WHERE retired_at IS NULL;

-- 入站去重
CREATE TABLE channel_inbound_dedup (
  installation_id TEXT NOT NULL,
  message_id TEXT NOT NULL,
  received_at TEXT NOT NULL,
  PRIMARY KEY (installation_id, message_id)
);

-- 回复路由快照（每个 run 一行）
CREATE TABLE channel_run_deliveries (
  run_id TEXT PRIMARY KEY,
  binding_id TEXT NOT NULL,
  installation_id TEXT NOT NULL,
  chat_id TEXT NOT NULL,
  chat_type TEXT NOT NULL,
  reply_to_message_id TEXT,                -- 触发消息（引用回复 / 话题回复）
  thread_id TEXT,
  sender_id TEXT,                          -- 群里回复时 @ 发送者
  typing_reaction_id TEXT,                 -- Typing 表情，结束时删除
  progress_message_id TEXT,                -- 进度卡片（P2）
  state TEXT NOT NULL DEFAULT 'pending',   -- pending | sent | failed
  attempts INTEGER NOT NULL DEFAULT 0,
  last_error TEXT,
  created_at TEXT NOT NULL,
  delivered_at TEXT
);

-- 机器人发出的消息（识别“回复机器人消息”、卡片回调定位）
CREATE TABLE channel_outbound_messages (
  installation_id TEXT NOT NULL,
  message_id TEXT NOT NULL,
  run_id TEXT,
  kind TEXT NOT NULL,                      -- reply | progress | permission | question | notice
  created_at TEXT NOT NULL,
  PRIMARY KEY (installation_id, message_id)
);
```

`messages` 新增两列：

```sql
ALTER TABLE messages ADD COLUMN source_type TEXT;   -- NULL=本地；'feishu' 等
ALTER TABLE messages ADD COLUMN source_meta_json TEXT; -- { installationId, chatId, messageId, senderId, senderName }
```

前端 `ChatMessageItem` 根据 `source_type` 在用户气泡上显示“来自飞书 · 张三”。

`run_events.event_type` 的 CHECK 约束问题（Plan 模式文档已指出）不影响本方案，渠道不新增事件类型。

### 5.3 密钥存储

- 首选系统钥匙串：新增 `keyring` crate（macOS Keychain / Windows Credential Manager），`secret_ref` 存条目名 `multara.channel.<installationId>`。
- 钥匙串不可用时回退到 SQLite，但必须加密（应用数据目录下随机生成的本地密钥 + AES-GCM，或复用已有的 `sha2/hmac` 体系加一个 `chacha20poly1305`）。
- `channel_installations` 的列表接口**永远不返回** app_secret，前端只显示“已配置”。
- 顺手修复：`settings_map` 不应把 `genApiKey` 这类密钥原样返回前端（单独问题，可另开任务）。

---

## 6. 渠道层（Rust）

新增目录 `src-tauri/src/channel/`：

```
channel/
  mod.rs          Channel trait、InboundMessage、OutboundMessage、Capability、注册表
  supervisor.rs   按 channel_installations 启停连接、指数退避重连、状态上报
  router.rs       入站管线：去重、白名单、群聊过滤、指令、防抖、调 ops::chat
  outbound.rs     run 结束 / 权限 / 提问 → 渠道消息
  commands.rs     /new /stop /status /help …
  feishu/
    mod.rs        FeishuChannel：实现 Channel
    ws.rs         长连接：endpoint、帧编解码、ping、ACK、分片拼包
    pbbp2.rs      protobuf Frame 定义（prost 或手写）
    api.rs        tenant_access_token 缓存、发消息、回复、表情、卡片、资源下载
    decode.rs     im.message.receive_v1 / card.action.trigger → InboundMessage
    card.rs       Markdown 卡片、权限卡片、提问卡片
    register.rs   扫码创建应用（设备码流程，P1）
```

### 6.1 接口

Multara 没有 tokio，按现有 `std::thread + mpsc` 风格设计成同步接口：

```rust
pub trait Channel: Send + Sync {
    fn kind(&self) -> ChannelKind;
    fn capabilities(&self) -> Capability;
    /// 阻塞运行接收循环。stop 被置位时返回 Ok(())；需要重连时返回 Err。
    fn run(&self, inbound: &dyn Fn(InboundEvent), stop: &AtomicBool) -> Result<(), String>;
    fn send(&self, msg: &OutboundMessage) -> Result<SendResult, String>;
    fn update(&self, message_id: &str, msg: &OutboundMessage) -> Result<(), String>; // MessageEdit
    fn add_typing(&self, message_id: &str) -> Result<Option<String>, String>;
    fn remove_typing(&self, message_id: &str, reaction_id: &str) -> Result<(), String>;
    fn download(&self, media: &MediaRef, dest: &Path) -> Result<DownloadedFile, String>;
}

pub enum InboundEvent {
    Message(InboundMessage),
    CardAction(CardAction),   // 按钮点击：权限、提问、停止
}

pub struct InboundMessage {
    pub installation_id: String,
    pub message_id: String,
    pub chat_id: String,
    pub chat_type: ChatType,          // P2p | Group
    pub thread_id: Option<String>,
    pub root_id: Option<String>,
    pub parent_id: Option<String>,
    pub sender_id: String,            // open_id
    pub sender_name: Option<String>,
    pub text: String,                 // 已替换 @_user_N、去掉 @机器人
    pub addressed_to_bot: bool,
    pub media: Vec<MediaRef>,
    pub created_at: String,
}
```

每个 installation 一个接收线程。`send` 可以在任意线程调用（内部持有 token 缓存的 `Mutex`）。

### 6.2 Supervisor

`channel::supervisor::start(app)` 在 `lib.rs::setup` 中和 `webhook::start`、`executor::start_poller` 一起启动：

1. 读取 `enabled = 1` 的 installation，为每个起一个线程执行 `channel.run()`。
2. `run` 返回错误时指数退避重连（1s → 2s → … → 最多 60s，加随机抖动），状态写回 `channel_installations.status / last_error`，并 `emit("channel-status")` 通知前端。
3. 前端增删改 installation 后调用 `supervisor::reload(app)`：比较配置指纹，变了的重启，删掉的停止。
4. 系统休眠唤醒后 WebSocket 往往半死，靠应用层 ping 超时（2 × ping 间隔无响应）判定断线并重连。

**单实例**：飞书长连接允许同一应用多个客户端同时在线，但每条事件只会随机投给其中一个。如果用户同时开了两个 Multara（或开发版 + 正式版用同一个应用），消息会“随机丢失”。处理方式：

- 引入 `tauri-plugin-single-instance`，正式版只允许一个实例。
- 开发版（`tauri:dev`）默认不启动渠道连接，设置里单独开关。

### 6.3 飞书长连接协议

按 Multica `ws_connector.go` 的实现（也就是官方 SDK 的行为）：

1. `POST https://open.feishu.cn/callback/ws/endpoint`，body `{"AppID": "...", "AppSecret": "..."}`，返回 `URL`（带 `device_id`、`service_id`）和 `ClientConfig`（`PingInterval`、`ReconnectInterval` 等）。
2. 连接 wss，收发二进制 protobuf `Frame`（pbbp2：`SeqID, LogID, service, method, headers[], payload_encoding, payload_type, payload, LogIDNew`）。
   - `method = 0` 控制帧：ping / pong，pong 里可能带新的 `ClientConfig`。
   - `method = 1` 数据帧：header `type = event | card`，`message_id`、`sum`、`seq` 用于分片拼包。
3. 处理完数据帧后，把同一个 Frame 的 payload 换成 `{"code":200}` 发回作为 ACK。**ACK 要在 3 秒内**，所以收到后先写去重表、投递到 router 的队列就 ACK，不能等运行开始。
4. 定时发 ping 帧。

依赖选择：

| 用途 | 选择 | 理由 |
|---|---|---|
| WebSocket | `tungstenite`（同步）+ `rustls` | 和现有线程模型一致，不引入 tokio |
| protobuf | `prost`（只用 derive）或手写解码 | Frame 只有 9 个字段，手写也可以，少一个构建依赖 |
| HTTP | 先用现有 `http_client.rs`（curl） | 已经在用；如果后面需要更好的错误处理再换 `ureq` |

### 6.4 飞书 API

`feishu/api.rs`，域名按 `region` 选 `open.feishu.cn` / `open.larksuite.com`：

| 用途 | 接口 |
|---|---|
| 应用 token | `POST /open-apis/auth/v3/tenant_access_token/internal`，按 app_id 缓存，提前 5 分钟刷新，遇 token 失效错误码清缓存重试一次 |
| 机器人信息 | `GET /open-apis/bot/v3/info`（拿 `open_id`，用于识别 @机器人） |
| 发消息 | `POST /open-apis/im/v1/messages?receive_id_type=chat_id` |
| 回复 | `POST /open-apis/im/v1/messages/{id}/reply`（话题内带 `reply_in_thread: true`） |
| 更新卡片 | `PATCH /open-apis/im/v1/messages/{id}`（卡片需 `update_multi: true`） |
| Typing 表情 | `POST/DELETE /open-apis/im/v1/messages/{id}/reactions` |
| 下载资源 | `GET /open-apis/im/v1/messages/{id}/resources/{file_key}?type=image|file` |
| 引用的消息 | `GET /open-apis/im/v1/messages/{id}` |
| 群内近期消息 | `GET /open-apis/im/v1/messages?container_id_type=chat&container_id=...`（P2，可选） |

### 6.5 入站解码

`feishu/decode.rs` 只处理：

- `im.message.receive_v1`：
  - `text`：取 `text`，把 `@_user_N` 换成 `@名字`，去掉 @机器人本身（按 `bot_open_id` 匹配），`addressed_to_bot` = mentions 中有机器人。
  - `post`（富文本）：拼成纯文本 + 图片 `MediaRef`。
  - `image` / `file` / `audio` / `media`：生成 `MediaRef { message_id, file_key, kind, name }`。
  - `merge_forward`：P2 支持展开，P0 回复“暂不支持合并转发”。
- `card.action.trigger`（卡片按钮回调，长连接同样可以收到）：解析 `action.value` 里的 `{ kind, runId, requestId, optionId }`。

---

## 7. 入站管线（router）

```
InboundMessage
  1. 去重：INSERT OR IGNORE channel_inbound_dedup；已存在 → 直接 ACK 丢弃
  2. ACK 飞书
  3. 群聊过滤：group 且 !addressed_to_bot 且不是回复机器人消息 → 丢弃
  4. 白名单：sender_id 不在 channel_allowed_users → 见 §9.2，回一句提示，不运行
  5. 指令：/new /stop /status /help … → §7.3，处理完结束
  6. 找会话：route_key = thread_id 存在 ? chat_id:thread_id : chat_id
            查 channel_session_bindings（未退役）；没有则建 chat_sessions + 绑定
  7. 媒体：下载到会话附件目录，生成 MessageAttachmentInput
  8. 防抖：同一 route_key 3 秒内的多条消息合并（文本换行拼接，附件合并）
  9. 加 Typing 表情
 10. ops::chat::send_message(session_id, text, attachments, source = Channel{…})
       → 会话空闲：入队新 run，写 channel_run_deliveries
       → 会话正在跑且可 steer：executor::steer 追加，给消息加 "已收到" 表情
       → 会话正在跑且不可 steer：进入渠道等待队列（§7.2）
```

### 7.1 抽出 `ops::chat::send_message`

把 `commands.rs::send_message_inner` 的主体搬到 `ops/chat.rs`，签名大致为：

```rust
pub struct SendOptions {
    pub attachments: Vec<MessageAttachmentInput>,
    pub source: MessageSource,             // Local | Channel { installation_id, chat_id, message_id, sender_id, sender_name }
    pub delivery: Option<ChannelDeliveryDraft>, // 有值时，与 run 在同一事务写 channel_run_deliveries
}

pub enum SendOutcome {
    Queued { user_message: ChatMessage, run_ids: Vec<String> },
    Steered { user_message: ChatMessage },
    Busy,                                  // 会话在跑且不能 steer
}

pub fn send_message(app: &AppHandle, session_id: &str, content: &str, opts: SendOptions)
    -> Result<SendOutcome, String>;
```

- Tauri 的 `send_message` command 改为薄包装：`source = Local`，`delivery = None`。行为保持不变。
- 成功后同样调用 `executor::notify_run` 和 `executor::dispatch`，并 `emit("chat-message-appended", …)`，让前端在不刷新的情况下看到飞书发来的用户消息（现在前端只在本地发送时做乐观插入，需要新增这个事件）。
- 小队目标：`dispatch()` 会把任务交给队长，队员运行是 `parent_run_id` 子运行。**只对顶层 run 写 delivery**，最终回复取队长运行结束后的总结；如果队长结束后还会派发队员，等整个 turn（`session::complete_turn`）结束后再回发。实现上 delivery 挂在 `turn_id` 上更稳，见 §8.1。

### 7.2 会话忙时

本地 UI 在会话忙时把消息留在前端队列里（`queuedMessages`），飞书没有前端队列，需要在后端排队：

- 内存队列 `HashMap<session_id, Vec<PendingInbound>>`，在 `executor::finish` 的渠道钩子里检查：会话空闲后取出合并为一条再发送。
- 进程重启时内存队列丢失，所以入队时同时给原消息加一个“排队中”表情，重启后清理表情并回复“Multara 重启，请重新发送”。P0 可以简单处理成直接回复“上一个任务还在运行，发送 /stop 可以中止”。

### 7.3 指令

| 指令 | 作用 |
|---|---|
| `/new` | 退役当前绑定，下一条消息新建会话 |
| `/stop` | 对该会话运行中的 run 调 `stop_run` |
| `/status` | 回复目标智能体、需求、工作区、运行状态、排队数 |
| `/project <名称>` | 切换新会话挂靠的需求（只限白名单 owner） |
| `/help` | 指令列表 |

指令只匹配消息开头（去掉 @机器人之后），不区分大小写。

### 7.4 会话命名与工作区

- 新会话标题：`飞书 · <群名或私聊对方名> · <首句前 20 字>`。
- `project_id` 取 installation 上的默认需求，没有就用 `scratch_workdir`（`<appData>/task-workspaces/<sessionId>`），和本地无需求聊天一致。
- 在 Multara UI 里可以把飞书会话手动关联到其他需求或工作区，绑定关系不受影响。

---

## 8. 出站

### 8.1 运行结束回发

在 `executor::finish` 末尾（已有 automation、task 钩子的位置）调用：

```rust
channel::outbound::on_run_finished(app, &run_id, status, &final_text);
```

内部：

1. 查 `channel_run_deliveries`。没有记录 → 本地消息，直接返回。
2. 小队：如果该 run 还有子运行未完成，等 turn 结束再发（检查 `agent_turns.status`）。
3. 根据状态生成内容：
   - `completed`：最终文本（就是 `save_terminal_message` 写入的内容）。
   - `failed`：`运行失败：<error>`，附“在 Multara 中查看”。
   - `stopped`：`已停止`。
4. 选择格式：
   - 不含 Markdown 语法 → `msg_type=text`；
   - 含 Markdown → `interactive` 卡片：`{"schema":"2.0","config":{"update_multi":true},"body":{"elements":[{"tag":"markdown","content":...}]}}`。
   - 飞书卡片 Markdown 不支持部分语法（表格、HTML、mermaid），发送前做一次转换：mermaid 块替换为“图表请在 Multara 查看”，`multara-questions` 块走 §8.3。
5. 选择目标：
   - 话题内：`reply` + `reply_in_thread: true`；
   - 普通群：`reply` 引用触发消息，正文前 @发送者；
   - 私聊：直接 `send`。
   - 回复目标失效（消息被撤回等）→ 降级为直接发到 chat。
6. 超长拆分：卡片正文约 30KB 上限，文本消息约 150KB。超过时按段落拆成多条，最后一条附“完整内容见 Multara”。
7. 删除 Typing 表情，更新 delivery `state = sent`，写 `channel_outbound_messages`。
8. 发送失败（网络等）：`attempts + 1`，后台重试 3 次（10s / 60s / 5min）；最终失败写 `last_error`，并在 Multara 收件箱提示“飞书回复发送失败”。

所有网络调用放到独立线程，**不能阻塞 `executor::finish`**。

### 8.2 运行中进度（P2）

Multica 去掉了“思考中卡片”，只保留 Typing 表情。Multara 的运行往往长达几分钟，建议做一个轻量进度卡片，作为可选项：

- 运行开始后 10 秒仍未结束，发一张进度卡片：“正在运行 · 已用 0:12 · 最近：编辑 src/app.ts”，带「停止」按钮。
- 订阅 run 的 `timeline_item`，节流（≥ 3 秒一次）`PATCH` 更新卡片，只显示工具调用摘要，不发思考内容和代码。
- 结束时把进度卡片更新成最终结果，或者删除进度卡片再发结果（二选一，在设置里配置）。
- 也可以用飞书 CardKit 的流式更新卡片接口实现逐字输出，作为 P3 选项。

### 8.3 权限确认与提问（P2）

**权限**：`permissionMode = ask` 且运行时是 ACP 时，`PermissionGate` 会等待最多 300 秒。

1. `executor` 发出 `permission` 时间线项时，如果该 run 有 delivery，`outbound` 发一张卡片：工具名、参数摘要（截断）、按钮来自 `options`（允许一次 / 总是允许 / 拒绝）。按钮 `value = { kind: "permission", runId, requestId, optionId }`。
2. 长连接收到 `card.action.trigger` → 校验点击人在白名单且 role = owner → `executor::resolve_permission(app, request_id, option_id)` → 把卡片更新为“已允许（张三）”。
3. 本地 UI 先处理了也要同步：resolve 后统一更新卡片，避免两边状态不一致。
4. 超时自动选择后同样更新卡片。

**提问**：智能体回复中的 ```` ```multara-questions ```` 块（`src/components/chat/question-block.ts`），在飞书里渲染成卡片：问题 + 选项按钮 / 下拉。点击后把答案当作下一条用户消息走 §7 的流程。没有选项的开放问题，提示用户直接回复文本。

### 8.4 文件（P3）

- 入站图片和文件：§7 第 7 步下载，写入 `message_attachments`，沿用现有 10 个 / 25MB 限制。
- 出站：智能体生成的文件暂不回传，提示“文件已保存在 Multara 工作区”。后续可以通过 `POST /open-apis/im/v1/files` 上传后发 `file` 消息，并像 Multica 一样在 prompt 里告诉智能体当前渠道能否发文件。

### 8.5 让智能体知道自己在飞书里

`runtime_brief::build` 在 run 有 delivery 时追加一小段说明：

- 当前对话来自飞书私聊 / 群聊，回答会被转成飞书消息；
- 避免依赖 mermaid、HTML、超宽表格；
- 不要输出密钥、本机绝对路径等敏感信息（群聊时尤其重要）。

---

## 9. 安全

接入 IM 等于把“在本机执行命令”的能力开放给一个远程聊天窗口。安全设计是本方案的前提，不是可选项。

### 9.1 威胁

1. 群里任何人 @机器人 → 在用户电脑上以跳过权限的方式运行 CLI。
2. 飞书应用凭据泄露 → 别人可以冒充机器人发消息，但**不能**给 Multara 发入站事件（入站只来自飞书推送）。
3. 智能体在群聊里回复了本机文件内容、环境变量、密钥。
4. 提示词注入：群里其他人的消息、引用的消息进入上下文，诱导智能体执行危险操作。

### 9.2 措施

1. **默认只接受白名单用户**。
   - 扫码安装（P1）时，扫码人的 open_id 自动加为 owner。
   - 手动配置（P0）时，首次有人私聊机器人，Multara 弹出系统通知和应用内确认：“open_id xxx（张三）请求使用 <智能体>，是否允许？”。也可以在设置里显示一个 6 位配对码，用户私聊机器人发送配对码完成绑定。
   - 非白名单用户的消息只回一次“未授权”，之后静默（防刷屏），记录到日志。
2. **群聊默认关闭**。开启后：
   - 只有白名单用户的 @ 才触发；
   - 可选 `member` 角色：只能触发“只读”运行（P3，需要运行时支持只读 / Plan 模式，依赖 Plan 模式方案的 `RunMode::Plan`）。
3. **按 installation 配置运行策略**：
   - 选择是否强制 `permissionMode = ask`（推荐飞书来源默认 ask，依赖 §3.2 第 8 条的修复）；
   - 允许的需求 / 工作区列表，`/project` 只能在列表里切换；
   - 是否允许 `direct` 隔离模式的工作区（默认不允许，只允许 worktree / copy）。
4. 群聊不拉取近期消息作为上下文（Multica 的 `<recent_context>`），只用 @机器人的那条和它引用的消息，降低注入面。
5. 密钥存钥匙串（§5.3），日志里不打印 token 和消息正文。

---

## 10. 常驻与生命周期

- 新增托盘图标（Tauri `tray-icon` feature）：显示渠道连接状态，菜单“打开 Multara / 暂停所有渠道 / 退出”。
- 有启用的渠道时，关闭主窗口改为隐藏到托盘（`on_window_event` 拦截 `CloseRequested`），并首次提示“Multara 会在后台继续接收飞书消息”。没有启用渠道时保持现在的行为。
- 可选：开机自启（`tauri-plugin-autostart`），设置里开关，默认关闭。
- 退出时：停止接收线程，正在运行的 run 按现有逻辑处理；对这些 run 的 delivery 回发“Multara 已退出，任务中断”。
- 启动时：扫描 `state = pending` 且 run 已结束的 delivery 补发（应对上次回发前退出），清理残留 Typing 表情。

---

## 11. 前端

### 11.1 入口

在 `SettingsView.vue` 新增「消息渠道」分区（或独立路由 `/channels`，和 `/automation` 并列，推荐后者，因为每个连接都有状态和白名单要管理）：

```
消息渠道
┌──────────────────────────────────────────────────────┐
│ 飞书 · 前端助手          ● 已连接     [编辑] [停用]      │
│   目标：智能体「Frontend」 · 默认需求：Multara           │
│   授权用户：张三（owner）、李四                          │
│   最近：2 分钟前收到消息                                 │
├──────────────────────────────────────────────────────┤
│ [+ 添加飞书机器人]  [+ 添加 Lark 机器人]                 │
└──────────────────────────────────────────────────────┘
```

另外在智能体 / 小队详情页加一个「集成」标签，显示绑定在它上面的渠道，提供快速添加。

### 11.2 添加流程

**P0：手动配置**

1. 引导文案：去飞书开放平台创建企业自建应用 → 启用机器人能力 → 事件订阅选择“使用长连接接收事件” → 订阅 `im.message.receive_v1`（以及卡片回调 `card.action.trigger`）→ 开通权限 → 发布版本。
2. 表单：名称、区域（飞书 / Lark）、App ID、App Secret、目标智能体 / 小队、默认需求、群聊开关、飞书来源是否强制审批。
3. 「测试连接」：调用 `tenant_access_token` 和 `bot/v3/info`，显示机器人名称和头像，检查长连接 endpoint 是否可用。
4. 保存后自动连接，显示配对码，引导用户私聊机器人完成 owner 绑定。

需要的权限（引导页列出）：`im:message`、`im:message:send_as_bot`、`im:message.group_at_msg:readonly`、`im:message.p2p_msg:readonly`、`im:resource`、`im:message.reactions:write_only`（以开放平台实际名称为准）。

**P1：扫码创建**

复用 Multica 的设备码流程（`registration.go`）：`archetype=PersonalAgent`，二维码由前端渲染，后台线程轮询，成功后自动写入 app_id / app_secret 并把扫码人加为 owner。用户无需进入开放平台。需要确认该注册接口对第三方客户端开放的条件（Multica 是以自己的身份调用的），如果不能直接用，就保留手动配置作为唯一方式。

### 11.3 聊天界面

- 飞书来源的用户消息显示来源标签和发送者名称。
- 会话列表中飞书会话显示飞书图标。
- 在 UI 中回复飞书会话：**默认只在本地**，不回发飞书（和 Multica 一致）。输入框上方显示提示“此会话绑定了飞书，本地发送的消息不会同步到飞书”。后续可以加“同时发送到飞书”开关。

### 11.4 Tauri 命令

| 命令 | 说明 |
|---|---|
| `list_channel_installations` | 列表 + 状态（不含密钥） |
| `save_channel_installation` | 新建 / 更新，密钥写钥匙串，触发 `supervisor::reload` |
| `delete_channel_installation` | 删除连接、密钥、绑定 |
| `test_channel_installation` | 测试凭据 |
| `set_channel_enabled` | 启停 |
| `list_channel_allowed_users` / `remove_channel_allowed_user` / `approve_channel_user` | 白名单 |
| `begin_feishu_registration` / `poll_feishu_registration` | 扫码创建（P1） |

事件：`channel-status`（连接状态）、`channel-pairing-request`（待批准的新用户）、`chat-message-appended`（外部消息写入）。

内置 CLI（`cli.rs`）同步增加 `multara channel list|enable|disable`，便于 Miku 管理。

---

## 12. 落地顺序

### P0：最小可用（飞书私聊，单智能体）

1. 抽出 `ops::chat::send_message`，Tauri command 改为包装，行为不变（先合这一步，回归测试）。
2. 数据库迁移：§5.2 的表和 `messages.source_*`。
3. `channel/` 框架 + `feishu/ws.rs` 长连接 + `api.rs` 发消息 / 回复 / Typing。
4. router：去重、白名单（配对码）、私聊、`/new` `/stop` `/status`。
5. `executor::finish` 钩子回发文本 / Markdown 卡片。
6. 设置页手动添加飞书应用，密钥存钥匙串。
7. 单实例插件；开发版默认不连。

### P1：可用性

1. 群聊（@ 过滤、话题会话、引用回复、@发送者）。
2. 会话忙时排队与防抖合并。
3. 托盘常驻、关窗隐藏、启动补发。
4. 扫码创建应用（如果注册接口可用）。
5. 图片 / 文件入站。
6. Lark 国际版（只是换域名）。

### P2：交互

1. 修复 `permissionMode` 保存；飞书来源强制 ask。
2. 权限卡片与卡片回调。
3. `multara-questions` 卡片。
4. 进度卡片 + 「停止」按钮。
5. 小队按 turn 汇总回发。

### P3：扩展

1. 其他平台：Slack（Socket Mode）、钉钉（Stream 模式）、企业微信（智能机器人长连接 `wss://openws.work.weixin.qq.com`）、Telegram（`getUpdates` 长轮询）。这几个都不需要公网地址，正好适合本机应用，只需实现 `Channel` trait 和各自的解码。
2. 出站文件。
3. 群聊 member 只读角色。
4. 流式卡片。

---

## 13. 文件清单

| 文件 | 改动 |
|---|---|
| `src-tauri/Cargo.toml` | 新增 `tungstenite`、`rustls`/`native-tls`、`keyring`、`tauri-plugin-single-instance`，可选 `prost`；`tauri` 开启 `tray-icon` |
| `src-tauri/src/channel/**` | 新增，见 §6 |
| `src-tauri/src/ops/chat.rs` | 新增 `send_message`、`MessageSource` |
| `src-tauri/src/commands.rs` | `send_message` 改为包装；新增 §11.4 命令；`save_settings` 白名单补 `permissionMode` |
| `src-tauri/src/executor.rs` | `finish` 调 `channel::outbound::on_run_finished`；`permission` 事件通知渠道 |
| `src-tauri/src/runtime_brief.rs` | 渠道来源提示 |
| `src-tauri/src/database.rs` | 迁移 |
| `src-tauri/src/models.rs` | `ChannelInstallation` 等模型；`ChatMessage` 增加 `sourceType`、`sourceMeta` |
| `src-tauri/src/lib.rs` | 启动 supervisor、托盘、单实例、关窗行为 |
| `src-tauri/src/main.rs` | 注册新命令 |
| `src-tauri/src/cli.rs` | `channel` 子命令 |
| `src-tauri/capabilities/*` | 新插件权限 |
| `src/domain/types.ts` | 渠道类型、消息来源字段 |
| `src/services/backend.ts` | 新命令封装 + 浏览器预览 mock |
| `src/stores/app.ts` | 监听 `chat-message-appended`、`channel-status` |
| `src/views/ChannelsView.vue`（新）、`src/router.ts` | 渠道管理页 |
| `src/components/channels/*`（新） | 添加表单、配对码、白名单、状态 |
| `src/components/ChatMessageItem.vue` | 来源标签 |
| `src/i18n/*` | 文案 |

---

## 14. 测试要求

**Rust 单元测试**

1. pbbp2 Frame 编解码、分片拼包、ACK 帧构造（用 Multica 测试里的帧样本对拍）。
2. `im.message.receive_v1` 各消息类型解码，@ 替换和去除机器人 mention。
3. router：重复 message_id 只运行一次；非白名单不运行；群聊未 @ 丢弃；话题生成独立 route_key；`/new` 后新建会话。
4. `ops::chat::send_message`：Local 来源行为与旧 command 一致（复用现有测试）；Channel 来源写入 `source_*` 和 delivery。
5. outbound：Markdown 检测、超长拆分、mermaid 替换、回复目标选择（p2p / 群 / 话题）、失效降级。
6. token 缓存过期与失效重试。

**集成测试**

1. 用本地假 WebSocket 服务器模拟飞书网关，跑通“收到消息 → 入队 → 假运行时完成 → 调用假 HTTP 发送”的全流程。
2. 断线重连、ping 超时、休眠唤醒（关闭假服务器再开）。
3. 运行中退出应用，重启后补发。

**手工验证**

1. 真实飞书应用：私聊、群 @、群话题、图片、`/stop`、权限卡片。
2. 两个 Multara 实例同时启动时第二个被拒绝。
3. 关窗后仍能收发，托盘退出后停止。

---

## 15. 风险

| 风险 | 应对 |
|---|---|
| 长连接协议非公开文档，可能变更 | 协议层独立在 `ws.rs`；对照官方 Go / Python SDK 实现；集成测试覆盖帧格式 |
| 用户电脑睡眠、断网导致消息延迟或丢失 | 飞书对未 ACK 事件有重投；重连后不补拉历史，在文档中说明“Multara 需要在线” |
| 远程触发高权限运行 | §9 白名单、群聊默认关、飞书来源默认 ask、限制工作区 |
| 多实例导致消息随机丢失 | 单实例插件 + 开发版默认不连 |
| 回答太长、格式在飞书里错乱 | 拆分 + 格式降级 + “在 Multara 查看”兜底 |
| 抽取 `send_message` 引入回归 | P0 第一步单独合并，补齐现有测试后再接渠道 |
| 扫码注册接口可能不对第三方开放 | P1 再做，失败则只保留手动配置 |

---

## 16. 完成标准

P0 完成时：

1. 在设置中填入飞书应用凭据后 5 秒内显示“已连接”。
2. 白名单用户私聊机器人，消息出现在 Multara 对应会话里，智能体运行，结束后飞书收到回复，Typing 表情消失。
3. 同一条飞书消息被重投不会触发第二次运行。
4. 非白名单用户的消息不会触发任何运行。
5. 网络断开再恢复后自动重连，期间的状态在界面上可见。
6. 本地聊天的发送、steer、小队派发行为与改动前一致。
