---
name: multara-squads
description: 스쿼드를 생성·수정하고, 리더를 통해 에이전트 그룹에 작업을 전달합니다.
---

# 스쿼드

스쿼드는 에이전트 그룹과 리더 한 명으로 이루어집니다. 상설 협업은 매번 임시로 사람을 불러 모으는 대신 스쿼드로 진행합니다.

## 생성

`multara squad create --name "…" --lead-id AGENT_ID --member-ids ID,ID [--description "…"]`

`--member-ids`는 쉼표로 구분한 에이전트 id 목록입니다. 리더는 반드시 멤버에 포함되어야 합니다.

## 멤버

`multara squad member SQUAD_ID --member-ids ID,ID`

멤버 명단 전체를 교체합니다. 리더는 여전히 명단에 있어야 합니다.

## 목록 / 조회 / 수정

- `multara squad list`
- `multara squad get SQUAD_ID`
- `multara squad update SQUAD_ID [--name "…"] [--description "…"] [--lead-id AGENT_ID] [--member-ids ID,ID]`

## 규칙

- 스쿼드를 만들거나 크게 바꾸기 전에는 미리보기를 보여 주고 사람의 확인을 기다립니다.
- 스쿼드를 건너뛰고 모든 멤버에게 직접 배정하지 말고, 리더를 통해 작업을 전달합니다.
