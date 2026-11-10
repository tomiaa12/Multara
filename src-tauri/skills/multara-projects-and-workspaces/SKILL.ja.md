---
name: multara-projects-and-workspaces
description: プロジェクト（要件）の作成・更新とワークスペースの紐づけを行います。複数のタスクが 1 つの成果を共有するとき、またはコードを変更する必要があるときに使います。
---

# プロジェクトとワークスペース

プロジェクトは、関連するタスクのまとまりを、同じゴールとリポジトリのコンテキストに結び付けます。プロジェクトに紐づいていないチャットでは、無関係なディレクトリを編集しないでください。

## 作成

`multara project create --name "…" [--description "…"]`

プロジェクトにフォルダーを 1 つ追加します。`--path` はリポジトリまたは通常のディレクトリを 1 つだけ渡します。複数のリポジトリを含む親ディレクトリは拒否され、個別に追加できるパスが列挙されます。

- 新しい Worktree：`multara workspace add --project PROJECT_ID --path /絶対パス --mode worktree-new --branch 新しいブランチ名 [--base 分岐元]`
- 既存ブランチを Worktree にする：`multara workspace add --project PROJECT_ID --path /絶対パス --mode worktree-existing --branch 既存ブランチ`
- 実ディレクトリを直接編集：`multara workspace add --project PROJECT_ID --path /絶対パス --mode current`

`--base` を省略すると `HEAD` から分岐します。`multara workspace list [--project PROJECT_ID]` で紐づいたディレクトリを確認します。紐づけ後は、そのプロジェクトでキューに入った実行がリポジトリのパス付きで起動します。

## 一覧 / 取得 / 更新

- `multara project list`
- `multara project get PROJECT_ID`
- `multara project update PROJECT_ID [--name "…"] [--description "…"]`

## ルール

- 複数のタスクが成果物を共有する場合は、同じプロジェクトにまとめます。
- この実行がすでにプロジェクトのワークスペースに紐づいている場合：ユーザーからコード変更を求められたら、一覧にあるディレクトリを直接編集します。
- まだプロジェクトのワークスペースがない場合：ユーザーにプロジェクトを開いてもらうか、`multara task create --project <id> --status todo` を実行します。
- 上のコマンドで Worktree か実ディレクトリかを選びます。作成後に、ディスク上の分離コピーのパスを手で書き換えないでください。
