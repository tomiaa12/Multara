---
name: multara-creating-agents
description: Create and update agents. Use when the user mentions a specialist, creating a new agent, or changing instructions or runtime configuration.
---

# Agents

An agent is an assignable executor. The steward Miku cannot be deleted, and `system_key` cannot be forged through the CLI.

## Create

`multara agent create --name "…" --runtime-id RUNTIME_ID [--description "…"] [--instructions "…"] [--avatar "🤖"]`

- `--runtime-id` is required; normally use the first online runtime from `multara runtime list`.
- What you create is a user specialist and has no `system_key`.
- `description` is for people only and does not go into the prompt.

## Update

`multara agent update AGENT_ID [--name "…"] [--description "…"] [--instructions "…"] [--runtime-id RUNTIME_ID] [--avatar "…"] [--max-concurrent-tasks N]`

Miku may change the display name, workspace notes (`instructions`), runtime, and concurrency; it cannot change `system_key` and cannot be deleted.

## List / get

- `multara agent list`
- `multara agent get AGENT_ID`
- `multara agent skills` — names of the built-in skills

## Rules

- Before creating or substantially changing an agent: show a preview and wait for a person to confirm.
- Turn recurring capabilities into specialists rather than piling all the work onto the steward.
