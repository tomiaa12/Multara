---
name: multara-creating-agents
description: 创建、更新智能体。用户提到 specialist、新建智能体、改指令或运行时配置时使用。
---

# 智能体

智能体是可指派的执行者。总管 Miku 不能删除，也不能通过 CLI 伪造 `system_key`。

## 创建

`multara agent create --name "…" --runtime-id RUNTIME_ID [--description "…"] [--instructions "…"] [--avatar "🤖"]`

- `--runtime-id` 必填；一般用 `multara runtime list` 里第一个在线运行时。
- 新建的是用户 specialist，没有 `system_key`。
- `description` 只给人看，不进 prompt。

## 更新

`multara agent update AGENT_ID [--name "…"] [--description "…"] [--instructions "…"] [--runtime-id RUNTIME_ID] [--avatar "…"] [--max-concurrent-tasks N]`

Miku 可以改显示名、工作区备注（`instructions`）、运行时和并发；不能改 `system_key`，不能删。

## 列出 / 读取

- `multara agent list`
- `multara agent get AGENT_ID`
- `multara agent skills` — 内置 skill 名称

## 规则

- 创建或大改智能体：先预览，等人确认。
- 反复出现的能力做成 specialist，而不是把所有工作堆在总管身上。
