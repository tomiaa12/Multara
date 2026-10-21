---
name: multara-automations
description: 创建定时、间隔或事件触发的自动化。应当按日程或外部事件启动、而不是等人来问时使用。
---

# 自动化

自动化按 cron、间隔或事件启动一次智能体运行。

## 创建

`multara automation create --name "…" --agent-id AGENT_ID --prompt "…" --project-id PROJECT_ID [--schedule "0 9 * * *"] [--enabled true]`

- `--schedule` 为 5 段 cron（分 时 日 月 周）。省略则为手动触发。
- `--project-id` 必填，自动化挂在需求下。
- `--enabled` 默认 true。

## 列出 / 读取 / 更新

- `multara automation list`
- `multara automation get AUTOMATION_ID`
- `multara automation update AUTOMATION_ID [--name "…"] [--prompt "…"] [--schedule "…"] [--enabled true|false]`

## 规则

- 创建对外、部署、花钱或破坏性自动化：先预览，等人确认。
- 每天摘要一类的例行工作适合自动化，而不是每次手聊。
