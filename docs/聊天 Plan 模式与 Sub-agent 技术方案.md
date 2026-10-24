# 聊天 Plan 模式、计划卡片与 Sub-agent 技术方案

## 1. 目标

在 Multara 的普通聊天、收件箱、右下角气泡聊天和工作台聊天中，补齐以下能力：

1. 用户在发送前选择「自动 / 规划 / 执行」模式。
2. 支持模型在运行过程中自主进入或退出 Plan 模式，并把状态变化实时展示出来。
3. 将模型产生的计划作为结构化卡片持久化、预览和继续执行。
4. 区分「计划」与执行阶段的 Todo，避免把 `TodoWrite` 误当作完整计划。
5. 展示运行时原生 Sub-agent 的执行树，并支持任意层级嵌套。
6. 后续允许 Multara 管理的智能体创建子运行，同时限制深度、并发和权限。
7. 不支持某项能力的运行时必须明确降级，不能让界面显示一个实际无效的开关。

本文是实现方案，不表示这些能力已经完成。

---

## 2. 当前实现与缺口

### 2.1 已有基础

- 工作台和气泡聊天复用 `ChatPanel.vue`；普通聊天也使用同一面板。
- 收件箱根据数据类型使用 `ChatPanel.vue` 或 `TaskActivity.vue`。
- 助手消息统一经过 `AssistantMessage.vue`、`ProcessFold.vue` 和 `timeline-display.ts`。
- Rust 已能持久化 `run_events`，前端通过 `run-event` 接收增量。
- `protocol.rs` 已能从 ACP 更新中提取：
  - `plan`
  - `available_commands_update`
  - `current_mode_update`
- `executor.rs` 已能识别 wire 协议的 `plan` 事件。
- `PlanCard.vue` 已能显示 Todo 列表。
- Claude 流中的 `parent_tool_use_id` 已写入 `parent_call_id`。
- `ProcessFold.vue` 已有一层子调用嵌套展示。
- `agent_runs` 已有 `parent_run_id`，并已预留 `mode` 字段。

### 2.2 主要缺口

1. 输入区没有模式选择器，发送接口也没有携带本轮模式。
2. ACP 的 `mode` 和 `commands` 事件在 `normalize_wire` 中被丢弃。
3. Claude、CodeBuddy 等运行时没有统一的模式能力声明；CodeBuddy 当前还明确禁用了 `EnterPlanMode` 和 `ExitPlanMode`。
4. `PlanCard` 实际展示的是 Todo，不是可审批、可继续执行的计划产物。
5. Plan、permission 等事件已经可能被解析，但新建数据库中的 `run_events.event_type` CHECK 仍只允许 `text/thinking/tool_use/tool_result/error`，需要先做表迁移。
6. `groupTimeline()` 只把直接子事件传给下一层递归。孙级事件虽然已落库，但不会进入子层分组，无法可靠展示多层 Sub-agent。
7. 原生 Sub-agent 只是普通工具行，没有代理名称、状态、耗时、结果摘要和独立折叠头。
8. 模式、计划和子代理状态没有稳定的数据契约，历史消息恢复后无法完整还原。

---

## 3. 概念边界

实现前必须拆开四个概念：

### 3.1 运行模式 `RunMode`

决定这一轮允许模型做什么：

- `auto`：运行时自行决定是否先规划。
- `plan`：只研究和产出计划，不修改项目、不执行有副作用的操作。
- `agent`：允许执行计划、修改文件和运行命令。

界面文案使用「自动 / 规划 / 执行」，协议和数据库使用稳定枚举 `auto | plan | agent`。

### 3.2 模式状态 `ModeTransition`

表示运行过程中真实发生的模式变化，例如模型调用 `EnterPlanMode`，或 ACP 返回 `current_mode_update`。

它是运行事件，不等同于用户发送前选择的期望模式：

- `requestedMode`：用户要求的模式。
- `effectiveMode`：运行时实际启动模式。
- `currentMode`：运行中的最新模式。

### 3.3 计划产物 `PlanArtifact`

模型在规划阶段交付给用户审阅的结果。它至少包含：

- 标题、摘要、Markdown 正文。
- 结构化步骤。
- 风险和待确认项。
- 状态：草稿、等待确认、已批准、已拒绝、已执行、已替代。

计划是可持久化、可预览、可批准的产物。

### 3.4 执行 Todo `ExecutionTodo`

模型执行过程中的任务清单，例如 Claude `TodoWrite`。它只表达当前执行进度，不代表模型已经进入 Plan 模式，也不能替代 `PlanArtifact`。

前端应拆成：

- `PlanArtifactCard.vue`：计划正文与批准操作。
- `TodoProgressCard.vue`：执行 Todo。

现有 `PlanCard.vue` 可重命名为 `TodoProgressCard.vue`，避免语义继续混淆。

---

## 4. 总体架构

```text
ChatComposer / TaskActivity
  └─ ChatModePicker
       └─ send_message(requestedMode)
            └─ agent_runs.mode
                 └─ RuntimeCapabilities + RuntimeAdapter
                      ├─ 启动参数 / ACP set_mode
                      └─ 流式输出
                           └─ NormalizedRunEvent
                                ├─ mode_change
                                ├─ plan
                                ├─ todo
                                ├─ agent_start / agent_end
                                └─ 原有事件
                                     ├─ SQLite run_events
                                     ├─ Tauri run-event
                                     └─ AssistantMessage
                                          ├─ ModeTransitionRow
                                          ├─ PlanArtifactCard
                                          ├─ TodoProgressCard
                                          └─ SubAgentTree
```

核心原则：

1. 运行时差异只留在 Rust adapter，Vue 不判断 Claude/Cursor/CodeBuddy。
2. 数据库存规范事件，不存某个 CLI 的原始字段名称。
3. 实时事件与历史加载使用同一结构，刷新后显示结果不能变化。
4. 计划、Todo、Sub-agent 都挂在具体 run 下，不做会话级全局猜测。

---

## 5. 运行时能力层

### 5.1 能力声明

在运行时目录模型中增加：

```rust
pub struct RuntimeCapabilities {
    pub requested_modes: Vec<RunMode>,
    pub observable_mode_changes: bool,
    pub interactive_mode_switch: bool,
    pub structured_plan: bool,
    pub native_subagents: bool,
    pub nested_subagents: bool,
}
```

前端 bootstrap 返回相同字段。模式选择器只展示当前目标运行时真正支持的模式。

能力含义：

- `requested_modes`：启动新一轮时可以要求的模式。
- `observable_mode_changes`：能否从流中确认模型实际切换了模式。
- `interactive_mode_switch`：运行中能否通过协议切换，而不重启进程。
- `structured_plan`：能否输出规范 Plan 事件。
- `native_subagents`：CLI 是否可能自主创建子代理。
- `nested_subagents`：流协议能否提供完整父子关系。

能力必须来自运行时适配与版本探测，不根据工具名字猜测。自定义 profile 继承基础 provider 的能力；网关只改变模型请求路径，不自动增加 CLI 能力。

### 5.2 启动适配

统一入口：

```rust
fn build_launch(
    runtime: &RuntimeLaunch,
    assignment: &str,
    requested_mode: RunMode,
) -> Result<LaunchSpec, String>;
```

每个 adapter 负责把规范模式转换成已验证的 CLI 参数或协议请求。

注意事项：

- Claude 当前使用 `--permission-mode bypassPermissions`。Plan 模式不能继续无条件复用该值，需由 Claude adapter 根据模式选择启动参数。
- CodeBuddy 当前通过 `--disallowedTools` 禁用了 `EnterPlanMode` 和 `ExitPlanMode`。只有在确认其流协议能返回模式和计划后才能移除。
- ACP 类运行时优先使用协议提供的 mode 列表和 `set_mode` 能力。
- 不支持原生 Plan 的运行时第一期只允许 `auto/agent`，不要用一段 prompt 伪装成“已支持 Plan”。
- 若后续提供 prompt-only 降级，界面必须标记为「规划提示词」，不能显示为原生 Plan 模式。

### 5.3 运行中的切换

Multara 当前多数 CLI 是一次性无头进程。运行中的手动切换按能力处理：

- `interactive_mode_switch=true`：调用 adapter 的 `set_mode(run_id, mode)`。
- 不支持交互切换：模式选择只影响下一条消息；当前 run 继续完成。
- 用户在运行中点击「执行计划」时，结束当前规划轮次，以同一会话的 resume id 启动新的 `agent` 轮次。

不能通过修改前端状态假装当前 CLI 已切换。

---

## 6. 数据模型

### 6.1 会话默认模式

`chat_sessions` 增加：

```sql
default_mode TEXT NOT NULL DEFAULT 'auto'
```

只保存用户对该会话下一轮的默认选择。

### 6.2 消息与队列快照

模式必须在点击发送时冻结，不能在排队后跟随会话设置变化：

```ts
interface QueuedMessage {
  // 原字段
  requestedMode: "auto" | "plan" | "agent"
}
```

`send_message` 输入增加 `requestedMode`。用户消息增加 `requested_mode` 字段，用于历史回放；`agent_runs.mode` 保存本轮最终采用的模式。

已有 `agent_runs.mode` 直接使用，不再新增同义列。

### 6.3 规范事件

扩展时间线事件：

```ts
type TimelineEventType =
  | "text"
  | "thinking"
  | "tool_use"
  | "tool_result"
  | "error"
  | "permission"
  | "mode_change"
  | "plan"
  | "todo"
  | "agent_start"
  | "agent_end";
```

`TimelineItem` 增加可选字段：

```ts
interface TimelineItem {
  // 原字段
  parentCallId?: string | null
  actorId?: string | null
  actorName?: string | null
  actorType?: "root" | "native_subagent" | "managed_subagent" | null
  depth?: number | null
}
```

模式事件 input：

```json
{
  "from": "agent",
  "to": "plan",
  "source": "user | model | runtime",
  "reason": ""
}
```

计划事件 input：

```json
{
  "planId": "uuid",
  "revision": 1,
  "title": "实现聊天 Plan 模式",
  "summary": "先统一运行时能力，再完善 UI",
  "markdown": "...",
  "steps": [
    { "id": "runtime", "text": "增加能力声明", "status": "pending" }
  ],
  "risks": [],
  "questions": [],
  "status": "awaiting_approval"
}
```

Todo 事件 input：

```json
{
  "todos": [
    { "id": "runtime", "text": "增加能力声明", "status": "in_progress" }
  ]
}
```

### 6.4 数据库迁移

必须先迁移 `run_events` 的 `event_type` 约束。SQLite 不能直接修改 CHECK，需要：

1. 新建 `run_events_v2`，包含完整事件枚举和现有所有列。
2. 复制旧数据。
3. 删除旧表并重命名。
4. 重建 `(run_id, seq)` 唯一索引。
5. 在事务中完成，并增加迁移测试。

同时增加：

```sql
ALTER TABLE chat_sessions ADD COLUMN default_mode TEXT NOT NULL DEFAULT 'auto';
ALTER TABLE messages ADD COLUMN requested_mode TEXT;
```

Plan 第一阶段继续以 `run_events` 为唯一事实来源，不增加 plans 表。只有需要跨消息检索、版本对比或独立编辑计划时，再引入 `plan_artifacts` 表。

---

## 7. 协议归一化

### 7.1 ACP / wire

`protocol.rs` 已输出 `plan`、`commands`、`mode`，需要在 `normalize_wire()` 中补齐：

- `mode` → `mode_change`
- `plan` → `plan`
- `commands` 不进消息时间线，更新该 run 的动态能力或命令缓存

ACP Plan 的 `entries` 要在 Rust 归一化为统一 `steps`，不要让 Vue 同时兼容 `entries/todos/content`。

### 7.2 Claude / CodeBuddy

归一化以下来源：

- `EnterPlanMode` / `ExitPlanMode` tool use → `mode_change`
- `TodoWrite` → `todo`
- 最终计划正文或专用 Plan 事件 → `plan`
- `Task` / `Agent` tool use → `agent_start`
- 对应 tool result → `agent_end`
- `parent_tool_use_id` → `parent_call_id`

如果只看到 Enter/Exit 工具但没有独立计划结构：

1. 模式变化仍正常展示。
2. 最终规划回复可包装成最小 `PlanArtifact`，`markdown` 取该规划轮最终文本。
3. 不从普通执行回复自动猜计划。

### 7.3 其他 CLI

每个 `normalize_*` 只做明确映射。未知事件保留为通用 tool，不通过工具名称包含 `plan` 或 `agent` 就强行升级成规范事件。

### 7.4 事件序列

每个 run 的 `seq` 必须全局单调递增，包括子代理事件。若运行时只在各子流内部提供序号，由 executor 收到后重新编号。

同一 `call_id` 的增量事件继续合并，但父关系不得在合并时丢失。

---

## 8. Plan 交互

### 8.1 输入区

新增独立组件 `src/components/chat/ChatModePicker.vue`，由 `ChatComposer` 的 tools slot 接入。

交互：

- 默认「自动」。
- 展开后选择「自动 / 规划 / 执行」。
- 运行时不支持的选项置灰，并显示原因。
- 当前消息已排队后，切换只影响后续消息。
- 会话切换后读取该会话的 `defaultMode`。
- 历史用户气泡只显示当时模式，不允许修改。

四个入口：

- `ChatPanel`：完整选择器。
- 工作台 `ChatPanel compact`：紧凑图标 + 当前模式文字。
- `FloatingChat`：与 compact 一致。
- `TaskActivity`：接入同一个组件，不复制模式判断。
- 收件箱只读状态不显示选择器；允许回复时显示。

### 8.2 模式变化提示

新增 `ModeTransitionRow.vue`：

- 「已进入规划模式」
- 「已切换到执行模式」
- 可选展示来源：用户指定 / 模型自主 / 运行时恢复

模式变化属于工作过程，运行结束后随 `ProcessFold` 折叠。

### 8.3 计划卡片

新增 `PlanArtifactCard.vue`：

- 标题、摘要、步骤数量、风险数量。
- 默认展示摘要和步骤预览。
- 展开后渲染完整 Markdown。
- 显示修订号和状态。
- 操作：
  - 「批准并执行」
  - 「继续完善」
  - 「拒绝」
  - 「复制计划」

按钮行为：

- 批准并执行：发送一条结构化内部指令，引用 `planId/revision`，本轮模式固定为 `agent`。
- 继续完善：模式固定为 `plan`，携带用户补充意见。
- 拒绝：只更新计划状态，不自动启动执行。
- 计划已被新 revision 替代后，旧卡片按钮禁用。

必须防止重复点击：后端以 `planId + revision + action` 做幂等校验。

### 8.4 Todo 卡片

`TodoProgressCard.vue` 只展示执行进度：

- 取该 run 最新一次 todo snapshot。
- 历史更新可在过程详情中查看，消息主区域只展示最新状态。
- Todo 完成不会自动把 Plan 标记为执行完成；应在根 run 完成后由后端更新对应 Plan 状态。

---

## 9. Sub-agent

### 9.1 两类子代理

必须区分：

1. `native_subagent`：Claude/Cursor 等 CLI 在自身进程里创建，Multara 只观察事件。
2. `managed_subagent`：父智能体通过 Multara 显式创建一条新的 `agent_runs` 子运行。

第一期先完善 native 展示；managed 子运行放到后续阶段，不能把普通小队消息冒充成 Sub-agent。

### 9.2 修复任意层级事件树

当前 `groupTimeline()` 递归时只传直接孩子，孙级事件会丢失。改为先构建完整节点索引：

```ts
interface EventTreeNode {
  item: TimelineItem
  children: EventTreeNode[]
}
```

算法：

1. 用 `callId` 建节点 Map。
2. 第二遍按 `parentCallId` 挂接，保留全部后代。
3. 父不存在、父等于自身或形成环时放到根级，并记录诊断信息。
4. 对每个节点的 children 按 `seq` 排序。
5. 渲染阶段递归转换节点，不再对子数组重新猜父关系。

UI 不直接无限缩进：

- 0–3 层正常缩进。
- 超过 3 层保持同一缩进宽度，用层级线和 `L4` 等标识深度。
- 每个代理节点可独立折叠。

### 9.3 子代理卡片

新增 `SubAgentCard.vue`：

- 代理名称或类型。
- 任务说明。
- 状态：排队、运行、完成、失败、取消。
- 运行时间。
- 子工具数量、改动文件数量。
- 结果摘要。
- 展开后显示该代理自己的 `ProcessFold`。

子代理正文是父代理的中间产物，不得覆盖根助手消息正文。当前 Claude 归一化已有这项处理，需要保留。

### 9.4 Multara 管理的子运行

第二阶段提供受控命令，例如：

```text
multara agent spawn --agent <id> --task <text> --parent-run <id>
```

后端创建新的 `agent_runs`：

- `parent_run_id` 指向父 run。
- 与父 run 共用 `session_id` 和 `turn_id`。
- 可指定已有智能体，不允许临时注入任意命令。
- 结果以结构化 tool result 返回父智能体。

限制：

- 默认最大深度 3，可配置上限不超过 5。
- 单个根 run 默认最多创建 8 个后代。
- 单个根 run 同时运行的子代理默认最多 3 个。
- 子代理继承父运行可见工作区，不得扩大文件系统范围。
- MCP、Skills 和环境变量按目标智能体自身配置加载，密钥不写入事件。
- 停止根 run 时递归取消所有 managed 子 run。
- native 子代理无法单独停止时，界面只提供停止根 run。

调度时必须避免父进程等待子运行造成并发死锁。managed 子运行使用独立的 nested concurrency 配额，不占普通用户任务并发槽；达到上限时 spawn 立即返回明确错误，不能无限排队让父进程永久等待。

---

## 10. 状态恢复与错误处理

### 10.1 刷新恢复

历史消息加载必须从 `run_events` 重建：

- 最新 effective/current mode。
- Plan revision 与状态。
- 最新 Todo snapshot。
- 完整 Sub-agent 树。

不得依赖 Pinia 中尚未持久化的临时状态。

### 10.2 Resume

- 规划轮批准后优先沿用运行时 conversation id。
- 新 run 的 `mode=agent`，不能因为 resume 恢复成旧 Plan 模式。
- 若运行时报告实际仍为 Plan，应显示 mode transition，并阻止界面提前标记为执行中。

### 10.3 降级与异常

- 运行时不支持 Plan：后端拒绝 `requestedMode=plan`，前端显示原始错误并恢复草稿。
- 计划 JSON 无效：保留 Markdown 正文，卡片标记「结构化步骤不可用」，不能整条丢弃。
- 父调用缺失：子事件显示在根级「未关联的子任务」分组。
- Sub-agent 流中断：节点标为失败，不自动把父 run 判失败；由父运行最终结果决定。
- 数据库事件类型未知：历史加载降级成通用工具事件，避免整个消息加载失败。

---

## 11. 安全与权限

Plan 模式必须由后端约束，不能只靠提示词：

1. 支持运行时原生只读/Plan 权限时，使用原生能力。
2. 不支持时不宣称原生 Plan。
3. `PlanArtifactCard` 中的 Markdown 按现有 `RichContent` 消毒流程渲染。
4. 计划里的命令、链接和文件路径只是预览，不自动执行。
5. 「批准并执行」只启动已选智能体的新一轮，不执行计划文本中提取出的 shell。
6. managed 子代理不能修改运行时、工作区、智能体或密钥配置。
7. 所有深度、数量和并发限制在 Rust 再校验，前端限制只用于交互提示。

---

## 12. 文件改动范围

### 12.1 Rust

- `src-tauri/src/database.rs`
  - 扩展 `run_events` 事件类型。
  - 增加会话和消息模式字段。
- `src-tauri/src/models.rs`
  - `RunMode`、运行时能力、扩展 `TimelineItem`。
- `src-tauri/src/runtime.rs`
  - 各运行时能力声明和模式启动参数。
- `src-tauri/src/protocol.rs`
  - ACP mode/plan/commands 规范化前置处理。
- `src-tauri/src/executor.rs`
  - 统一 `NormalizedRunEvent`。
  - 模式、Plan、Todo、Sub-agent 事件归一化与持久化。
- `src-tauri/src/ops/run.rs`
  - 保存 requested/effective mode。
  - 后续创建 managed child run。
- `src-tauri/src/commands.rs`
  - 发送输入增加 mode。
  - Plan action command。
  - 可选的 active run `set_mode` command。

### 12.2 前端

- `src/domain/types.ts`
  - mode、capabilities、Plan 和 Sub-agent 类型。
- `src/services/backend.ts`
  - 扩展发送、Plan action、set mode 接口。
- `src/stores/app.ts`
  - 会话默认模式、排队消息模式快照、Plan action。
- `src/components/chat/ChatModePicker.vue`
- `src/components/chat/ModeTransitionRow.vue`
- `src/components/chat/PlanArtifactCard.vue`
- `src/components/chat/TodoProgressCard.vue`
- `src/components/chat/SubAgentCard.vue`
- `src/components/chat/timeline-display.ts`
  - 改成完整事件树构建和展示模型转换。
- `src/components/ChatPanel.vue`
  - 接入模式选择。
- `src/views/TaskActivity.vue`
  - 复用同一模式选择和 Plan action。
- 五套 locale
  - 补齐所有模式、计划、子代理和错误文案。

不要分别为四种聊天入口实现四套 Plan 逻辑。

---

## 13. 实施阶段

### 第一阶段：数据契约与原生展示

1. 修复 `run_events` CHECK 迁移。
2. 增加运行时能力声明。
3. 贯通 requested/effective/current mode。
4. 接通 ACP `mode` 与 `plan`。
5. 拆分 Plan 和 Todo 卡片。
6. 修复任意深度事件树。
7. 完善 native Sub-agent 卡片。

完成后具备：能正确识别、持久化和展示运行时已有能力。

### 第二阶段：用户控制

1. 四种聊天入口接入 `ChatModePicker`。
2. 排队消息冻结模式。
3. Plan 批准、继续完善、拒绝。
4. 不支持运行时的明确降级。
5. Resume 后模式校验。

完成后具备：用户可完成「规划 → 审阅 → 执行」闭环。

### 第三阶段：Managed Sub-agent

1. 增加创建子运行命令。
2. 子运行调度、并发配额和递归取消。
3. 子运行结果回传父智能体。
4. 跨运行的 Sub-agent 树展示。

完成后再宣称 Multara 自身支持 Sub-agent 编排；此前只能称为“展示运行时原生子代理”。

---

## 14. 测试方案

### 14.1 Rust 单测

- 数据库从旧 schema 迁移后可写入所有新事件。
- 不合法 mode 被拒绝。
- 每个 runtime capability 与 adapter 行为一致。
- ACP `current_mode_update` 转成 `mode_change`。
- Claude Enter/Exit Plan、TodoWrite、Task/Agent 正确归一化。
- 子代理文字不会覆盖根回复。
- Plan action 幂等。
- managed 子运行深度、数量、并发和递归取消。

### 14.2 前端单测

- 模式选择器按 capability 禁用选项。
- 排队后切换模式不改变已排队消息。
- Todo 不再被渲染成 PlanArtifact。
- Plan revision 更新后旧按钮禁用。
- 三层以上 parentCallId 树不丢节点。
- 缺父、环、自引用不会造成递归崩溃。
- 子代理失败不隐藏父代理后续事件。

### 14.3 组件与端到端

四个入口分别验证：

1. 普通聊天选择 Plan 并发送。
2. 工作台收到计划卡片，批准后进入执行。
3. 气泡聊天收起再打开，模式和卡片不丢。
4. 收件箱刷新后恢复 Plan 和 Sub-agent 树。
5. 运行时不支持 Plan 时，选择器不可选且原因可见。
6. 根 run 停止后所有 managed 子 run 被取消。

至少使用 Claude 原生流、一个 ACP runtime 和一个不支持 Plan 的 runtime 做真实集成测试。

---

## 15. 验收标准

- 用户发送前能看到当前模式，并能确认该运行时是否真正支持。
- 发送时选择的模式在排队、执行、刷新和历史记录中保持一致。
- 模型自主切换模式时，界面实时显示，刷新后仍存在。
- Plan 是可展开、可复制、可批准的独立卡片；Todo 只显示执行进度。
- 批准计划后启动执行模式的新一轮，并引用准确的 plan revision。
- 原生 Sub-agent 的父子关系、状态和结果可见。
- 至少三层 Sub-agent 嵌套不丢事件、不无限缩进、不覆盖根回复。
- 不支持的能力不会静默伪装成功。
- 四种聊天入口共享同一数据契约和核心组件。

---

## 16. 实现决策

建议按以下决策落地：

1. 模式采用 `auto | plan | agent`，不使用容易和聊天 role 混淆的 `normal`。
2. `agent_runs.mode` 作为单轮事实，`chat_sessions.default_mode` 只作为下一轮默认值。
3. Plan 与 Todo 分离。
4. 第一阶段不新增 plans 表，先以规范 `run_events` 持久化；需要独立编辑和检索时再升级。
5. 先支持并展示运行时原生 Sub-agent，再实现 Multara managed Sub-agent。
6. 前端只消费 capabilities 和规范事件，不写 provider 分支。
7. 不用 prompt 模拟原生能力；任何降级都必须在 UI 明示。
