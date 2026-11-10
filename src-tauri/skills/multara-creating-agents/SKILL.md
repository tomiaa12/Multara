---
name: multara-creating-agents
description: 创建、更新智能体。用户提到 specialist、新建智能体、改指令或运行时配置时使用。
---

# 智能体

智能体是可指派的执行者。总管 Miku 不能删除，也不能通过 CLI 伪造 `system_key`。

## 创建

`multara agent create --name "…" --runtime-id RUNTIME_ID [--description "…"] [--instructions "…"] [--avatar "🤖"] [--skills 名称或id,…] [--mcps 名称或id,…]`

- `--runtime-id` 必填；一般用 `multara runtime list` 里第一个在线运行时。
- 新建的是用户 specialist，没有 `system_key`。
- `description` 只给人看，不进 prompt。

## 更新

`multara agent update AGENT_ID [--name "…"] [--description "…"] [--instructions "…"] [--runtime-id RUNTIME_ID] [--avatar "…"] [--max-concurrent-tasks N] [--skills 名称或id,…] [--mcps 名称或id,…] [--remove-skills …] [--remove-mcps …]`

Miku 可以改显示名、工作区备注（`instructions`）、运行时和并发；不能改 `system_key`，不能删。

`--skills` / `--mcps` 是给智能体勾选，追加到已有列表，不会清掉原来的。取消勾选用 `--remove-skills` / `--remove-mcps`。名称能唯一对上就用名称，多个来源同名时用 `multara skill list` 或 `multara mcp list` 里的完整 id。用户要求绑上 Skill 或 MCP 时直接执行，不要让用户自己去设置里勾。

## 列出 / 读取

- `multara agent list`
- `multara agent get AGENT_ID`
- `multara agent skills` — 内置 skill 名称
- `multara skill list` — 本机扫描到的 Skill
- `multara mcp list` — 本机扫描到的 MCP

## 规则

- 创建或大改智能体：先预览，等人确认。
- 反复出现的能力做成 specialist，而不是把所有工作堆在总管身上。
