---
name: multara-automations
description: 定時・インターバル・イベントで起動する自動化を作成します。誰かが尋ねてくるのを待たず、スケジュールや外部イベントで作業を開始すべきときに使います。
---

# 自動化

自動化は、cron・インターバル・イベントに応じてエージェントの実行を 1 回起動します。

## 作成

`multara automation create --name "…" --agent-id AGENT_ID --prompt "…" --project-id PROJECT_ID [--schedule "0 9 * * *"] [--enabled true]`

- `--schedule` は 5 フィールドの cron 式（分 時 日 月 曜日）です。省略すると手動トリガーになります。
- `--project-id` は必須です。自動化はプロジェクトの配下に置かれます。
- `--enabled` のデフォルトは true です。

## 一覧 / 取得 / 更新

- `multara automation list`
- `multara automation get AUTOMATION_ID`
- `multara automation update AUTOMATION_ID [--name "…"] [--prompt "…"] [--schedule "…"] [--enabled true|false]`

## ルール

- 外部への発信、デプロイ、費用の発生、または破壊的な自動化を作成する場合は、先にプレビューを提示し、人の確認を待ちます。
- 毎日のサマリーのような定型業務は、そのたびに手で依頼するのではなく、自動化に向いています。
