import type { ChatMessage, TimelineItem } from "../../domain/types";

const at = "2026-09-10T01:12:26.732359+00:00";

export const roundOnePrompt =
  "这是原始接口数据采集实测的第 1 轮。请按顺序完成：1）先给出简短计划和思考摘要；2）调用读取工具读取项目根目录 package.json；3）调用终端执行 node --version 和 git status --short -- docs；4）创建文件 multara-agent-protocol-test.txt，内容必须是 round-1: tool-and-terminal-complete；5）最后用普通文本、Markdown 列表和一个代码块汇总执行结果。除该测试文件外不要修改任何文件。";

export const roundTwoPrompt =
  "这是同一会话的第 2 轮。请结合上一轮上下文按顺序完成：1）先说明你记得上一轮做了什么，并给出简短思考摘要；2）调用读取工具读取 multara-agent-protocol-test.txt；3）调用搜索工具在该文件中查找 round-1；4）调用终端执行 wc -l multara-agent-protocol-test.txt 和 git diff -- multara-agent-protocol-test.txt；5）修改该文件，在末尾追加 round-2: context-search-edit-complete；6）再次读取文件验证；7）最后用普通文本、Markdown 列表和 JSON 代码块汇总两轮结果。除该测试文件外不要修改任何文件。";

const roundOneFinal = `第 1 轮操作已完成。

- Node：\`v16.20.2\`
- \`docs\` 无 Git 变更
- 已创建指定测试文件
- 项目未配置 \`typecheck\` 脚本，因此检查被跳过

\`\`\`text
round-1: tool-and-terminal-complete
\`\`\``;

const roundTwoFinal = `**上一轮记忆**：创建/写入 \`multara-agent-protocol-test.txt\`，内容为 \`round-1: tool-and-terminal-complete\`。

**思考摘要**：本轮在保留 round-1 的前提下追加 round-2，并用读/搜/终端交叉验证。

- 读取文件成功
- Grep 命中 \`round-1\`（第 1 行）
- \`wc -l\` → \`1\`
- 追加 \`round-2: context-search-edit-complete\`
- 再次读取验证通过

\`\`\`json
{
  "round1": "tool-and-terminal-complete",
  "round2": "context-search-edit-complete"
}
\`\`\``;

function timeline(
  seq: number,
  eventType: TimelineItem["eventType"],
  options: Partial<Omit<TimelineItem, "seq" | "eventType">> = {},
): TimelineItem {
  return {
    seq,
    eventType,
    callId: null,
    tool: null,
    content: null,
    input: null,
    output: null,
    createdAt: at,
    ...options,
  };
}

const roundOneTimeline: TimelineItem[] = [
  timeline(1, "thinking", {
    content: "严格限制在 mock 工作区内操作；先读 package.json，再执行终端命令，最后只创建指定测试文件。",
  }),
  timeline(2, "tool_use", {
    callId: "round-1-read",
    tool: "Read",
    input: { path: "protocol-debug/package.json" },
  }),
  timeline(3, "tool_result", {
    callId: "round-1-read",
    tool: "Read",
    output: '{"success":{"fileSize":24229,"totalLines":339}}',
  }),
  timeline(4, "tool_use", {
    callId: "round-1-shell",
    tool: "Shell",
    input: { command: "node --version && git status --short -- docs" },
  }),
  timeline(5, "tool_result", {
    callId: "round-1-shell",
    tool: "Shell",
    output: '{"exitCode":0,"stdout":"v16.20.2\\n"}',
  }),
  timeline(6, "tool_use", {
    callId: "round-1-edit",
    tool: "Edit",
    input: {
      path: "protocol-debug/multara-agent-protocol-test.txt",
      content: "round-1: tool-and-terminal-complete",
    },
  }),
  timeline(7, "tool_result", {
    callId: "round-1-edit",
    tool: "Edit",
    output: '{"linesAdded":1,"linesRemoved":0}',
  }),
];

const roundTwoTimeline: TimelineItem[] = [
  timeline(1, "thinking", {
    content: "当前运行没有续接上一轮 Cursor conversationId，先通过工作区文件确认 round-1，再继续修改。",
  }),
  timeline(2, "tool_use", {
    callId: "round-2-read",
    tool: "Read",
    input: { path: "protocol-debug/multara-agent-protocol-test.txt" },
  }),
  timeline(3, "tool_use", {
    callId: "round-2-grep",
    tool: "Grep",
    input: { path: "protocol-debug/multara-agent-protocol-test.txt", pattern: "round-1" },
  }),
  timeline(4, "tool_use", {
    callId: "round-2-shell",
    tool: "Shell",
    input: { command: "wc -l multara-agent-protocol-test.txt && git diff -- multara-agent-protocol-test.txt" },
  }),
  timeline(5, "tool_result", {
    callId: "round-2-read",
    tool: "Read",
    output: '{"content":"round-1: tool-and-terminal-complete\\n"}',
  }),
  timeline(6, "tool_result", {
    callId: "round-2-grep",
    tool: "Grep",
    output: '{"matched":true,"lineNumber":1}',
  }),
  timeline(7, "tool_result", {
    callId: "round-2-shell",
    tool: "Shell",
    output: '{"exitCode":0,"stdout":"       1 multara-agent-protocol-test.txt\\n"}',
  }),
  timeline(8, "tool_use", {
    callId: "round-2-edit",
    tool: "Edit",
    input: {
      path: "protocol-debug/multara-agent-protocol-test.txt",
      streamContent:
        "round-1: tool-and-terminal-complete\nround-2: context-search-edit-complete",
    },
  }),
  timeline(9, "tool_result", {
    callId: "round-2-edit",
    tool: "Edit",
    output: '{"linesAdded":1,"linesRemoved":0}',
  }),
];

function message(
  id: string,
  role: ChatMessage["role"],
  content: string,
  runId: string | null = null,
  items: TimelineItem[] = [],
): ChatMessage {
  return {
    id,
    sessionId: "protocol-replay",
    role,
    content,
    runId,
    agentId: role === "assistant" ? "miku" : null,
    audienceType: role === "assistant" ? "user" : null,
    audienceId: null,
    targetType: role === "user" ? "agent" : null,
    targetId: role === "user" ? "miku" : null,
    failureReason: null,
    elapsedMs: role === "assistant" && content ? 79444 : null,
    messageKind: "message",
    quickActions: [],
    timeline: items,
    attachments: [],
    createdAt: at,
  };
}

export interface ProtocolReplaySnapshot {
  title: string;
  detail: string;
  sessionTitle: string;
  composer: string;
  messages: ChatMessage[];
  busy: boolean;
  original: string;
  sent: string;
  fileLines: string[];
  changedLine: number | null;
  fileStatus: "missing" | "added" | "modified";
}

export const protocolReplayStepCount = 12;

export function protocolReplaySnapshot(step: number): ProtocolReplaySnapshot {
  const current = Math.max(0, Math.min(protocolReplayStepCount, step));
  const messages: ChatMessage[] = [];
  let composer = "";
  let fileLines: string[] = [];
  let changedLine: number | null = null;
  let fileStatus: ProtocolReplaySnapshot["fileStatus"] = "missing";

  if (current === 1) composer = roundOnePrompt;
  if (current >= 2) messages.push(message("replay-user-1", "user", roundOnePrompt));
  if (current >= 3) {
    const timelineCount = current === 3 ? 1 : current === 4 ? 2 : current === 5 ? 5 : 7;
    messages.push(
      message(
        "replay-assistant-1",
        "assistant",
        current >= 7 ? roundOneFinal : "",
        "replay-run-1",
        roundOneTimeline.slice(0, timelineCount),
      ),
    );
  }
  if (current >= 6) {
    fileLines = ["round-1: tool-and-terminal-complete"];
    changedLine = 1;
    fileStatus = "added";
  }
  if (current === 8) composer = roundTwoPrompt;
  if (current >= 9) messages.push(message("replay-user-2", "user", roundTwoPrompt));
  if (current >= 10) {
    const timelineCount = current === 10 ? 4 : 9;
    messages.push(
      message(
        "replay-assistant-2",
        "assistant",
        current >= 12 ? roundTwoFinal : "",
        "replay-run-2",
        roundTwoTimeline.slice(0, timelineCount),
      ),
    );
  }
  if (current >= 11) {
    fileLines = [
      "round-1: tool-and-terminal-complete",
      "round-2: context-search-edit-complete",
    ];
    changedLine = 2;
    fileStatus = "modified";
  }

  const labels = [
    ["等待开始", "点击“开始”把第一轮原始问题放入输入框。"],
    ["第一轮 · 输入", "问题只进入输入框，尚未发送。"],
    ["第一轮 · 发送", "问题已发送，输入框清空。"],
    ["第一轮 · 思考", "右侧开始展示思考摘要。"],
    ["第一轮 · 读取", "调用 Read 读取 mock package.json。"],
    ["第一轮 · 终端", "调用 Shell 并展示 node 与 Git 结果。"],
    ["第一轮 · 修改文件", "左侧新增测试文件并高亮第 1 行。"],
    ["第一轮 · 完成", "展示第一轮最终 Markdown 回复。"],
    ["第二轮 · 输入", "第二轮问题进入输入框，尚未发送。"],
    ["第二轮 · 发送", "第二轮问题已发送。"],
    ["第二轮 · 读搜终端", "并行展示 Read、Grep 与 Shell 调用。"],
    ["第二轮 · 修改文件", "追加第 2 行并同步左侧文件变化。"],
    ["第二轮 · 完成", "展示完整回复与最终文件状态。"],
  ] as const;
  const [title, detail] = labels[current];

  const original = current >= 1 && current <= 7
    ? roundOnePrompt
    : current >= 8
      ? roundTwoPrompt
      : "";
  return {
    title,
    detail,
    sessionTitle: "样式预览",
    composer,
    messages,
    busy: [3, 4, 5, 6, 10, 11].includes(current),
    original,
    sent: original,
    fileLines,
    changedLine,
    fileStatus,
  };
}
