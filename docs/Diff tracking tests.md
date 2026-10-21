# Multara AI 改动追踪系统 — 测试用例 v2

> 新增：实时 diff 测试、重新提问场景测试、按文件快照策略测试

---

## 第一类：Diff 算法单元测试

### T-D01：纯新增行
```
baseline: "line1\nline2"
current:  "line1\nNEW_A\nNEW_B\nline2"
期望：hunk[0] oldLines=[] newLines=["NEW_A","NEW_B"] newStart=2
```

### T-D02：纯删除行
```
baseline: "line1\nDEL_ME\nline2"
current:  "line1\nline2"
期望：hunk[0] oldLines=["DEL_ME"] newLines=[] newStart=2
```

### T-D03：替换行
```
baseline: "line1\nOLD\nline3"
current:  "line1\nNEW\nline3"
期望：hunk[0] oldLines=["OLD"] newLines=["NEW"] newStart=2
```

### T-D04：多段不连续变更
```
baseline: "A\nB\nC\nD\nE"
current:  "A\nB2\nC\nD\nE2"
期望：2 个 hunk
  hunk[0]: oldLines=["B"] newLines=["B2"] newStart=2
  hunk[1]: oldLines=["E"] newLines=["E2"] newStart=5
```

### T-D05：AI 仅新增一行空行（与 git diff 行为一致）
```
baseline: "function foo() {\n  return 1\n}"
current:  "function foo() {\n  return 1\n\n}"
期望：hunk[0] oldLines=[] newLines=[""] newStart=3
注意：第 4 行 "}" 不变，不在 hunk 里
```

### T-D06：内容相同
```
期望：computeLineDiff 返回 []
```

### T-D07：空文件 → 有内容
```
baseline: ""
current: "line1\nline2"
期望：hunk[0] oldLines=[] newLines=["line1","line2"] newStart=1
```

### T-D08：有内容 → 空文件
```
baseline: "line1\nline2"
current: ""
期望：hunk[0] oldLines=["line1","line2"] newLines=[] newStart=1
```

### T-D09：Unicode 内容
```
baseline: 'const msg = "Hello"'
current:  'const msg = "你好，世界"'
期望：1 个 hunk，正确处理多字节字符
```

### T-D10：反向 Patch —— 还原修改
```
current: "line1\nNEW\nline3"
hunk: { newStart:2, newLines:["NEW"], oldLines:["OLD"] }
applyReversePatch(current, hunk) → "line1\nOLD\nline3"
```

### T-D11：反向 Patch —— 还原新增（删除行）
```
current: "line1\nEXTRA\nline2"
hunk: { newStart:2, newLines:["EXTRA"], oldLines:[] }
applyReversePatch(current, hunk) → "line1\nline2"
```

### T-D12：反向 Patch —— 还原删除（重新插入）
```
current: "line1\nline3"  （AI 删了 line2）
hunk: { newStart:2, newLines:[], oldLines:["line2"] }
applyReversePatch(current, hunk) → "line1\nline2\nline3"
```

### T-D13：手动在绿色区域内添加行（自动归入绿色）
```
baseline: "A\nB\nC"
Turn1 AI: "A\nX\nC"   （B→X）
用户手动: "A\nX\nMANUAL\nC"

computeLineDiff("A\nB\nC", "A\nX\nMANUAL\nC"):
  期望：hunk[0] oldLines=["B"] newLines=["X","MANUAL"]
         MANUAL 自动归入绿色 ✓
```

### T-D14：在绿色区最后一行按回车（新空行也是绿色）
```
baseline: "A\nB\nC"
Turn1 AI: "A\nX\nY\nC"  （B→X Y）
用户在 Y 后按回车: "A\nX\nY\n\nC"

diff("A\nB\nC", "A\nX\nY\n\nC"):
  期望：hunk[0] oldLines=["B"] newLines=["X","Y",""]
         空行也归入绿色 ✓
```

---

## 第二类：快照策略测试（按文件按需）

### T-S01：快照在 tool_use 时触发，非 send_message 时
```
前提：workspace 有 src/a.ts（内容 v0）、src/b.ts（内容 u0）
操作：send_message → AI 只修改 src/a.ts（tool_use Edit a.ts → tool_result）

期望：
  turn_file_snapshots 中只有 src/a.ts 的记录，无 src/b.ts
  src/a.ts 快照内容 = v0（修改前）
  快照时间戳 < tool_result 时间戳
```

### T-S02：同一 turn 同一文件多次修改只快照一次
```
前提：src/a.ts 初始 = v0
操作：AI 调用 Edit a.ts（第 1 次，v0→v1）
      AI 再调用 Edit a.ts（第 2 次，v1→v2）

期望：
  turn_file_snapshots 中 a.ts 只有 1 条记录，content = v0
  第 2 次 tool_use 时看到已有快照，跳过
  diff(v0, v2) = 两次改动的累计结果
```

### T-S03：同一 turn 修改不同文件各自独立快照
```
前提：a.ts = v0, b.ts = u0
操作：AI 先 Edit a.ts（快照 a.ts=v0），再 Edit b.ts（快照 b.ts=u0）

期望：turn_file_snapshots 有 2 条记录，各自独立
```

### T-S04：AI 创建新文件（不存在 → 有内容）
```
前提：new.ts 不存在
操作：AI tool_use Write new.ts，此时文件不存在

期望：
  快照 new.ts content = ""（空字符串）
  diff("", newContent) = 全部绿色
  Undo → 删除 new.ts 文件
```

### T-S05：快照通过 run-event FileSnapshot 实时推送前端
```
前提：AI 运行中，tool_use Edit a.ts 事件到达

期望：
  Rust 在处理 tool_use 时 emit RunEvent::FileSnapshot { message_id, file_path, content }
  前端 listen("run-event") 收到后立即调用 diffStore.onSnapshotReceived(...)
  快照进入内存缓存，无需等待 run 结束
```

### T-S06：超大文件（> 2MB）不快照
```
前提：AI 修改 huge.sql（3MB 文件）

期望：
  capture_file_before_edit 检测到 size > 2MB，返回 None
  不存快照，不展示 diff（该文件的 diff 直接跳过）
```

### T-S07：二进制文件不快照
```
前提：AI 修改 asset.png

期望：
  is_binary() 返回 true，跳过快照
```

---

## 第三类：实时 diff 展示（运行中）

### T-RT01：AI 调用第一个工具后立即显示 diff

```
前提：Monaco 打开 src/a.ts（内容 v0）
操作：AI 正在运行，调用 Edit a.ts（v0 → v1）

期望时序：
  1. tool_use 事件 → Rust 快照 a.ts=v0 → emit FileSnapshot
  2. 前端收到 FileSnapshot → diffStore.onSnapshotReceived
  3. CLI 写 a.ts = v1（磁盘）
  4. tool_result 事件 → emit RunEvent::ToolResult
  5. 前端 refreshActiveFile() → Monaco.setValue(v1)
  6. onDidChangeModelContent → refreshDiff() → overlay.applyChunks([chunk])
  7. Monaco 中 v0→v1 的 diff 立即显示 ✓
  
  注意：此时 run 仍在进行，Accept/Undo 按钮禁用
```

### T-RT02：AI 调用多个工具，diff 逐步累积

```
操作：AI 依次修改 a.ts（v0→v1）、b.ts（u0→u1）、a.ts 再次（v1→v2）

期望：
  第 1 次 tool_result 后：a.ts Monaco 显示 diff(v0, v1)
  第 2 次 tool_result 后：如果 b.ts 当前打开，显示 diff(u0, u1)；a.ts 的 diff 不变
  第 3 次 tool_result 后：a.ts Monaco 显示 diff(v0, v2)（累计两次改动）
```

### T-RT03：运行中 Accept/Undo 按钮禁用

```
前提：AI 正在运行（run status = running），a.ts 有 pending diff

期望：
  Monaco diff overlay 可见（红/绿颜色正常）
  Accept/Undo 按钮显示 disabled 状态，无法点击
  hover 显示 tooltip "AI 仍在运行中，完成后可操作"
```

### T-RT04：运行结束后 Accept/Undo 按钮激活

```
前提：AI 运行完成（run status = completed）

期望：
  Accept/Undo 按钮变为可点击状态
  点击 Accept → diff 消失 ✓
```

### T-RT05：运行被用户中止（Stop）后 diff 保留可操作

```
操作：AI 运行中，用户点 Stop

期望：
  已产生的 diff overlay 保留
  Accept/Undo 按钮激活（run status = stopped）
  未完成的 AI 改动 diff 仍可 undo
```

---

## 第四类：Accept / Undo 操作

### T-A01：Accept 单个 Chunk，其他 Chunk 不变
```
Turn 1：a.ts diff 有 chunk[0]（B→X）和 chunk[1]（E→Y）
操作：Accept chunk[1]

期望：
  chunk[0] 仍 pending（diff 可见）
  chunk[1] accepted（diff 消失，代码显示 Y，无红/绿）
  磁盘不变
```

### T-A02：Accept 所有 Chunk → 文件无 diff
```
操作：依次 Accept chunk[0]、chunk[1]
期望：Monaco 无任何红/绿标记，全部代码显示为普通文本
```

### T-A03：Undo 纯新增行
```
Turn1 chunk[0]: oldLines=[] newLines=["NEW"]
操作：Undo chunk[0]
期望：磁盘中 NEW 这行消失，diff 消失
```

### T-A04：Undo chunk[0]，chunk[1] 不受影响
```
baseline: "A\nB\nC\nD\nE"
current:  "A\nX\nC\nD\nY"（chunk[0]=B→X, chunk[1]=E→Y）
操作：Undo chunk[0]
期望：
  磁盘 = "A\nB\nC\nD\nY"（X 还原，Y 保持）
  重新 diff：chunk[0] 消失，chunk[1] 仍 pending ✓
```

### T-A05：Undo AI 创建的新文件
```
baseline: new.ts 不存在（快照 = ""）
current: new.ts 内容 = "export default {}"
操作：Undo chunk[0]

期望：new.ts 从磁盘删除（不是写入空字符串）
```

### T-A06：Undo 后 Accept 同一 Chunk（幂等处理）
```
操作：先 Undo chunk[0]，再 Accept chunk[0]

期望：
  Undo：磁盘还原，status = 'undone'
  Accept：chunkStates 更新为 'accepted'
  （实际 diff 此时应为空，Accept 是无效操作但不 crash）
```

---

## 第五类：Rollback 操作

### T-R01：Rollback 到 Turn2 之前，Turn1 diff 仍可见

```
时间线：
  Turn 1 (msg1)：a.ts 改动，chunk[0] pending
  Turn 2 (msg2)：b.ts 改动，chunk[0] pending
    （Turn2 拍快照时：a.ts 磁盘 = Turn1 AI 改后的值）

操作：重新提问 msg2，选择「回滚文件」

期望：
  b.ts 磁盘 = Turn2 快照值（即 Turn2 之前的状态）
  a.ts 磁盘 = Turn2 快照值中 a.ts 的内容（= Turn1 的改动后）
  Turn2 的 chunk 清除
  显示：a.ts 的 Turn1 chunk[0] 仍 pending ✓
```

### T-R02：Rollback 到 Turn1 之前 → 工作区完全还原

```
Turn 1 (msg1)：a.ts = v0 → v1

操作：重新提问 msg1，选择「回滚文件」

期望：
  a.ts 磁盘 = v0
  diff 列表为空
  chunkStates 全部清空
```

### T-R03：Rollback + 保持代码，对话清除，文件不变

```
时间线：Turn1 改 a.ts，Turn2 改 b.ts

操作：重新提问 msg2，选择「保持现有代码」

期望：
  a.ts、b.ts 磁盘不变
  msg2 之后的消息从 chat 中删除
  Turn2 的 chunk_states 从 diffStore 清除
  Turn1 的 chunk 仍 pending ✓
```

### T-R04：Rollback 后立即发送，新 turn 快照正确

```
前提：执行了 rollback to msg2（a.ts 回到 v_snap）
操作：发送新的 msg2'

期望：
  msg2' 触发 AI 修改 a.ts 前，快照此时磁盘内容（= v_snap）
  新快照 baseline = v_snap（不是 v0，也不是 v2）
```

---

## 第六类：重新提问（清除后续消息）

### T-RA01：重新提问 —— 弹窗出现时机与内容

```
前提：chat 历史有 msg1(user) + reply1(ai) + msg2(user) + reply2(ai)
操作：用户点击 msg2 的编辑图标，编辑内容，按 Enter

期望：
  弹出选项弹窗：
    - 「回滚文件」选项
    - 「保持现有代码」选项
    - 取消按钮
  弹窗显示：「之后 2 条消息将被删除」（reply2 + 任何后续）
```

### T-RA02：重新提问 —— 选「回滚文件」后消息清除

```
前提：msg2 之后有 reply2（含 AI 改动）
操作：选「回滚文件」+ 确认

期望：
  reply2 从 chat 中消失
  store.messages 中不再有 reply2
  DB：messages 表中 reply2 被删除
  DB：相关 agent_turns / agent_runs 被删除
  DB：Turn2 的 turn_file_snapshots 被删除
  diffStore：Turn2 的 chunk_states 和 snapshots 被清除
  msg2 之后重新开始 → 相当于提问前状态
```

### T-RA03：重新提问 —— 选「保持现有代码」后消息清除

```
前提：同上
操作：选「保持现有代码」+ 确认

期望：
  reply2 从 chat 中消失
  文件磁盘内容不变（含 Turn2 的 AI 改动）
  Turn2 的 turn_file_snapshots 保留（因为文件没有回滚，baseline 仍有意义）
  diffStore：Turn2 的 chunk_states 清除（重新提问后 chunk 状态重置）
  Turn1 的 pending chunk 仍可见 ✓
```

### T-RA04：重新提问后 AI 再次修改，diff 正确叠加

```
前提：
  Turn1 (msg1) a.ts 改动，chunk[0] pending
  重新提问 msg2'（选保持现有代码）

操作：msg2' 的 AI 修改 b.ts

期望：
  b.ts：msg2' 的 chunk 出现
  a.ts：msg1 的 chunk[0] 仍 pending
  两个 diff 同时显示 ✓
```

### T-RA05：重新提问 —— 用户修改了消息内容后发送

```
前提：msg2 内容 = "优化登录函数"
操作：点编辑 msg2，改为"优化登录函数并添加错误处理"，选回滚+发送

期望：
  新消息内容 = "优化登录函数并添加错误处理"（非原始内容）
  后续流程与 T-RA02 相同
```

### T-RA06：重新提问 —— 取消后无任何变化

```
操作：点编辑 msg2 → 弹窗出现 → 点取消

期望：
  消息列表不变
  文件不变
  diff 状态不变
  输入框清空（退出「重新提问」模式）
```

---

## 第七类：多轮多文件综合

### T-M01：Turn1 改 a.ts，Turn2 改 b.ts（不重叠）
```
Turn1：a.ts diff chunk[0] pending
Turn2：b.ts diff chunk[0] pending

期望：
  a.ts 和 b.ts 各自独立 diff，互不干扰
  Accept a.ts chunk[0] 不影响 b.ts chunk[0]
```

### T-M02：Turn1 改 a.ts 多处，Turn2 再改 a.ts 另一处
```
Turn1：a.ts 第 2 行和第 10 行各有一个 chunk（pending）
Turn2 baseline：a.ts = Turn1 改后的版本
Turn2：a.ts 第 20 行新增一行

期望：
  getActiveChunksForFile 返回 3 个 chunks
  Turn1 chunk[0]（line 2）、Turn1 chunk[1]（line 10）、Turn2 chunk[0]（line 20）
  三者按 newStart 排序展示 ✓
```

### T-M03：Chunk 部分 Accept 后 Turn3 快照状态

```
Turn1：a.ts chunk[0](pending) + chunk[1](accepted by user)
Turn2：send_message 触发 → Turn2 AI 修改 a.ts → 在 tool_use 时快照 a.ts

此时 a.ts 磁盘 = Turn1 的全部改动（含 chunk[0] 和 chunk[1]，因为 accept 不改磁盘）

Turn2 baseline = 含 chunk[0]+chunk[1] 的版本
Rollback to Turn2 → 恢复到该 baseline
  显示：chunk[0] pending ✓，chunk[1] accepted（不显示）✓
```

---

## 第八类：边界与特殊情况

### T-E01：运行失败后 diff 情况

```
前提：send_message → AI tool_use Edit a.ts → 快照已拍
      AI 运行失败（CLI 退出码非 0）
      a.ts 可能已被修改，也可能没有

情况 A：a.ts 已被修改
  期望：diff(v0, v_modified) 正常展示，用户可 Undo

情况 B：a.ts 未被修改
  期望：diff(v0, v0) = []，无 diff 展示 ✓
```

### T-E02：Monaco 内容有未保存修改时执行 Rollback

```
前提：Monaco 编辑 a.ts 有 dirty 内容（未保存）
操作：Rollback

期望：
  弹窗提示："以下文件有未保存修改，Rollback 将覆盖：src/a.ts [继续] [取消]"
  选继续：磁盘写入 snapshot，Monaco.setValue() 更新，dirty 状态清除
  选取消：Rollback 中止，dirty 内容保留
```

### T-E03：chunkIndex 在文件被外部修改后的稳定性

```
前提：Turn1 diff a.ts 有 chunk[0]（line 5）、chunk[1]（line 20）

操作：用户在 line 1 添加一行注释

期望：
  chunk[0] newStart 变为 6，chunk[1] newStart 变为 21
  chunkId 仍为 "...:0" 和 "...:1"（index 不变，内容匹配）
  accepted 状态不受影响
```

### T-E04：差异按文件唯一，会话数据仍隔离

```
前提：Session A 有 pending chunks，Session B 无
操作：切换到 Session B，打开同一个文件

期望：
  该文件仍显示 Session A 留下的 pending diff（文件唯一）
  显式按 Session B 查询时没有属于 B 的 snapshot
  Session A 的 rollback / 重新提问仍只清 A 的消息和快照
```

### T-E05：同时打开多个文件 Tab，diff 各自独立

```
前提：Turn1 修改 a.ts 和 b.ts
操作：同时在 editor 中打开两个 Tab

期望：
  a.ts Tab 显示 a.ts 的 diff
  b.ts Tab 显示 b.ts 的 diff
  切换 Tab 时 overlay 正确更新
```

### T-E06：快照压缩与解压正确性

```
操作：对含中/英/日文和特殊符号的文件做快照
期望：
  zstd 压缩后写入，get_snapshot_content 解压后内容与原始字节一致
  UTF-8 无截断无乱码
```

---

## 手动验证 Checklist

- [ ] 发送消息 → AI 第一个 Edit 完成后，Monaco 立即出现红/绿 diff（不等全部完成）
- [ ] AI 运行中，diff 可见但按钮禁用，hover 有 tooltip
- [ ] AI 完成后，Accept/Undo 按钮激活
- [ ] Accept chunk → 该块 diff 消失，代码变普通显示
- [ ] Undo chunk → 磁盘还原，diff 消失
- [ ] 在绿色行末尾按 Enter，新行变绿（无需任何操作）
- [ ] AI 同一 turn 修改同一文件两次，diff 展示累计结果（一次 Undo 全部还原）
- [ ] 两轮对话，两处 diff 并排展示
- [ ] 点历史用户消息编辑图标 → 内容填入输入框 → 弹窗出现
- [ ] 选「回滚文件」→ 文件回到提问前 → 后续消息清除
- [ ] 选「保持现有代码」→ 文件不动 → 后续消息清除
- [ ] 取消重新提问 → 一切不变
- [ ] Rollback 时有 dirty 文件 → 弹确认
- [ ] 新开对话 / 切换 Session → 同一文件的 pending diff 仍在
- [ ] 重启 App → 只恢复仍有 Accept/Undo 的文件 diff，打开对话才加载消息