---
name: multara-workflows
description: 创建和修改工作流。用户要把多步交给智能体、分流、人工确认，或用集成消息启动时使用。
---

# 工作流

一条工作流只有三种节点：开始、智能体、结束。开始节点创建时自带，不能再加，也不能删。结束节点只能有一个。命令改的是草稿，要跑起来必须先发布。

## 工作流

- `multara workflow create --name "…"`，可加 `--description "…"`
- `multara workflow list`
- `multara workflow get ID` 看草稿摘要；`multara workflow draft ID` 看完整草稿
- `multara workflow rename ID --name "…"`
- `multara workflow duplicate ID`
- `multara workflow enable ID` / `multara workflow disable ID`
- `multara workflow publish ID`，校验通过才发布
- `multara workflow delete ID`，还有运行没结束时会拒绝
- `multara workflow run ID --input key=value`，或 `--input-json '{…}'`
- `multara workflow runs [ID]`

## 节点

- `multara workflow node add ID --kind agent|output --title "…" [--id step] [--prompt "…"] [--config '{…}'] [--timeout 30]`
- `multara workflow node set ID NODE --title "…" --prompt "…" --config '{…}'`
- `multara workflow node remove ID NODE`，同时删掉连着它的线

节点 id 以小写字母开头，只能含小写字母、数字和下划线。智能体的 `--config` 会合并进现有配置：

- `target`：`{ "type": "agent" 或 "squad", "id": "…" }`
- `prompt`：这一步要做的事
- `context`：`previous` 只带上一步，`all` 从开始带到现在
- `route`：多条出路时 `all` 都送下去，`auto` 由这一步的智能体选，`human` 暂停等人选。只有一条出路时不用选
- `humanConfirm`：`true` 时这一步做完会停住，等人在运行页确认后才进入下一步。智能体不能自己用 `node next` 往下走

关联需求不在创建工作流时填写。需求是需求菜单里的那一条，id 用 `multara project list` 查。运行中用 `multara workflow tasks` 查看，`multara workflow tasks add --id 需求id` 绑上，`multara workflow tasks remove --id 需求id` 解开。任务说明里的「这次运行的文件目录」一直在。绑了需求也可以把文件写在那里，由你决定写目录还是写需求绑定的项目。
- `allowTaskCommands`：`true` 时这一步可以创建或修改任务

## 连线

- `multara workflow edge add ID --from NODE --to NODE [--max-iterations 2]`
- `multara workflow edge remove ID EDGE_ID`

`--max-iterations` 表示打回上游，取 1 到 20。

## 开始入参

- `multara workflow input add ID --name topic [--label "主题"] [--required true|false] [--default "…"] [--primary]`
- `multara workflow input set ID NAME …`
- `multara workflow input remove ID NAME`

新入参默认必填。`--primary` 只能有一个：集成里 `/flow 工作流名` 后面的正文填进这一项。都没标时填到第一个入参。

## 跑到这一步时

正在执行的工作流节点不能改画布。多条出路且 `route` 是 `auto` 时，用 `multara node next --id NODE --reason "…"` 选已有的下一步。做不到时用 `multara node fail --reason "…"`。`route` 是 `all` 时不用选。

## 规则

- 对外发送、部署、花钱或删除数据的工作流：先把草稿给人看，确认后再 `publish` 和 `enable`。
- 触发来源（手动、自动化、Webhook、集成消息、命令行）在开始节点上改。这条 CLI 不改来源。
