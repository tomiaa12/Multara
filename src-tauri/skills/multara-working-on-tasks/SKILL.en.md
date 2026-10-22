---
name: multara-working-on-tasks
description: Create and update tasks, and list tasks and runs. Use when the user mentions tasks, issues, or handing work to an agent.
---

# Tasks

A task is a unit of work that can be assigned and tracked. When you need to leave an owner, a status, and a result, or hand the work to another agent or split it across multiple rounds, create a task.

When this run in the workbench is already bound to a project workspace and the user asks for code changes, just make the changes. Don't create a task first in order to change code.

## Create

`multara task create --title "…" [--description "…"] [--assignee <agentId>] [--project <id>] [--status todo|backlog] [--parent <taskId>]`

- `--status` defaults to `todo`: if there is an assignee, a run is queued immediately.
- `--status backlog`: only saved, not run.
- If `--assignee` is omitted, the task is assigned to the current agent; to assign it to someone else you must pass their agent id (run `multara agent list` first).
- `--project` may be omitted; once a project is bound, the run carries the project's workspace.
- `--parent`: attaches the task under a parent task, so the subtask appears on the right side of the parent's detail view, and an assignment (@-mention) message is written in the parent conversation. The current task id is in the environment variable `MULTARA_TASK_ID`; when you create a subtask inside a task's run, it is also attached to the current task automatically even if `--parent` is not passed.
- On success, stdout is the task JSON (including `id`).

## Delegating (including subtasks)

When you need other agents or squad members to work in parallel, create subtasks and assign them; don't just write "assigned to so-and-so" in the body text:

```bash
multara task create --title "…" --description "…" --assignee <agentId> --parent "$MULTARA_TASK_ID" --status todo
```

- The parent task stays `in_progress`; delegating is not delivery.
- Once all subtasks are settled (no longer backlog/todo/in_progress/blocked), the system wakes the parent task's owner again to summarize; when woken, look at the subtask results and then decide whether the parent moves to `in_review` / `done`.
- Don't both create a subtask for the same person and expect an automatic fan-out in the same conversation; board subtasks and squad chat delegation are two separate paths.

## Update

`multara task update TASK_ID --status done --comment "Delivery notes"`

You can change `title`, `description`, `status`, `assignee`, and `project`. `--comment` is written into that task's conversation as a system message.

## Status / assignment

- `multara task status TASK_ID todo|backlog|in_progress|in_review|done|blocked|cancelled`
- `multara task assign TASK_ID --to AGENT_ID`
- Setting the status to `todo` only puts it back on the to-do list; it does not queue another run.

## List / get / comment

- `multara task list [--status todo] [--assignee <id>] [--project <id>]`
- `multara task get TASK_ID`
- `multara task comment TASK_ID --comment "Progress"`
- `multara task runs TASK_ID`

## Who writes the status

The system performs only these automatic syncs:

1. When a task actually starts running from `backlog` / `todo`, it moves to `in_progress` automatically.
2. When a run succeeds and the task has no other active runs, if it is still `in_progress`, it moves to `in_review`.
3. When a run fails and the task has no other active runs, if it is still `in_progress`, it falls back to `todo`.

**It never writes `done` automatically.** Actually completing the goal still has to be recorded explicitly.

As work progresses, update the status through the CLI (you can write it at any point during a turn; don't wait until the end):

| Situation | Status |
|-----------|--------|
| You start delivering the task itself (implementation, research, design, review — whatever the task asks for) | `in_progress` immediately |
| This round has delivered the task's own result and is waiting for a person to confirm / review | `in_review` |
| Not finished yet and work continues (including subtasks not all in) | Stay `in_progress` |
| Blocked by an external dependency or condition | `blocked` |
| The goal is truly achieved, or the user confirms completion | `done` |
| Only answering questions, chatting, or consulting on someone else's work, without advancing this task's delivery | **Do not change the status** |

`done` means the goal is complete, not "this run finished." Squad leads: delegating to members is not delivery; keep the parent task `in_progress`, and move it to `in_review` / `done` once the overall goal is confirmed.

## Rules

- When the workbench is already bound to a project workspace, don't use "create a task first" to block the code changes the user asked for.
- If creating would change assignment, scope, or permissions, show a preview before acting.
- Write progress in the task conversation. Keep updates short, with the task identifier and the next step.
