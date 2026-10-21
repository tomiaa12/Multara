# Diff 追踪系统 — 现有实现审计与修复方案

> 对照《Diff tracking spec.md》(v2) 对现有代码的全面审计结果。
> 审计范围:`src-tauri/src/{snapshot,snapshot_db,diff_state_db,executor,commands,database}.rs`、
> `src/{domain/diff,utils/diff-engine,stores/diff,stores/reask,stores/app}.ts`、
> `src/components/workbench/{DiffOverlayManager.ts,EditorWithDiff.vue}`、
> `src/components/chat/{ReshuffleModal,UserPrompt}.vue`、`src/services/backend.ts`

---

## 一、严重 Bug(P0 — 会导致数据丢失或撤错代码)

### P0-1 Undo 会撤销错误区域,或静默失败并损坏状态

**位置**:`src/stores/diff.ts:220-231`(`undoChunk`)

**问题**:chunkId 中的 `chunkIndex` 是 `assignHunkIndices`(diff-engine.ts)经缓存重映射后的**稳定索引**,但 undo 时执行:

```typescript
const hunks = computeLineDiff(baseline, current)
const hunk = hunks[chunkIndex]   // ← 用稳定索引当原始数组下标
```

二者一旦分歧就出错:

- **撤错区域**:文件有一个 AI hunk(稳定索引 0),用户在其上方手动新增一段代码(原始数组变为 `[userHunk@0, aiHunk@1]`)。此时 undo AI 的改动,`hunks[0]` 取到的是**用户手写的 hunk** → `applyReversePatch` 把用户的代码删掉。
- **静默失败 + 状态永久损坏**:3 个 chunk(索引 0/1/2),先 undo 索引 0;再 undo 索引 2 时新 diff 只剩 2 个 hunk,`hunks[2]` 为 `undefined` → 进入 `!hunk` 分支,**磁盘未回滚却把 chunkId 标成 `undone` 并持久化**,该改动此后永远无法撤销。

**修复方案**:

1. undo 不再重新裸 diff 后按下标取,而是直接复用 `getChunksForFile` 返回的 chunk(稳定索引与 hunk 已正确配对):

```typescript
async function undoChunk(chunkId: string, workspaceId: string, filePath: string) {
  const parsed = parseChunkId(chunkId)
  if (!parsed) return
  const current = await backend.readWorkspaceFile(workspaceId, filePath)
    .then(f => f.content, () => "")
  // 复用与 overlay 一致的稳定索引配对结果
  const chunks = getChunksForFile(workspaceId, filePath, current)
  const chunk = chunks.find(c => c.chunkId === chunkId)
  if (!chunk) {
    console.warn(`[diff] chunk ${chunkId} 已不存在,取消 undo`)
    return   // ← 绝不能在失败时写入 'undone' 状态
  }
  const patched = applyReversePatch(current, chunk.hunk)
  // …后续写盘逻辑不变
}
```

2. `!hunk`/`!chunk` 分支**只警告、不落状态**,保证下次刷新后用户还能重试。

### P0-2 Rollback 把未涉及的文件回滚到过旧状态(数据丢失)

**位置**:`src-tauri/src/snapshot_db.rs:181-209`(`load_session_snapshots` 的"继承"逻辑)、`get_turn_snapshots`

**问题**:`load_session_snapshots` 会把早前轮次的 baseline "继承"复制到后续每一轮。失败场景:

```
Turn1 改 a.ts(baseline=v0,改后 v1)
Turn2 只改 b.ts
rollback 到 Turn2 → 继承逻辑使 a.ts=v0 被纳入 Turn2 的快照集
→ a.ts 被错误还原到 v0,Turn1 已完成并保留的改动丢失
```

spec §10.6 明确:Turn2 快照的 a.ts 应该是**含 Turn1 改动的磁盘现状**(如果 Turn2 真的改了它),而不是 Turn1 的旧 baseline。

**副作用**:继承条目还会让前端对同一文件产生跨轮重复 chunk。

**修复方案**:

- 删除继承逻辑,`get_turn_snapshots` 只返回 `WHERE message_id = ?` 实际存储的行。
- `load_session_diff_data` 同理只返回各轮实际快照。

### P0-3 Rollback 不完整:目标轮之后修改的文件未被恢复(spec 与实现共同缺陷)

**位置**:`src/stores/diff.ts:255-273`(`rollbackToTurn`)+ spec §1.3

**问题**:rollback 只恢复**目标 turn 自己快照过**的文件。若 Turn3 修改了 Turn1 未触碰的文件 X,回滚到 Turn1 时 X 保留全部 AI 改动,违背 spec"整个 workspace 所有被修改文件回到消息前状态"。

**修复方案**(与 P0-2 配合):后端新增/改造查询——聚合**目标 turn 及之后所有 turn** 的快照,每个文件取**最早**那份 baseline:

```sql
SELECT file_path, content FROM turn_file_snapshots
WHERE workspace_id = ? AND session_id = ?
  AND snapshot_at >= (SELECT MIN(snapshot_at) FROM turn_file_snapshots WHERE message_id = ?)
GROUP BY file_path
HAVING snapshot_at = MIN(snapshot_at)   -- 每个文件最早的 baseline
```

(实际实现建议用窗口函数或在 Rust 侧按 file_path 分组取最早行。)这样 rollback 语义正确:每个被任何后续轮次改过的文件,都回到"目标消息提问前"的状态。

### P0-4 路径穿越 / 绝对路径逃逸(安全)

**位置**:`src-tauri/src/snapshot.rs:55-77`(`resolve_workspace_file`)、`:94`、`:134-139`(`normalize_rel_path`)

**问题**:

- `normalize_rel_path` 不处理 `../`;
- `Path::join` 遇到绝对路径会直接替换 base;
- 不匹配任何 workspace 的绝对路径会回退到第一个 workspace 并保留原始路径。

失败场景:工具 input 中出现 `file_path: "/etc/hosts"` 或 `"../../secret"` → 任意文件被读取、入库并 emit 给前端。虽然 file_path 来自 AI 工具调用而非直接用户输入,但 prompt 注入可诱导 AI 构造该路径。

**修复方案**:

```rust
fn safe_resolve(work_path: &Path, rel: &str) -> Result<PathBuf> {
    let joined = work_path.join(rel);
    let canonical = joined.canonicalize()
        .or_else(|_| joined.parent()
            .ok_or(anyhow!("invalid path"))?
            .canonicalize()
            .map(|p| p.join(joined.file_name().unwrap())))?;  // 新文件:父目录 canonicalize
    let root = work_path.canonicalize()?;
    if !canonical.starts_with(&root) {
        bail!("path escapes workspace: {rel}");
    }
    Ok(canonical)
}
```

同时拒绝含 `..` 组件的相对路径、拒绝无法归属 workspace 的绝对路径(不回退到第一个 workspace)。前端 `readWorkspaceFile`/`writeWorkspaceFile`/`deleteWorkspaceEntry` 对应的后端命令也需同样校验。

---

## 二、高优先级(P1 — 功能明显不符或缺失)

### P1-1 多 turn 重叠 chunk:Accept 需点两次 / 同区域双重红块

**位置**:`src/utils/diff-engine.ts:109-137`(`getActiveChunksForFile`)

**问题**:每个 turn 独立 diff 到 current,Turn2 的改动同时出现在 Turn1 视角的 diff 里:

- 行内容相同时 `dedupeVisualChunks` 只保留 Turn1 的 chunkId,Accept 它之后被去重隐藏的 Turn2 同 hunk 立即重新显示 → 需要 Accept 两次;
- 行内容不同时(Turn1 baseline 更旧),同一区域渲染两套删除块。

**修复方案**(二选一,推荐 a):

a. **Accept/Undo 级联**:accept 一个 chunk 时,同时把所有 visual-key(行范围+内容摘要)相同的其他 turn chunkId 一并标记;
b. **重叠归属最新 turn**:重叠 hunk 只归属产生它的最新 turn,旧 turn 的重叠部分不生成 chunk。

### P1-2 spec §10.5(AI 删除文件)完全未实现

**位置**:`src-tauri/src/executor.rs`(`finish()` 无对应逻辑)+ 前端无 deleted chunk 渲染路径

**问题**:AI 用 `Bash rm` 删除文件后,无红色 diff、无法 Undo 恢复,该文件的删除对用户完全不可见。

**修复方案**:

1. 后端:run 结束时(`finish()`)遍历该 turn 的 `turn_file_snapshots`,对磁盘已不存在的文件 emit `RunEvent::FileDeleted { message_id, file_path }` 并落库一条特殊 chunk state;
2. 前端:文件树中该文件显示删除标记;点开时以只读视图展示 baseline 全文红色 + Undo(恢复文件)按钮。

### P1-3 重新提问选"保持现有代码"后产生僵尸快照

**位置**:`src-tauri/src/commands.rs:2155-2158`(`truncate_session_from_message`)

**问题**:`also_clear_snapshots=false` 时删除了 messages 和 chunk_states,但保留了 turn_file_snapshots。重启后 `load_active_snapshots`(无 chunk state = 全 pending)会**复活已删除轮次的 diff**,与 spec §8.4"目标消息之后的 chunk 清除"矛盾。

**修复方案**:被删消息对应的快照无论哪种选项都应删除(消息没了,快照的 turn 归属已失效);或最低限度让 `load_active_snapshots` JOIN messages 表排除孤儿快照:

```sql
... AND EXISTS (SELECT 1 FROM messages m WHERE m.id = tfs.message_id)
```

### P1-4 chunk 状态持久化用索引作 key,重启后可能错位

**位置**:`src/utils/diff-engine.ts:74-107`(`assignHunkIndices` 稳定性缓存为内存态)

**问题**:重启后缓存为空,首次 diff 按原始顺序分配索引。若磁盘在 accept 之后又被改过,持久化的 accepted 状态可能吸附到**不同的 hunk** 上(视觉上:该显示的 diff 消失了/不该显示的显示了)。

**修复方案**:chunk 身份改用**内容哈希**——`hash(oldLines.join('\n') + '\x00' + newLines.join('\n'))` 取前 12 位 hex 作为 chunkId 的第 4 段,替代顺序索引。DB `diff_chunk_states.chunk_index` 列改为 `chunk_key TEXT`(需要一次 migration,旧行可直接丢弃——只是视觉状态)。索引位置变了但内容没变时,状态仍能正确吸附。

### P1-5 未匹配的纯新增 hunk 被静默丢弃

**位置**:`src/utils/diff-engine.ts:96` — `if (hunk.oldLines.length === 0) continue;`

**问题**:缓存存在时,新出现且未匹配的纯新增 hunk 不进入结果。违背 spec §10.2(用户手动新增行应自然归入绿色);AI 同轮第二次 Edit 新增的行也可能不显示。

**修复方案**:未匹配的纯新增 hunk 应分配新的稳定索引(或 P1-4 落地后此问题随内容哈希方案自然消失——纯新增 hunk 的哈希天然唯一)。

### P1-6 工具名覆盖不全,部分编辑不触发快照

**位置**:`src-tauri/src/snapshot.rs:22-38`

**问题**:

- 未覆盖 Claude CLI 的 `MultiEdit`/`NotebookEdit`、Codex 的 `apply_patch` → 这些编辑不会拍快照,diff 静默缺失;
- `input.command ∈ {edit,create,write}` 的兜底对**任意未知工具**生效,过宽(如 Bash 的 `{"command": "write"}` 会被误判)。

**修复方案**:显式枚举各 CLI 的文件编辑工具名(含 MultiEdit/NotebookEdit/apply_patch);兜底判断限定在已知的编辑器类工具名(如 `str_replace_editor`)内。

---

## 三、中优先级(P2)

| # | 位置 | 问题 | 修复 |
|---|---|---|---|
| P2-1 | `snapshot_db.rs:304-324` `get_snapshot_content` | 目标 message 无快照时返回 `""`,前端把空 baseline 当"新文件创建"→ **undo 时误删文件** | 查不到返回 `Err`,前端捕获后放弃 undo 并提示 |
| P2-2 | `snapshot_db.rs:357-403` `delete_snapshots_from` | 逐条 DELETE 无事务,中途失败留半清理状态;message 查不到时只删目标轮,后续轮成孤儿 | 包事务;与 `truncate_messages_from` 的重复逻辑合并为一处 |
| P2-3 | `executor.rs:1231` `user_message_id_for_run` | 无 turn_id 时回退取"最新 user 消息",run 期间用户又发消息则快照挂错 message_id | 在 run 启动时就固定 message_id 并随 run 传递 |
| P2-4 | `EditorWithDiff.vue:35-37` | isRunning 全局化:任意 session 有 run 在跑就禁用**所有**文件的 Accept/Undo | 限定为与当前 workspace 相关的 run |
| P2-5 | `app.ts:105` file_snapshot 分支 | workspaceId 缺失时回退 `""`,快照 key 永不匹配,实时 diff 静默不显示 | 缺失时 console.error 并丢弃事件,便于发现协议问题 |
| P2-6 | `reask.ts:4-6` | "之后 N 条将清除"的计数把目标消息本身算入,与 spec 表述差 1 | 与 Rust 侧 truncate 边界(`>` vs `>=`)对齐后修正 |
| P2-7 | Cursor/Codex 流时序未验证 | 快照正确性依赖"tool 事件在执行前输出";Claude 已确认,Cursor/Codex 未验证,若事件后置则 baseline 拍到改后内容 | 实测两个 CLI;若后置,对该 CLI 改为在**上一个 tool_result 到达时**预拍下一个文件(或放弃实时,run 结束统一 diff) |

---

## 四、低优先级(P3)

- **`diff-engine.ts:44`** `!line.startsWith("\\")` 是死代码(以 `\` 开头的行不会同时以 `+`/`-` 开头),"\ No newline" 过滤本意未达成,但 jsdiff 已正确处理,无实害,可删。
- **CRLF/混合行尾**:`applyReversePatch` 按 `\n` split,`\r` 保留在行内基本自洽,但混合行尾文件可能产生噪音 diff。可在 diff 前统一探测行尾并在写盘时还原。
- **`executor.rs:817`** tool_result 的 `affected_file` 回退取 `item.input`,而 tool_result 通常无 input,恒为 None 的死代码,可删(实际靠 call_id 关联)。
- **`cleanup_old_snapshots`** 只在启动 migrate 时执行一次,长时间不重启的会话期间不清理;可挂到每次 run 结束后低频触发。
- **命名偏离**:实现的 command 叫 `truncate_session_from_message`,spec 写 `clear_messages_after`,建议 spec 更新为实名。

---

## 五、spec 本身未考虑到的点(建议补充设计)

1. **Undo 与用户未保存编辑的冲突**:用户在 Monaco 中有未保存修改时点 Undo,`readWorkspaceFile` 读到的是磁盘旧内容,写盘后 Monaco 的未保存内容与磁盘分叉。建议:Undo 前检查编辑器 dirty 状态,强制先保存或提示。
2. **外部修改文件**(用户用其他编辑器改了文件):baseline diff 会把外部改动也标为 AI 改动。spec §10.2 只考虑了 Monaco 内的手动编辑。可接受(行为与 git diff 一致),但建议在文档中明确此语义。
3. **文件重命名/移动**:AI 通过 Bash `mv` 移动文件时,旧路径快照成孤儿、新路径无 baseline。与 P1-2 的删除检测可一并处理:run 结束时对"快照存在但磁盘消失"的文件,若同 turn 出现了内容哈希相同的新文件,识别为 rename。
4. **rollback 与 git 的交互**:rollback 写盘会让 git 工作区变脏,SourceControlPanel 需要刷新;若用户在轮次之间做过 commit,rollback 会产生"倒退于 HEAD"的状态。建议 rollback 确认弹窗中提示当前 git 状态。
5. **多窗口/多实例**:两个窗口打开同一 workspace 时 chunkStates 内存态不同步。低优先,可先文档标注不支持。

---

## 六、建议的修复顺序

```
第 1 批(数据安全,先行):
  P0-1 undoChunk 索引错位     → src/stores/diff.ts
  P0-4 路径穿越               → src-tauri/src/snapshot.rs
  P2-1 空 baseline 误删文件    → snapshot_db.rs + diff.ts(与 P0-1 同文件顺手修)

第 2 批(rollback 语义,后端一次改对):
  P0-2 去掉继承逻辑           → snapshot_db.rs
  P0-3 rollback 聚合后续轮次   → snapshot_db.rs + stores/diff.ts
  P1-3 僵尸快照               → commands.rs
  P2-2 delete 事务化           → snapshot_db.rs

第 3 批(chunk 身份重构,含 migration):
  P1-4 内容哈希替代顺序索引    → diff-engine.ts + diff_state_db.rs + migration
  P1-5 纯新增 hunk 丢弃        → 随 P1-4 消失
  P1-1 重叠 chunk 级联 accept  → diff-engine.ts + stores/diff.ts

第 4 批(功能补全):
  P1-2 删除文件检测           → executor.rs + 前端渲染
  P1-6 工具名覆盖             → snapshot.rs
  P2-3~P2-7、P3、第五节增强
```

第 1、2 批各自可独立发布;第 3 批涉及 DB migration 和 chunkId 格式变更,需要一起合入。

---

## 七、审计中确认无问题的点(不需要改)

- hash/压缩均针对 raw bytes,`is_binary` 拒绝非 UTF-8,zstd 往返对称(有测试)✓
- `INSERT OR REPLACE` + 先查 exists,受全局 db mutex 保护,无并发冲突 ✓
- `parseChunkId` 用 `slice(2, -1).join(':')` 正确处理 filePath 含 `:`(spec §5.1 的示例代码 `chunkId.split(':')` 反而有此 bug,建议 spec 修正)✓
- 删除行渲染用 `textContent` + `monaco.colorize`,无 XSS(优于 spec 的 escapeHtml + innerHTML 方案)✓
- Accept/Undo 按钮用 OverlayWidget 分组共享,合理增强 ✓
- 运行中禁用 Accept/Undo 已实现(粒度问题见 P2-4)✓
