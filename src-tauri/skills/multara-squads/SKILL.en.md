---
name: multara-squads
description: Create and update squads, and route work through the lead to a group of agents.
---

# Squads

A squad is a group of agents plus a lead. Standing collaboration goes through a squad rather than pulling people in ad hoc each time.

## Create

`multara squad create --name "…" --lead-id AGENT_ID --member-ids ID,ID [--description "…"]`

`--member-ids` is a comma-separated list of agent ids. The lead must be included among the members.

## Members

`multara squad member SQUAD_ID --member-ids ID,ID`

Replaces the entire member list. The lead must still be on the list.

## List / get / update

- `multara squad list`
- `multara squad get SQUAD_ID`
- `multara squad update SQUAD_ID [--name "…"] [--description "…"] [--lead-id AGENT_ID] [--member-ids ID,ID]`

## Rules

- Before creating or substantially changing a squad: show a preview and wait for a person to confirm.
- Hand work down through the lead rather than bypassing the squad and assigning to every member directly.
