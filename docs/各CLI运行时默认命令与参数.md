# 各 CLI 运行时的默认命令与参数

范围：`src-tauri/src` 里为每个运行时拼出来的启动命令，以及它们在**任务、收件箱、需求、工作台、右下角气泡聊天**这几个入口里的差别。

## 0. 先读这个

**怎么核对的**

- 命令怎么拼：我读了 `runtime.rs`（`build_args`、`compose_args`、`prompt_via_stdin`、`protocol_driver`、`capabilities`）、`executor.rs`（`spawn_run`、`apply_options`、`workspace_context`、`generate_text`）、`conversation.rs`（`apply_resume`）、`protocol.rs`（ACP 驱动）。这部分是读代码得出的，可靠。
- 参数是什么意思：这台机器上装了 24 个相关 CLI，我对其中大部分跑了 `--help` 并摘录原文。标 **[help 已核]** 的就是这样来的。
- **没装的**（`codex`、`hermes`）和 `--help` 里找不到该参数的，标 **[未核]**，含义是按该类 CLI 的通用约定写的，请当作参考。
- **我没有真的运行过任何带"绕过权限"标志的命令**（`--yolo`、`--dangerously-skip-permissions` 等）去验证它们是否被接受，因为那会让 CLI 真的去执行；所以"参数是否被该版本接受"只能靠 `--help` 推断。

**两条结论**

1. **五个入口用的是同一套命令参数。** 入口只影响"在哪个目录运行、给哪些环境变量、是否续接会话、本轮是什么模式"，不改变 CLI 自己的那组固定参数（见第 4 节）。
2. **默认（`auto`）模式下，几乎所有 CLI 都是"免确认"运行**：Claude 用 `bypassPermissions`，Cursor/Qwen 用 `--yolo`，Codex 用危险绕过标志等。也就是说，不选 Plan 时 CLI 对文件和命令的权限是全开的。

## 1. 命令是怎么拼出来的

```text
程序路径  = PATH 查找（Cursor 试 cursor-agent、agent；Copilot 试 github-copilot-cli、copilot）
参数      = build_args(运行时 id, 提示词, 模式)          ← 第 3 节的表
          + 网关参数（仅 Codex 走自定义网关时）          ← 在提示词之前插入
          → 把"任务文本"替换成完整提示词
          + apply_options：--workspace / --model / --add-dir / MCP 配置
          + apply_resume：--resume <id> 或 --session <id>
工作目录  = 会话关联的第一个工作区；没有关联项目时是 <应用数据>/task-workspaces/<会话id>
环境变量  = CI=1、MULTARA_SESSION_ID / AGENT_ID / RUN_ID / TASK_ID / APP_DATA / DB / CLI，
            PATH 前面插入 multara 命令行垫片目录
进程      = 独立进程组（unix: process_group(0)），stdout/stderr 管道
stdin     = 走 stdin 的 CLI 是管道（写入提示词后保持打开以便转向），其余是 /dev/null
```

**提示词怎么进去**（`prompt_via_stdin` / `protocol_driver`）：

| 方式 | 运行时 |
|---|---|
| **stdin 写提示词** | claude、codebuddy、cursor、qwen、opencode、codearts、pi、omp |
| **命令行参数（argv）** | codex（最后一个参数）、gemini（`-p <提示词>`）、copilot（`-p <提示词>`）、deveco（最后一个参数）、openclaw（`--message <提示词>`）、antigravity（`-p <提示词>`），以及未知运行时的默认分支 |
| **ACP / JSON-RPC 协议**（stdin 里发协议消息，不是原始提示词） | hermes、kimi、reasonix、dim、kiro、qoder、qoderclicn、traecli、grok、qwenpaw、mcode、zeroclaw（ACP）；dsh（自有协议） |

> 提示词走 argv 的运行时，整段提示词（含简介、历史、附件）都在命令行里：本机 `ps` 能看到，并且长度受系统限制（Linux 单参数约 128KB，Windows 整条命令行约 32K 字符，macOS 总量约 1MB）。

**会话续接**（`conversation.rs`）：只有 `cursor`、`claude`、`copilot`、`opencode` 通过 CLI 续接；其它运行时每轮由 Multara 把最近 6 万字符的历史拼进提示词。

- cursor / claude / copilot：追加 `--resume <会话id>`
- opencode：追加 `--session <会话id>`

**`apply_options` 追加的参数**

| 条件 | 追加 |
|---|---|
| cursor | `--workspace <第一个工作区路径>` |
| 模型不是 `auto` 且运行时在 cursor、claude、codebuddy、codex、copilot、opencode、codearts、deveco、gemini、qwen、pi、omp | `--model <模型>` |
| cursor、claude、codex 且有多个工作区 | 第 2 个及之后每个工作区一个 `--add-dir <路径>` |
| claude 且智能体配置了 MCP | `--strict-mcp-config --mcp-config <临时 JSON 文件>` |

## 2. 模式：`auto` / `plan` / `agent`

当前代码已经有 `RunMode`（`models.rs`），`build_args` 按模式出不同参数。

- **`auto`**：默认值。大多数 CLI 与 `agent` 相同（免确认）；**Claude 的 `auto` 额外带 `--permission-prompt-tool stdio --permission-prompts host`，并在 stdin 里先发一个 `initialize` 控制请求**，让模型可以自己进入 / 退出规划（`capabilities.auto_plan` 只对 claude 为真）。
- **`plan`**：只读 / 规划。每个 CLI 用它自己的只读机制（见下表"Plan 模式"列）。
- **`agent`**：执行模式，免确认。

支持 `plan` 的运行时（`capabilities`）：claude、cursor、codex、gemini、qwen、opencode、codebuddy、codearts、deveco、copilot、pi、omp；ACP 类只有在握手时发现了规划模式才支持（`acp_plan_mode`），否则 `plan` 被后端拒绝（"当前智能体不支持规划模式"）。不支持的：openclaw、antigravity、dsh，以及发现不了规划模式的 ACP 运行时。

## 3. 每个运行时的默认命令

下面的"默认命令"是 `auto`/`agent` 模式的参数，**不含**第 1 节 `apply_options` 与续接追加的部分。`⟨提示词⟩` 表示命令行里放提示词的位置。

### 3.1 一览表

| id | 可执行文件 | 默认命令（auto / agent） | Plan 模式的差异 | 提示词 |
|---|---|---|---|---|
| claude | `claude` | `-p --output-format stream-json --input-format stream-json --verbose --permission-mode bypassPermissions --disallowedTools AskUserQuestion`；`auto` 另加 `--permission-prompt-tool stdio --permission-prompts host` | `--permission-mode plan` | stdin（stream-json） |
| codebuddy | `codebuddy` | `-p --output-format stream-json --input-format stream-json --verbose --permission-mode bypassPermissions --disallowedTools AskUserQuestion --disallowedTools EnterPlanMode --disallowedTools ExitPlanMode` | `--permission-mode plan`（其余同左） | stdin（stream-json） |
| codex | `codex` | `exec --json --skip-git-repo-check --dangerously-bypass-approvals-and-sandbox ⟨提示词⟩` | `--sandbox read-only --ask-for-approval never` 取代绕过标志 | argv |
| cursor | `cursor-agent`（或 `agent`） | `-p --output-format stream-json --yolo` | `--trust --mode plan` 取代 `--yolo` | stdin |
| qwen | `qwen` | `--output-format stream-json --yolo` | `--approval-mode plan` 取代 `--yolo` | stdin |
| gemini | `gemini` | `-p ⟨提示词⟩ --output-format stream-json` | 另加 `--approval-mode plan --skip-trust` | argv（`-p`） |
| copilot | `copilot`（或 `github-copilot-cli`） | `-p ⟨提示词⟩ --output-format json --allow-all --no-ask-user` | 另加 `--plan --available-tools=view,grep,glob` | argv（`-p`） |
| opencode | `opencode` | `run --format json --dangerously-skip-permissions` | `--agent plan` 取代绕过标志 | stdin |
| codearts | `codearts` | `run --format json --auto` | `--agent plan` 取代 `--auto` | stdin |
| deveco | `deveco` | `run --format json --dangerously-skip-permissions ⟨提示词⟩` | `--agent plan` 取代绕过标志 | argv（末尾） |
| openclaw | `openclaw` | `agent --local --json --session-id multara-<毫秒时间戳> --message ⟨提示词⟩` | 不支持 | argv（`--message`） |
| pi | `pi` | `-p --mode json` | 另加 `--tools read,grep,find,ls --no-mcp` | stdin |
| omp | `omp` | `-p --mode json` | 另加 `--tools read,grep,glob` | stdin |
| antigravity | `agy` | `-p ⟨提示词⟩` | 不支持 | argv（`-p`） |
| hermes | `hermes` | `acp` | 看握手结果 | ACP |
| kimi | `kimi` | `acp` | 同上 | ACP |
| reasonix | `reasonix` | `acp` | 同上 | ACP |
| dim | `dim` | `acp` | 同上 | ACP |
| qwenpaw | `qwenpaw` | `acp` | 同上 | ACP |
| mcode | `mcode` | `acp` | 同上 | ACP |
| zeroclaw | `zeroclaw` | `acp` | 同上 | ACP |
| kiro | `kiro-cli` | `acp --trust-all-tools` | 同上 | ACP |
| qoder | `qodercli` | `--yolo --acp` | 同上 | ACP |
| qoderclicn | `qoderclicn` | `--yolo --acp` | 同上 | ACP |
| traecli | `traecli` | `acp serve --yolo` | 同上 | ACP |
| grok | `grok` | `--no-auto-update agent --always-approve stdio` | 同上 | ACP |
| dsh | `dsh` | `--profile multica --stdio` | 不支持 | dsh 协议 |

**ACP 运行时的规划模式**：握手后 Multara 发 `session/new`（参数只有工作目录和空的 MCP 列表），从应答里找规划模式 id 并缓存；规划轮在 `session/prompt` 之前先发 `session/set_mode`，并等 `current_mode_update` 确认，没确认就报错"运行时未确认规划模式"。

### 3.2 每个参数是什么意思

**图例**：**[help 已核]** = 这台机器上该 CLI 的 `--help` 原文支持；**[未核]** = 没装、`--help` 里没找到，或我没法验证。

**Claude Code（`claude`）**

| 参数 | 含义 |
|---|---|
| `-p` / `--print` | 非交互模式：处理完提示词就退出，结果打印到标准输出。**[help 已核]** |
| `--output-format stream-json` | 输出格式；`stream-json` = 一行一个 JSON 事件，实时流式。只在 `--print` 下有效。**[help 已核]** |
| `--input-format stream-json` | 输入格式；`stream-json` = 从 stdin 一行一个 JSON 消息（这样 Multara 才能中途"转向"）。只在 `--print` 下有效。**[help 已核]** |
| `--verbose` | 覆盖配置里的详细输出开关；流式 JSON 输出需要它。**[help 已核]** |
| `--permission-mode <mode>` | 权限模式。可选：`acceptEdits`、`auto`、`bypassPermissions`、`manual`、`dontAsk`、`plan`。`bypassPermissions` = 跳过所有权限确认，`plan` = 只规划。**[help 已核]** |
| `--permission-prompts host` | 谁来回答权限提示：`host`（SDK 宿主 / 权限提示工具）或 `none`（谁也不答，需要提示的一律自动拒绝）。**[help 已核]** |
| `--permission-prompt-tool stdio` | 通过 stdio 向宿主请求权限。**这个参数不在 `--help` 里（隐藏参数），本机版本是否仍接受它 [未核]。** |
| `--disallowedTools AskUserQuestion` | 禁用某些工具；这里禁用"向用户提问"工具（无头模式下没人能回答）。**[help 已核]** |
| `--model <model>` | 指定模型。**[help 已核]** |
| `--add-dir <dir...>` | 额外允许工具访问的目录（多工作区时使用）。**[help 已核]** |
| `--strict-mcp-config` | 只使用 `--mcp-config` 指定的 MCP，忽略其它来源。**[help 已核]** |
| `--mcp-config <file>` | 从 JSON 文件加载 MCP 配置。**[help 已核]** |
| `--resume <id>` | 继续指定会话。**[help 已核]** |

**CodeBuddy（`codebuddy`）**：`-p`、`--output-format`、`--input-format`、`--verbose`、`--permission-mode`（可选 `acceptEdits`、`bypassPermissions`、`default`、`plan`、`dontAsk`、`auto`）、`--disallowedTools`（逗号或空格分隔的禁用工具名）含义同 Claude **[help 已核]**。这里额外禁用 `AskUserQuestion`、`EnterPlanMode`、`ExitPlanMode`：不让模型自己提问或自己切换规划模式。

**Codex（`codex`，本机未安装，全部 [未核]）**

| 参数 | 含义（按 Codex CLI 的通用用法） |
|---|---|
| `exec` | 非交互执行子命令 |
| `--json` | 事件以 JSON 行输出 |
| `--skip-git-repo-check` | 不要求工作目录是 Git 仓库根（任务 / 气泡聊天可能没有 Git） |
| `--dangerously-bypass-approvals-and-sandbox` | 关闭审批和沙箱，完全放开 |
| `--sandbox read-only` | 沙箱只读（Plan 模式） |
| `--ask-for-approval never` | 从不请求审批（Plan 模式） |
| `-c key=value` / `-m <model>` | 覆盖配置项 / 指定模型（走自定义网关时 Multara 会加 `model_provider=multara` 等 4 个 `-c` 和 `-m`） |

> 注意：`--ask-for-approval` 是否被 `codex exec` 子命令接受取决于版本，这点我无法在本机验证。

**Cursor（`cursor-agent`）**

| 参数 | 含义 |
|---|---|
| `-p` / `--print` | 把响应打印到控制台，用于脚本。**[help 已核]** |
| `--output-format stream-json` | 输出格式（text / json / stream-json），只在 `--print` 下有效。**[help 已核]** |
| `--yolo` | `--force` 的别名：强制允许命令，除非被明确拒绝。**[help 已核]** |
| `--trust` | 不询问，直接信任当前工作区。**[help 已核]** |
| `--mode plan` | 以规划模式启动：只读、分析并提出计划，不编辑。可选 `plan`、`ask`。**[help 已核]** |
| `--workspace <path>` | 指定工作区目录。**[help 已核]** |
| `--model <model>` | 指定模型。**[help 已核]** |
| `--resume [chatId]` | 继续会话。**[help 已核]** |

**Qwen Code（`qwen`）**

| 参数 | 含义 |
|---|---|
| `--output-format stream-json` | 输出格式。**[help 已核]**（注意：它**没有** `-p`，提示词走 stdin） |
| `--yolo` / `-y` | 自动接受所有操作。**[help 已核]** |
| `--approval-mode plan` | 审批模式；`plan` = 只分析，不修改文件、不执行命令。**[help 已核]** |
| `--model` / `-m` | 指定模型。**[help 已核]** |

**Gemini CLI（`gemini`）**

| 参数 | 含义 |
|---|---|
| `-p ⟨提示词⟩` / `--prompt` | 非交互（headless）模式并给出提示词。**[help 已核]** |
| `--output-format stream-json` / `-o` | 输出格式（text / json / stream-json）。**[help 已核]** |
| `--approval-mode plan` | 审批模式：`default`（询问）、`auto_edit`、`yolo`、`plan`（只读）。**[help 已核]** |
| `--skip-trust` | 本次会话信任当前工作区。**[help 已核]** |
| `--model` / `-m` | 指定模型。**[help 已核]** |

> 注意：默认（auto / agent）命令里**没有** `--yolo`，所以 Gemini 在 `agent` 模式下是否免确认取决于它自己的默认值和配置；这与 Cursor、Qwen 不一致。

**GitHub Copilot（`copilot`）**

| 参数 | 含义 |
|---|---|
| `-p ⟨提示词⟩` / `--prompt` | 非交互执行提示词后退出。**[help 已核]** |
| `--output-format json` | `text`（默认）或 `json`（每行一个 JSON 对象）。**[help 已核]** |
| `--allow-all` | 启用全部权限（等价于 `--allow-all-tools` + 路径 + URL）。**[help 已核]** |
| `--no-ask-user` | 不要向用户提问。**[help 已核]** |
| `--plan` | 规划模式。**[help 已核]**（Multara 的注释说它只拦截工作区写入，shell 与 MCP 写工具仍放行，所以再加白名单） |
| `--available-tools=view,grep,glob` | 只有这些工具对模型可用。**[help 已核]** |
| `--resume` / `-r` | 继续会话。**[help 已核]** |

**OpenCode（`opencode`）**

| 参数 | 含义 |
|---|---|
| `run` | 带一条消息运行 OpenCode。**[help 已核]** |
| `--format json` | 输出格式（default / json）。**[help 已核]** |
| `--dangerously-skip-permissions` | 跳过权限确认。**[未核：本机 `opencode run --help` 列出的是 `--auto`（"自动批准未被明确拒绝的权限"），没有这个名字。]** |
| `--agent plan` | 使用名为 `plan` 的内置智能体。**[help 已核]**（`--agent` 本身存在） |
| `--session <id>` | 继续或创建会话。**[help 已核]** |

**CodeArts（`codearts`）**：`run`（带消息运行）、`--format json`（default / json 原始事件）、`--auto`（"run commands in always_allow mode"，即命令总是允许）、`--agent plan`（指定智能体）、`-m` / `--model` **[help 已核]**。

**DevEco（`deveco`）**：`run`、`--format json`、`--agent plan` **[help 已核]**；`--dangerously-skip-permissions` **[未核：本机 `deveco run --help` 列出的是 `--auto`（"自动批准未被明确拒绝的权限，危险！"）。]**

**OpenClaw（`openclaw`）**：`agent`（经 Gateway 运行一轮智能体）、`--local`（本地运行）、`--message`（提示词）**[help 已核，部分]**；`--json`、`--session-id` **[未核]**。**本机 `openclaw --help` 直接报错：它要求 Node 24.16+ 或 26.1+（当前 nvm 是 22.23.2），所以这台机器上 OpenClaw 实际跑不起来。**

**Pi（`pi`）/ OMP（`omp`）**

| 参数 | 含义 |
|---|---|
| `-p` / `--print` | 非交互：处理提示词后退出。**[help 已核]** |
| `--mode json` | 输出模式：`text`（默认）、`json`、`rpc`（omp 还有 `rpc-ui`）。**[help 已核]** |
| `--tools <list>` | 逗号分隔的工具白名单。**[help 已核]** |
| `--no-mcp`（仅 pi） | 关闭内置 MCP。**[help 已核]** |

**Antigravity（`agy`）**：`-p` 是 `--print` 的别名，`--prompt` 也是别名 **[help 已核]**。没有格式参数，输出是纯文本。

**ACP 类**

| 运行时 | 参数 | 含义 |
|---|---|---|
| kimi | `acp` | 作为 ACP（Agent Client Protocol）服务器通过 stdio 运行 **[help 已核]** |
| kiro | `acp --trust-all-tools` | 同上；`--trust-all-tools` / `-a` 信任所有工具 **[help 已核]** |
| traecli | `acp serve --yolo` | 作为 ACP 服务器运行；`--yolo` / `-y` **[help 已核]** |
| grok | `--no-auto-update agent --always-approve stdio` | `agent stdio`：经 stdio 运行智能体；`--always-approve`：自动批准所有工具执行 **[help 已核]**；`--no-auto-update` **[未核：help 里没找到]** |
| qoder / qoderclicn | `--yolo --acp` | **[未核：本机 `qodercli --help` 没有 `--yolo` 和 `--acp`；它列出的是 `--dangerously-skip-permissions` 和 `--permission-mode`。]** `qoderclicn` 我没查 |
| hermes | `acp` | 本机未安装 **[未核]** |
| reasonix、dim、qwenpaw、mcode、zeroclaw | `acp` | 本机已安装，但我没能确认 `acp` 子命令 **[未核]** |
| dsh | `--profile multica --stdio` | 本机已安装，没有核实 **[未核]** |

### 3.3 命令行里的参数与本机 `--help` 对不上的地方

这几条是这次核对发现的，可能是版本差异，也可能是参数被静默忽略或报错。**建议用真实运行确认**：

| 运行时 | Multara 用的参数 | 本机 `--help` 实际情况 |
|---|---|---|
| opencode | `--dangerously-skip-permissions` | 列出的是 `--auto` |
| deveco | `--dangerously-skip-permissions` | 列出的是 `--auto` |
| qoder | `--yolo`、`--acp` | 两个都没列出；有 `--dangerously-skip-permissions`、`--permission-mode` |
| grok | `--no-auto-update` | 没找到 |
| claude | `--permission-prompt-tool stdio` | 没列出（隐藏参数，`--permission-prompts` 是列出的） |
| openclaw | 整个命令 | 在 Node 22 下无法运行 |

另一条是代码层面的不一致（读代码得出，**[推断]**）：`generate_text`（生成会话标题、提交信息、压缩历史、生成 Skill）用 `RunMode::Auto` 拼参数，所以 Claude 的这类调用也会带上 `--permission-prompt-tool stdio --permission-prompts host`；但它写 stdin 用的是 `runtime::stdin_prompt`（`Agent` 模式，不带 `initialize` 控制请求）。生成类调用不会用工具，通常不会触发权限提示，所以多半没问题，但这是个两边模式不一致的地方。

## 4. 五个入口的差别

### 4.1 共同点：命令参数一样

所有入口最终都走 `executor::spawn_run`，同一个 `build_args`。**入口不会改变第 3 节的那组固定参数。** 下表只列"会变的东西"。

| 项 | 工作台右栏 | 右下角气泡 | 任务 | 需求（创建） | 收件箱 |
|---|---|---|---|---|---|
| 界面 | `ChatPanel compact` | `FloatingChat` → `ChatPanel compact` | `TaskActivity` | `TaskCreateDialog`（表单） | 有任务用 `TaskActivity`，否则 `ChatPanel embedded` |
| 工作目录 | 会话关联的第一个工作区 | 有项目时同左；**没有项目时是 `<应用数据>/task-workspaces/<会话id>`** | 任务关联的工作区（需求的 worktree / 副本 / 原目录） | 同任务 | 沿用该任务 / 会话的工作区 |
| 多工作区 | 第 2 个起 `--add-dir`（仅 cursor、claude、codex） | 同左（有项目时） | 同左 | 同左，需求常挂多个仓库 | 同左 |
| `MULTARA_TASK_ID` | 仅当该会话属于某个任务时 | 同左 | **有** | **有**（创建任务时） | 任务线程有，普通聊天无 |
| 提示词里的"工作区说明" | "只能修改以下需求工作区目录…" | 无项目时："这次运行没有关联项目…不要修改目录以外的路径" | 同工作台 | 同工作台 | 同工作台 |
| 本轮模式来源 | 输入框里的模式选择器，默认 `auto` | 同左 | 同左 | 表单里的"先出计划"开关（`planFirst`），默认关 = `auto` | 只读状态下无；允许回复时同输入框 |
| 会话续接 | 有 | 有 | 有 | 创建后的第一轮没有可续接的 | 有 |

要点：

1. **"右下角气泡"和"普通聊天"如果没有关联项目，是在应用数据目录下的临时目录里运行**，所以代码里的注释写着"需求 / 任务 / 收件箱 / 右下角聊天都可能没有 Git"。这就是 Codex 要加 `--skip-git-repo-check` 的原因。
2. **需求 = 创建任务**：没有"聊天阶段"，创建后立刻入队运行；它的模式只能在表单里选。需求默认会给智能体多个工作区，所以 cursor、claude、codex 会出现多个 `--add-dir`。
3. **收件箱**：不是独立的运行来源，只是展示某个已有会话 / 任务的线程；回复时走同一条发送路径，参数与原会话一致。
4. **没有界面的入口**（小队队员、自动化、飞书渠道、`multara` 命令行创建的任务）也用同一套参数；小队队员各自是独立运行，继承 `agent`。

### 4.2 每个 CLI 在这些入口里"实际会执行什么"

因为参数与入口无关，**一个 CLI 在五个入口里的默认命令是同一条**，只有"目录、环境变量、模式、是否续接、模型、`--add-dir`、MCP"这几项会变。下面用 Claude 和 Cursor 举两个完整的例子，其余按第 3.1 表套用即可。

**例 1：Claude，工作台，单工作区，默认模式（auto），首轮**

```text
cd <工作区>
claude -p --output-format stream-json --input-format stream-json --verbose \
  --permission-mode bypassPermissions \
  --permission-prompt-tool stdio --permission-prompts host \
  --disallowedTools AskUserQuestion \
  --model <智能体模型，auto 则省略> \
  [--strict-mcp-config --mcp-config <临时文件>]      # 智能体配了 MCP 才有
# stdin：先一条 initialize 控制请求，再一条 user 消息（含完整提示词）
```

**例 2：Cursor，需求创建（带 3 个工作区），选了"先出计划"，续接**

```text
cd <第一个工作区>
cursor-agent -p --output-format stream-json --trust --mode plan \
  --workspace <第一个工作区> --model <模型> \
  --add-dir <第二个工作区> --add-dir <第三个工作区> \
  --resume <上一轮的会话 id>                          # 首轮没有
# stdin：提示词原文
```

**差异汇总（对所有 CLI）**

| 场景 | 与默认命令相比 |
|---|---|
| 选了 Plan | 按第 3.1 表"Plan 模式"列替换 / 追加 |
| 续接（cursor、claude、copilot） | 末尾加 `--resume <id>`；提示词只放本轮任务，不重复简介和历史 |
| 续接（opencode） | 末尾加 `--session <id>` |
| 其它 CLI 的"续接" | 没有，每轮重新拼历史（最多 6 万字符，每条 8000 字符） |
| 自定义网关运行时（claude、codex、gemini、qwen、opencode） | 清掉受管的环境变量后写入网关地址与令牌；Codex 额外在提示词前加 4 个 `-c` 和 `-m`；Claude 额外用 `--settings` 指定代理（见 `gateway_settings_args`）。参数里的模型换成网关的 `cli_model` |
| 自定义 profile（`custom:` 前缀） | 命令和前缀参数由用户配置，其后拼上 `build_args(provider)` |

## 5. 不是"运行一轮对话"的 CLI 调用

`generate_text`（会话标题、提交信息、历史压缩、Skill 生成）也会启动 CLI：用的是同一个 `build_args`（`auto` 模式）+ `apply_options`（不带工作区和 MCP），**工作目录是系统临时目录**，同样是 `CI=1`、独立进程组。选哪个智能体：优先用指定的，否则遍历所有智能体取第一个能启动的。如果智能体配的是 API 源（`mode == "api"`），则不启动 CLI，直接调接口。

另外，`--version` 探测（检测 CLI 是否可用、取版本号）是 `<cli> --version`，`CI=1`，8 秒超时，结果按路径缓存。

## 6. 这份文档没有核实的

- 没有真的启动任何带绕过权限标志的 CLI 去验证参数被接受（见第 0 节）。
- `codex`、`hermes` 没装，`dsh`、`dim`、`reasonix`、`qwenpaw`、`mcode`、`zeroclaw`、`qoderclicn` 装了，但我没能确认它们的子命令，相关含义都是 **[未核]**。
- 没有在各入口真实触发一轮，第 4 节的"实际命令"是按代码推出的；`MULTARA_TASK_ID` 的"仅当会话属于任务"是读 `spawn_run` 里的查询得出的。
- CLI 的版本会变。`--help` 的结果只代表这台机器当前安装的版本（Claude 的版本号、Cursor 的版本号我没有逐个记录）。
