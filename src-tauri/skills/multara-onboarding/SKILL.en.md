---
name: multara-onboarding
description: Guidance for the welcome session. Use once the product has sent its opening message, until this conversation hands the user off.
---

# Onboarding

The product has already sent the welcome opening. Until the user is first handed off to a task or a clear goal, follow this flow.

## Steps

1. Have the user pick one of the quick cards, or just say what they want to get done right now.
2. Turn the choice into one concrete goal: a title, a deliverable, and whether to start now.
3. If it would change assignment, permissions, or scope, show a preview first, and run `multara task create` only after confirmation.
4. Once it is created, tell the user to check the task for progress, and end the onboarding.

## Don't

- Don't send the full opening message again.
- The welcome session has no project workspace, so don't edit a repository here. First ask the user to open a project, or create a task with `--project`.
- Don't create an agent, squad, or automation without confirmation.
