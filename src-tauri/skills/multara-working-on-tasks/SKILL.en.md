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
- Once all subtasks are settled (no longer backlog/todo/in_progress/blocked), the system wakes the parent task's owner again to summarize. When that summary run succeeds and no subtask is still open, the system moves the parent to `done`. Keep the parent `in_progress` while subtasks are open.
- Don't both create a subtask for the same person and expect an automatic fan-out in the same conversation; board subtasks and squad chat delegation are two separate paths.

## Update

`multara task update TASK_ID --status done --comment "Delivery notes"`

You can change `title`, `description`, `status`, `assignee`, and `project`. `--comment` is written into that task's conversation as a system message.

## Status / assignment

- `multara task status TASK_ID todo|backlog|in_progress|done|blocked|cancelled`
- `multara task assign TASK_ID --to AGENT_ID`
- Setting the status to `todo` only puts it back on the to-do list; it does not queue another run.

## List / get / comment

- `multara task list [--status todo] [--assignee <id>] [--project <id>]`
- `multara task get TASK_ID`
- `multara task comment TASK_ID --comment "Progress"`
- `multara task runs TASK_ID`

## Who writes the status

The system performs only these automatic syncs:

1. When a task starts from `backlog` / `todo` / `done` / `blocked`, it moves to `in_progress`. Cancelled stays cancelled.
2. When a run succeeds and the task has no other active runs, no open subtasks, and is not waiting on a plan, an `in_progress` task moves to `done`. File edits are marked for review separately; that is not a status.
3. When a run fails or is interrupted and the task has no other active runs, an `in_progress` task returns to `todo`, with the failure or interruption recorded.
4. When a plan is waiting for approval, an `in_progress` task moves to `blocked`.

The system writes `done` when the run finishes. Do not write a status just so a person can look at the result. Use `blocked` when stuck. Stay `in_progress` while subtasks are still open.

As work progresses, update the status through the CLI (you can write it at any point during a turn; don't wait until the end):

| Situation | Status |
|-----------|--------|
| You start delivering the task itself (implementation, research, design, review — whatever the task asks for) | `in_progress` immediately |
| Not finished yet and work continues (including subtasks not all in) | Stay `in_progress` |
| Blocked by an external dependency or condition | `blocked` |
| Only answering questions, chatting, or consulting on someone else's work, without advancing this task's delivery | **Do not change the status** |

`done` means this round has finished, not that the code is merged or the user has accepted it. Squad leads: delegating is not delivery; keep the parent `in_progress`. After subtasks settle and the summary run succeeds, the system writes `done`.

## Rules

- When the workbench is already bound to a project workspace, don't use "create a task first" to block the code changes the user asked for.
- If creating would change assignment, scope, or permissions, show a preview before acting.
- Write progress in the task conversation. Keep updates short, with the task identifier and the next step.
