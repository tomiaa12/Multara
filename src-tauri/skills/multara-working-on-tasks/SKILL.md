---
name: multara-working-on-tasks
description: 创建、更新任务，并列出任务与运行。用户提到任务、Issue、把工作交给智能体时使用。
---

# 任务

任务是一次可指派、可追踪的工作。需要留下负责人、状态和结果，或要交给别的智能体、拆成多轮时，建成任务。

工作台里这次运行已经绑了需求工作区、用户要求改代码时，直接改。不要为了改代码先建任务。

## 创建

`multara task create --title "…" [--description "…"] [--assignee <id>] [--project <id>] [--status todo|backlog]`

- 默认 `--status todo`：有负责人时立刻排队一次运行。
- `--status backlog`：只落库，不跑。
- `--assignee` 省略则指派给当前智能体。
- `--project` 可省略；绑定需求后，运行会带上需求工作区。
- 成功时 stdout 是任务 JSON（含 `id`）。

## 更新

`multara task update TASK_ID --status done --comment "交付说明"`

可改 `title`、`description`、`status`、`assignee`、`project`。`--comment` 会作为系统消息写入该任务会话。

## 状态 / 指派

- `multara task status TASK_ID todo|backlog|in_progress|in_review|done|blocked|cancelled`
- `multara task assign TASK_ID --to AGENT_ID`
- 把状态改成 `todo` 只是放回待办，不会再排队一次运行。

## 列出 / 读取 / 评论

- `multara task list [--status todo] [--assignee <id>] [--project <id>]`
- `multara task get TASK_ID`
- `multara task comment TASK_ID --comment "进度"`
- `multara task runs TASK_ID`

## 状态由谁写

系统只会做这些自动同步：

1. 任务从 `backlog` / `todo` 真正开跑时，自动进 `in_progress`。
2. 运行成功且该任务没有其它活跃运行时，若仍是 `in_progress`，进 `in_review`。
3. 运行失败且该任务没有其它活跃运行时，若仍是 `in_progress`，回退到 `todo`。

**不会自动写成 `done`。** 目标真正完成仍要显式写入。

按工作推进用 CLI 更新状态（可在回合中随时写，不要等结束）：

| 情况 | 状态 |
|------|------|
| 开始做本任务本身的交付（实现、调研、设计、审查——只要它是任务要的） | 立刻 `in_progress` |
| 本轮已交付任务本身的结果，等人确认 / 审核 | `in_review` |
| 还没做完，还要继续（含子任务未齐） | 保持 `in_progress` |
| 被外部依赖或条件卡住 | `blocked` |
| 目标真正达成，或用户确认完成 | `done` |
| 只是答疑、闲聊、咨询别人的工作，没有推进本任务交付 | **不要改状态** |

`done` 表示目标完成，不是「这一轮 run 跑完了」。小队队长：派活给队员不算交付，父任务保持 `in_progress`，整体目标确认后再到 `in_review` / `done`。

## 规则

- 工作台已绑定需求工作区时，不要用「先建任务」挡住用户要求的代码修改。
- 创建前若会改指派、范围或权限，先给预览再执行。
- 进度写在任务会话里。更新要短，带任务标识和下一步。
