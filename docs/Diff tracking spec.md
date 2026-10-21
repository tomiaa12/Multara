# Multara AI 改动追踪系统 — 技术方案 v2

> 本版修正三处关键设计：快照时机、实时差异展示、重新提问语义

---

## 1. 核心思路

### 1.1 基本原理

```
diff = computeLineDiff(fileBaseline, diskContent)
```

- **fileBaseline**：AI 调用 Edit/Write 工具修改某个文件 **之前**，该文件在磁盘上的内容
- **diskContent**：该文件当前磁盘内容（随 AI 工具调用实时变化）

### 1.2 三项设计原则

| 原则 | 说明 |
|---|---|
| **按文件按需快照** | 只在 AI 实际修改一个文件之前快照那一个文件，不快照整个项目 |
| **差异按文件唯一展示** | Overlay / Accept / Undo 跟文件走，不跟当前对话 tab 走。快照、回滚、重新提问仍按会话轮次记录 |
| **按需加载** | 启动只加载仍有 Accept/Undo 的文件 baseline；对话消息打开会话再拉；历史轮次只在 rollback 时现查 |
| **实时展示差异** | 每个 Edit/Write 工具调用结束（tool_result）后立即更新 Monaco inline diff |
| **重新提问清空后续** | 在历史消息上触发重新提问，该消息之后的所有对话清除；文件可选回滚或保持现状 |

### 1.3 Accept / Undo / Rollback 一句话

| 操作 | 效果 |
|---|---|
| **Accept** | 视觉隐藏该 Chunk 的 diff 标记；磁盘不动 |
| **Undo** | 将磁盘中该 Chunk 对应行还原成 baseline；视觉 diff 消失 |
| **Rollback** | 整个 workspace 所有被修改文件回到某条消息问之前的状态 |

### 1.4 Accept/Undo 在运行中的可用性

AI 仍在运行（run status = running）时，diff overlay **可见但按钮禁用**。原因：运行中 Undo 会改变磁盘，可能导致 AI 后续的 old_string 匹配失败而报错。运行结束后按钮自动激活。

---

## 2. 数据模型

### 2.1 `turn_file_snapshots`

```sql
CREATE TABLE turn_file_snapshots (
    id            TEXT    NOT NULL PRIMARY KEY,
    message_id    TEXT    NOT NULL,  -- 触发此轮的 user message id
    session_id    TEXT    NOT NULL,
    workspace_id  TEXT    NOT NULL,
    file_path     TEXT    NOT NULL,  -- 相对于 work_path，统一 /
    content       BLOB    NOT NULL,  -- zstd 压缩的 UTF-8 文本
    content_hash  TEXT    NOT NULL,  -- SHA-256 hex
    snapshot_at   INTEGER NOT NULL,
    UNIQUE(message_id, workspace_id, file_path)    -- 同一 turn 同一文件只快照一次
);
CREATE INDEX idx_tfs_session ON turn_file_snapshots(session_id, snapshot_at);
CREATE INDEX idx_tfs_message ON turn_file_snapshots(message_id);
```

### 2.2 `diff_chunk_states`

```sql
CREATE TABLE diff_chunk_states (
    id          TEXT    NOT NULL PRIMARY KEY,
    message_id  TEXT    NOT NULL,
    session_id  TEXT    NOT NULL,
    workspace_id TEXT   NOT NULL,
    file_path   TEXT    NOT NULL,
    chunk_index INTEGER NOT NULL,   -- 在 diff 结果数组中的顺序（0-based）
    status      TEXT    NOT NULL DEFAULT 'pending',  -- pending|accepted|undone
    updated_at  INTEGER NOT NULL,
    UNIQUE(message_id, workspace_id, file_path, chunk_index)
);
CREATE INDEX idx_dcs_session ON diff_chunk_states(session_id);
```

---

## 3. 快照策略：按文件按需快照

### 3.1 触发时机

**不在 send_message 时快照整个项目。**

快照在 `executor.rs::read_stream()` 里，当解析出 `tool_use` 事件且工具为文件操作时触发：

```
流式输入 → normalize_runtime_line → NormalizedEvent::ToolUse { tool: "Edit" | "Write" | "Create" }
                                          ↓
                              snapshot::capture_file_before_edit()
                                          ↓
                              读磁盘 → 存 DB + 推送给前端
                                          ↓
                              CLI 执行文件操作（写磁盘）
                                          ↓
                              NormalizedEvent::ToolResult
                                          ↓
                              emit("run-event") → 前端刷新文件 + 刷新 diff
```

### 3.2 时序保证

Claude CLI 的流式协议保证：`tool_use` 完整输出 → CLI 执行工具 → `tool_result` 输出。
因此在 `tool_use` 事件处理时读取磁盘，文件尚未被写入。

不同 CLI 的工具名映射：

| CLI | Edit 工具名 | Write/Create 工具名 |
|---|---|---|
| Claude | `str_replace_editor`（command=edit\|create） | `str_replace_editor`（command=write） |
| Cursor | `Edit` | `Write` |
| Codex | `EditFile` | `WriteFile` |

在 `normalize_runtime_line` 已经统一为 `tool: "Edit"` / `tool: "Write"`，直接判断即可。

### 3.3 快照实现

```rust
// src-tauri/src/snapshot.rs

pub async fn capture_file_before_edit(
    pool: &SqlitePool,
    message_id: &str,
    session_id: &str,
    workspace: &Workspace,
    rel_file_path: &str,   // 相对路径，来自 tool_use.input["file_path"]
) -> Result<Option<CapturedSnapshot>> {
    // 同一 turn 同一文件只快照一次（保留最原始的 baseline）
    if db::snapshot_exists(pool, message_id, &workspace.id, rel_file_path).await? {
        return Ok(None);
    }

    let abs_path = Path::new(&workspace.work_path).join(rel_file_path);

    // 文件不存在（AI 准备创建新文件）→ baseline = ""
    let raw_content = if abs_path.exists() {
        tokio::fs::read(&abs_path).await?
    } else {
        vec![]
    };

    // 检测是否为可处理的文本文件
    if raw_content.len() > 2 * 1024 * 1024 || is_binary(&raw_content) {
        return Ok(None);
    }

    let content_str = String::from_utf8_lossy(&raw_content).into_owned();
    let hash = sha256_hex(&raw_content);
    let compressed = zstd::bulk::compress(&raw_content, 3)?;

    db::insert_turn_snapshot(pool, TurnFileSnapshot {
        id: new_ulid(),
        message_id: message_id.to_string(),
        session_id: session_id.to_string(),
        workspace_id: workspace.id.clone(),
        file_path: rel_file_path.to_string(),
        content: compressed,
        content_hash: hash,
        snapshot_at: unix_now(),
    }).await?;

    // 返回未压缩内容，供调用方通过 run-event 推送给前端
    Ok(Some(CapturedSnapshot {
        file_path: rel_file_path.to_string(),
        content: content_str,
    }))
}
```

### 3.4 在 `executor.rs` 中集成

```rust
// executor.rs::read_stream 内，处理 NormalizedEvent 时：

NormalizedEvent::ToolUse { call_id, tool, input } => {
    // 文件操作类工具：在文件被修改前拍快照
    if matches!(tool.as_str(), "Edit" | "Write" | "Create") {
        if let Some(file_path) = extract_file_path(&tool, &input) {
            let snapshot = snapshot::capture_file_before_edit(
                &pool, &message_id, &session_id, &workspace, &file_path
            ).await?;

            // 把快照内容附在 tool_use 事件里一起发给前端
            // 前端收到后立即更新本地 snapshot 缓存
            if let Some(snap) = snapshot {
                emit_event(&app, RunEvent::FileSnapshot {
                    run_id: run_id.clone(),
                    message_id: message_id.clone(),
                    file_path: snap.file_path,
                    content: snap.content,
                });
            }
        }
    }

    // 正常记录 tool_use 事件到 run_events 表 + emit
    db::insert_run_event(pool, ...).await?;
    emit_event(&app, RunEvent::ToolUse { ... });
}

NormalizedEvent::ToolResult { call_id, output } => {
    db::insert_run_event(pool, ...).await?;
    // tool_result 触发前端刷新文件和 diff
    emit_event(&app, RunEvent::ToolResult {
        run_id: run_id.clone(),
        call_id,
        tool: matched_tool.clone(),
        output,
        // 附上被修改的文件路径，供前端定向刷新
        affected_file: current_tool_file.clone(),
    });
}
```

### 3.5 快照的存储规模

只存实际被修改的文件：
- 典型一次 AI 回答：修改 2-5 个文件 × 平均 20KB = ~100KB 原始，压缩后 ~20KB
- 100 轮对话：~2MB

远比之前"整个项目"的方案小。

---

## 4. 实时差异展示（运行中）

### 4.1 事件序列与前端响应

```
Rust emit RunEvent::FileSnapshot { message_id, file_path, content }
  → 前端 listen("run-event")
  → diffStore.onSnapshotReceived(messageId, filePath, content)
    （前端 snapshot 缓存更新，供后续 diff 计算用）

Rust emit RunEvent::ToolResult { affected_file }
  → 前端 listen("run-event")
  → WorkspaceWorkbench 已有逻辑：refreshActiveFile()
      → Monaco setValue(新磁盘内容)
      → onDidChangeModelContent 触发
  → 新增：diffStore.getChunksForFile(...) → overlay.applyChunks()
    （此时 snapshot 已在缓存中，diff 可立即计算）
```

每个 tool_result 完成，Monaco 里就能看到最新的 diff overlay。AI 改多个文件时，diff 逐步累积出现。

### 4.2 前端 run-event 处理（新增分支）

```typescript
// stores/app.ts::registerRunEvents() 中增加：

case 'file_snapshot': {
  // 运行中收到文件快照，立即存入 diff store
  diffStore.onSnapshotReceived(
    payload.messageId,
    payload.filePath,
    payload.content,
    activeSessionId,
  )
  break
}

case 'tool_result': {
  // 原有：刷新文件树、Git 状态
  // 新增：如果是文件操作，刷新对应文件的 diff overlay
  if (payload.affectedFile && payload.affectedFile === currentOpenFilePath) {
    // refreshActiveFile() 已触发 Monaco 更新，
    // onDidChangeModelContent 会自动调用 refreshDiff()
  }
  break
}
```

### 4.3 运行中 Accept/Undo 按钮状态

```typescript
// EditorWithDiff.vue
const isRunning = computed(() =>
  appStore.runStates[currentRunId]?.status === 'running'
)

// DiffOverlayManager 收到 isRunning = true 时，
// Accept/Undo 按钮加 disabled 属性并显示 tooltip：
// "AI 仍在运行中，完成后可操作"
```

---

## 5. Diff 计算算法（前端）

### 5.1 类型

```typescript
// src/domain/diff.ts

export interface DiffHunk {
  oldStart: number     // 在 baseline 中的起始行（1-based）
  oldLines: string[]   // baseline 中被删/替换的原始行
  newStart: number     // 在当前文件中的起始行
  newLines: string[]   // 当前文件中新增/替换的行
}

export interface DiffChunk {
  chunkId: string      // `${messageId}:${workspaceId}:${filePath}:${chunkIndex}`
  messageId: string
  workspaceId: string
  filePath: string
  chunkIndex: number
  status: 'pending' | 'accepted' | 'undone'
  hunk: DiffHunk
}
```

### 5.2 Diff 引擎

```typescript
// src/utils/diff-engine.ts
import { structuredPatch } from 'diff'  // npm: diff（MIT）

export function computeLineDiff(baseline: string, current: string): DiffHunk[] {
  if (baseline === current) return []
  const patch = structuredPatch('', '', baseline, current, '', '', { context: 0 })
  return patch.hunks.map(h => ({
    oldStart: h.oldStart,
    oldLines: h.lines.filter(l => l[0] === '-').map(l => l.slice(1)),
    newStart: h.newStart,
    newLines: h.lines.filter(l => l[0] === '+').map(l => l.slice(1)),
  }))
}

/** Undo 用：把 current 中 hunk 对应的新行替换回旧行 */
export function applyReversePatch(current: string, hunk: DiffHunk): string {
  const lines = current.split('\n')
  lines.splice(hunk.newStart - 1, hunk.newLines.length, ...hunk.oldLines)
  return lines.join('\n')
}

/** 合并多轮 diff，返回所有 pending 的 chunks */
export function getActiveChunksForFile(
  workspaceId: string,
  filePath: string,
  currentContent: string,
  turns: Array<{ messageId: string; baselineContent: string }>,
  chunkStates: Map<string, 'pending' | 'accepted' | 'undone'>,
): DiffChunk[] {
  const result: DiffChunk[] = []
  for (const { messageId, baselineContent } of turns) {
    computeLineDiff(baselineContent, currentContent).forEach((hunk, idx) => {
      const chunkId = `${messageId}:${workspaceId}:${filePath}:${idx}`
      const status = chunkStates.get(chunkId) ?? 'pending'
      if (status === 'pending') {
        result.push({ chunkId, messageId, workspaceId, filePath, chunkIndex: idx, status, hunk })
      }
    })
  }
  return result.sort((a, b) => a.hunk.newStart - b.hunk.newStart)
}
```

---

## 6. Monaco Inline Diff

### 6.1 DiffOverlayManager（核心）

```typescript
// src/components/workbench/DiffOverlayManager.ts

export class DiffOverlayManager {
  private decorationIds: string[] = []
  private viewZoneIds: string[] = []
  private widgets: Monaco.editor.IContentWidget[] = []

  constructor(
    private editor: Monaco.editor.IStandaloneCodeEditor,
    private onAccept: (chunkId: string) => void,
    private onUndo: (chunkId: string) => void,
    private isRunning: () => boolean,
  ) {}

  applyChunks(chunks: DiffChunk[]): void {
    this.clearAll()
    const decorations: Monaco.editor.IModelDeltaDecoration[] = []

    this.editor.changeViewZones(accessor => {
      for (const chunk of chunks) {
        const { hunk } = chunk

        // ① 红色行（删除/替换前）：插入 ViewZone
        if (hunk.oldLines.length > 0) {
          const dom = this.buildRemovedZone(hunk.oldLines)
          const zoneId = accessor.addZone({
            afterLineNumber: hunk.newStart - 1,
            heightInLines: hunk.oldLines.length,
            domNode: dom,
            suppressMouseFromPropagatingToEditor: true,
          })
          this.viewZoneIds.push(zoneId)
        }

        // ② 绿色行（新增/替换后）：Decoration
        if (hunk.newLines.length > 0) {
          decorations.push({
            range: {
              startLineNumber: hunk.newStart,
              endLineNumber: hunk.newStart + hunk.newLines.length - 1,
              startColumn: 1, endColumn: 1,
            },
            options: {
              isWholeLine: true,
              className: 'diff-line-added',
              glyphMarginClassName: 'diff-gutter-added',
              overviewRuler: { color: 'rgba(76,175,80,0.6)', position: 4 },
            },
          })
        }

        // ③ Accept/Undo 按钮（运行中禁用）
        const widget = this.buildChunkWidget(chunk, this.isRunning())
        this.editor.addContentWidget(widget)
        this.widgets.push(widget)
      }
    })

    this.decorationIds = this.editor.deltaDecorations([], decorations)
  }

  private buildRemovedZone(oldLines: string[]): HTMLElement {
    const container = document.createElement('div')
    container.className = 'diff-removed-zone'
    for (const line of oldLines) {
      const row = document.createElement('div')
      row.className = 'diff-removed-row'
      row.innerHTML = `<span class="diff-prefix">−</span><span>${escapeHtml(line)}</span>`
      container.append(row)
    }
    return container
  }

  private buildChunkWidget(chunk: DiffChunk, disabled: boolean): Monaco.editor.IContentWidget {
    const el = document.createElement('div')
    el.className = `diff-chunk-controls${disabled ? ' diff-controls-disabled' : ''}`

    const mk = (cls: string, text: string, cb: () => void) => {
      const btn = document.createElement('button')
      btn.className = `diff-btn ${cls}`
      btn.textContent = text
      btn.disabled = disabled
      if (disabled) btn.title = 'AI 仍在运行中，完成后可操作'
      btn.addEventListener('click', e => { e.stopPropagation(); cb() })
      return btn
    }

    el.append(
      mk('diff-btn-accept', 'Accept', () => this.onAccept(chunk.chunkId)),
      mk('diff-btn-undo',   'Undo',   () => this.onUndo(chunk.chunkId)),
    )

    const widgetLine = chunk.hunk.newLines.length > 0
      ? chunk.hunk.newStart + chunk.hunk.newLines.length - 1
      : Math.max(1, chunk.hunk.newStart - 1)

    return {
      getId: () => `diff-widget-${chunk.chunkId}`,
      getDomNode: () => el,
      getPosition: () => ({
        position: { lineNumber: widgetLine, column: Number.MAX_SAFE_INTEGER },
        preference: [0],
      }),
    }
  }

  clearAll(): void {
    this.decorationIds = this.editor.deltaDecorations(this.decorationIds, [])
    this.editor.changeViewZones(a => {
      this.viewZoneIds.forEach(id => a.removeZone(id))
      this.viewZoneIds = []
    })
    this.widgets.forEach(w => this.editor.removeContentWidget(w))
    this.widgets = []
  }
}
```

### 6.2 CSS 关键样式

```css
/* diff overlay */
.diff-line-added   { background: rgba(76,175,80,0.13) !important; border-left: 3px solid #4caf50; }
.diff-gutter-added { background: #4caf50; width: 3px !important; }

.diff-removed-zone { background: rgba(244,67,54,0.08); border-left: 3px solid #f44336;
                     font-family: inherit; font-size: inherit; line-height: 19px; }
.diff-removed-row  { display: flex; padding: 0 0 0 24px; color: #ef9a9a; white-space: pre; }
.diff-prefix       { color: #f44336; margin-right: 8px; user-select: none; }

.diff-chunk-controls { display: inline-flex; gap: 4px; background: var(--color-surface,#1e1e1e);
                       border: 1px solid rgba(255,255,255,0.12); border-radius: 4px; padding: 1px 4px; }
.diff-btn            { font-size: 11px; padding: 0 8px; border: none; border-radius: 3px;
                       cursor: pointer; font-weight: 500; }
.diff-btn-accept     { background: #4caf50; color: #fff; }
.diff-btn-undo       { background: #616161; color: #fff; }
.diff-controls-disabled .diff-btn { opacity: 0.4; cursor: not-allowed; }
```

---

## 7. Pinia Diff Store

```typescript
// src/stores/diff.ts

export const useDiffStore = defineStore('diff', () => {
  // sessionId → messageId(按时间升序) → filePath → baselineContent
  const snapshots = ref(new Map<string, Map<string, Map<string, string>>>())
  // chunkId → status
  const chunkStates = ref(new Map<string, 'pending' | 'accepted' | 'undone'>())
  // sessionId → messageId[]（有快照的轮次，按时间升序）
  const turnOrder = ref(new Map<string, string[]>())

  // ── 接收 run-event 中的实时快照 ──────────────────────────
  function onSnapshotReceived(
    messageId: string, filePath: string, content: string, sessionId: string,
  ) {
    if (!snapshots.value.has(sessionId)) snapshots.value.set(sessionId, new Map())
    const session = snapshots.value.get(sessionId)!
    if (!session.has(messageId)) session.set(messageId, new Map())
    session.get(messageId)!.set(filePath, content)

    const order = turnOrder.value.get(sessionId) ?? []
    if (!order.includes(messageId)) turnOrder.value.set(sessionId, [...order, messageId])
  }

  // ── 查询当前文件的 pending chunks ────────────────────────
  function getChunksForFile(
    workspaceId: string, filePath: string, currentContent: string, sessionId?: string,
  ): DiffChunk[] {
    // 展示按文件唯一：默认聚合所有会话里该文件的轮次快照。
    // 传入 sessionId 时仍只看该会话（rollback / 重新提问用）。
    const turns = collectTurns(workspaceId, filePath, sessionId)
    return getActiveChunksForFile(workspaceId, filePath, currentContent, turns, chunkStates.value)
  }

  // ── Accept ───────────────────────────────────────────────
  async function acceptChunk(chunkId: string) {
    chunkStates.value.set(chunkId, 'accepted')
    const [messageId, workspaceId, filePath, idx] = chunkId.split(':')
    await backend.updateChunkState({ messageId, workspaceId, filePath, chunkIndex: +idx, status: 'accepted' })
  }

  // ── Undo ─────────────────────────────────────────────────
  async function undoChunk(chunkId: string, workspaceId: string, filePath: string) {
    const [messageId, , , idxStr] = chunkId.split(':')
    const chunkIndex = parseInt(idxStr)
    const baseline = await backend.getSnapshotContent(messageId, workspaceId, filePath)
    const current  = await backend.readWorkspaceFile(workspaceId, filePath)
    const hunks    = computeLineDiff(baseline, current)
    const hunk     = hunks[chunkIndex]
    if (!hunk) {
      console.warn(`[diff] undo: chunk ${chunkIndex} not found after re-diff`)
      chunkStates.value.set(chunkId, 'undone')
      return
    }
    const patched = applyReversePatch(current, hunk)
    // 特例：undo 新文件创建 → 删除文件
    if (!baseline && !patched.trim()) {
      await backend.deleteWorkspaceFile(workspaceId, filePath)
    } else {
      await backend.writeWorkspaceFile(workspaceId, filePath, patched)
    }
    chunkStates.value.set(chunkId, 'undone')
    await backend.updateChunkState({ messageId, workspaceId, filePath, chunkIndex, status: 'undone' })
  }

  // ── Rollback to Turn ─────────────────────────────────────
  async function rollbackToTurn(messageId: string, workspaceId: string, sessionId: string) {
    const files = await backend.getTurnSnapshots(messageId, workspaceId)
    for (const { filePath, content } of files) {
      if (content === '') {
        await backend.deleteWorkspaceFile(workspaceId, filePath)
      } else {
        await backend.writeWorkspaceFile(workspaceId, filePath, content)
      }
    }
    // 清除该 turn 及之后的内存状态
    const order = turnOrder.value.get(sessionId) ?? []
    const fromIdx = order.indexOf(messageId)
    const toRemove = new Set(order.slice(fromIdx))
    for (const [key] of chunkStates.value) {
      if (toRemove.has(key.split(':')[0])) chunkStates.value.delete(key)
    }
    const session = snapshots.value.get(sessionId)
    if (session) toRemove.forEach(id => session.delete(id))
    turnOrder.value.set(sessionId, order.slice(0, fromIdx))
    await backend.deleteSnapshotsFrom(messageId, workspaceId, sessionId)
  }

  return {
    snapshots, chunkStates, turnOrder,
    onSnapshotReceived, getChunksForFile,
    acceptChunk, undoChunk, rollbackToTurn,
  }
})
```

---

## 8. 重新提问：语义与实现

### 8.1 触发方式

每条历史**用户消息**右侧显示一个编辑图标（hover 时可见）。点击后：

1. 该消息内容填入聊天输入框，输入框切换为「重新提问」模式（可编辑、有取消按钮）
2. 输入框上方显示提示条：`从这里重新提问，之后的 N 条消息将被清除`
3. 用户编辑内容（或保持原文），按 Enter / 点发送

### 8.2 弹窗（发送时触发）

```
╔══════════════════════════════════════╗
║  重新提问                             ║
╠══════════════════════════════════════╣
║  之后 N 条消息将被删除。              ║
║  文件状态如何处理？                   ║
║                                      ║
║  ○ 回滚文件  ─ 恢复到这条消息        ║
║                提问前的代码状态       ║
║                                      ║
║  ○ 保持现有代码  ─ 文件不改变，       ║
║                   只删除对话记录      ║
║                                      ║
║        [取消]   [确认并发送]          ║
╚══════════════════════════════════════╝
```

### 8.3 执行流程

```
用户选择 + 确认
  ↓
if 选择"回滚文件":
  diffStore.rollbackToTurn(targetMessageId, workspaceId, sessionId)
  （文件回到目标消息提问前的状态）

执行消息清除：
  backend.clearMessagesAfter(sessionId, targetMessageId)
  → DB: DELETE FROM messages WHERE session_id=? AND created_at > target.created_at
  → DB: DELETE FROM agent_turns WHERE ...（关联清除）
  → DB: DELETE FROM agent_runs WHERE ...
  → DB: 清除对应 diff_chunk_states 和 turn_file_snapshots（若选了回滚）

前端：
  从 store.messages 移除 targetMessageId 之后的所有消息
  diffStore 中清除对应轮次的 chunk states 和 snapshots

发送新消息（新的 send_message，会重新拍快照）：
  store.send(newContent, sessionId)
```

### 8.4 两种选择对 diff 的影响

| 选择 | 文件状态 | diff 展示 |
|---|---|---|
| 回滚文件 | 恢复到目标消息提问前 | 目标消息之前的未 accept chunk 仍可见 |
| 保持现有代码 | 不变（含所有 AI 改动、手动改动） | 目标消息之前的 pending chunk 仍可见；目标消息之后的 chunk 清除 |

### 8.5 新增 Tauri Command

```rust
/// 清除某条消息之后的所有对话（含 agent_turns/runs、snapshots、chunk states）
#[tauri::command]
pub async fn clear_messages_after(
    state: State<'_, AppState>,
    session_id: String,
    after_message_id: String,
    also_clear_snapshots: bool,  // 回滚时 true；保持代码时 false
) -> Result<(), String> {
    let db = &state.db;
    let threshold = db::message_created_at(db, &after_message_id).await?;

    // 删除消息
    db::delete_messages_after(db, &session_id, threshold).await?;

    if also_clear_snapshots {
        db::delete_snapshots_after(db, &session_id, &after_message_id).await?;
        db::delete_chunk_states_after(db, &session_id, &after_message_id).await?;
    }
    Ok(())
}
```

---

## 9. 其余 Tauri Commands

```rust
/// 获取某文件的 baseline 内容（Undo 时使用）
#[tauri::command]
pub async fn get_snapshot_content(
    state: State<'_, AppState>,
    message_id: String, workspace_id: String, file_path: String,
) -> Result<String, String>

/// 获取某 turn 所有快照（Rollback 时使用）
#[tauri::command]
pub async fn get_turn_snapshots(
    state: State<'_, AppState>,
    message_id: String, workspace_id: String,
) -> Result<Vec<SnapshotEntry>, String>

/// 更新 chunk 状态（accept/undo 持久化）
#[tauri::command]
pub async fn update_chunk_state(
    state: State<'_, AppState>,
    message_id: String, workspace_id: String, file_path: String,
    chunk_index: i64, status: String,
) -> Result<(), String>

/// 删除某消息后的快照（rollback 清理用）
#[tauri::command]
pub async fn delete_snapshots_from(
    state: State<'_, AppState>,
    message_id: String, workspace_id: String, session_id: String,
) -> Result<(), String>

/// 启动 / 刷新：只加载仍有 Accept/Undo 的文件 baseline，不拉全部会话历史
#[tauri::command]
pub async fn load_active_diff_data() -> Result<ActiveDiffData, String>

/// 按会话加载快照（测试或显式指定会话时用；打开对话只拉 messages）
#[tauri::command]
pub async fn load_session_diff_data(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<SessionDiffData, String>
```

---

## 10. 特殊场景

### 10.1 同一 turn 同一文件多次修改

```
tool_use(Edit, src/a.ts) → 快照 a.ts = v0 ← 只在这里快照
tool_result → a.ts 变为 v1（AI 第 1 次改）
tool_use(Edit, src/a.ts) → 已有快照，跳过
tool_result → a.ts 变为 v2（AI 第 2 次改）

最终 diff(v0, v2) = 第 1 次和第 2 次改动的累计结果
用户看到的是本轮所有改动的汇总 diff，一个 Undo 把全部还原为 v0
```

### 10.2 手动在绿色区域内添加代码

无需特殊处理。Monaco `onDidChangeModelContent` → `refreshDiff(editor.getValue())`。用户新增的行相对于 baseline 仍是"新增"，自然归入绿色。

### 10.3 AI 仅新增一行空行

`computeLineDiff` 结果：`{ oldLines: [], newLines: [''] }`。只有那行空行是绿色，与 git diff 行为完全一致。

### 10.4 AI 创建新文件（baseline = ""）

快照内容 = 空字符串。`diff("", newContent)` = 全文绿色。Undo = 文件删除（见 7. undoChunk 特例处理）。

### 10.5 AI 删除文件

工具调用类型（如 `Bash rm file`）不一定有 Edit/Write，此时不会触发快照。

解决：在 run 完成时（`finish()`），对比 turn_file_snapshots 里有快照但现在磁盘不存在的文件，自动生成一个 "deleted" 状态的 chunk（全文红色，无绿色行）。

### 10.6 差异留了一半，下次发消息的快照

- Turn 1：Chunk A（pending）、Chunk B（accepted）
- 用户发 Turn 2
- Turn 2 AI 改 src/a.ts 前：快照此时磁盘的 a.ts（含 A 和 B 的改动，因为都在磁盘上）
- Rollback 到 Turn 2 之前 → 恢复到含 A+B 改动的状态 → 显示 Chunk A（pending）、Chunk B 不显示（accepted）✓

---

## 11. 文件架构变更

### 新增 Rust

```
src-tauri/src/
  snapshot.rs        capture_file_before_edit / get_content / get_turn_files
  snapshot_db.rs     turn_file_snapshots CRUD
  diff_state_db.rs   diff_chunk_states CRUD
```

`executor.rs` 修改：在解析 ToolUse 事件时调用 `snapshot::capture_file_before_edit`，并在 emit 的 RunEvent 中附加 FileSnapshot 数据。

### 新增前端

```
src/
  domain/diff.ts
  utils/diff-engine.ts
  stores/diff.ts
  assets/diff-overlay.css
  components/workbench/
    DiffOverlayManager.ts
    EditorWithDiff.vue
```

### 修改前端

```
src/stores/app.ts           registerRunEvents 中增加 file_snapshot / tool_result 分支
src/services/backend.ts     新增 diff 相关 invoke
src/components/
  workbench/WorkspaceWorkbench.vue   编辑器替换为 EditorWithDiff
  chat/UserPrompt.vue                 增加编辑图标 + 重新提问输入框模式
  chat/ReshuffleModal.vue             新增：重新提问选项弹窗（文件保持 or 回滚）
```