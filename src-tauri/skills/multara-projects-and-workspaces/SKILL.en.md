---
name: multara-projects-and-workspaces
description: Create and update projects (requirements) and bind workspaces. Use when several tasks share one outcome, or when code needs to change.
---

# Projects and workspaces

A project ties a group of related tasks to the same goal and repository context. In a chat that isn't bound to a project, don't edit arbitrary directories.

## Create

`multara project create --name "…" [--description "…"]`

Add one folder to the project. `--path` is a single repository or plain directory. A parent that contains several repositories is rejected and the paths are listed.

- New Worktree: `multara workspace add --project PROJECT_ID --path /absolute/path --mode worktree-new --branch new-name [--base source-branch]`
- Existing branch as a Worktree: `multara workspace add --project PROJECT_ID --path /absolute/path --mode worktree-existing --branch existing-name`
- Edit the real directory: `multara workspace add --project PROJECT_ID --path /absolute/path --mode current`

`--base` defaults to `HEAD`. `multara workspace list [--project PROJECT_ID]` lists bound directories. Once bound, queued runs under that project start with the repository path.

## List / get / update

- `multara project list`
- `multara project get PROJECT_ID`
- `multara project update PROJECT_ID [--name "…"] [--description "…"]`

## Rules

- When several tasks share a deliverable, put them under the same project.
- If this run is already bound to a project workspace: when the user asks for code changes, edit the listed directories directly.
- If there is no project workspace yet: ask the user to open a project, or run `multara task create --project <id> --status todo`.
- Choose a Worktree or the real directory with the command above. After it is created, don't hand-edit the isolated copy on disk.
