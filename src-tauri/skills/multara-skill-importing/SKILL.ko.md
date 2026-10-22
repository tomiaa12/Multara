---
name: multara-skill-importing
description: 작업 영역 skill과 내장 skill을 나열합니다. 1차 버전에서는 URL 가져오기를 지원하지 않습니다.
---

# Skill

Skill은 에이전트가 로드할 수 있는 작업 지침입니다. 내장 `multara-*` skill은 앱에 포함되어 제공되고, 작업 영역 skill은 연결된 저장소의 `.cursor/skills` 같은 디렉터리에서 가져옵니다.

## 명령

- `multara skill list` — 현재 작업 영역에서 보이는 사용자 skill(스캔 결과)
- `multara skill get SKILL_ID`
- `multara skill builtin` — 제품 내장 skill의 이름과 설명
- `multara skill import` — 1차 버전에서는 URL 가져오기가 구현되지 않음

## 규칙

- 플랫폼 객체를 만들거나 바꾸기 전에 해당하는 `multara-*` skill을 먼저 로드합니다.
- SKILL.md 전문을 시스템 프롬프트에 붙여 넣지 마세요. 런타임 brief에는 이름만 나열하고, 필요할 때 파일을 읽습니다.
- `~/.multara/skills`는 사용자 skill입니다. 내장 `multara-*`와 섞어 쓰지 마세요.
