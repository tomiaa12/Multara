---
name: multara-workflows
description: 建立和修改工作流程。使用者要把多個步驟交給智慧體、分流、人工審批，或用整合訊息啟動時使用。
---

# 工作流程

一條工作流程只有四種節點：開始、智慧體、人工關卡、結束。開始節點建立時自帶，不能再加，也不能刪。結束節點只能有一個。命令改的是草稿，要跑起來必須先發布。

## 工作流程

- `multara workflow create --name "…"`，可加 `--description "…"`
- `multara workflow list`
- `multara workflow get ID` 看草稿摘要；`multara workflow draft ID` 看完整草稿
- `multara workflow rename ID --name "…"`
- `multara workflow duplicate ID`
- `multara workflow enable ID` / `multara workflow disable ID`
- `multara workflow publish ID`，驗證通過才發布
- `multara workflow delete ID`，還有執行沒結束時會拒絕
- `multara workflow run ID --input key=value`，或 `--input-json '{…}'`
- `multara workflow runs [ID]`

## 節點

- `multara workflow node add ID --kind agent|output --title "…" [--id step] [--prompt "…"] [--config '{…}'] [--timeout 30]`
- `multara workflow node set ID NODE --title "…" --prompt "…" --config '{…}'`
- `multara workflow node remove ID NODE`，同時刪掉連著它的線

節點 id 以小寫字母開頭，只能含小寫字母、數字和底線。智慧體的 `--config` 會合併進現有設定：

- `target`：`{ "type": "agent" 或 "squad", "id": "…" }`
- `prompt`：這一步要做的事
- `context`：`previous` 只帶上一步，`all` 從開始帶到現在
- `route`：多條出路時 `all` 都送下去，`auto` 由這一步的智慧體選，`human` 暫停等人選。只有一條出路時不用選
- `allowTaskCommands`：`true` 時這一步可以建立或修改任務

人工關卡預設三個決定：`approve` 通過、`changes` 打回、`reject` 拒絕。

## 連線

- `multara workflow edge add ID --from NODE --to NODE [--when approve|changes|reject] [--max-iterations 2]`
- `multara workflow edge remove ID EDGE_ID`

`--when` 只用於人工關卡的出線。`--max-iterations` 表示打回上游，取 1 到 20。

## 開始入參

- `multara workflow input add ID --name topic [--label "主題"] [--required true|false] [--default "…"] [--primary]`
- `multara workflow input set ID NAME …`
- `multara workflow input remove ID NAME`

新入參預設必填。`--primary` 只能有一個：整合裡 `/flow 工作流程名稱` 後面的正文填入這一項。都沒標時填到第一個入參。

## 跑到這一步時

正在執行的工作流程節點不能改畫布。多條出路且 `route` 是 `auto` 時，用 `multara node next --id NODE --reason "…"` 選已有的下一步。做不到時用 `multara node fail --reason "…"`。`route` 是 `all` 時不用選。

## 規則

- 對外傳送、部署、花錢或刪除資料的工作流程：先把草稿給人看，確認後再 `publish` 和 `enable`。
- 觸發來源（手動、自動化、Webhook、整合訊息、命令列）在開始節點上改。這條 CLI 不改來源。
