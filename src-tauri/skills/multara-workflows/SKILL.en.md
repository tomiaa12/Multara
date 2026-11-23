---
name: multara-workflows
description: Create and edit workflows. Use when the user wants several steps handed to agents, a split, a human gate, or a start from an integration message.
---

# Workflows

A workflow has only four kinds of node: start, agent, human gate, and end. The start node comes with the workflow. It cannot be added again or deleted. There can be only one end. These commands edit the draft. Publish before a run can use it.

## Workflow

- `multara workflow create --name "…"`, optionally `--description "…"`
- `multara workflow list`
- `multara workflow get ID` for a draft summary; `multara workflow draft ID` for the full draft
- `multara workflow rename ID --name "…"`
- `multara workflow duplicate ID`
- `multara workflow enable ID` / `multara workflow disable ID`
- `multara workflow publish ID` publishes only when validation passes
- `multara workflow delete ID` refuses while a run is still going
- `multara workflow run ID --input key=value`, or `--input-json '{…}'`
- `multara workflow runs [ID]`

## Nodes

- `multara workflow node add ID --kind agent|output --title "…" [--id step] [--prompt "…"] [--config '{…}'] [--timeout 30]`
- `multara workflow node set ID NODE --title "…" --prompt "…" --config '{…}'`
- `multara workflow node remove ID NODE` also removes edges that touch it

A node id starts with a lowercase letter and contains only lowercase letters, digits, and underscores. An agent's `--config` merges into the existing config:

- `target`: `{ "type": "agent" or "squad", "id": "…" }`
- `prompt`: what this step should do
- `context`: `previous` carries only the last step; `all` carries every step since the start
- `route`: with several outs, `all` sends to every one, `auto` lets this step's agent choose, `human` waits for a person. One out needs no choice
- `allowTaskCommands`: `true` lets this step create or change tasks

A gate starts with three decisions: `approve`, `changes`, and `reject`.

## Edges

- `multara workflow edge add ID --from NODE --to NODE [--when approve|changes|reject] [--max-iterations 2]`
- `multara workflow edge remove ID EDGE_ID`

`--when` is only for an edge leaving a gate. `--max-iterations` sends work back upstream, from 1 to 20.

## Start inputs

- `multara workflow input add ID --name topic [--label "Topic"] [--required true|false] [--default "…"] [--primary]`
- `multara workflow input set ID NAME …`
- `multara workflow input remove ID NAME`

A new input is required. Only one `--primary` is kept: the text after `/flow name` in an integration fills that input. If none is marked, the first input is used.

## While a step is running

A running workflow node cannot edit the canvas. When there are several outs and `route` is `auto`, choose an existing next node with `multara node next --id NODE --reason "…"`. If the step cannot be done, use `multara node fail --reason "…"`. `route` `all` needs no choice.

## Rules

- For a workflow that sends outward, deploys, spends money, or deletes data: show the draft to a person, then `publish` and `enable` after they confirm.
- Trigger sources (manual, automation, webhook, integration message, command line) are edited on the start node. This CLI does not change them.
