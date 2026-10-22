---
name: multara-working-on-tasks
description: 建立、更新任務，並列出任務與執行紀錄。使用者提到任務、Issue、把工作交給智慧體時使用。
---

# 任務

任務是一次可指派、可追蹤的工作。需要留下負責人、狀態與結果，或要交給其他智慧體、拆成多輪時，建立成任務。

工作台裡這次執行已經綁定需求工作區、使用者要求修改程式碼時，直接修改。不要為了改程式碼而先建立任務。

## 建立

`multara task create --title "…" [--description "…"] [--assignee <agentId>] [--project <id>] [--status todo|backlog] [--parent <taskId>]`

- 預設 `--status todo`：有負責人時會立刻排入一次執行。
- `--status backlog`：只寫入資料庫，不執行。
- 省略 `--assignee` 則指派給目前的智慧體；要派給別人時必須傳入對方的 agent id（先執行 `multara agent list`）。
- `--project` 可省略；綁定需求後，執行會帶上需求工作區。
- `--parent`：掛到父任務之下，父任務詳情右側才會顯示子任務，並在父會話寫一則 @ 指派訊息。目前的任務 id 在環境變數 `MULTARA_TASK_ID`；在某個任務的 run 裡建立子任務時，即使未傳 `--parent`，也會自動掛到目前任務。
- 成功時 stdout 是任務 JSON（含 `id`）。

## 派工（含子任務）

需要其他智慧體或小隊成員平行處理時，建立子任務並指派，不要只在內文裡寫「已派給某某」：

```bash
multara task create --title "…" --description "…" --assignee <agentId> --parent "$MULTARA_TASK_ID" --status todo
```

- 父任務保持 `in_progress`；派工本身不算交付。
- 子任務都收口（不再是 backlog/todo/in_progress/blocked）後，系統會再次喚起父任務負責人做彙整；被喚起後先看子任務結果，再決定父任務進入 `in_review` / `done`。
- 不要對同一個人既建立子任務、又期望同一會話自動 fan-out；看板子任務與小隊聊天派工是兩套路徑。

## 更新

`multara task update TASK_ID --status done --comment "交付說明"`

可修改 `title`、`description`、`status`、`assignee`、`project`。`--comment` 會以系統訊息寫入該任務會話。

## 狀態 / 指派

- `multara task status TASK_ID todo|backlog|in_progress|in_review|done|blocked|cancelled`
- `multara task assign TASK_ID --to AGENT_ID`
- 把狀態改成 `todo` 只是放回待辦，不會再排入一次執行。

## 列出 / 讀取 / 留言

- `multara task list [--status todo] [--assignee <id>] [--project <id>]`
- `multara task get TASK_ID`
- `multara task comment TASK_ID --comment "進度"`
- `multara task runs TASK_ID`

## 狀態由誰寫入

系統只會做這些自動同步：

1. 任務從 `backlog` / `todo` 真正開始執行時，自動進入 `in_progress`。
2. 執行成功且該任務沒有其他進行中的執行時，若仍是 `in_progress`，進入 `in_review`。
3. 執行失敗且該任務沒有其他進行中的執行時，若仍是 `in_progress`，退回 `todo`。

**不會自動寫成 `done`。** 目標真正完成仍須明確寫入。

依工作推進，用 CLI 更新狀態（可在回合中隨時寫入，不要等到結束）：

| 情況 | 狀態 |
|------|------|
| 開始做本任務本身的交付（實作、調研、設計、審查——只要是任務要求的） | 立刻 `in_progress` |
| 本輪已交付任務本身的結果，等人確認 / 審核 | `in_review` |
| 還沒做完，還要繼續（含子任務未齊） | 保持 `in_progress` |
| 被外部依賴或條件卡住 | `blocked` |
| 目標真正達成，或使用者確認完成 | `done` |
| 只是答疑、閒聊、諮詢別人的工作，沒有推進本任務交付 | **不要改狀態** |

`done` 表示目標完成，不是「這一輪 run 跑完了」。小隊隊長：派工給隊員不算交付，父任務保持 `in_progress`，整體目標確認後再進入 `in_review` / `done`。

## 規則

- 工作台已綁定需求工作區時，不要用「先建立任務」來擋住使用者要求的程式碼修改。
- 建立前若會改變指派、範圍或權限，先給預覽再執行。
- 進度寫在任務會話裡。更新要簡短，附上任務識別碼與下一步。
