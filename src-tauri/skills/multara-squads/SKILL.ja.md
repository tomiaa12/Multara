---
name: multara-squads
description: スクワッドを作成・更新し、リーダー経由で複数のエージェントに作業を割り当てます。
---

# スクワッド

スクワッドは、エージェントのグループと 1 人のリーダーで構成されます。常設の協働は、そのつど人を呼び集めるのではなく、スクワッドで行います。

## 作成

`multara squad create --name "…" --lead-id AGENT_ID --member-ids ID,ID [--description "…"]`

`--member-ids` はエージェント id のカンマ区切りリストです。リーダーはメンバーに含まれている必要があります。

## メンバー

`multara squad member SQUAD_ID --member-ids ID,ID`

メンバー名簿全体を置き換えます。リーダーは引き続き名簿に含まれている必要があります。

## 一覧 / 取得 / 更新

- `multara squad list`
- `multara squad get SQUAD_ID`
- `multara squad update SQUAD_ID [--name "…"] [--description "…"] [--lead-id AGENT_ID] [--member-ids ID,ID]`

## ルール

- スクワッドを作成または大幅に変更する場合は、先にプレビューを提示し、人の確認を待ちます。
- スクワッドを飛ばして全メンバーに直接割り当てるのではなく、リーダー経由で作業を指示します。
