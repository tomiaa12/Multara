# Diff 追踪系统 — 修复记录

> 对照《Diff tracking 修复方案.md》落地的实现说明。
> 对应技术方案：《Diff tracking spec.md》(v2)。

---

## 一、改动概要

本次按审计方案把 P0–P3 一次合入，核心目标：

1. **不要再撤错代码 / 写坏状态**（Undo 按下标取 hunk、空 baseline 当新建文件删盘）。
2. **Rollback 语义与 spec 对齐**（去掉快照继承；回滚目标轮及之后所有被改文件）。
3. **Chunk 身份改为内容哈希**，重启后 Accept/Undo 状态仍吸附到同一 hunk。
4. **补齐删除文件、工具名、路径安全** 等缺口。

**DB 变更**：`diff_chunk_states.chunk_index INTEGER` → `chunk_key TEXT`。启动时若检测到旧列，整表重建并丢弃旧视觉状态（只是 accepted/undone 标记，不含文件内容）。

---

## 二、按条目说明：改了什么、怎么改

### P0-1 Undo 撤错区域 / 静默失败损坏状态

**原来**：`undoChunk` 重新 `computeLineDiff` 后用 `hunks[chunkIndex]` 取 hunk。稳定索引和当前数组下标一旦不一致，就会撤到用户手写代码，或 `hunks[i]` 为 `undefined` 却仍把 chunk 标成 `undone`。

**现在**：

- 用 `listAllChunksForFile` 按 **内容哈希 chunkId** 找回 hunk，再 `applyReversePatch`。
- 找不到 hunk 时只 `console.warn`，**不写 `undone`**，下次刷新还能重试。
- Overlay 的 Undo 把编辑器当前内容传进去，避免未保存缓冲和磁盘分叉。

涉及：`src/stores/diff.ts`、`src/utils/diff-engine.ts`、`src/components/workbench/EditorWithDiff.vue`。

### P0-2 Rollback 把无关文件带回过旧 baseline

**原来**：`load_session_snapshots` 把早前轮次的文件「继承」复制到后续每一轮。Rollback Turn2 时会把 Turn1 改过、Turn2 没碰的 `a.ts` 一并还原到 Turn1 提问前。

**现在**：该函数只返回 `WHERE session_id = ?` 实际存过的行，不再继承。

涉及：`src-tauri/src/snapshot_db.rs`。

### P0-3 Rollback 漏掉目标轮之后才改的文件

**原来**：只恢复目标 turn 自己快照过的文件。Turn3 改了 Turn1 没碰的文件 X，回滚到 Turn1 时 X 仍带着 AI 改动。

**现在**：`get_turn_snapshots` 取目标消息及之后所有消息，每个 `file_path` 取 **最早** 那份 baseline（提问前状态）。`message_ids_from` 与删除逻辑共用同一套消息边界。

涉及：`src-tauri/src/snapshot_db.rs`、`src/stores/diff.ts`（仍走原 command，语义由后端改对）。

### P0-4 路径穿越 / 绝对路径逃逸

**原来**：`normalize_rel_path` 不处理 `..`；`Path::join` 遇到绝对路径会换根；匹配不到 workspace 时回退到第一个 workspace 并保留原路径。

**现在**：

- 含 `..` 的相对路径直接拒绝。
- 绝对路径必须落在某个 workspace 根下，否则丢弃（不再回退）。
- 真正读盘走 `files::resolve_existing_or_new`：存在则 canonicalize 并校验前缀；不存在则只允许无 `..` 的相对拼接。
- 读写删命令本身已有 `files::normalize_relative`，本次补的是快照路径。

涉及：`src-tauri/src/snapshot.rs`、`src-tauri/src/files.rs`。

### P1-1 多 turn 重叠 chunk：Accept 要点两次

**原来**：每个 turn 独立 diff 到 current，去重只藏后面的 chunkId。Accept 第一个后，被藏的同视觉 hunk 立刻冒出来。

**现在**：Accept / Undo 时用 `hunkVisualKey`（行号 + 旧行 + 新行）找出所有 pending 的相同视觉 chunk，一并落状态。

涉及：`src/utils/diff-engine.ts`（`matchingVisualChunkIds`）、`src/stores/diff.ts`。

### P1-2 AI 删除文件无 diff、无法 Undo

**原来**：`Bash rm` 等不拍快照；即便本轮已有快照、磁盘已删，也没有事件和树节点。

**现在**：

- `finish()` 遍历本轮 `turn_file_snapshots`，磁盘不存在则 emit `file_deleted`。
- 前端 `missingFiles` + 文件树 `git-deleted` 幽灵节点；打开失败但有快照时以只读空内容打开（全文红块），Undo 写回 baseline。

涉及：`src-tauri/src/executor.rs`、`src/stores/{app,diff}.ts`、`src/components/WorkspaceWorkbench.vue`。

说明：纯 `rm`、本轮从未 Edit/Write 过的文件仍然没有 baseline，无法凭空恢复。只有「本轮拍过快照、后来磁盘没了」的路径可追踪。

### P1-3 「保持现有代码」留下僵尸快照

**原来**：`also_clear_snapshots=false` 时删了 messages / chunk_states，却留着 `turn_file_snapshots`。重启后无 chunk state = 全 pending，已删轮次的 diff 复活。

**现在**：

- `truncate_messages_from` **无论是否回滚文件都删除**被删消息的快照。
- `load_active_snapshots` 额外 `EXISTS messages`，排除孤儿行。

涉及：`src-tauri/src/commands.rs`、`src-tauri/src/snapshot_db.rs`。

### P1-4 / P1-5 Chunk 身份：顺序索引 → 内容哈希

**原来**：chunkId 第 4 段是数组下标；`assignHunkIndices` 用内存缓存做稳定映射。重启缓存为空会错位。缓存存在时，未匹配的纯新增 hunk 被 `continue` 丢掉。

**现在**：

```
chunkKey = FNV-1a64(oldLines.join('\n') + '\0' + newLines.join('\n')) 的前 12 位 hex
chunkId  = `${messageId}:${workspaceId}:${filePath}:${chunkKey}`
```

去掉 `assignHunkIndices`。行号变了但内容没变，Accept 状态仍吸附；纯新增 hunk 哈希天然唯一，P1-5 随即消失。

DB：

```sql
-- 旧表若仍有 chunk_index，启动时 DROP 后重建
chunk_key TEXT NOT NULL
UNIQUE(message_id, workspace_id, file_path, chunk_key)
```

涉及：`src/domain/diff.ts`、`src/utils/diff-engine.ts`、`src/services/backend.ts`、`src-tauri/src/{diff_state_db,database,commands}.rs`。

### P1-6 工具名覆盖不全 / 兜底过宽

**原来**：未覆盖 `MultiEdit`、`NotebookEdit`、`apply_patch`；任意工具 `input.command ∈ {edit,create,write}` 都当文件编辑（Bash `{"command":"write"}` 会误拍）。

**现在**：显式枚举 Edit/Write/Create/EditFile/WriteFile/MultiEdit/NotebookEdit/apply_patch；`command` 兜底只对 `str_replace_editor` 一类编辑器工具生效。

涉及：`src-tauri/src/snapshot.rs`。

### P2-1 空 baseline 误删文件

`get_snapshot_content` 查不到不再返回 `""`，改为 `Err("没有该文件快照")`。Undo 侧已改为用 hunk 的 `oldLines` 判断是否「新建文件」，不再依赖这次拉取。

### P2-2 `delete_snapshots_from` 无事务

删除目标轮及之后的 snapshots / chunk_states 包进 `unchecked_transaction`；与 truncate 共用 `message_ids_from`。

### P2-3 Run 期间新消息导致快照挂错 message_id

`user_message_id_for_run` 在 `spawn_run` 开头固定，传进 `read_stream`，不再每条流里现查「最新 user 消息」。无 `turn_id` 时回退为 **不晚于该 run `created_at`** 的最近一条 user 消息。

### P2-4 Accept/Undo 被任意 session 的 running 禁用

`EditorWithDiff` 只在「当前 workspace 属于该 run 的 session」或事件自带同一 `workspaceId` 时禁用按钮。

### P2-5 `file_snapshot` 缺 workspaceId 静默不显示

缺失时 `console.error` 并丢弃事件，不再用 `""` 当 workspaceId 导致缓存永匹配不上。

### P2-6 重新提问计数差 1

`countMessagesFrom` 改为 `messages.length - index - 1`（不含目标消息本身），与文案「之后的 N 条」一致。Rust truncate 仍删除目标消息（随后会发一条新的）。

### P2-7 Cursor/Codex 快照时序

未对真实 CLI 实测。规范化代码里 Cursor 用 `subtype=completed` 区分 tool_result，Codex 有独立 `tool_call` / result。快照仍只在 `tool_use` 时拍。若某 CLI 把 tool 事件后置，baseline 可能拍到改后内容——需实机验证后再改时序。

### P3

- 删掉 diff 行过滤里无效的 `!line.startsWith("\\")`。
- 去掉 tool_result 上对 `item.input` 取路径的死代码（实际靠 `call_id`）。
- `finish()` 里调用 `cleanup_old_snapshots`，不必等下次启动。

---

## 三、文件清单

| 文件 | 改动 |
|---|---|
| `src/domain/diff.ts` | `chunkIndex` → `chunkKey`；`chunkKeyFor` |
| `src/utils/diff-engine.ts` | 去掉索引缓存；哈希身份；级联 visual key |
| `src/stores/diff.ts` | Undo/Accept/Rollback/缺失文件 |
| `src/stores/app.ts` | `file_snapshot` / `file_deleted`；导出 `sessionWorkspaces` |
| `src/stores/reask.ts` | 之后 N 条计数 |
| `src/services/backend.ts` | `updateChunkState.chunkKey` |
| `src/components/workbench/EditorWithDiff.vue` | 按 workspace 禁用；Undo/Accept 用编辑器内容 |
| `src/components/workbench/DiffOverlayManager.ts` | 按 `chunkKey` 排序 |
| `src/components/WorkspaceWorkbench.vue` | 删除文件树节点、只读幽灵打开 |
| `src-tauri/src/snapshot.rs` | 路径安全、工具名 |
| `src-tauri/src/files.rs` | `resolve_existing_or_new` |
| `src-tauri/src/snapshot_db.rs` | 去继承、rollback 聚合、事务删除、JOIN messages |
| `src-tauri/src/diff_state_db.rs` | `chunk_key` |
| `src-tauri/src/database.rs` | migration |
| `src-tauri/src/commands.rs` | chunk_key；truncate 始终删快照 |
| `src-tauri/src/executor.rs` | 固定 message_id、删除检测、清理快照 |

测试同步改了对应 `*.test.ts` 与 Rust `#[cfg(test)]`。

---

## 四、行为变化（给使用者）

| 操作 | 变化 |
|---|---|
| Accept | 同一视觉区域的其他轮次一并藏掉，不用点两次 |
| Undo | 按改动内容匹配，不再误删上方手写代码；失败不会把按钮作废 |
| Rollback 到某条消息 | 该消息提问之后改过的文件都会回到提问前；未再改过的更早文件不动 |
| 重新提问 · 保持代码 | 对话删了，对应轮次的 diff 快照也删，重启不会复活 |
| AI 删了本轮改过的文件 | 文件树红色删除项，打开可 Undo 恢复 |
| 升级后第一次启动 | 旧的 Accept/Undo 勾选状态清空，diff 会重新以 pending 出现；文件内容不受影响 |

---

## 五、未做或仅部分做

| 项 | 状态 |
|---|---|
| P2-7 实机验证 Cursor/Codex 流时序 | 未测 |
| spec 第五节：Undo 前提示未保存 | Overlay 已用编辑器缓冲区，无单独确认框 |
| 文件 rename（mv 识别为同一文件） | 未做；删除侧可恢复旧路径 |
| Rollback 确认框提示 git 脏状态 | 未做 |
| 多窗口 chunk 状态同步 | 未做，仍不支持 |
| 把 spec 里的 `clear_messages_after` 改成实现名 `truncate_session_from_message` | 未改 spec 文档 |

---

## 六、验证

- 前端：`pnpm test`（136）
- Rust：`cargo test`（67，含路径逃逸、去继承、rollback 聚合、空 baseline 报错、truncate 清快照）
