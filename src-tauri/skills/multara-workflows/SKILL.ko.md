---
name: multara-workflows
description: 워크플로를 만들고 수정합니다. 여러 단계를 에이전트에 맡기거나, 분기, 사람 승인, 통합 메시지로 시작해야 할 때 사용합니다.
---

# 워크플로

워크플로 노드는 시작, 에이전트, 사람 게이트, 끝 네 가지뿐입니다. 시작 노드는 만들 때 함께 생기며 추가하거나 지울 수 없습니다. 끝 노드는 하나만 있을 수 있습니다. 명령은 초안을 바꿉니다. 실행하려면 먼저 게시합니다.

## 워크플로

- `multara workflow create --name "…"`. `--description "…"` 도 붙일 수 있습니다
- `multara workflow list`
- `multara workflow get ID` 는 초안 요약, `multara workflow draft ID` 는 초안 전체
- `multara workflow rename ID --name "…"`
- `multara workflow duplicate ID`
- `multara workflow enable ID` / `multara workflow disable ID`
- `multara workflow publish ID` 는 검증을 통과해야 게시합니다
- `multara workflow delete ID` 는 실행이 남아 있으면 거부합니다
- `multara workflow run ID --input key=value`, 또는 `--input-json '{…}'`
- `multara workflow runs [ID]`

## 노드

- `multara workflow node add ID --kind agent|output --title "…" [--id step] [--prompt "…"] [--config '{…}'] [--timeout 30]`
- `multara workflow node set ID NODE --title "…" --prompt "…" --config '{…}'`
- `multara workflow node remove ID NODE` 는 그 노드에 닿는 연결도 지웁니다

노드 id는 소문자로 시작하고 소문자, 숫자, 밑줄만 씁니다. 에이전트의 `--config` 는 기존 설정에 합쳐집니다.

- `target`: `{ "type": "agent" 또는 "squad", "id": "…" }`
- `prompt`: 이 단계에서 할 일
- `context`: `previous` 는 이전 단계만, `all` 은 시작부터 지금까지
- `route`: 나가는 길이 여러 개일 때 `all` 은 모두로, `auto` 는 이 단계의 에이전트가 고르고, `human` 은 사람이 고를 때까지 멈춥니다. 나가는 길이 하나면 고르지 않습니다
- `allowTaskCommands`: `true` 면 이 단계는 작업을 만들거나 수정할 수 있습니다

사람 게이트는 처음부터 결정이 세 개입니다. `approve`, `changes`, `reject`.

## 연결

- `multara workflow edge add ID --from NODE --to NODE [--when approve|changes|reject] [--max-iterations 2]`
- `multara workflow edge remove ID EDGE_ID`

`--when` 은 사람 게이트에서 나가는 연결에만 씁니다. `--max-iterations` 는 앞 단계로 되돌리는 횟수이며 1에서 20입니다.

## 시작 입력

- `multara workflow input add ID --name topic [--label "주제"] [--required true|false] [--default "…"] [--primary]`
- `multara workflow input set ID NAME …`
- `multara workflow input remove ID NAME`

새 입력은 기본이 필수입니다. `--primary` 는 하나만 남습니다. 통합에서 `/flow 이름` 뒤에 쓴 본문이 그 입력에 들어갑니다. 아무것도 표시하지 않으면 첫 입력에 들어갑니다.

## 단계가 실행 중일 때

실행 중인 워크플로 노드는 캔버스를 바꿀 수 없습니다. 나가는 길이 여러 개이고 `route` 가 `auto` 이면 `multara node next --id NODE --reason "…"` 로 이미 있는 다음 단계를 고릅니다. 할 수 없으면 `multara node fail --reason "…"` 입니다. `route` 가 `all` 이면 고르지 않습니다.

## 규칙

- 밖으로 보내거나, 배포하거나, 비용이 들거나, 데이터를 지우는 워크플로는 먼저 초안을 사람에게 보여 주고, 확인한 다음 `publish` 와 `enable` 을 합니다.
- 시작 출처(수동, 자동화, Webhook, 통합 메시지, 명령줄)는 시작 노드에서 바꿉니다. 이 CLI는 출처를 바꾸지 않습니다.
