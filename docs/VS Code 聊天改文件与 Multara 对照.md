# VS Code 聊天改文件与 Multara 对照

这份文档对照 **VS Code 工作台 Chat**（`src/vs/workbench/contrib/chat`）和 **Multara 当前代码**，从输入框能放什么、发出去的参数、响应流格式，到文件怎么被改、差异怎么比、保留 / 撤回、重新提问怎么把文件恢复回来。

范围是侧栏 / 面板聊天，不是 Inline Chat 的全部。VS Code 里普通 Inline Chat 共用同一套 `ChatEditingSession`；`lockToAgent` 的轻量 review 会话没有 checkpoint 时间线，本文不展开。

VS Code 核心只定义请求和进度契约，真正调模型的是扩展 participant（Copilot 等）。Multara 把 Claude / Cursor / Qwen 等 CLI 当运行时，CLI 自己带 Edit / Write 工具。

Multara 输入框、气泡、队列的界面细节见 `docs/工作台聊天输入与响应.md`。快照和 hunk 的产品规则见 `docs/Diff tracking spec.md`。本文只写两边机制的差别。

源码对照（约行号，随版本会漂）：

| 环节 | VS Code | Multara |
|---|---|---|
| 输入框 | `browser/widget/input/chatInputPart.ts` | `src/components/chat/ChatComposer.vue`、`ChatComposerField.vue`、`composer-content.ts` |
| 发送 | `common/chatService/chatServiceImpl.ts` 的 `sendRequest` / `sendRequestInternal` | `src/stores/app.ts` 的 `send`；`src-tauri/src/commands.rs` 的 `send_message_inner` |
| 请求形状 | `common/participants/chatAgents.ts` 的 `IChatAgentRequest` | `assignment` 字符串 + CLI stdin |
| 响应 | `common/chatService/chatService.ts` 的 `IChatProgress` | `src/domain/types.ts` 的 `RunEvent` |
| 改文件 | `browser/chatEditing/chatEditingSession.ts` | CLI 直接写磁盘；`src-tauri/src/snapshot.rs`、`executor.rs` |
| 差异 | `chatEditingTextModelChangeService.ts` 的 `computeDiff` | 聊天卡片 `timeline-display.ts`；编辑器 `src/utils/diff-engine.ts` |
| 保留 / 撤回 | `_keepHunk` / `_undoHunk`、`session.accept` / `reject` | `src/stores/diff.ts` 的 `acceptChunk` / `undoChunk` |
| 重问恢复 | `chatExecuteActions.ts` 编辑后再发送；`restoreSnapshot` | `src/stores/reask.ts`、`ReshuffleModal.vue` |
| 样式 | `browser/widget/media/chat.css`、`chatEditing/media/chatEditingEditorOverlay.css` | `src/styles.css`、`src/assets/diff-overlay.css` |

---

## 1. 一句话差别

VS Code 把「模型要改的文件」收进 **editing session**：磁盘上的文件是 modified，旁边还有一份 baseline（`originalModel`）。差异是这两份文本算出来的。Keep 把 baseline 推到当前内容，Undo 把工作区写回 baseline。重新编辑某一轮提问时，按 **checkpoint 时间线**把工作区恢复到那一轮开始前，再把后面的对话标成待删除。

Multara 让 **CLI 直接写工作区**。应用在 `tool_use` 之前把磁盘内容拍进 `turn_file_snapshots`。聊天里的 diff 卡片只解析工具入参 / 结果做展示，不写盘。编辑器里的绿红块是「该轮 baseline vs 当前文件」算出来的。Accept 只藏掉标记、磁盘不动；Undo 才按反向补丁写回。重新提问时由用户选「回滚文件」或「保持现有代码」，再截断对话。

---

## 2. 端到端流程

### VS Code

```
ChatInputPart（Monaco 输入 + 附件 pill）
  → ChatWidget.acceptInput
  → ChatService.sendRequest(session, text, IChatSendRequestOptions)
  → ChatRequestParser 拆 parts
  → ChatModel.addRequest（气泡立刻出现）+ 空的 ChatResponseModel
  → 异步补 instructions / hooks / 变量
  → IChatAgentRequest 交给扩展 participant
  → progress 回调：markdown / thinking / toolInvocation / textEdit / undoStop …
  → ChatResponseModel 聚合成 textEditGroup
  → addRequest 时 ChatEditingSession.createSnapshot(requestId)
  → textEditGroup 流式写入 modifiedModel（真实文件）
  → 流结束后 computeDiff(originalModel, modifiedModel)
  → 编辑器 overlay：整文件或单个 hunk 的 Keep / Undo
  → 再编辑这条提问并发送：restoreSnapshot(该 request) → 清掉后续请求 → 重新 sendRequest
```

会话创建时就会 `model.startEditingSession(true)`（`chatServiceImpl.ts` 里 `_startSession`），后面每一轮都挂在同一个全局 editing session 上。

### Multara

```
ChatComposerField（contenteditable）
  → formatAgentPrompt → 一段纯文本
  → store.send → invoke send_message { sessionId, content, attachments }
  → 后端拼 assignment，spawn CLI，cwd = 工作区
  → CLI 自己的 Edit/Write 写磁盘
  → tool_use 之前：capture_file_before_edit → run-event file_snapshot
  → tool_result：时间线 + affectedFile → 刷新编辑器
  → FileDiffCard 用工具 I/O 画展示 diff
  → 编辑器 computeLineDiff(baseline, 当前内容) → Accept / Undo
  → 重问：弹窗选回滚或保持 → 可选写回磁盘 → truncate 消息和快照 → 再 send
```

---

## 3. 输入框支持的内容

| 内容 | VS Code | Multara |
|---|---|---|
| 编辑区 | Monaco 编辑器（`_inputEditor`），不是 `contenteditable` | `contenteditable`，序列化成 `ComposerNode[]`：`text` 或 `chip` |
| 换行 | 编辑器默认换行；提交是 Enter（可配置） | Enter 发送；Shift+Enter 换行 |
| 纯文本粘贴 | 走编辑器粘贴；可变成 `kind: 'paste'` 附件 | 只留 `text/plain`，HTML 丢掉 |
| 文件 / 文件夹引用 | `#file:` 等动态变量，或附件 pill（`kind: 'file' \| 'directory'`） | 芯片。发给模型是 `[文件] path` / `[文件夹] path`，有正文再加围栏 |
| 代码行 / 选区 | `kind: 'paste'` 或 symbol / selection 变量，带 range | 芯片带 `startLine`/`endLine`/`text`。标签是 `文件名 (12-40)`，prompt 是 `[代码引用] path:12-40` 加围栏 |
| 符号 | `kind: 'symbol'` | 没有 |
| @ 智能体 / 斜杠命令 | 开头的 `@agent`、`/slash`、`@agent /subcmd` 解析成独立 part，**不进入**发给模型的 `message` | 没有 @ / 斜杠协议。执行者是页脚 `<select>`（智能体或小队），存在会话上，不写进正文 |
| 工具开关 | Agent 模式下 `userSelectedTools: Record<string, boolean>`，也可用 `#toolName` | 没有。工具由 CLI 自己决定 |
| 模式 | `ChatModeKind`: `ask` / `edit` / `agent`，外加自定义 agent 的 `modeInstructions`、权限级别 | 没有 Ask/Edit/Agent 切换 |
| 模型 | `userSelectedModelId` + `userSelectedModelConfiguration` | 绑在智能体配置上，发送参数里不带模型 id |
| 图片 | 附件 `kind: 'image'`；拖入 png/jpg 会转成图片。限额按模型（Claude 20、Gemini 10） | 粘贴图缩略图，发送前存成附件。上限见现有聊天文档 |
| 隐式上下文 | 当前编辑器文件可自动附上（`chat.implicitContext.*`）。Ask 模式默认可建议上下文 | 没有自动附上当前文件。文件树 / 编辑器要把路径显式送进输入框 |
| 拖放 | `DropIntoEditorController` + `ChatWidgetPasteTarget` | 工作区文件拖进输入框变成芯片 |
| 草稿 | 输入历史在 `ChatInputPart.acceptInput` 里写 | 按会话 id 放内存 `drafts`，刷新丢失 |
| 排队 | `queue: ChatRequestQueueKind`。Steering 还会让当前请求尽快收尾 | 本会话队列。正在跑且输入区有内容时，发送按钮变成「加入队列」 |
| 空内容 | 无文本时拒绝，除非有 file/image 附件或指定了 agent/slash | 无文字但有芯片、图片或附件，也允许发送 |

Multara 芯片同一 `data-context-id` 只插一次。发给模型的最终字符串是引用块 + 可见句子（芯片位置换成标签），多段之间空行隔开。详见 `docs/工作台聊天输入与响应.md` 第 3 节。

### 输入框样式

| | VS Code | Multara |
|---|---|---|
| 容器 | `.interactive-session` 最宽约 950px。底部 `.interactive-input-part` > `.chat-input-container` | `.chat-composer`，工作台宽 `min(720px, 100% - 32px)`，水平居中，底边距 14px |
| 外观 | `--vscode-input-background`，描边 `--vscode-input-border`，圆角 `--vscode-cornerRadius-large`，内边距 `0 6px 6px` | 1px `--control-border`，圆角 9px，背景 `--control-bg`，padding 9px |
| 聚焦 | 编辑器自身焦点样式 | 边框 `--blue`，2px `--control-ring` |
| 进行中 | `.working`：边框上转一圈彗星光（`conic-gradient`，时长随宽度变，默认约 4s），颜色 `--vscode-chat-inputWorkingBorderColor1` | 没有输入框光带动画。运行状态是列表底部单独一行 |
| 上方停靠 | `.chat-editing-session` 贴在输入框上沿：文件工作集、`+N` `-N`（`--vscode-chat-linesAddedForeground` / `linesRemovedForeground`）、Keep / Undo 工具条。圆角只留顶部，和输入框合成一块 | 没有贴在输入框上的工作集。变更出现在助手时间线的 diff 卡片里 |
| 附件 | `.attached-context-container` 里的 pill | 上方 72×72 粘贴图，或文件名芯片。引用芯片是深蓝底 `#2b3542`、字 `#7ec8e8`、圆角 6px、14px 文件图标 |
| 页脚 | `.chat-input-toolbars`：模式、模型、工具、提交（`.chat-submit-button`） | 22px 头像 + 执行者 `<select>`，回形针、停止（方块）、蓝色上箭头 |

---

## 4. 发送出去的参数

### VS Code：`IChatSendRequestOptions` → `IChatAgentRequest`

UI 调用 `sendRequest(sessionResource, message, options)`。`message` 仍是输入框原文，解析在服务里做。

`IChatSendRequestOptions`（`chatService.ts`）里和改文件相关的字段：

| 字段 | 作用 |
|---|---|
| `modeInfo` | `ask` / `edit` / `agent`，以及自定义 agent 指令、`permissionLevel` |
| `userSelectedModelId` / `userSelectedModelConfiguration` | 模型与上下文长度、思考力度等 |
| `userSelectedTools` | 工具开关 |
| `location` | 面板是 `ChatAgentLocation.Chat`（对外常写 `'panel'`） |
| `parserContext` | 选中的 agent、模式、附件能力 |
| `attachedContext` / `resolvedVariables` | 附件和已经解析好的变量（含目录里的图片） |
| `agentId` / `agentIdSilent` / `slashCommand` | 不靠正文里的 `@` 指定 participant |
| `instructionContext` | 发送后异步收集 `.instructions.md`、skills，不挡住气泡出现 |
| `acceptedConfirmationData` / `rejectedConfirmationData` | 工具确认框的回传 |
| `queue` / `pauseQueue` | 排队；Steering 会给当前请求发 yield |
| `attempt` | 重试次数 |
| `isSystemInitiated` / `hideFromTranscript` | 系统插入的消息，可以画成一行进度而不是大气泡 |

解析：`ChatRequestParser.parseChatRequest` 扫 `#`、`@`、`/` 和动态变量。`getPromptText` 把各 part 的 `promptText` 拼起来。`@agent` 的 `promptText` 为空，所以模型看到的 `message` 不含这个名字。

真正交给扩展的 `IChatAgentRequest` 大致是：

```ts
{
  sessionResource, requestId, agentId,
  message,                 // 拼好的 prompt
  command?,                // agent 子命令
  variables: { variables: IChatRequestVariableEntry[] },
  location,                // Chat
  userSelectedModelId?, userSelectedTools?,
  modeInstructions?, permissionLevel?,
  editedFileEvents?,       // 工作区里已有的编辑事件，给后续轮次
  hooks?, workingDirectory?,
  acceptedConfirmationData?, rejectedConfirmationData?
}
```

变量条目 `IChatRequestVariableEntry` 是联合类型，常见 `kind`：`file`、`directory`、`image`、`paste`、`symbol`、`implicit`、`tool`、`toolset`、`terminal`、`promptFile`、`generic`。文件引用带 URI，粘贴带文本和语言，图片带二进制或引用。

空消息会被拒，除非带着 file/image，或指定了 agent / slash。已有请求在跑且没排队时，并发发送会被拒。

气泡先插入，instructions 和 hooks 后补，再 `invokeAgent`。扩展通过 `progress` 回调往回推，核心不实现 LLM。

### Multara：一个字符串 + 附件 JSON

前端 `send_message`：

```ts
{ sessionId, content, attachments }
```

`content` 已经是 `formatAgentPrompt` 的纯文本。后端要求正文 trim 后非空，或至少有一个附件。附件最多 10 个，路径 canonicalize，单文件不超过 25MB。

交给运行时的 `assignment`：

- 无附件：`content` 原样。
- 有附件：正文后再接一段说明，加上 `[{ name, path, mimeType, size }]`。

没有模式、模型、工具开关、变量数组。历史由后端组 `RuntimeBrief` 后和 `assignment` 一起进 `build_prompt()`，从 **stdin** 喂给 CLI，工作目录是会话的 `work_path`。

---

## 5. 响应格式

### VS Code：`IChatProgress` 流

扩展每推一批 part，`ChatModel.acceptResponseProgress` → `ChatResponseModel.updateContent`。相邻的 markdown、thinking 会合并。同一个 URI 的 `textEdit` 收成内部的 `textEditGroup`（`edits: TextEdit[][]`），`textEditGroup` **不是**扩展直接发的 kind。

和改文件有关的 kind：

| kind | 含义 |
|---|---|
| `markdownContent` | 正文 |
| `thinking` | 思考，相邻 chunk 合并 |
| `progressMessage` | 带 id 的进度行，同 id 原地替换 |
| `toolInvocation` | 工具状态：流式 → 等待确认 → 执行 → 完成 / 取消 |
| `confirmation` | 确认按钮，结果经下次请求的 `acceptedConfirmationData` 回去 |
| `textEdit` | `{ uri, edits: TextEdit[], done?, isExternalEdit? }`。`done: true` 表示这个 URI 的编辑结束 |
| `notebookEdit` | Notebook 单元格编辑，聚合方式同 textEdit |
| `codeblockUri` | 把某段 markdown 代码块指到一个文件 URI（Edit 工具用来挂 diff pill） |
| `workspaceEdit` | 工作区文件操作。核心目前只处理删除 |
| `externalEdit` | 工具已经在磁盘改完。带 before/after URI 和预先算好的 `added`/`removed`，用来画静态 pill，不走 editing session 的 diff |
| `undoStop` | 一个可单独撤销的停点（通常是一次工具调用）。session 在这里再打一个 checkpoint |
| `multiDiff` | 多文件 diff 数据 |

内置 `EditTool`（`vscode_editFile_internal`，`builtinTools/editFileTool.ts`）自己不写文件。它发 `markdownContent` + `codeblockUri`，再用 `ICodeMapperService` 把代码块映射成 `textEdit`，经 `acceptResponseProgress` 交回 editing session。

`apply_patch` 不在核心工具目录里，在 Copilot / Agent Host。核心只提供 `textEdit` 管道和 `externalEdit`。工具如果已经写盘，应发 `externalEdit` 或走 `startExternalEdits`，而不是再套一层 textEdit。

流结束：`response.complete()`，再补 followups。

### Multara：`run-event`

CLI stdout 归一化成事件，经 Tauri 推到前端。和文件有关的是：

| 字段 | 含义 |
|---|---|
| `eventKind: "file_snapshot"` | 改这个文件之前的磁盘内容：`messageId`、`filePath`、`snapshotContent` |
| `eventKind: "file_deleted"` | 本轮改过的文件后来被删了 |
| `timelineItem` | `tool_use` / `tool_result`，含 `tool`、`input`、`output`、`callId` |
| `affectedFile` | `tool_result` 时关联的路径 |
| `chunk` | 助手正文增量 |
| `status` | 运行状态 |

前端把 `tool_use`/`tool_result` 配成一对，edit/write 画成 `FileDiffCard`，其余进思考、探索、终端、工具行。正文是 Markdown（`RichContent`）。

Multara **没有** `textEdit` 这种「请应用这些编辑」的进度。文件内容变化发生在 CLI 进程里。

---

## 6. 改文件怎么落地

### VS Code：先记 baseline，再把编辑应用到真实文件

`ChatEditingService.installAutoApplyObserver` 听 `chatModel.onDidChange`：

1. `addRequest` → `session.createSnapshot(requestId)`。时间线在会话开始还有一个 `requestId = undefined` 的初始 checkpoint。
2. 响应里出现 `textEditGroup`（且不是 `isExternalEdit`）→ `startStreamingEdits` → `entry.acceptAgentEdits`，写入 **工作区文件**（`modifiedModel`）。
3. 该文件第一次被这轮碰到时，`recordFileBaseline` 记下当前内容。
4. 遇到 `undoStop`，或这轮结束，再 `createSnapshot`。

每个文件条目：

| 模型 | 是什么 |
|---|---|
| `modifiedModel` | 真实工作区 URI，用户和 AI 都在改它 |
| `originalModel` | scheme `chat-editing-text-model`。当前这一轮 review 的 baseline，不是「会话刚打开时」的内容 |
| `initialContent` | 这个条目创建时、AI 第一次改它之前的内容 |
| 历史快照 | scheme `chat-editing-snapshot-text-model`，按 `requestId` + `undoStop` 取回 |

条目状态：`Modified`（待审）、`Accepted`（Keep）、`Rejected`（Undo）。

新建：文件不存在就建空文件，`ChatEditKind.Created`。整文件 Undo 时删掉。删除：读盘后删文件，做成 `ChatEditingDeletedFileEntry`（全文删除）。Undo 删除会把原文写回去。重命名在时间线里有回放，但 `applyWorkspaceEdit` 目前只实现了删除。

流式过程中编辑器只读，画「即将写入」和「最后编辑行」，完整 hunk 在这轮编辑结束后再算。

用户在 AI 改完后继续手改：`_mirrorEdits` 尝试把用户编辑 rebase 到 AI 的编辑上，成功则 baseline 跟着走；rebase 失败就做 compose。改回 `initialContent` 会自动变成 Rejected。

### Multara：CLI 写盘，应用只负责拍照

`is_file_mutation_tool` 认出 Edit、Write、Create、MultiEdit、NotebookEdit、`apply_patch` 等。`tool_use` 到达、CLI 还没写盘时：

- 读磁盘，zstd 后写入 `turn_file_snapshots`。
- 同一轮同一文件只保留 **最早** 那份（改之前的样子）。
- 新文件 baseline 是空字符串。
- 大于 2MB 或二进制跳过。

然后 CLI 写磁盘。`tool_result` 后前端刷新文件树、Git、打开变更文件，`diffStore.notifyDiskChanged()`。

Run 结束时，有快照但磁盘上已经没有的文件，发 `file_deleted`。

没有被 Edit/Write 碰过、只被 shell `rm` 掉的文件，没有 baseline，撤不回来。

---

## 7. 差异怎么比

两边都有 **两套 diff**，用途不同。

### VS Code

**编辑器里的待审 diff（Keep / Undo 用的就是这份）**

`ChatEditingTextModelChangeService._updateDiffInfo` 调用编辑器 worker：

```ts
computeDiff(originalModel.uri, modifiedModel.uri, {
  ignoreTrimWhitespace: false,  // 必须精确，否则 Keep/Undo 会错位
  computeMoves: false,
  maxComputationTimeMs: 3000
}, 'advanced')
```

比的是 **当前 review baseline vs 工作区当前内容**，不是「上一轮结束 vs 现在」，也不是 git HEAD。多轮未 Keep 的改动叠在同一个 `modifiedModel` 上，baseline 保持在上次 Keep 之前。Keep 之后 baseline 被整文件换成当前内容，diff 清空，下一轮再从新 baseline 起算。

粒度是 `DetailedLineRangeMapping`（行范围，内部还有字符级 `innerChanges`）。`changesCount` 就是 hunk 个数。

画法：

- 新增行：`diffAddDecoration` / 整行新增装饰，overview 和 minimap 用 SCM 的 `overviewRulerAddedForeground` 等。
- 删除：view zone 显示原文，配删除色。
- 每个 hunk 一个 `DiffHunkWidget`，悬停出 Keep / Undo。
- 流式：`.chat-editing-pending-edit`（编辑器背景 0.6 透明）、`.chat-editing-last-edit`（range highlight）、`.chat-editing-atomic-edit`（约 350ms）。

**时间线上的 diff（聊天列表、跨请求摘要）**

`ChatEditingCheckpointTimelineImpl._computeDiff` 对两个 **快照 URI** 再调一次 `computeDiff`。`getEntryDiffBetweenStops` / `getEntryDiffBetweenRequests` 比的是停点 N 和停点 N+1。`getDiffsForFilesInSession` 比的是该文件第一次 baseline 到当前。聊天里的 `+N` `-N` 和文件 pill 用的是这类统计，不是 git。

### Multara

**聊天 `FileDiffCard`（只展示）**

`timeline-display.ts` 的 `editDiff()`，按顺序：

1. 工具 input 的 `old_string` / `new_string`，前端 LCS 行 diff。两边行数乘积超过 80000 就整段删加整段增。
2. input 里的 unified patch。
3. `tool_result` 里的 `diffString` / `diff` / `patch`，或纯文本 unified diff。
4. Write 只有新内容时，每一行都是 `+`。

这和磁盘上的真实差异可以不一致（工具参数被截断、多次编辑、用户又手改过）。点文件名只 `revealWorkspaceFile`，不写盘。卡片上没有 Keep / Undo。

**编辑器 inline diff（Accept / Undo 用的就是这份）**

`computeLineDiff(baseline, current)`。baseline 来自该文件在相关轮次里 **最早的快照**（AI 改它之前的磁盘内容），current 是编辑器或磁盘上的现在。不用 git HEAD。Git gutter 是另一条线，比的是 index vs 当前内容。

身份是内容哈希 `chunkKey`，不是「第几个 hunk」。同一视觉块跨轮会级联。

| | 聊天卡片 | 编辑器 overlay |
|---|---|---|
| 左 | 工具声称的旧文本 | 轮次快照 |
| 右 | 工具声称的新文本 | 当前文件 |
| 能否改磁盘 | 否 | Undo 可以；Accept 不写盘 |

---

## 8. 保留与撤回

### VS Code

| 操作 | 粒度 | 磁盘 / 模型 |
|---|---|---|
| Keep 文件 | 整文件 | `originalModel` 设成 `modifiedModel` 的全文，diff 清空，并保存 |
| Undo 文件 | 整文件 | 工作区写回 `originalModel`。若本轮新建，则删文件 |
| Keep hunk | 一个 `DetailedLineRangeMapping` | 把 modified 侧文本写进 `originalModel` 的对应 range，再重算 diff。全部 hunk 都处理完且两边相同，条目变成 Accepted |
| Undo hunk | 一个 mapping | 把 original 侧文本写回 `modifiedModel`（工作区）。全部撤完则 Rejected |
| Keep All / Undo All | 所有 `Modified` 条目 | `session.accept()` / `session.reject()` |

配置 `chat.editing.autoAcceptDelay`：大于 0 且不在 review 模式时，请求结束后倒计时自动 Keep。设为 0 则一直停在待审。

编辑器顶部 `ChatEditorOverlayWidget`：Keep、Undo、上一个 / 下一个 hunk、忙碌时脉冲。主按钮用 `--vscode-button-background`。Hunk 上的小条是 `.chat-diff-change-content-widget`。

输入框上方的工作集对单个文件也有 Keep（`chatEditing.acceptFile`）和 Undo（`chatEditing.discardFile`）。

另外还有 **时间线 undo/redo**（`undoInteraction` / `redoInteraction`）：按 checkpoint epoch 进退，和单个 hunk 的 Keep 不是一回事。

### Multara

| 操作 | 粒度 | 磁盘 |
|---|---|---|
| 聊天卡片 | 无按钮 | 不动 |
| Accept 一块 / 全部 | 编辑器里一个视觉 hunk | **不写盘**（CLI 已经写过）。只记 `diff_chunk_states.status = accepted`，藏掉 overlay。同视觉块会级联 |
| Undo 一块 / 全部 | 同上 | `applyReversePatch` 写回。新建文件的 Undo 是删文件。写盘失败不记 `undone` |
| 接受「文件已删除」 | 整文件 | `acceptMissingFile`，只从导航里拿掉 |
| Git 面板丢弃 | SCM | `git restore`，和 AI diff 状态无关 |

AI 还在跑（同工作区 `status === "running"`）时，Accept / Undo 按钮 disabled。

样式在 `diff-overlay.css`：新增行 `rgba(76, 175, 80, 0.12)`，左侧 3px `#4caf50`；删除行 view zone `rgba(244, 67, 54, 0.12)`。按钮条贴在块上，Accept 绿 `#4caf50`，Undo 灰 `#616161`，11px 字。没有 VS Code 那种编辑器顶部总条，也没有自动接受倒计时。

聊天卡片样式在 `styles.css`：`<details class="diff-card">` 默认展开。新增行 `rgba(63, 185, 80, 0.14)` / 字 `#b4e0c0`，删除行 `rgba(248, 81, 73, 0.14)` / 字 `#efb0b5`。默认只显示 4 行。这套颜色和编辑器 overlay 不是同一套色值。

---

## 9. 重新提问时文件怎么恢复

### VS Code：恢复是默认动作，按 checkpoint 整段撤销

用户编辑一条历史提问再发送时（`chatExecuteActions.ts` 的 `SubmitAction`，`viewModel.editing`）：

1. 找出这条 request 以及它后面的全部 request。
2. 若这些 request 改过 working set 里的文件，且 `chat.editing.confirmEditRequestRemoval === true`，弹确认：「将去掉后续请求，并撤销对 N 个文件的编辑」。
3. 确认后 `session.restoreSnapshot(这条 request 的 id, undefined)`。
4. 时间线 `navigateToCheckpoint`：把 epoch 降到 **这条 request 开始时的 checkpoint**。`undoStopId` 为空时，恢复的是该 request 记录的 file baseline（把用户在轮次之间的手改也考虑进去），再反向做创建 / 删除 / 重命名。
5. 后续 request 标 `shouldBeRemovedOnSend`，列表立刻隐藏。真正 `removeRequest` 发生在下一次发送。
6. 然后用编辑后的输入重新 `acceptInput`。

其它入口：`workbench.action.chat.undoEdits`、`restoreCheckpoint`、`restoreLastCheckpoint`、`startOver`，都进 `restoreSnapshotWithConfirmationByRequestId`。先取消当前请求，再 `restoreSnapshot`。

没有「保留文件、只删对话」这个选项。要留住改动，应先 Keep（baseline 前移），再编辑更早的提问；被恢复点盖住的未 Keep 改动会被写回去。

### Multara：用户二选一，再截断数据库

历史气泡里改字再发送 → `ReshuffleModal`，默认「保持现有代码」：

| 选项 | 文件 | 对话 |
|---|---|---|
| 回滚文件 | `rollbackToTurn`：目标消息 **以及之后** 各文件的快照里，取每文件最早 baseline，`writeWorkspaceFile` / `deleteWorkspaceEntry` 写回磁盘 | `truncateSessionFromMessage` 删掉这条及之后的消息、run、快照、chunk 状态，并清 runtime conversation id |
| 保持现有代码 | 磁盘不动。`clearChunkStatesFrom` 只清后续 hunk 的视觉状态。被截断消息的快照仍会删掉 | 同样 truncate，然后 `send` 新内容 |

回滚语义是「这条用户消息提问前」。更早轮次已经改过、且这轮没再碰的文件不会被写。未保存的编辑器缓冲会先 `confirmRollbackIfDirty`。

附件不能改，重发时仍用原来的附件。取消弹窗不把气泡里改过的字写回。

和 VS Code 的差别：Multara 把「撤文件」和「撤对话」拆开，默认留文件；VS Code 把两者绑在同一次 checkpoint 恢复上，确认框只决定做不做，不提供保留文件的分支。

---

## 10. 样式对照（改文件相关）

| 表面 | VS Code | Multara |
|---|---|---|
| 聊天里的文件变更 | 输入框上方工作集：文件名、`+N` `-N`、悬停出 Keep/Undo。颜色走 `chat.linesAddedForeground` / `linesRemovedForeground` | 助手过程折叠里的 `.diff-card`，三列（行号 / `+` `-` / 代码），无操作按钮 |
| 编辑器待审 | 顶部浮层 `.chat-editor-overlay-widget`：`--vscode-editorWidget-background`、6px 圆角、`--vscode-shadow-md`。忙碌时 2.3s 脉冲。主按钮是主题 button 色，11px | 每个 hunk 右上角 `.diff-chunk-controls-bar`，面板色底、细边框。绿 Accept、灰 Undo |
| 行内高亮 | 复用 SCM diff 装饰（增 / 删 / overview ruler） | 自绘绿底行 + 红色 view zone，色值写死，不跟主题 token |
| 流式写入 | pending / last-edit / atomic 三类装饰 | 没有「正在写入这一行」的编辑器装饰。流式体现在时间线卡片和正文 chunk |
| 用户气泡 | `.interactive-request`，列表是 Monaco List | `.user-prompt`，历史提问本身又是一个 `ChatComposer` |
| 助手正文 | `.interactive-item-container` + markdown part | `AssistantMessage`：说话人行、`ProcessFold`、失败气泡、`RichContent`、页脚 |
| 主题 | 全部 `--vscode-*` | 工作台暗色变量（`--control-*`、`--blue`）加 diff 硬编码色 |

---

## 11. 差别清单

下面这些是行为差别，不是待办。

1. **谁写第一笔。** VS Code 由核心把 `TextEdit[]` 应用到 `modifiedModel`。Multara 由 CLI 写磁盘，应用只在写之前拍照。
2. **协议。** VS Code 是结构化变量 + `IChatProgress`。Multara 是一段拼好的 prompt 文本，文件变更靠 `file_snapshot` 和工具时间线推断。
3. **模式 / 模型 / 工具。** VS Code 发送时带 Ask/Edit/Agent、模型配置、工具开关、权限级别。Multara 只有会话上的执行者，工具在 CLI 内。
4. **引用。** VS Code 用 `#` 变量和附件 kind，模型侧拿到 URI 和范围。Multara 把芯片展成 `[文件]` / `[代码引用]` 围栏文本。
5. **隐式当前文件、符号、斜杠命令、@agent。** VS Code 有。Multara 没有。
6. **两套 diff 的基准。** VS Code 待审 diff 是 originalModel vs 工作区；列表统计是 checkpoint 之间。Multara 卡片 diff 是工具 I/O；待审 diff 是轮次快照 vs 当前文件。卡片和编辑器可能对不上。
7. **Keep 的含义。** VS Code Keep 会移动 baseline 并保存。Multara Accept 只确认「这块不用再显示」，文件已经在磁盘上。
8. **Undo 的含义。** 两边都会把工作区写回去。VS Code 写的是 baseline 模型；Multara 写的是反向补丁。VS Code 可以只撤一个字符级 inner change 合成的 hunk；Multara 按视觉 chunk。
9. **重问。** VS Code 默认连文件一起恢复到该轮 checkpoint，没有「只删对话」。Multara 默认保持文件，回滚是可选项，而且是整轮最早快照，不是 hunk。
10. **用户中途手改。** VS Code 会 rebase / compose 进 baseline。Multara 下次 `computeLineDiff` 会把用户改动和 AI 改动算在同一份当前文本里，没有单独的 rebase。
11. **流式装饰和自动接受。** VS Code 有正在写入的行高亮，以及可配置的自动 Keep。Multara 跑的时候只是把 Accept/Undo 禁用。
12. **删除和新建。** 两边都能对「这轮新建的文件」做撤销删除。VS Code 的删除条目在 review 模型里；Multara 靠空 baseline 或 `file_deleted`。纯 shell 删除且没有快照时，Multara 恢复不了。
13. **确认框。** VS Code 的编辑请求确认是「要不要撤销 N 个文件」，可勾选不再询问。Multara 每次重问都问回滚还是保持。
14. **样式。** VS Code 变更列表贴在输入框上，编辑器顶部一条总控，颜色走主题。Multara 变更在消息流里，操作在 hunk 上，绿红是写死的。

---

## 12. 关键调用（便于接着读源码）

VS Code：

- 发送：`ChatService.sendRequest` → `sendRequestInternal` → `ChatRequestParser.parseChatRequest` → `ChatModel.addRequest` → `invokeAgent`
- 应用编辑：`installAutoApplyObserver` → `observerEditsInResponse` → `ChatEditingSession.startStreamingEdits` → `acceptAgentEdits`
- 算 diff：`ChatEditingTextModelChangeService._updateDiffInfo` → `IEditorWorkerService.computeDiff`
- hunk：`_keepHunk` 写 original，`_undoHunk` 写 modified
- 整文件：`ChatEditingModifiedDocumentEntry._doAccept` / `_doReject`
- 恢复：`SubmitAction` → `session.restoreSnapshot` → `ChatEditingCheckpointTimelineImpl.navigateToCheckpoint`

Multara：

- 发送：`ChatPanel` → `formatAgentPrompt` → `app.send` → `send_message_inner` → `executor.spawn_run`
- 拍照：`maybe_capture_file_snapshot` → `capture_file_before_edit`
- 卡片 diff：`groupTimeline` → `editDiff`
- 编辑器 diff：`diffStore.getChunksForFile` → `computeLineDiff`
- Accept / Undo：`acceptChunk` / `undoChunk` → `applyReversePatch`
- 重问：`ReshuffleModal` → `executeReask` → `rollbackToTurn`（可选）→ `truncateSessionFromMessage` → `send`
