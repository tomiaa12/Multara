---
name: multara-creating-agents
description: 에이전트를 생성하고 수정합니다. 사용자가 specialist, 새 에이전트 생성, 지침 또는 런타임 설정 변경을 언급할 때 사용합니다.
---

# 에이전트

에이전트는 배정할 수 있는 실행 주체입니다. 총괄 에이전트 Miku는 삭제할 수 없으며, CLI로 `system_key`를 위조할 수도 없습니다.

## 생성

`multara agent create --name "…" --runtime-id RUNTIME_ID [--description "…"] [--instructions "…"] [--avatar "🤖"]`

- `--runtime-id`는 필수입니다. 보통 `multara runtime list`에서 온라인인 첫 번째 런타임을 사용합니다.
- 새로 만들어지는 것은 사용자 specialist이며 `system_key`가 없습니다.
- `description`은 사람이 보기 위한 것이며 prompt에는 들어가지 않습니다.

## 수정

`multara agent update AGENT_ID [--name "…"] [--description "…"] [--instructions "…"] [--runtime-id RUNTIME_ID] [--avatar "…"] [--max-concurrent-tasks N]`

Miku는 표시 이름, 작업 영역 메모(`instructions`), 런타임, 동시 실행 수를 바꿀 수 있습니다. `system_key`는 바꿀 수 없고 삭제할 수도 없습니다.

## 목록 / 조회

- `multara agent list`
- `multara agent get AGENT_ID`
- `multara agent skills` — 내장 skill 이름

## 규칙

- 에이전트를 만들거나 크게 바꾸기 전에는 미리보기를 보여 주고 사람의 확인을 기다립니다.
- 반복해서 필요한 능력은 specialist로 만들고, 모든 일을 총괄 에이전트에게 몰아주지 마세요.
