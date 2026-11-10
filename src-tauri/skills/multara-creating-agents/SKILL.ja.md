---
name: multara-creating-agents
description: エージェントを作成・更新します。ユーザーが specialist、新しいエージェントの作成、指示やランタイム設定の変更に言及したときに使います。
---

# エージェント

エージェントは、割り当て可能な実行主体です。執事エージェントの Miku は削除できず、CLI から `system_key` を偽装することもできません。

## 作成

`multara agent create --name "…" --runtime-id RUNTIME_ID [--description "…"] [--instructions "…"] [--avatar "🤖"] [--skills 名前またはid,…] [--mcps 名前またはid,…]`

- `--runtime-id` は必須です。通常は `multara runtime list` で最初にオンラインのランタイムを使います。
- 作成されるのはユーザー specialist で、`system_key` はありません。
- `description` は人が読むためのもので、prompt には含まれません。

## 更新

`multara agent update AGENT_ID [--name "…"] [--description "…"] [--instructions "…"] [--runtime-id RUNTIME_ID] [--avatar "…"] [--max-concurrent-tasks N] [--skills 名前またはid,…] [--mcps 名前またはid,…] [--remove-skills …] [--remove-mcps …]`

Miku は、表示名、ワークスペースのメモ（`instructions`）、ランタイム、並列数を変更できます。`system_key` の変更と削除はできません。

`--skills` と `--mcps` はそのエージェントにチェックを入れ、既存の一覧へ追加します。外すときは `--remove-skills` と `--remove-mcps` です。名前が一意なら名前を使い、同名が複数あるときは `multara skill list` または `multara mcp list` の完全な id を使います。ユーザーが Skill や MCP の割り当てを求めたらコマンドを実行し、設定画面で自分でチェックするよう案内しないでください。

## 一覧 / 取得

- `multara agent list`
- `multara agent get AGENT_ID`
- `multara agent skills` — 組み込み skill の名前
- `multara skill list` — このマシンで見つかった Skill
- `multara mcp list` — このマシンで見つかった MCP

## ルール

- エージェントを作成または大幅に変更する場合は、先にプレビューを提示し、人の確認を待ちます。
- 繰り返し必要になる能力は specialist にし、すべての作業を執事エージェントに積み上げないようにします。
