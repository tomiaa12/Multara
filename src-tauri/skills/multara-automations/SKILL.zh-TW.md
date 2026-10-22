---
name: multara-automations
description: 建立定時、間隔或事件觸發的自動化。應當依排程或外部事件啟動、而不是等人來問時使用。
---

# 自動化

自動化依 cron、間隔或事件啟動一次智慧體執行。

## 建立

`multara automation create --name "…" --agent-id AGENT_ID --prompt "…" --project-id PROJECT_ID [--schedule "0 9 * * *"] [--enabled true]`

- `--schedule` 為 5 段 cron（分 時 日 月 週）。省略則為手動觸發。
- `--project-id` 必填，自動化掛在需求之下。
- `--enabled` 預設為 true。

## 列出 / 讀取 / 更新

- `multara automation list`
- `multara automation get AUTOMATION_ID`
- `multara automation update AUTOMATION_ID [--name "…"] [--prompt "…"] [--schedule "…"] [--enabled true|false]`

## 規則

- 建立對外、部署、花錢或破壞性的自動化：先預覽，等人確認。
- 像每日摘要這類例行工作適合用自動化，而不是每次手動詢問。
