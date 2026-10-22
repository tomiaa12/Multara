---
name: multara-projects-and-workspaces
description: 요구사항(프로젝트)을 생성·수정하고 작업 영역을 연결합니다. 여러 작업이 하나의 결과를 공유하거나 코드를 수정해야 할 때 사용합니다.
---

# 요구사항과 작업 영역

요구사항은 관련된 작업 묶음을 같은 목표와 저장소 컨텍스트에 연결합니다. 요구사항에 연결되지 않은 채팅에서는 임의의 디렉터리를 수정하지 마세요.

## 생성

`multara project create --name "…" [--description "…"]`

작업 영역 연결은 앱에서 합니다(Git 저장소 또는 폴더 추가). 연결한 뒤에는 해당 요구사항에서 대기열에 오른 실행이 저장소 경로와 함께 시작됩니다.

## 목록 / 조회 / 수정

- `multara project list`
- `multara project get PROJECT_ID`
- `multara project update PROJECT_ID [--name "…"] [--description "…"]`

## 규칙

- 여러 작업이 결과물을 공유한다면 같은 요구사항 아래에 둡니다.
- 이번 실행이 이미 요구사항 작업 영역에 연결되어 있다면, 사용자가 코드 수정을 요청할 때 나열된 디렉터리를 바로 수정합니다.
- 아직 요구사항 작업 영역이 없다면 사용자에게 요구사항을 열어 달라고 하거나 `multara task create --project <id> --status todo`를 실행합니다.
- 격리 모드(worktree / copy / direct)는 앱이 작업 영역을 만들 때 결정합니다. 디스크에 있는 격리 사본의 경로를 직접 수정하지 마세요.
