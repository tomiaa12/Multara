---
name: multara-squads
description: 建立、更新小隊，把工作透過隊長下達給一組智慧體。
---

# 小隊

小隊是一組智慧體加上一名隊長。常設協作走小隊，而不是每次臨時拉人。

## 建立

`multara squad create --name "…" --lead-id AGENT_ID --member-ids ID,ID [--description "…"]`

`--member-ids` 為以逗號分隔的智慧體 id。隊長必須包含在成員裡。

## 成員

`multara squad member SQUAD_ID --member-ids ID,ID`

會取代整份成員名單。隊長仍須在名單中。

## 列出 / 讀取 / 更新

- `multara squad list`
- `multara squad get SQUAD_ID`
- `multara squad update SQUAD_ID [--name "…"] [--description "…"] [--lead-id AGENT_ID] [--member-ids ID,ID]`

## 規則

- 建立或大幅修改小隊：先預覽，等人確認。
- 透過隊長下達工作，而不是繞過小隊直接指派給所有成員。
