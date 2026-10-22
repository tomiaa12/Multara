---
name: multara-skill-importing
description: ワークスペースの skill と組み込み skill を一覧表示します。第 1 期では URL からのインポートはサポートしません。
---

# Skill

Skill は、エージェントが読み込める操作手順です。組み込みの `multara-*` skill はアプリに同梱され、ワークスペースの skill は紐づけたリポジトリの `.cursor/skills` などのディレクトリから取得されます。

## コマンド

- `multara skill list` — 現在のワークスペースで見えるユーザー skill（スキャン結果）
- `multara skill get SKILL_ID`
- `multara skill builtin` — プロダクト組み込み skill の名前と説明
- `multara skill import` — 第 1 期では URL インポートは未実装

## ルール

- プラットフォームのオブジェクトを作成・変更する前に、対応する `multara-*` skill を読み込みます。
- SKILL.md の全文をシステムプロンプトに貼り付けないでください。ランタイムの brief には名前だけを載せ、必要になったときにファイルを読みます。
- `~/.multara/skills` はユーザー skill です。組み込みの `multara-*` と混在させないでください。
