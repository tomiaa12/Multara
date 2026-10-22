---
name: multara-skill-importing
description: 列出工作區 skill 與內建 skill。第一期不支援 URL 匯入。
---

# Skill

Skill 是智慧體可載入的操作說明。內建的 `multara-*` skill 由應用程式隨附提供；工作區 skill 來自綁定儲存庫中的 `.cursor/skills` 等目錄。

## 指令

- `multara skill list` — 目前工作區可見的使用者 skill（掃描結果）
- `multara skill get SKILL_ID`
- `multara skill builtin` — 產品內建 skill 的名稱與描述
- `multara skill import` — 第一期尚未實作 URL 匯入

## 規則

- 建立或修改平台物件之前，先載入對應的 `multara-*` skill。
- 不要把 SKILL.md 全文貼進系統提示詞；執行時 brief 只列出名稱，需要時再讀取檔案。
- `~/.multara/skills` 是使用者 skill，不要與內建的 `multara-*` 混用。
