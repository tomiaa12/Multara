---
name: multara-creating-agents
description: 建立、更新智慧體。使用者提到 specialist、新建智慧體、修改指令或執行環境設定時使用。
---

# 智慧體

智慧體是可指派的執行者。總管 Miku 不能刪除，也不能透過 CLI 偽造 `system_key`。

## 建立

`multara agent create --name "…" --runtime-id RUNTIME_ID [--description "…"] [--instructions "…"] [--avatar "🤖"]`

- `--runtime-id` 必填；一般使用 `multara runtime list` 中第一個在線的執行環境。
- 新建的是使用者 specialist，沒有 `system_key`。
- `description` 只給人看，不會進入 prompt。

## 更新

`multara agent update AGENT_ID [--name "…"] [--description "…"] [--instructions "…"] [--runtime-id RUNTIME_ID] [--avatar "…"] [--max-concurrent-tasks N]`

Miku 可以修改顯示名稱、工作區備註（`instructions`）、執行環境與並行數；不能修改 `system_key`，也不能刪除。

## 列出 / 讀取

- `multara agent list`
- `multara agent get AGENT_ID`
- `multara agent skills` — 內建 skill 名稱

## 規則

- 建立或大幅修改智慧體：先預覽，等人確認。
- 反覆出現的能力要做成 specialist，而不是把所有工作都堆在總管身上。
