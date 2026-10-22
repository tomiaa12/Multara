---
name: multara-projects-and-workspaces
description: Create and update projects (requirements) and bind workspaces. Use when several tasks share one outcome, or when code needs to change.
---

# Projects and workspaces

A project ties a group of related tasks to the same goal and repository context. In a chat that isn't bound to a project, don't edit arbitrary directories.

## Create

`multara project create --name "…" [--description "…"]`

Binding a workspace is done in the app (add a Git repository or a folder). Once bound, queued runs under that project start with the repository path.

## List / get / update

- `multara project list`
- `multara project get PROJECT_ID`
- `multara project update PROJECT_ID [--name "…"] [--description "…"]`

## Rules

- When several tasks share a deliverable, put them under the same project.
- If this run is already bound to a project workspace: when the user asks for code changes, edit the listed directories directly.
- If there is no project workspace yet: ask the user to open a project, or run `multara task create --project <id> --status todo`.
- The isolation mode (worktree / copy / direct) is decided by the app when it creates the workspace; don't hand-edit the path of an isolated copy on disk.
