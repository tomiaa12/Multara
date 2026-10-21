---
name: multara-squads
description: 创建、更新小队，把工作通过队长下达给一组智能体。
---

# 小队

小队是一组智能体加上一名队长。常设协作走小队，而不是每次临时拉人。

## 创建

`multara squad create --name "…" --lead-id AGENT_ID --member-ids ID,ID [--description "…"]`

`--member-ids` 为逗号分隔的智能体 id。队长必须包含在成员里。

## 成员

`multara squad member SQUAD_ID --member-ids ID,ID`

替换整份成员名单。队长仍须在名单中。

## 列出 / 读取 / 更新

- `multara squad list`
- `multara squad get SQUAD_ID`
- `multara squad update SQUAD_ID [--name "…"] [--description "…"] [--lead-id AGENT_ID] [--member-ids ID,ID]`

## 规则

- 创建或大改小队：先预览，等人确认。
- 通过队长下达工作，而不是绕过小队直接指派所有成员。
