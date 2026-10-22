---
name: multara-working-on-tasks
description: 작업을 생성·수정하고 작업과 실행 목록을 조회합니다. 사용자가 작업이나 Issue를 언급하거나 에이전트에게 일을 맡기려 할 때 사용합니다.
---

# 작업

작업은 배정하고 추적할 수 있는 하나의 일 단위입니다. 담당자, 상태, 결과를 남겨야 하거나 다른 에이전트에게 넘기거나 여러 라운드로 나눠야 할 때는 작업으로 만듭니다.

작업대에서 이번 실행이 이미 요구사항 작업 영역에 연결되어 있고 사용자가 코드 수정을 요청했다면 바로 수정합니다. 코드를 고치기 위해 먼저 작업을 만들지 마세요.

## 생성

`multara task create --title "…" [--description "…"] [--assignee <agentId>] [--project <id>] [--status todo|backlog] [--parent <taskId>]`

- 기본값은 `--status todo`이며, 담당자가 있으면 즉시 실행 한 번이 대기열에 올라갑니다.
- `--status backlog`: 저장만 하고 실행하지 않습니다.
- `--assignee`를 생략하면 현재 에이전트에게 배정됩니다. 다른 에이전트에게 맡기려면 그 에이전트의 id를 반드시 전달해야 합니다(먼저 `multara agent list` 실행).
- `--project`는 생략할 수 있습니다. 요구사항을 연결하면 실행이 해당 요구사항의 작업 영역을 가지고 시작됩니다.
- `--parent`: 상위 작업 아래에 연결합니다. 이렇게 해야 상위 작업 상세의 오른쪽에 하위 작업이 표시되고, 상위 대화에 @ 배정 메시지가 한 건 기록됩니다. 현재 작업 id는 환경 변수 `MULTARA_TASK_ID`에 있습니다. 어떤 작업의 run 안에서 하위 작업을 만들면 `--parent`를 전달하지 않아도 현재 작업에 자동으로 연결됩니다.
- 성공하면 stdout이 작업 JSON(`id` 포함)입니다.

## 작업 분배(하위 작업 포함)

다른 에이전트나 스쿼드 멤버가 병렬로 일해야 할 때는 본문에 "누구에게 배정함"이라고만 쓰지 말고, 하위 작업을 만들어 배정하세요.

```bash
multara task create --title "…" --description "…" --assignee <agentId> --parent "$MULTARA_TASK_ID" --status todo
```

- 상위 작업은 `in_progress`로 유지합니다. 작업을 분배하는 것 자체는 결과물 제출이 아닙니다.
- 하위 작업이 모두 마무리되면(더 이상 backlog/todo/in_progress/blocked가 아님) 시스템이 상위 작업의 담당자를 다시 깨워 취합하게 합니다. 깨워진 뒤에는 하위 작업의 결과를 확인하고, 상위 작업을 `in_review` / `done`으로 넘길지 판단합니다.
- 같은 사람에게 하위 작업을 만들어 놓고 같은 대화에서 자동 fan-out이 일어나기를 기대하지 마세요. 보드의 하위 작업과 스쿼드 채팅을 통한 분배는 서로 다른 경로입니다.

## 수정

`multara task update TASK_ID --status done --comment "전달 설명"`

`title`, `description`, `status`, `assignee`, `project`를 바꿀 수 있습니다. `--comment`는 시스템 메시지로 해당 작업 대화에 기록됩니다.

## 상태 / 배정

- `multara task status TASK_ID todo|backlog|in_progress|in_review|done|blocked|cancelled`
- `multara task assign TASK_ID --to AGENT_ID`
- 상태를 `todo`로 바꾸는 것은 할 일 목록으로 되돌릴 뿐이며, 실행이 다시 대기열에 오르지는 않습니다.

## 목록 / 조회 / 댓글

- `multara task list [--status todo] [--assignee <id>] [--project <id>]`
- `multara task get TASK_ID`
- `multara task comment TASK_ID --comment "진행 상황"`
- `multara task runs TASK_ID`

## 상태는 누가 기록하는가

시스템이 자동으로 동기화하는 것은 다음뿐입니다.

1. 작업이 `backlog` / `todo`에서 실제로 실행되기 시작하면 자동으로 `in_progress`가 됩니다.
2. 실행이 성공하고 그 작업에 다른 활성 실행이 없을 때, 여전히 `in_progress`라면 `in_review`가 됩니다.
3. 실행이 실패하고 그 작업에 다른 활성 실행이 없을 때, 여전히 `in_progress`라면 `todo`로 되돌아갑니다.

**`done`은 자동으로 기록되지 않습니다.** 목표가 실제로 달성되면 직접 명시적으로 기록해야 합니다.

작업이 진행되는 대로 CLI로 상태를 갱신하세요(턴 도중 언제든 기록할 수 있으며, 끝날 때까지 기다리지 마세요).

| 상황 | 상태 |
|------|------|
| 이 작업 자체의 결과물 작업을 시작함(구현, 조사, 설계, 검토 등 작업이 요구하는 것이면 무엇이든) | 즉시 `in_progress` |
| 이번 라운드에서 작업 자체의 결과를 전달했고 사람의 확인 / 검토를 기다림 | `in_review` |
| 아직 끝나지 않았고 계속해야 함(하위 작업이 다 모이지 않은 경우 포함) | `in_progress` 유지 |
| 외부 의존성이나 조건에 막힘 | `blocked` |
| 목표가 실제로 달성되었거나 사용자가 완료를 확인함 | `done` |
| 질의응답, 잡담, 다른 사람의 작업 자문일 뿐 이 작업의 결과물은 진전되지 않음 | **상태를 바꾸지 않음** |

`done`은 목표가 완료되었다는 뜻이지 "이번 run이 끝났다"는 뜻이 아닙니다. 스쿼드 리더에게: 멤버에게 일을 분배하는 것은 결과물 제출이 아닙니다. 상위 작업은 `in_progress`로 유지하고, 전체 목표가 확인된 뒤에 `in_review` / `done`으로 넘기세요.

## 규칙

- 작업대가 이미 요구사항 작업 영역에 연결되어 있다면 "먼저 작업부터 만들라"며 사용자가 요청한 코드 수정을 막지 마세요.
- 생성으로 인해 배정, 범위 또는 권한이 바뀐다면 실행 전에 미리보기를 보여 줍니다.
- 진행 상황은 작업 대화에 씁니다. 업데이트는 짧게, 작업 식별자와 다음 단계를 포함합니다.
