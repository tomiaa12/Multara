# 对齐 Miku 总管、内置 CLI 与 Builtin Skills

对照 Multica 的 Mika 模型，把 Multara 从「预置两个专家 + 一个小队」改成「一个产品总管 + 按需 specialist」。智能体跑任务时，平台操作走全局内置 CLI，方法手册走内置 skill。

本文只写要改什么、改哪里、为什么。不在这一步改代码。

---

## 1. 目标与非目标

### 要达到的效果

1. **默认只有一个产品智能体**：Miku，工作区 Chief of Staff。不预置阿库、小审、产品开发小队。
2. **产品提示词热更新**：Miku 的身份说明嵌在二进制里，不写进 `instructions` 列；之后改提示词发版即生效，不会盖掉用户 notes。
3. **平台操作用 CLI**：agent 子进程里能跑 `multara task / agent / squad / project / automation / skill …`，禁止让模型用 curl、直接猜 SQLite 路径。
4. **内置 skill 按需加载**：所有智能体都能看到 `multara-*` 手册；创建任务、智能体、小队、自动化之前先读对应 skill。
5. **工作模型对齐 Mika**：聊天里澄清和协调；真正改代码、多轮交付走 **任务（Task）**，由被指派的那次 run 去做。

产品还没人用，**不写阿库/小审 → Miku 的数据迁移**。发布后的软件更新 **不得删除、重建用户的 sqlite**。

### 明确不抄的部分

Multara 是本地单用户桌面应用，不是 Multica 那种多成员云工作区。下面这些 **不要做**：

- `permission_mode` / invocation target / 工作区成员 @mention
- HTTP API + 登录 token
- `kind=system` 的隐藏 builder 载体（除非以后做「用 AI 建智能体」）
- 把整个 SKILL.md 全文塞进 prompt（Multara 现在对用户 skill 就是这么干的，内置 skill 不要再走这条路）

---

## 2. 现在 vs 要对齐的形态

| | 现在（Multara） | 目标（对齐 Mika） |
|---|---|---|
| 默认智能体 | seed `frontend`（阿库）、`reviewer`（小审） | 只 seed `miku` |
| 默认小队 | seed `product`，欢迎会话对着小队 | 不 seed 小队；欢迎会话对着 Miku |
| 身份 | `id` 即身份，instructions 写进行里 | `system_key=miku`；产品半段嵌二进制，行内只放 notes |
| 提示词拼装 | `instructions + workspace + 已绑 skill 全文 + 历史 + 用户任务` | Runtime brief + **You are: Miku** + 产品提示词 + notes + 用户任务 |
| 平台工具 | 无。agent 只能改磁盘、用本机 CLI 原生工具 | 子进程 PATH 上有 `multara` CLI，操作 SQLite 领域对象 |
| Skill | 扫描 `~/.cursor/skills` 等，只有绑到 agent 才注入全文 | 内置 `multara-*` 始终可见；用户 skill 仍按绑定走 |
| 干活单位 | 聊天 session 为主；`tasks.session_id` 一对一，建任务等于再建一个会话，且 **不自动开跑** | 聊天可以建任务；`todo` 且指定了智能体 → enqueue run（对齐 issue 指派即开工） |
| Runtime | seed 写死 `cursor`，即使 `not_found` | 创建时绑本机第一个 `online` runtime；没有则仍创建，跑任务时再报「请绑定运行时」 |

概念映射：

| Multica | Multara |
|---|---|
| Issue | Task（要升成一等业务对象，见 §6） |
| Workspace | 本机这一份 app data / 当前项目 |
| Agent | Agent，默认只留 Miku |
| Squad | Squad，功能保留，不预置 |
| Autopilot | Automation |
| `multica` CLI | 新的 `multara` CLI |
| Builtin `multica-*` skills | Builtin `multara-*` skills |
| Mika | **Miku** |

---

## 3. Miku 产品定义

常量建议集中在 `src-tauri/src/builtin_agents.rs`（新建），前端只读 `system_key`，不要自己拼一套身份。

| 字段 | 值 | 说明 |
|---|---|---|
| `id` | 稳定 `miku` | seed 每次启动幂等插入 |
| `system_key` | `miku` | 认身份用这个，不认显示名 |
| `name` | `Miku` | 用户可改名 |
| `avatar` | `🦄` | 与现有 emoji 头像一致 |
| `description` | 你的工作区总管。Miku 把目标变成任务、协调智能体，并帮你建可复用工作流。 | 只给人看，**不进 prompt**（现在 description 本来也不进 prompt） |
| `instructions` 列 | `''` | 留给用户 notes |
| `runtime_id` | 第一个 `online` 的 runtime，否则 `cursor` 占位 | 见 §7 |
| `model` | `auto` | 跟现在默认 |
| `max_concurrent_tasks` | `3` | 总管要比专家高；现在专家默认 1–2 |
| 删除 | 禁止删 Miku | `delete_agent` 对 `system_key=miku` 返回错误 |
| 用户 skill / MCP | 默认不绑 | 与 Mika 一致 |

产品提示词（嵌二进制，中文，结构对齐 Mika，用语换成 Multara）：

- 你是 {{AGENT_NAME}}，Multara 的默认总管。
- 用用户语言回复。
- 用户给的是目标，不是路由决策；自己分流并告诉对方选了什么。
- 一轮能答完、答案本身就是交付 → 聊天里说完。
- 需要改代码、仓库、多轮、或以后要回来看 → **建任务**，让被指派的 run 去做。
- 工作台（这次运行已绑需求工作区）：用户要求改代码时直接改。任务用来追踪、分派、多轮，不是改代码的前置条件。
- 没有需求工作区的会话：不要改随机目录。先打开需求，或建任务并带上 `--project`。
- 路由从小到大：自己 → 已有 specialist → 新建 specialist（先预览再确认）→ 小队 → 自动化。
- 普通任务操作：用户说清楚了就可以做。
- 创建/大改智能体、小队、自动化，以及破坏性操作：先预览，等人确认。
- 动手前 load 对应 `multara-*` skill。

欢迎会话：

- id 仍可用 `welcome`，标题「开始使用 Multara」
- `target_type=agent`，`target_id=miku`
- **开场白由产品写死**（不要先跑一轮模型），对齐 Mika 的四拍：Multara 是什么、Miku 是谁、接下来怎么做、下面选一张卡片或直接说目标。

Starter 卡片（可选 P2，UI 在 `ChatPanel`）：看板化当前目标、让 Miku 自己跑一小段调研、建一个定时摘要自动化。默认路径仍是「一个任务，指派给 Miku」。

---

## 4. 提示词栈（执行期）

现在 `conversation.rs::build_prompt` 是：

```
{instructions}

{workspace_context}

{resource_context}

{history}用户任务：{assignment}
```

目标拆成稳定前缀 + 本轮内容，方便以后做 resume / cache：

```
# Multara Agent Runtime
你是 Multara 里的 coding agent。平台对象只能通过 `multara` CLI 操作，不要 curl，不要直接打开 sqlite。

## Agent Identity
**You are: Miku** (ID: `miku`)

<产品提示词 ComposeMikuInstructions(name, notes)>

## Workspace
只能改这些目录：…

## Skills
- **multara-working-on-tasks**
- **multara-creating-agents**
- …（只列名字，正文在磁盘上）

## Available Commands
（核心子集，见 §5）

## Workflow
聊天模式 / 任务模式 二选一

--- 以下为本轮 ---
历史（若 CLI 不支持 --resume）
用户任务：…
```

要改的文件：

- `src-tauri/src/conversation.rs`：`build_prompt` 接收 brief，而不是把 instructions 当全文
- `src-tauri/src/executor.rs`：`spawn_run` 组装 brief；resume 时仍只传 assignment（Cursor/Claude `--resume` 已有）
- `src-tauri/src/builtin_agents.rs`：`compose_miku_instructions(name, notes)`
- 新建 `src-tauri/src/runtime_brief.rs`：写 brief 各节（从 `executor.rs` 里把 workspace/skill 文案拆出来）

`Agent Identity` 优先于 Runtime Workflow：总管提示词说「聊天里不要改代码」时，通用「去 work_path 改文件」的句子要让路。

---

## 5. 内置 CLI

这是对齐 Mika 的硬依赖。没有 CLI，Miku 只能在聊天里空转，建不了任务、建不了 specialist。

### 5.1 形态

在同一个 crate 加 `[[bin]]`：

```
src-tauri/src/bin/multara.rs   # CLI 入口
src-tauri/src/ops/             # 从 commands.rs 抽出来的领域操作
```

Tauri command 变薄：校验 + 调 `ops::*` + 推 UI 事件。  
CLI：解析 argv + 打开同一份 SQLite + 调同一套 `ops::*` + JSON stdout。

包进桌面应用：

- macOS：`Contents/MacOS/multara` 或 `Contents/Resources/bin/multara`
- Windows：安装目录旁 `multara.exe`

`executor.rs::spawn_run` 给子进程：

```
PATH = cli_dir + PATH
MULTARA_APP_DATA = {app_data_dir}
MULTARA_DB = {app_data_dir}/multara.sqlite3
CI=1                         # 已有
```

SQLite 开 WAL + `busy_timeout`，CLI 和 UI 并发可接受。agent 跑的时候 UI 一定在，不需要再做一个 Multica 式的独立 daemon。

### 5.2 命令面（第一期）

只覆盖 Miku 工作模型用得到的。stdout JSON，提示和确认走 stderr。模型解析 stdout 时禁止 `2>&1`。

```text
multara --help

multara task list|get|create|update|status|assign|comment
multara agent list|get|create|update|skills
multara squad list|get|create|member
multara project list|get|create
multara automation list|get|create|trigger-add
multara skill list|get|import
multara runtime list
multara session history          # 当前聊天续上下文（可选，resume 失败时用）
multara session resume [id]     # 用已保存的 Cursor/Claude conversation id 接着上次任务
```

`task create` 必须能：

1. 插入 `tasks` + 必要时插入配套 `chat_sessions`（现状就是这样）
2. `--assignee miku|agent-id`
3. `--status todo` → **enqueue `agent_runs`**（现状建完停在 todo，对不齐「指派即开工」）
4. `--status backlog` → 只落库，不开跑
5. `--project <id>` 把任务绑到需求，这样工作区目录能进那次 run 的 `workspace_context`

`agent create` 对齐现有 `save_agent`：`--name` + `--runtime-id` 必填，其余默认 `auto` / 并发 1 / 空 skill。Miku 的 `system_key` 不允许通过 CLI 伪造。

### 5.3 要从 `commands.rs` 抽走的逻辑

`commands.rs` 已经过重。至少这些函数先搬到 `ops/`，两边复用：

| 现有 | CLI 对应 |
|---|---|
| `create_task` / `update_task_status` | `task create` / `task status` |
| `save_agent` / `delete_agent` | `agent create` / `agent update` |
| `save_squad` | `squad create` / `squad member add` |
| `create_project` | `project create` |
| `save_automation` | `automation create` |
| `list_*` 系列 | `* list --output json` |
| `dispatch` / enqueue run | `task create --status todo` 内部调用 |

CLI 跑完写库后，UI 不会自动刷新。需要一条轻量通知：

- 方案 A：CLI 写完在 db 里插 `ui_invalidate` 行，前端轮询（丑）
- 方案 B：CLI 连本机已有 webhook 端口（设置里 `webhookPort=43127`）发一条 `entity-changed`
- 方案 C：WAL + 前端在任务列表/智能体页 focus 时重新 `bootstrap` 片段

第一期用 **B 或 C**。agent 创建任务后，用户主要是在聊天里看到 Miku 回的链接/标题，不强制即时刷新整棵 store。

### 5.4 Runtime brief 里列出的核心命令

不要把 `--help` 全文塞进 prompt。brief 只钉这一组：

```text
multara task get <id> --output json
multara task create --title "..." --description-file ./description.md --assignee <id> --status todo|backlog --project <id>
multara task status <id> <status>
multara task assign <id> --to <id>
multara agent list|get|create
multara project list|get
multara runtime list --output json
```

其余 `multara <cmd> --help`。创建/改 agent、squad、automation 前 load skill，不要靠 brief 教细节。

---

## 6. Task 要补的语义

现在的缺口（`database.rs` + `create_task`）：

- `tasks.session_id UNIQUE`：一个任务 = 一个聊天会话
- 建任务 **不会** `dispatch` run
- 任务没有 `project_id`（会话倒是有，但 create_task 把 `project_id` 写成 NULL）
- 没有评论表，进度只能写回那个新会话的 messages

要对齐「聊天协调、任务交付」：

1. **`task create` 从 Miku 会话里调用时**，新任务会话的 `project_id` 继承当前会话；这样执行端能拿到工作区路径。
2. **`status=todo` 且 assignee 是 agent** → 立刻 `agent_runs queued`，assignment = 任务描述。这是 Mika「create issue and let the assigned run do the work」的本地版。
3. 任务会话里的助手消息，视为该任务的进度记录（先不新建 `task_comments` 表）。
4. Miku 的聊天会话保持原样；她只在自己的会话里回「已创建任务 xxx，交给 yyy」。

若以后要任务脱离会话（一块看板、多个 run），再拆表。第一期不要做。

---

## 7. 数据层改动

`agents` 表加列（`database.rs` 已有 `ensure_column` 模式）：

```sql
ALTER TABLE agents ADD COLUMN system_key TEXT;
CREATE UNIQUE INDEX IF NOT EXISTS idx_agents_system_key
  ON agents(system_key) WHERE system_key IS NOT NULL;
```

`seed()` 直接写成终态，不写从阿库/小审迁过来的兼容分支：

1. 插入 Miku（`INSERT OR IGNORE`，因为 `seed()` 每次启动都会跑）
2. 不再插入 `frontend` / `reviewer` / `product`
3. 插入 welcome，对着 `miku`（`target_type=agent`）

`INSERT OR IGNORE` 的含义：行已存在就跳过，**不会删库、不会清空用户后来建的智能体/任务/会话**。软件更新只换二进制，sqlite 仍在 `{app_data}/multara.sqlite3`。

现在这台开发机如果还留着旧 seed（阿库、小审），那是开发数据，自己把该文件删掉再开一次即可。这是人手清本地库，不是安装器或更新器的行为，发布后也不能做成「更新时自动删库」。

`delete_agent`：`system_key=miku` 拒绝。  
`save_agent`：允许改 Miku 的 name / notes / runtime / model / 并发 / 用户 skill；不允许改 `system_key`，不允许把别人改成 miku。

新建智能体默认保持现状：🤖、`auto`、并发 1、runtime = 第一个 online。这是「specialist」，不是第二个总管。

---

## 8. Builtin Skills

新建目录（跟产品走，打进二进制）：

```
src-tauri/skills/
  multara-onboarding/SKILL.md
  multara-working-on-tasks/SKILL.md
  multara-creating-agents/SKILL.md
  multara-projects-and-workspaces/SKILL.md
  multara-squads/SKILL.md
  multara-automations/SKILL.md
  multara-skill-importing/SKILL.md
```

Rust `include_dir!` 或每个文件 `include_str!`。`user-invocable: false`。frontmatter 写清何时 load。

跑任务时写入应用数据目录 `builtin-skills/`（`app_data_dir`），**不要**写进项目工作区，否则会被提交进 git：

```
{app_data}/builtin-skills/multara-working-on-tasks/SKILL.md
...
```

brief 的 `## Skills` **只列名字**，并给出上面的绝对路径。Cursor / Claude 不会扫到应用数据目录，所以要靠 brief 里的路径去读。不要再走现在的 `resource_context`「把 SKILL.md 全文拼进 prompt」（`executor.rs` 约 1441–1517 行）。

用户绑定的 skill 两期可以并存：第一期继续全文注入；第二期也改成落盘 + 列名。

各 skill 职责（对照 Multica，按 Multara 对象改）：

| Skill | 教什么 |
|---|---|
| `multara-onboarding` | 仅欢迎会话。开场白已发出，不要再自我介绍。三个 starter play。默认一个任务指派给自己。 |
| `multara-working-on-tasks` | 状态机、todo vs backlog、何时 `--status todo` 会开跑、进度写在任务会话 |
| `multara-creating-agents` | `multara agent create` 字段、description 不进 prompt、不可伪造 Miku |
| `multara-projects-and-workspaces` | 需求、工作区路径、隔离模式；绑定 project 才能改代码 |
| `multara-squads` | 创建小队、队长分发（现有 `dispatch_squad_members`） |
| `multara-automations` | 对应现有 schedule / webhook / interval |
| `multara-skill-importing` | 以后若做 URL 导入；第一期可只 list 本机扫描结果 |

`resources.rs` 扫描的 `owner=multara` 目录（`~/.multara/skills`）继续给 **用户** skill，不要和内置 `multara-*` 混在一个命名空间。内置前缀保留给二进制。

---

## 9. 执行环境（`executor.rs`）

`spawn_run` 现有步骤后面加：

1. 解析 agent：若 `system_key=miku`，`instructions = compose_miku_instructions(name, notes)`
2. 把 CLI 目录 prepend 到 `PATH`
3. 设置 `MULTARA_DB` / `MULTARA_APP_DATA`
4. 把 builtin skills 写到应用数据目录 `builtin-skills/`，不要写进项目仓库
5. 用 `runtime_brief::build(...)` 再 `build_prompt`

`resource_context`：用户 skill 仍按 `agent_resources` 过滤。内置 skill 不进这张表。

无项目会话（cwd=home）：brief 的 Workspace 节写明「当前没有需求工作区，不要改随机目录；要改代码先 `multara task create --project` 或让用户打开一个需求」。已绑定需求工作区的工作台会话允许直接改列出的目录，任务模式同时可用。

---

## 10. 前端要改的面

| 文件 | 改什么 |
|---|---|
| `src/domain/types.ts` | `Agent.systemKey?: string \| null` |
| `src/services/backend.ts` | preview mock 只留 Miku，删阿库/小审/产品小队；welcome 对着 miku |
| `src/views/AgentDetailView.vue` | Miku：产品提示词只读展示，notes 单独编辑；隐藏删除 |
| `src/views/CollectionView.vue` | 总管角标；删除按钮对 Miku 不可用 |
| `src/components/ChatPanel.vue` | 欢迎会话开场白 + 可选 starter 卡片 |
| `src/stores/app.ts` | 新建会话默认 target = Miku（现在是「第一个 agent」） |
| `src/components/chat/preview-messages.ts` | 预览数据跟新 seed 走 |

UI 不实现 CLI，不把产品提示词写进前端常量（避免和 Rust 漂移）。详情页展示用 bootstrap 下发的 `systemInstructions` 只读字段。

---

## 11. 建议落地顺序

不要一次做完。每一期都要能独立跑、有测试。

### P0 — 默认只剩 Miku

- `agents.system_key`
- `seed()` 只写 Miku + 对着 Miku 的 welcome；删掉阿库 / 小审 / 产品小队
- `delete_agent` / `save_agent` 保护
- 产品提示词 embed + `compose_miku_instructions`
- 前端 mock / 默认会话 target
- 测试：seed 幂等、不能删 Miku、rename 后仍按 system_key 认出

**这一期还没有 CLI。** Miku 只能聊天，不能真的建任务/建专家。先把身份和列表做对。

### P1 — CLI + 指派即开工

- `ops/` 抽取 + `src-tauri/src/bin/multara.rs`
- 打包进 app，`spawn_run` 注入 PATH / env
- `task create --status todo` enqueue run
- `create_task` 继承 `project_id`
- `cargo test` 覆盖 CLI JSON 契约
- 手测：在 Miku 聊天里（先人工跑 CLI）建任务，工作区里真的起第二次 run

### P2 — Runtime brief + Builtin skills

- `runtime_brief.rs`
- 内置 skill 落盘 + brief 只列名
- Miku 提示词要求「先 load skill」
- `multara-working-on-tasks` / `creating-agents` 两份先写满，其余可以薄

### P3 — 欢迎开场白与 starter 卡片

- 产品开场白写入 welcome 第一条 assistant message（不跑模型）
- Chat UI 三张卡片
- `multara-onboarding` skill

### P4 — 收口

- 用户 skill 也改为落盘而不是全文注入
- CLI 变更通知 UI
- 文档：`docs/架构.md` §3 按本文改掉「默认 frontend/reviewer」

---

## 12. 文件清单（按仓库）

新建：

```
src-tauri/src/builtin_agents.rs
src-tauri/src/builtin_agents/miku/INSTRUCTIONS.md
src-tauri/src/runtime_brief.rs
src-tauri/src/ops/{mod,task,agent,squad,project,automation,skill}.rs
src-tauri/src/bin/multara.rs
src-tauri/skills/multara-*/SKILL.md
docs/Miku 总管与内置 CLI.md          ← 本文件
```

大改：

```
src-tauri/src/database.rs            seed / 列 / 索引
src-tauri/src/commands.rs            变薄，调 ops
src-tauri/src/executor.rs            PATH、brief、Miku 合成、skill 落盘
src-tauri/src/conversation.rs        build_prompt 形状
src-tauri/src/main.rs                注册不变；bin 是另一入口
src-tauri/Cargo.toml                 [[bin]] name = "multara"
src/domain/types.ts
src/services/backend.ts
src/views/AgentDetailView.vue
src/views/CollectionView.vue
src/stores/app.ts
src/components/ChatPanel.vue
docs/架构.md                         实现跟上之后再改
```

---

## 13. 测试要求

| 层 | 覆盖 |
|---|---|
| `database` seed | 空库只有 Miku、无小队；跑两次还是一个 |
| `compose_miku_instructions` | notes 空不加 Workspace notes 节；改名后正文用新名字 |
| `delete_agent` | miku 失败；普通 agent 成功 |
| CLI | `task create` JSON 含 id；`todo` 产生 `agent_runs`；`backlog` 不产生 |
| `save_agent` | 不能把 `system_key` 写成 miku |
| 前端 mock | bootstrap 预览数据不再出现 frontend/reviewer |

跑 `cd src-tauri && cargo test`。P1 之后加 CLI 的 `#[cfg(test)]` 用内存 sqlite，不要碰用户 app_data。

---

## 14. 风险

1. **写死 `cursor` 的 seed**：无 Cursor 的机器上 Miku 会 offline。P0 就要「优先 online runtime」。
2. **PATH 注入失败**：部分 CLI 用 login shell 的 PATH，子进程看不到 sidecar。需要测 Cursor / Claude / Codex 三种 spawn。不行就给 `MULTARA_CLI` 绝对路径，brief 里写明用该路径。
3. **SQLite 锁**：UI 持有 Mutex 包着 Connection 时，CLI 另一进程可能忙等。必须 WAL + timeout；长时间事务不要包住 spawn。
4. **任务一对一会话**：用户会感觉「Miku 建任务后多出聊天」。产品上要在 Miku 回复里带「已在任务列表打开」，并考虑任务页入口。这是语义问题，不是 bug。

---

## 15. 完成标准（效果）

打开应用：

1. 智能体列表只有 Miku。
2. 默认聊天是「开始使用 Multara」，对象是 Miku，有产品开场白。
3. 对 Miku 说一个需要改代码的目标 → 她建任务并指派（自己或她刚提议并经确认的 specialist）→ 工作区里出现一次真正的 agent run。
4. 对 Miku 问「这是什么按钮」→ 只聊天，不建任务、不改文件。
5. 用户仍可自己「新建智能体」当 specialist；Miku 不会在未确认时批量创建小队。
