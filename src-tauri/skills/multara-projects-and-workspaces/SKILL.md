---
name: multara-projects-and-workspaces
description: 创建、更新需求（项目），绑定工作区。多个任务共享一个结果、或需要改代码时使用。
---

# 需求与工作区

需求把一组相关任务绑在同一个目标和仓库上下文上。没有绑定需求的聊天不要改随机目录。

## 创建

`multara project create --name "…" [--description "…"]`

绑定工作区在应用里完成（添加 Git 仓库或文件夹）。绑定后，该需求下排队的运行会带着仓库路径启动。

## 列出 / 读取 / 更新

- `multara project list`
- `multara project get PROJECT_ID`
- `multara project update PROJECT_ID [--name "…"] [--description "…"]`

## 规则

- 多个任务共享交付物时放到同一需求下。
- 这次运行已经绑了需求工作区：用户要求改代码时，直接改列出的目录。
- 还没有需求工作区：让用户打开一个需求，或 `multara task create --project <id> --status todo`。
- 隔离模式（worktree / copy / direct）由应用创建工作区时决定，不要手改磁盘上的隔离副本路径。
