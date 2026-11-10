---
name: multara-projects-and-workspaces
description: 创建、更新需求（项目），绑定工作区。多个任务共享一个结果、或需要改代码时使用。
---

# 需求与工作区

需求把一组相关任务绑在同一个目标和仓库上下文上。没有绑定需求的聊天不要改随机目录。

## 创建

`multara project create --name "…" [--description "…"]`

给需求添加一个文件夹。`--path` 一次只传一个仓库或普通目录；父目录里有多个仓库时会拒绝，并列出可以分别添加的路径。

- 新建 Worktree：`multara workspace add --project PROJECT_ID --path /绝对路径 --mode worktree-new --branch 新分支名 [--base 迁出的分支]`
- 迁出已有分支：`multara workspace add --project PROJECT_ID --path /绝对路径 --mode worktree-existing --branch 已有分支`
- 直接改真实目录：`multara workspace add --project PROJECT_ID --path /绝对路径 --mode current`

`--base` 不写时从 `HEAD` 迁出。`multara workspace list [--project PROJECT_ID]` 查看已绑定的目录。绑定后，该需求下排队的运行会带着仓库路径启动。

## 列出 / 读取 / 更新

- `multara project list`
- `multara project get PROJECT_ID`
- `multara project update PROJECT_ID [--name "…"] [--description "…"]`

## 规则

- 多个任务共享交付物时放到同一需求下。
- 这次运行已经绑了需求工作区：用户要求改代码时，直接改列出的目录。
- 还没有需求工作区：让用户打开一个需求，或 `multara task create --project <id> --status todo`。
- 用上面的命令选择 Worktree 或真实目录。建好后不要手改磁盘上的隔离副本路径。
