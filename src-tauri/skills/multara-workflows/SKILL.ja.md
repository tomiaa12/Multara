---
name: multara-workflows
description: ワークフローを作成・編集します。複数の手順をエージェントに任せたり、分岐、人の承認、連携メッセージからの起動をしたいときに使います。
---

# ワークフロー

ワークフローのノードは開始、エージェント、人のゲート、終了の四つだけです。開始ノードは作成時に付きます。追加も削除もできません。終了ノードは一つだけです。コマンドが変えるのは下書きです。実行するには先に公開します。

## ワークフロー

- `multara workflow create --name "…"`。`--description "…"` も付けられます
- `multara workflow list`
- `multara workflow get ID` は下書きの要約、`multara workflow draft ID` は下書き全体
- `multara workflow rename ID --name "…"`
- `multara workflow duplicate ID`
- `multara workflow enable ID` / `multara workflow disable ID`
- `multara workflow publish ID` は検証を通ったときだけ公開します
- `multara workflow delete ID` は実行が残っていると拒否します
- `multara workflow run ID --input key=value`、または `--input-json '{…}'`
- `multara workflow runs [ID]`

## ノード

- `multara workflow node add ID --kind agent|output --title "…" [--id step] [--prompt "…"] [--config '{…}'] [--timeout 30]`
- `multara workflow node set ID NODE --title "…" --prompt "…" --config '{…}'`
- `multara workflow node remove ID NODE` は、それに触れる接続も消します

ノード id は小文字で始まり、小文字・数字・アンダースコアだけです。エージェントの `--config` は既存の設定にマージされます。

- `target`：`{ "type": "agent" または "squad", "id": "…" }`
- `prompt`：このステップですること
- `context`：`previous` は前のステップだけ、`all` は開始から今まで
- `route`：出口が複数のとき、`all` はすべてへ、`auto` はこのステップのエージェントが選び、`human` は人が選ぶまで止まります。出口が一つなら選ぶ必要はありません
- `allowTaskCommands`：`true` ならこのステップはタスクを作成・変更できます

人のゲートは最初から三つの決定を持ちます。`approve`、`changes`、`reject` です。

## 接続

- `multara workflow edge add ID --from NODE --to NODE [--when approve|changes|reject] [--max-iterations 2]`
- `multara workflow edge remove ID EDGE_ID`

`--when` は人のゲートから出る接続だけに使います。`--max-iterations` は上流へ戻す回数で、1 から 20 です。

## 開始の入力

- `multara workflow input add ID --name topic [--label "件名"] [--required true|false] [--default "…"] [--primary]`
- `multara workflow input set ID NAME …`
- `multara workflow input remove ID NAME`

新しい入力は必須が既定です。`--primary` は一つだけ残ります。連携で `/flow 名前` の後に書いた本文がその入力に入ります。どれも指定しなければ最初の入力に入ります。

## ステップの実行中

実行中のワークフローノードはキャンバスを変更できません。出口が複数で `route` が `auto` のときは、`multara node next --id NODE --reason "…"` で既存の次を選びます。できないときは `multara node fail --reason "…"` です。`route` が `all` のときは選びません。

## ルール

- 外部へ送る、デプロイする、費用がかかる、またはデータを消すワークフローは、先に下書きを人に見せ、確認してから `publish` と `enable` をします。
- 起動元（手動、自動化、Webhook、連携メッセージ、コマンドライン）は開始ノードで変えます。この CLI は起動元を変えません。
