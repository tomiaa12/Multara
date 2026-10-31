# Plan 模式与 Sub-agent 落地方案：按入口和场景逐个展开

本文是《聊天 Plan 模式与 Sub-agent 技术方案.md》的**落地补充**。那份文档定义了数据契约（`RunMode`、`PlanArtifact`、规范事件、能力声明）和三个阶段，这里不重复；本文回答另一个问题：**这些能力要落到哪几个入口、每个入口有什么特殊情况、哪些没有界面的发起路径会被漏掉**。

覆盖入口：**任务**、**右下角聊天**、**需求**、**收件箱**，以及共用同一个聊天面板的**工作台**和**普通聊天页**。

**证据说明**：下文"现状"部分是我读代码核对过的；涉及外部 CLI 实际行为的地方（例如 `claude --permission-mode plan` 在无头模式下怎么表现）我没有运行过，一律标成 **[需验证]**，实施前必须先用真实 CLI 录一份样本。

---

## 1. 现状核对

### 1.1 五个入口其实只有两套界面

| 入口 | 组件链 | 发送方式 | 备注 |
|---|---|---|---|
| 工作台右栏 | `WorkspaceWorkbench` → `ChatPanel compact` | `store.send` | 带会话标签页（可并行多个会话） |
| 右下角气泡 | `App.vue` → `FloatingChat` → `ChatPanel compact session-commands` | `store.send` | `/projects*` 与 `/preview` 路由下隐藏；组件被保留挂载（`retained`），按 `projectId` 重建 |
| 普通聊天页 | `ChatView` → `ChatPanel show-session-list` | `store.send` | 有左侧会话列表 |
| 任务详情 | `TaskDetailView` → `TaskActivity` | `store.send` | **有自己的输入框和一套消息渲染**，不经过 `ChatPanel` |
| 收件箱 | `InboxView` → 有 `taskId` 用 `TaskActivity`，否则 `ChatPanel embedded` | `store.send` | `history-view-only`：只有状态为 `needs_input` / `needs_permission` 时才允许回复 |
| **需求** | 仪表盘"需求"页 → `TaskCreateDialog` → `create_task` | **没有输入框，一次性表单** | 创建即派发第一轮运行，没有聊天阶段 |

重要结论：

1. **`ChatPanel` 一套组件覆盖四个入口，`TaskActivity` 单独一套覆盖任务和收件箱的任务线程，`TaskCreateDialog` 是第三套。** 模式选择器、计划卡片必须能同时接入这三处，否则会出现"聊天里能规划、任务里不行"。
2. 渲染链是共用的：`AssistantMessage` → `ProcessFold` → `timeline-display.ts`。所以 Plan 卡片和 Sub-agent 展示**只在这条链上做一次**，三套入口自动获得；需要分别处理的只有"选择模式"和"审批操作"两处。
3. "需求"就是任务创建：`TaskCreateDialog.create` 调 `store.createTask`，后端 `create_task` → `ops::task::create`，入队后 `executor::dispatch`。用户在创建时没有机会对话，**模式只能在表单里选**。

### 1.2 没有界面的运行发起路径（最容易漏）

同一个 `agent_runs` 表，下面这些路径都会产生运行，但都**不经过输入框**：

| 路径 | 位置 | 对 Plan 模式的影响 |
|---|---|---|
| 小队队长完成后派发给队员 | `executor::dispatch_squad_members`（只在 `parent_run_id` 为空时触发，即**只有一层**） | 队长在 Plan 模式下的输出会被当成"分配方案"派给队员，**队员会真的去执行** |
| 子任务完成后恢复父任务 | `ops::task::maybe_resume_parent` | 父任务若处于待审批计划状态，不应被恢复 |
| 自动化（定时 / webhook） | `execute_automation_source` | 无人值守，没有人可以批准计划 |
| 飞书 / Lark 渠道 | `send_channel_message` → `send_message_inner_ext` | 渠道里没有选择器，需要斜杠命令和卡片按钮 |
| 智能体经 CLI 创建任务 / 恢复会话 | `multara task create`、`session resume` | CLI 没有绑定调用它的运行，Plan 运行中的智能体可以绕过只读限制去创建任务 |
| 重新提问 / 截断 | `truncate_session_from_message` + reask | 重放哪一轮的模式？ |
| 排队、立即发送、转向 | `store.send(priority)`、`executor::steer` | 排队后的模式快照；对运行中的 Plan 轮转向 |
| 压缩历史 | `compact_chat_session` / `generate_text` | 计划卡片是否进入压缩摘要 |

### 1.3 与 Plan 相关的现有代码（核对结果）

- `agent_runs.mode` 列已存在（`database.rs` 的 `ensure_column`），**没有任何代码写它**。
- 发送链路没有 mode 参数：`ChatComposer.vue`、`ChatPanel.vue` 里没有模式选择；`backend.sendMessage` 不带模式。
- Claude 的启动参数固定为 `--permission-mode bypassPermissions`（`runtime.rs` `build_args`）；CodeBuddy 还显式 `--disallowedTools EnterPlanMode / ExitPlanMode`。
- ACP 驱动（`protocol.rs`）：`session/new` 的参数只有 `cwd` 和 `mcpServers`，**没有传 mode**，也没有调用 `session/set_mode`；`current_mode_update` 被解析成 `mode` 事件，但 `normalize_wire` 当前没有对应的规范事件。`plan` 事件已被映射成 `plan`（数据库约束也已放开）。
- `PlanCard.vue` 渲染的是待办列表（`todosFromInput`），用来显示 `TodoWrite`；没有"批准 / 继续完善 / 拒绝"。
- 事件树：`groupTimeline` 只建一层子级，孙级丢失（我已用 A→B→C 的测试复现：`B` 下面没有 `C`）；`ProcessFold.nestedItems` 又会丢掉子块自己的 `children`。
- Sub-agent 没有独立展示：Claude 的 `Task` 工具走通用 `ToolCallRow`。
- 子代理文字被 `normalize_claude` 有意改成 `thinking`，避免覆盖根回复——这个行为要保留。
- 我之前加的时间线加载上限 `MAX_TIMELINE_ITEMS_PER_RUN = 3000` 是按"丢最早的"裁剪，**可能裁掉父 `Task` 调用，让它的所有子级变成孤儿**。
- 熔断：我加的"连续 25 次 `tool_use_error` 就停止"是按整个 run 计数，**不区分根代理和子代理**。

---

## 2. 先修的事：与 Plan 无关、但会让 Sub-agent 展示错的问题

这些是 Sub-agent 展示的前置修复，可以独立先做、风险低、不改数据契约。

### 2.1 事件树：保留全部后代

把 `groupTimeline` 的"边分组边递归"改成"先建完整树，再转展示块"：

1. 过滤出过程事件后，按 `callId` 建 `Map<callId, node>`（`tool_use` 与同 `callId` 的 `tool_result` 合并成一个节点）。
2. 按 `parentCallId` 挂接；**挂接用整个事件集合**，不要用上一层传下来的子集。
3. 异常处理：父不存在 → 放根级；父等于自己 → 放根级；形成环（沿父链走超过节点数）→ 打断并放根级。三种都记录一次 `console.warn` 级别的诊断，不抛错。
4. 子节点按 `seq` 排序；`tool_result` 缺失的节点标记为"未完成"。
5. `ProcessFold` 的 `nestedItems` 不再把子块拍平成事件，直接递归渲染节点（或让节点带完整 `children`）。

测试：A→B→C 三层、四层；缺父；自引用；A→B→A 环；同一父下混有 thinking / 工具 / 权限事件；并行的两个子代理交错事件。

### 2.2 加载上限不能裁掉被引用的父调用

`load_session_timelines` 现在对每个 run 超过 3000 条就丢最早的。改成：先挑出要保留的最近 N 条，再把这些条目引用到的所有祖先（沿 `parent_call_id` 回溯）补回来，并且保留它们对应的 `tool_use` / `tool_result`。同时在被裁剪的 run 上带一个 `truncatedBefore` 标记，界面显示"更早的 X 条过程已折叠"。

### 2.3 熔断按角色计数

`consecutive_tool_errors` 改成按 `parent_call_id`（即按根 / 各子代理）分别计数，任一代理触发才中止；中止时在错误事件里写明是哪个子代理。否则一个子代理的死循环要么被根代理的成功调用"冲淡"，要么错怪根代理。

### 2.4 用量按根归集，避免重复

`usage.rs` 对每个 stdout 行 `ingest`，带 `parent_tool_use_id` 的行也会被计入。需确认 Claude 子代理的 usage 是否已经包含在父级汇总里（**[需验证]**，用一个真实的带 `Task` 的运行对比 `result` 事件里的总量与逐行累加值）。如果重复，只累加根级行；如果不重复，要把子代理用量按 actor 分开记录，供子代理卡片展示。

---

## 3. 运行时能力：每个 provider 怎么落地

先说原则：**不确定的能力一律声明为不支持**，界面灰掉并给出原因；不拿提示词冒充原生 Plan。

| 运行时 | 现状（已核对） | 落地做法 | 待验证 |
|---|---|---|---|
| **Claude** | `--permission-mode bypassPermissions`，stdin 是 stream-json，可转向 | 请求 `plan` 时改用 Claude 自身的 plan 权限模式启动；`agent` 保持现状；`auto` 保持现状（不强行规划） | **[需验证]** 无头 `-p` 下 plan 模式遇到 `ExitPlanMode` 要等人批准，会不会直接结束并输出计划正文；`--permission-mode plan` 的确切取值；plan 模式下 MCP 工具是否仍被允许 |
| **CodeBuddy** | 显式禁用了 Enter/ExitPlanMode | 保持不支持，直到确认它的流里有对应事件；移除禁用项前先做样本验证 | **[需验证]** 是否有等价参数 |
| **ACP 系**（hermes、kimi、kiro、qoder、traecli 等） | `session/new` 不带 mode；`current_mode_update` 已解析；没有 `set_mode` | `session/new` 的应答里取可用模式列表（若有）→ 能力声明里记下哪个 mode id 对应规划；需要时调用 `session/set_mode`；`current_mode_update` 转 `mode_change` | **[需验证]** 各家的 mode id 命名（不要写死 `plan`，取协议返回的列表）；`set_mode` 是否普遍支持 |
| **Codex、Gemini、Copilot、Deveco、OpenClaw、Antigravity** | 提示词走命令行参数 | 第一期声明为不支持 Plan，只允许 `auto` / `agent` | 若某 CLI 有原生只读 / 规划开关，单独验证后再开 |
| **Cursor、Qwen、OpenCode、CodeArts、Pi、Omp** | 提示词走 stdin | 同上，默认不支持 | 同上 |
| **自定义 profile / 网关** | 网关只改变请求路径 | 继承基础 provider 的能力；**网关不增加能力** | — |

关键的"安全"问题：当前所有运行都是绕过权限确认的（`bypassPermissions`、`--yolo` 等）。**Plan 模式在这样的运行时里如果只靠提示词，模型照样能改文件**。所以：

1. 只有当运行时有**原生只读 / 规划权限**（启动参数或协议 `set_mode`）时才声明支持 `plan`。
2. Plan 轮结束后，后端用工作区的 git 状态（或 `turn_file_snapshots`）做一次**事后检查**：如果规划轮改动了工作区，把这一轮标成"规划轮产生了改动"，在卡片上提示用户并允许一键撤销（复用现有的撤销机制）。这是兜底，不是替代原生约束。
3. 不支持 Plan 的运行时被请求 `plan` 时，**后端拒绝并返回明确错误**，前端恢复输入草稿（复用 `sendFailure` 机制）。

---

## 4. 各入口的落地细节

### 4.1 共用部分（做一次，四个入口受益）

| 项 | 做法 |
|---|---|
| 模式选择器 | `ChatModePicker.vue`，由 `ChatComposer` 的 tools 区接入；`TaskActivity` 的输入框同样用 `ChatComposer`，所以**只要 `ChatComposer` 支持，任务页和收件箱的任务线程也自动有** |
| 紧凑布局 | 工作台右栏和右下角气泡宽度窄：选择器在 compact 下只显示图标 + 当前模式一个字，下拉用现有 `UiSelect`；不新增一行占位 |
| 计划卡片 | `PlanArtifactCard.vue`，在 `ProcessFold` 外、正文下方渲染（计划是交付物，不是"过程"，不能被折叠进工作过程里） |
| 待办卡片 | 把现有 `PlanCard.vue` 重命名为 `TodoProgressCard.vue` |
| 模式变化行 | `ModeTransitionRow.vue`，放在过程里 |
| 子代理卡片 | `SubAgentCard.vue`，在过程里递归渲染 |
| 发送 | `store.send(content, sessionId, attachments, { priority, mode })`；`QueuedMessage` 增加 `requestedMode`，**点击发送时冻结** |

### 4.2 右下角气泡聊天

- 宽度很小，计划卡片默认只显示"标题 + 摘要 + 步骤数 + 三个按钮"，完整 Markdown 展开在卡片内部滚动，不弹出新面板。
- 气泡被收起时组件仍然挂载（`retained`），所以**待审批计划在收起期间不会丢**；但用户看不到。需要：气泡图标上显示"有待审批计划"的徽标（复用现有未读 / 运行中指示的位置）。
- 路由进入 `/projects*` 时气泡隐藏，工作台右栏接管。**同一个会话可能在气泡和工作台里各有一个视图**，两边读的是同一个 `store.messages[sessionId]`，所以计划状态天然一致；但**批准按钮必须幂等**（后端按 `planId + revision + action` 校验），否则两处同时点会重复启动执行轮。
- 气泡没有项目时（`projectId` 为空，全局会话）没有工作区：Plan 是只读调研，没有问题；"批准并执行"如果需要工作区而当前没有，应提示选择项目，不静默失败。

### 4.3 任务

- `TaskActivity` 的输入框要接选择器。任务的第一轮来自**创建任务**（见 4.4 需求），后续追问才走输入框。
- 任务有自己的状态机（看板列：七态）。**待审批的计划要映射到现有状态，不新增状态列**：运行结束且带有 `awaiting_approval` 的计划时，任务状态置为"需要输入"（对应收件箱的 `needs_input`），批准后回到进行中。需要核对 `ops::task` 里 `settle_finished` 的结算规则，避免它把"计划已出、等待批准"的任务直接结算成"已完成"。（`settle_finished` 现在按运行状态结算，**这是必须改的点**。）
- 子任务与父任务：`maybe_resume_parent` 会在子任务完成后恢复父任务。父任务若有待审批计划，不应被恢复；子任务本身如果在规划轮结束，**不能算"子任务完成"**。
- 任务详情页同时显示任务描述和线程，计划卡片出现在线程里，不在任务描述区。
- 看板卡片上可显示一个"计划待确认"小标记（可选，第二期）。

### 4.4 需求（任务创建）

这是最容易做错的入口：**创建时没有对话，第一轮就是任务描述。**

- `TaskCreateDialog` 增加"先出计划"开关（对应 `requestedMode: plan | auto`），默认关；该选项跟随所选目标智能体的能力，不支持的灰掉。**默认不要开**：需求通常会挂多个仓库并创建 worktree，规划轮会白白创建隔离工作区；见下。
- `create_task` 的输入增加 `requestedMode`，`ops::task::create` 把它写进第一轮 `agent_runs.mode`，并保存到任务上（`tasks.default_mode` 或复用会话的 `default_mode`），后续追问沿用。
- **规划轮的工作区准备**：需求的隔离模式是 worktree / copy / direct（`workspaces.isolation_mode`）。规划轮只读，建议**规划轮直接使用源码目录只读运行，不创建 worktree**；批准后的执行轮再创建 worktree。这能避免"规划完用户拒绝了，白白留下一堆 worktree"。如果第一期不做这个优化，必须保证拒绝 / 删除任务时 worktree 被清理（`delete_task` 目前不清理草稿目录，见后端审计）。
- 目标是**小队**或 **Miku（总管）**时：队长 / 总管收到 Plan 模式，输出计划；**不能触发 `dispatch_squad_members`**，也不能让 Miku 通过 CLI 去创建子任务（见 4.7）。批准后才真正分发。
- 从需求页创建后会跳转到任务详情或收件箱，用户看到的第一屏要能直接看到计划卡片和批准按钮。
- 批量 / 模板创建需求（如果存在）：每个都带自己的模式快照。

### 4.5 收件箱

- 收件箱项的状态来自运行结果：`needs_input` / `needs_permission` / `completed` 等，列表项和通知文案由 `captureInboxNotice`、`ops::inbox` 生成。新增一种来源：**计划待批准 → `needs_input`**，通知文案用"计划待确认：{标题}"，而不是"已完成"。
- `history-view-only` 目前的规则是：不是 `needs_input` / `needs_permission` 就只读。待审批计划映射成 `needs_input` 后，**批准 / 继续完善 / 拒绝按钮和输入框会自然可用**；批准之后状态回到进行中，再变成 `completed`。
- 已批准、已拒绝、已被替代的计划卡片在收件箱历史里是只读的，按钮禁用并显示最终状态。
- 未读分割线（`unreadMessageId`）定位到的消息可能是计划所在的助手消息：计划卡片要在滚动定位后可见，不被折叠进 `ProcessFold`。
- 子代理树：收件箱是回看场景，**默认全部折叠**，只显示"3 个子代理，2 个完成，1 个失败"的摘要行，点开再展开；避免长树撑满收件箱详情区。
- 收件箱中的 `ChatPanel embedded` 与 `TaskActivity` 两种分支都要验证，因为任务类收件箱项走 `TaskActivity`，普通聊天类走 `ChatPanel`。

### 4.6 工作台

- 工作台右栏有**并行聊天标签页**（每个标签一个会话）：模式选择是**每个会话一份**（`chat_sessions.default_mode`），切换标签时选择器随会话变化，不能是全局状态。
- 计划里的文件路径要可点击并在编辑器里打开：`RichContent` 已经会把路径转成 `workspace-path` 并调用 `revealWorkspaceFile`，计划 Markdown 走同一渲染就自动具备。
- **"批准并执行"与编辑器的联动**：执行轮开始后会产生文件修改，现有的 diff 审阅机制（`file_snapshot`、保留 / 撤销）继续工作，不需要为 Plan 特别处理。
- 规划轮不应触发 `filesBeingWritten` 的只读锁（规划轮没有写文件的工具调用）；如果规划轮意外写了文件，按 3 的兜底提示。

### 4.7 没有界面的路径（必须逐个处理）

| 路径 | 规则 |
|---|---|
| **小队队长 → 队员** | `dispatch_squad_members` 开头增加：本 run 的 `mode == plan` 或存在待批准计划 → 直接返回，不派发。批准后的执行轮（`mode=agent`）结束时才派发。队员也是独立运行，创建时继承 `agent`。 |
| **CLI 创建任务 / 恢复会话**（智能体通过 `multara` 调用） | CLI 需要知道"我是哪个运行"：执行器启动子进程时已设置 `MULTARA_TASK_ID` 等，**再加 `MULTARA_RUN_ID`**；CLI 的写命令（`task create`、`session resume`、`agent update`、`automation create` 等）先查该运行的 `mode`，`plan` 时拒绝并返回"规划模式下不能创建任务"。这同时缓解了"CLI 没有绑定调用方"的问题（见后端审计 S1），但不能替代把密钥移出数据库。 |
| **自动化 / webhook** | 无人值守，没人批准。规则：自动化的 `requestedMode` 只允许 `auto` / `agent`；配置界面不显示 Plan；后端也拒绝。 |
| **飞书 / Lark** | 输入侧：支持斜杠命令 `/plan <内容>` 和 `/agent`，会话默认模式同样适用；输出侧：计划以**文本摘要 + 卡片按钮**回复（批准 / 继续完善 / 拒绝），按钮动作走同一个 Plan action 命令，同样按 `planId + revision + action` 幂等。渠道里已有"owner 才能批准工具权限"的校验，计划批准**复用同一个角色检查**。注意卡片动作重放问题（后端审计已指出 `answer_question` 点击可重放），Plan action 必须幂等。 |
| **重新提问 / 截断** | 重放某条用户消息时，沿用**该条消息保存的 `requested_mode`**；截断掉的轮次里的计划，状态改为 `superseded`；重放到 Plan 轮之前的位置，会话默认模式不自动改。 |
| **排队、立即发送、转向** | 排队消息带自己的模式快照（4.1）；"立即发送"（`priority`）前插队时同样带快照；**转向（steer）**把文字写入运行中的 stdin，不改变模式，在 UI 上给出"当前为规划轮，转向仅补充调研要求"提示；运行时不支持运行中切换模式（多数一次性 CLI 都不支持）时，"立即发送一条执行模式的消息"= **结束当前规划轮并开新一轮**，不能假装切换。 |
| **压缩历史** | 压缩摘要需要保留最新一份计划的标题 / 摘要 / 状态，否则压缩后"批准并执行"丢失依据。计划卡片本身从 `run_events` 渲染，不受压缩影响。 |
| **停止** | 规划轮被停止：没有产出完整计划，不生成 `PlanArtifact`；已流式出现的文字保留（现有"停止保留已有正文"逻辑）；卡片不出现。 |
| **运行失败 / 超时** | 同上；计划状态不变。 |
| **应用重启（`recover_interrupted_runs`）** | 重启时把 `running` 的运行标为失败；如果该运行已经产出过计划事件，则计划保持 `awaiting_approval`（从事件重建），用户仍可批准。 |

---

## 5. Plan 生命周期

沿用原方案的 `PlanArtifact`，这里补**状态机和并发规则**。

```text
            ┌──────────── 继续完善（新 revision）────────────┐
            ▼                                               │
drafting ─► awaiting_approval ─► approved ─► executing ─► executed
                 │                  │            │
                 ├─► rejected       │            └─► failed
                 └─► superseded ◄───┘（被更新的 revision 或截断替代）
```

规则：

1. **状态从事件重建，不单独存**（第一阶段）：`awaiting_approval` = 该 revision 的计划事件存在且没有后续的批准 / 拒绝事件；批准 / 拒绝本身也写成事件（`plan_action`），带 `planId`、`revision`、`action`、`by`（用户 / 渠道发送者）。
2. **幂等键** `planId + revision + action`：后端先查是否已有同键事件，有则直接返回成功但不重复启动；这解决了气泡与工作台同时点击、渠道卡片重放、网络重试。
3. **批准并执行**：创建一条新的用户消息（带 `requested_mode=agent`，内容是结构化引用 `planId/revision`，**界面显示为"批准计划：{标题}"，不要把整个计划正文塞进用户气泡**），并启动执行轮；resume 沿用同一个 CLI 会话 id。执行轮的提示词由后端按 `planId/revision` 取计划正文拼接，前端不拼。
4. **继续完善**：新用户消息 `requested_mode=plan`，携带用户补充意见和上一版 revision 引用；产出的新计划 revision+1，旧版置 `superseded`。
5. **拒绝**：只写 `plan_action(reject)`；不启动运行；任务回到"待办 / 进行中"由用户决定。
6. **多个计划并存**：同一会话允许有多个 plan，每个有独立 `planId`；界面只让**最新一个 `awaiting_approval`** 的卡片有可用按钮，其余置灰。
7. **用户直接在输入框发消息而不点按钮**：视为对当前待审批计划的隐式"继续完善"还是"忽略"？建议：如果当前会话有 `awaiting_approval` 计划，且本条消息的模式是 `plan`，则当作完善；如果是 `agent`，当作"忽略计划自行执行"，计划置 `superseded`。两种都要在发送前给一行提示（不是弹窗）。
8. **规划轮改了工作区**（3 里的事后检查）：在卡片上显示"本轮规划产生了 N 处文件改动"，提供"撤销这些改动"；批准按钮不受影响。
9. **计划 JSON 无效**：保留 Markdown 正文，卡片标"结构化步骤不可用"，不整条丢弃；按钮仍可用（批准依据是 Markdown）。

### 5.1 状态怎么映射到现有界面状态

| 计划状态 | 任务状态 | 收件箱 | 输入框 |
|---|---|---|---|
| awaiting_approval | 需要输入 | `needs_input` | 可用，提示"回复将完善计划" |
| approved / executing | 进行中 | 无变化 | 正常 |
| rejected | 保持原状态 | 项标记已处理 | 可用 |
| superseded / executed | 不影响 | 只读 | 正常 |

---

## 6. Sub-agent 落地

### 6.1 先做展示（native），后做编排（managed）

- **native**：CLI 自己创建的子代理，Multara 只观察。能展示的前提：事件里有可靠的父子关系。Claude 已经有（`parent_tool_use_id`）；其他运行时 **[需验证]** 各自是否有等价字段。没有就不假装有树。
- **managed**：Multara 自己创建的子运行。**小队已经是一个一层的 managed 子运行**（队长 → 队员，`parent_run_id`），这套已有展示是"多条助手消息 + 说话人"。**不要把它和 native 子代理合并成同一个视觉概念**：小队成员是独立的、有名字的智能体，消息各占一条；native 子代理是一个工具调用内部的过程。两者可以并存，但文案要区分（"队员"与"子代理"）。

### 6.2 展示规则（所有入口共用）

| 场景 | 规则 |
|---|---|
| 层级 | 0–3 层正常缩进；更深保持同一缩进宽度，用深度徽标（`L4`）；每个节点可独立折叠 |
| 运行中 | 子代理节点默认**展开最后一个正在运行的**，其它折叠；完成后整体折叠，只留摘要 |
| 摘要行 | "子代理：{描述} · 完成 · 12 次工具调用 · 34 秒"；失败显示失败原因首行 |
| 实时状态行 | 聊天底部的"工作中"行（`liveWorkParts`）要体现"N 个子代理运行中"，否则用户会以为卡住 |
| 并行子代理 | 同一父下多个 `Task` 并行：按 `seq` 排序，各自独立状态，不合并 |
| 子代理文字 | 保持 `thinking`，不进正文；子代理的**最终结果**以 `tool_result` 呈现在卡片的"结果"区 |
| 失败 | 子代理失败只标记该节点；父运行是否失败由父运行最终结果决定 |
| 右下角 / 工作台窄屏 | 缩进最大 3 层 × 8px；卡片内部横向不滚动，长文本换行 |
| 收件箱 / 回看 | 默认全折叠，仅显示摘要行 |
| 大树性能 | 折叠节点不渲染子内容（`ProcessFold` 已有"展开才渲染"），上千个节点也只渲染可见层；树构建用 `Map`，O(n) |
| 停止 | native 子代理无法单独停止，卡片上**不放停止按钮**，只有根运行的停止；managed 子运行（后续阶段）才可单独停止 |

### 6.3 managed 子运行（第三阶段）要先回答的问题

1. **调度死锁**：父进程等待子运行结果，如果子运行排队等不到并发槽（父占着槽），会死锁。需要独立的 nested 配额（原方案已写），并且 `spawn` 在配额满时**立即报错而不是排队**。
2. **深度**：现在 `dispatch_squad_members` 有 `parent_run_id.is_some()` 就返回，**天然只允许一层**。支持递归要同时改这里，并给根运行记录 `root_run_id`、`depth`，所有限制按根计数。
3. **权限继承**：子运行的权限不得高于父运行；父是 `plan` 则子只能 `plan`（且只读）。
4. **取消**：停根运行 = 递归取消所有后代，包括进程组；这依赖我们之前审计里提到的"正常退出路径不杀进程组"问题先修好。
5. **费用 / 用量**：按 `root_run_id` 汇总。
6. **CLI 入口**：`multara agent spawn` 需要 `MULTARA_RUN_ID` 校验调用方，并限制只能创建当前根运行的后代。

---

## 7. 数据与迁移清单（相对原方案的补充）

原方案已列：`chat_sessions.default_mode`、`messages.requested_mode`、`agent_runs.mode`（已有）、事件类型扩展。补充：

| 变更 | 原因 |
|---|---|
| `tasks.default_mode`（或复用会话的） | 需求入口创建时就要保存，后续追问沿用 |
| 事件类型 `plan_action` | 批准 / 拒绝 / 完善需要落库，做幂等和历史重建；`run_events` 的 CHECK 要再放开一次（这次迁移记得放进事务，保持外键开启，参考上次迁移的测试写法） |
| `MULTARA_RUN_ID` 环境变量 | CLI 判断调用方运行的模式 |
| 运行时能力表 | Rust 端常量 + 前端 bootstrap 返回；不要写进数据库 |
| `agent_runs.root_run_id`、`depth`（第三阶段） | managed 子运行的限制与汇总 |
| 时间线 `truncatedBefore` | 2.2 的"已折叠"提示 |

---

## 8. 实施顺序和每步的验收

**阶段 A：不依赖外部 CLI 的修复（1–2 天量级，风险低）**

| 步骤 | 内容 | 验收 |
|---|---|---|
| A1 | 完整事件树（2.1），含渲染递归 | 单测：三层、四层、缺父、环、自引用；界面里三层 `Task` 全部可见 |
| A2 | 加载上限保留祖先（2.2） | 单测：超限 run 里父 `Task` 不丢 |
| A3 | 熔断按角色计数（2.3） | Rust 单测 |
| A4 | `PlanCard` 重命名为 `TodoProgressCard`，新增 `SubAgentCard`（名称、状态、耗时、摘要、折叠） | 组件测试 + 四个入口目测 |
| A5 | 用量归集核对（2.4） | 用真实运行对比总量 |

**阶段 B：契约与能力（需要真实 CLI 样本）**

| 步骤 | 内容 | 验收 |
|---|---|---|
| B0 | **录样本**：Claude plan 模式、一个 ACP 运行时的 mode/plan 事件、带嵌套 `Task` 的 Claude 运行，存成测试夹具 | 夹具入库；这一步决定后面所有"[需验证]"项 |
| B1 | `RuntimeCapabilities` 与各 provider 声明（3） | 单测：每个 provider 的 `requested_modes` 与 `build_args` 一致；请求不支持的模式被后端拒绝 |
| B2 | `normalize_*` 增加 `mode_change` / `plan` / `todo` / `agent_start` / `agent_end` | 夹具回放测试 |
| B3 | `run_events` CHECK 再迁移 + `chat_sessions.default_mode` + `messages.requested_mode` | 迁移测试（旧库升级后可写入所有新事件） |
| B4 | `send_message` / `create_task` / 渠道 / 队列带 `requestedMode` 并冻结 | 单测：排队后切换模式不影响已排队消息 |

**阶段 C：用户控制与入口**

| 步骤 | 内容 | 验收 |
|---|---|---|
| C1 | `ChatModePicker` 接入 `ChatComposer`（覆盖聊天页、工作台、气泡、任务页、收件箱线程） | 五处都能选，且按能力置灰 |
| C2 | `TaskCreateDialog` 的"先出计划"（4.4） | 创建后第一轮为规划轮；小队 / Miku 目标不触发分发 |
| C3 | `PlanArtifactCard` + 三个动作 + 幂等 + 状态机（5） | 重复点击只执行一次；气泡与工作台同时点只执行一次 |
| C4 | 无界面路径的限制（4.7）：小队分发、CLI（`MULTARA_RUN_ID`）、自动化拒绝 Plan | Rust 单测：Plan 运行中 CLI 写命令被拒、队长计划不触发分发 |
| C5 | 任务状态 / 收件箱映射（5.1）；`settle_finished` 不把待批计划结算成完成 | 集成测试 |
| C6 | 渠道 `/plan` 与卡片按钮 | 手动 + 单测（幂等、owner 校验） |
| C7 | 规划轮改动检测与撤销提示 | 构造一个写文件的规划轮 |

**阶段 D：managed 子运行**（单独立项，依赖进程组清理等前置修复；见 6.3）

---

## 9. 测试矩阵

入口 × 场景，每格都要有至少一个自动化或手动用例：

| 场景 \ 入口 | 聊天页 | 工作台 | 右下角 | 任务 | 需求（创建） | 收件箱 |
|---|---|---|---|---|---|---|
| 选择 Plan 并发送 | ✓ | ✓ | ✓ | ✓ | ✓（开关） | ✓（回复） |
| 运行时不支持 Plan | 置灰+原因 | 同 | 同 | 同 | 开关置灰 | 同 |
| 计划卡片出现、折叠不吞掉 | ✓ | ✓ | ✓（窄屏） | ✓ | 跳转后可见 | ✓ |
| 批准并执行（幂等） | ✓ | ✓ | ✓ | ✓ | — | ✓ |
| 继续完善 → revision+1，旧卡置灰 | ✓ | ✓ | ✓ | ✓ | — | ✓ |
| 拒绝 | ✓ | ✓ | ✓ | ✓ | — | ✓ |
| 刷新 / 重启后恢复 | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| 三层以上子代理不丢 | ✓ | ✓ | ✓ | ✓ | ✓ | ✓（默认折叠） |
| 小队队长规划不触发分发 | ✓ | ✓ | ✓ | ✓ | ✓ | — |
| Plan 运行中 CLI 创建任务被拒 | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| 排队 / 立即发送 / 转向时的模式 | ✓ | ✓ | ✓ | ✓ | — | ✓ |
| 停止规划轮 | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| 重新提问 / 截断后计划状态 | ✓ | ✓ | ✓ | ✓ | — | ✓ |
| 自动化 / webhook 请求 Plan 被拒 | — | — | — | — | — | — |
| 飞书 `/plan`、卡片批准 | — | — | — | — | — | — |

至少用真实的 Claude、一个 ACP 运行时、一个不支持 Plan 的运行时各跑一遍端到端。

---

## 10. 风险与需要你拍板的决策

| # | 问题 | 我的建议 |
|---|---|---|
| 1 | 规划轮是否使用 worktree？ | 规划轮只读使用源码目录，批准后再建 worktree；第一期做不到就必须保证拒绝 / 删除时清理 |
| 2 | 用户在有待审批计划时直接发 `agent` 模式消息，算什么？ | 视为"忽略计划自行执行"，计划置 `superseded`，发送前一行提示 |
| 3 | 不支持原生 Plan 的运行时要不要做"规划提示词"降级？ | 第一期不做；做的话必须标"规划提示词"，且不能叫 Plan 模式 |
| 4 | 自动化是否允许先出计划再由人批准？ | 第一期不允许；后续可做成"计划进收件箱等人批准"的工作流 |
| 5 | 小队成员（一层 managed）与 native 子代理的视觉关系 | 保持两个概念，文案区分，不合并 |
| 6 | 待审批计划映射为 `needs_input` 是否会让收件箱出现语义偏差？ | 建议接受，并在通知文案里写明"计划待确认"；比新增状态列改动小得多 |
| 7 | `MULTARA_RUN_ID` + CLI 校验能否同时作为后端审计里"智能体可直接写库"的缓解？ | 只能缓解 CLI 路径；智能体仍可直接打开 SQLite，真正解决要把密钥移出数据库并让 CLI 走带令牌的本地 IPC，另行立项 |

## 11. 本文没有核对的

- 所有外部 CLI 的实际行为（Claude plan 模式、各 ACP 家的 mode id、`set_mode` 支持度）都没有运行过，阶段 B0 的样本录制就是为此。
- `ops::task` 的七态看板各列的确切名称，以及 `settle_finished` 完整规则，我只读过局部，C5 之前要完整读一遍。
- 飞书卡片动作的现有幂等实现细节没有读；4.7 的渠道部分按后端审计里的结论写。
- 我没有评估各入口的视觉稿，4.2、4.5 里的布局是按现有组件约束推的，需要设计确认。
