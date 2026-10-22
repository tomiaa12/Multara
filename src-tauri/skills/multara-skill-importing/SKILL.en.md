---
name: multara-skill-importing
description: List workspace skills and built-in skills. URL import is not supported in the first release.
---

# Skills

A skill is a set of operating instructions an agent can load. Built-in `multara-*` skills ship with the app; workspace skills come from directories such as `.cursor/skills` in the bound repository.

## Commands

- `multara skill list` — user skills visible in the current workspace (scan results)
- `multara skill get SKILL_ID`
- `multara skill builtin` — names and descriptions of the product's built-in skills
- `multara skill import` — URL import is not implemented in the first release

## Rules

- Before creating or changing platform objects, load the matching `multara-*` skill.
- Don't paste the full SKILL.md into the system prompt; the runtime brief lists names only, and you read the file when needed.
- `~/.multara/skills` holds user skills; don't mix it up with the built-in `multara-*` skills.
