---
name: multara-automations
description: 정기·주기·이벤트 기반 자동화를 만듭니다. 누가 물어보기를 기다리지 않고 일정이나 외부 이벤트로 작업을 시작해야 할 때 사용합니다.
---

# 자동화

자동화는 cron, 주기 또는 이벤트에 따라 에이전트 실행을 한 번 시작합니다.

## 생성

`multara automation create --name "…" --agent-id AGENT_ID --prompt "…" --project-id PROJECT_ID [--schedule "0 9 * * *"] [--enabled true]`

- `--schedule`은 5개 필드 cron 표현식(분 시 일 월 요일)입니다. 생략하면 수동 트리거가 됩니다.
- `--project-id`는 필수이며, 자동화는 요구사항 아래에 속합니다.
- `--enabled`의 기본값은 true입니다.

## 목록 / 조회 / 수정

- `multara automation list`
- `multara automation get AUTOMATION_ID`
- `multara automation update AUTOMATION_ID [--name "…"] [--prompt "…"] [--schedule "…"] [--enabled true|false]`

## 규칙

- 외부 전송, 배포, 비용 발생 또는 파괴적인 자동화를 만들 때는 먼저 미리보기를 보여 주고 사람의 확인을 기다립니다.
- 매일 요약처럼 반복되는 정기 업무는 매번 직접 요청하기보다 자동화가 적합합니다.
