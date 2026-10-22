---
name: multara-automations
description: Create scheduled, interval, or event-triggered automations. Use when work should start on a schedule or an external event instead of waiting for someone to ask.
---

# Automations

An automation starts an agent run on a cron schedule, an interval, or an event.

## Create

`multara automation create --name "…" --agent-id AGENT_ID --prompt "…" --project-id PROJECT_ID [--schedule "0 9 * * *"] [--enabled true]`

- `--schedule` is a 5-field cron expression (minute hour day month weekday). Omit it for manual triggering.
- `--project-id` is required; an automation hangs under a project.
- `--enabled` defaults to true.

## List / get / update

- `multara automation list`
- `multara automation get AUTOMATION_ID`
- `multara automation update AUTOMATION_ID [--name "…"] [--prompt "…"] [--schedule "…"] [--enabled true|false]`

## Rules

- Before creating an automation that is external-facing, deploys, spends money, or is destructive: show a preview and wait for a person to confirm.
- Routine work such as a daily digest suits an automation better than asking by hand every time.
