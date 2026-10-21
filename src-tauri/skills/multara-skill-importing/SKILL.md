---
name: multara-skill-importing
description: 列出工作区 skill 与内置 skill。第一期不支持 URL 导入。
---

# Skill

Skill 是智能体可加载的操作说明。内置 `multara-*` skill 由应用随包提供；工作区 skill 来自绑定仓库的 `.cursor/skills` 等目录。

## 命令

- `multara skill list` — 当前工作区可见的用户 skill（扫描结果）
- `multara skill get SKILL_ID`
- `multara skill builtin` — 产品内置 skill 名称与描述
- `multara skill import` — 第一期未实现 URL 导入

## 规则

- 创建或改平台对象之前，先 load 对应的 `multara-*` skill。
- 不要把 SKILL.md 全文贴进系统提示词；运行时 brief 只列名称，需要时再读文件。
- `~/.multara/skills` 是用户 skill，不要和内置 `multara-*` 混用。
