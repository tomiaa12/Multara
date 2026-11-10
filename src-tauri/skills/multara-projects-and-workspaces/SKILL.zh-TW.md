---
name: multara-projects-and-workspaces
description: 建立、更新需求（專案），綁定工作區。多個任務共用一個成果、或需要修改程式碼時使用。
---

# 需求與工作區

需求把一組相關任務綁在同一個目標與儲存庫上下文上。沒有綁定需求的聊天不要修改隨機目錄。

## 建立

`multara project create --name "…" [--description "…"]`

給需求新增一個資料夾。`--path` 一次只傳一個儲存庫或普通目錄；父目錄裡有多個儲存庫時會拒絕，並列出可以分別新增的路徑。

- 新建 Worktree：`multara workspace add --project PROJECT_ID --path /絕對路徑 --mode worktree-new --branch 新分支名 [--base 遷出的分支]`
- 遷出已有分支：`multara workspace add --project PROJECT_ID --path /絕對路徑 --mode worktree-existing --branch 已有分支`
- 直接改真實目錄：`multara workspace add --project PROJECT_ID --path /絕對路徑 --mode current`

`--base` 不寫時從 `HEAD` 遷出。`multara workspace list [--project PROJECT_ID]` 查看已綁定的目錄。綁定後，該需求下排隊的執行會帶著儲存庫路徑啟動。

## 列出 / 讀取 / 更新

- `multara project list`
- `multara project get PROJECT_ID`
- `multara project update PROJECT_ID [--name "…"] [--description "…"]`

## 規則

- 多個任務共用交付物時放到同一個需求下。
- 這次執行已經綁定需求工作區：使用者要求修改程式碼時，直接修改所列出的目錄。
- 還沒有需求工作區：請使用者開啟一個需求，或執行 `multara task create --project <id> --status todo`。
- 用上面的命令選擇 Worktree 或真實目錄。建好後不要手動修改磁碟上隔離副本的路徑。
