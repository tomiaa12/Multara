---
name: multara-creating-agents
description: エージェントを作成・更新します。ユーザーが specialist、新しいエージェントの作成、指示やランタイム設定の変更に言及したときに使います。
---

# エージェント

エージェントは、割り当て可能な実行主体です。統括エージェントの Miku は削除できず、CLI から `system_key` を偽装することもできません。

## 作成

`multara agent create --name "…" --runtime-id RUNTIME_ID [--description "…"] [--instructions "…"] [--avatar "🤖"]`

- `--runtime-id` は必須です。通常は `multara runtime list` で最初にオンラインのランタイムを使います。
- 作成されるのはユーザー specialist で、`system_key` はありません。
- `description` は人が読むためのもので、prompt には含まれません。

## 更新

`multara agent update AGENT_ID [--name "…"] [--description "…"] [--instructions "…"] [--runtime-id RUNTIME_ID] [--avatar "…"] [--max-concurrent-tasks N]`

Miku は、表示名、ワークスペースのメモ（`instructions`）、ランタイム、並列数を変更できます。`system_key` の変更と削除はできません。

## 一覧 / 取得

- `multara agent list`
- `multara agent get AGENT_ID`
- `multara agent skills` — 組み込み skill の名前

## ルール

- エージェントを作成または大幅に変更する場合は、先にプレビューを提示し、人の確認を待ちます。
- 繰り返し必要になる能力は specialist にし、すべての作業を統括エージェントに積み上げないようにします。
