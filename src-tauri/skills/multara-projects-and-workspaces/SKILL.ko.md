---
name: multara-projects-and-workspaces
description: 요구사항(프로젝트)을 생성·수정하고 작업 영역을 연결합니다. 여러 작업이 하나의 결과를 공유하거나 코드를 수정해야 할 때 사용합니다.
---

# 요구사항과 작업 영역

요구사항은 관련된 작업 묶음을 같은 목표와 저장소 컨텍스트에 연결합니다. 요구사항에 연결되지 않은 채팅에서는 임의의 디렉터리를 수정하지 마세요.

## 생성

`multara project create --name "…" [--description "…"]`

요구사항에 폴더를 하나 추가합니다. `--path`에는 저장소 또는 일반 디렉터리를 하나만 넘깁니다. 저장소가 여러 개인 상위 디렉터리는 거부되고, 각각 추가할 수 있는 경로가 나열됩니다.

- 새 Worktree: `multara workspace add --project PROJECT_ID --path /절대경로 --mode worktree-new --branch 새-브랜치 [--base 기준-브랜치]`
- 기존 브랜치를 Worktree로: `multara workspace add --project PROJECT_ID --path /절대경로 --mode worktree-existing --branch 기존-브랜치`
- 실제 디렉터리를 직접 수정: `multara workspace add --project PROJECT_ID --path /절대경로 --mode current`

`--base`를 생략하면 `HEAD`에서 분기합니다. `multara workspace list [--project PROJECT_ID]`로 연결된 디렉터리를 봅니다. 연결한 뒤에는 해당 요구사항에서 대기열에 오른 실행이 저장소 경로와 함께 시작됩니다.

## 목록 / 조회 / 수정

- `multara project list`
- `multara project get PROJECT_ID`
- `multara project update PROJECT_ID [--name "…"] [--description "…"]`

## 규칙

- 여러 작업이 결과물을 공유한다면 같은 요구사항 아래에 둡니다.
- 이번 실행이 이미 요구사항 작업 영역에 연결되어 있다면, 사용자가 코드 수정을 요청할 때 나열된 디렉터리를 바로 수정합니다.
- 아직 요구사항 작업 영역이 없다면 사용자에게 요구사항을 열어 달라고 하거나 `multara task create --project <id> --status todo`를 실행합니다.
- 위 명령으로 Worktree 또는 실제 디렉터리를 선택합니다. 만든 뒤에는 디스크의 격리 사본 경로를 직접 수정하지 마세요.
