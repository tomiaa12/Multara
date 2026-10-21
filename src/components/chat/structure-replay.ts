import type { ChatMessage, TimelineItem } from "../../domain/types";

const at = "2026-09-14T15:11:00.000+00:00";

export const structureOriginalPrompt = "看一下这个项目结构";

export const structureSentPrompt = [
  "只能修改以下需求工作区目录。用户拖入或点名的工作区外文件可以阅读，不要修改那些路径：",
  "H:/跨端应用/Multara",
  "",
  "本次未授权任何 Multara Skill。",
  "本次未由 Multara 注入任何 MCP。",
  "",
  `用户任务：${structureOriginalPrompt}`,
].join("\n");

export const structureFallbackTitle = "看一下这个项目结构";
export const structureGeneratedTitle = "查看项目结构";

const rootListing = [
  "src/",
  "src-tauri/",
  "prototypes/",
  "docs/",
  "package.json",
  "pnpm-lock.yaml",
  "README.md",
  "vite.config.ts",
].join("\n");

const packageJson = `{
  "name": "multara",
  "private": true,
  "version": "0.1.0",
  "scripts": {
    "dev": "vite",
    "tauri": "node scripts/with-cargo.mjs"
  }
}`;

const srcListing = [
  "App.vue",
  "main.ts",
  "router.ts",
  "styles.css",
  "components/",
  "views/",
  "stores/",
  "domain/",
  "services/",
].join("\n");

const tauriListing = [
  "src/",
  "Cargo.toml",
  "tauri.conf.json",
].join("\n");

const finalReply = `这是 **Multara** 的本地桌面工作区，核心分层如下：

- \`src/\`：Vue 3 前端。\`views/\` 管路由页，\`components/\` 是工作台和聊天，\`stores/app.ts\` 负责发送与会话标题。
- \`src-tauri/\`：Tauri 2 后端。\`commands.rs\` 接 \`send_message\`，\`executor.rs\` 组装提示词并拉起运行时。
- \`prototypes/\`、\`docs/\`：原型和说明，不参与主流程。

入口是 \`src/main.ts\` → \`router.ts\`，需求工作台在 \`/projects/:projectId\`，样式预览在 \`/#/preview\`。`;

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

const steps: TimelineItem[] = [
  timeline(1, "thinking", {
    content: "先从仓库根目录看目录和 package.json，再分别列出 src 与 src-tauri，最后用分层说明回复。",
  }),
  timeline(2, "tool_use", {
    callId: "structure-ls-root",
    tool: "LS",
    input: { path: "." },
  }),
  timeline(3, "tool_result", {
    callId: "structure-ls-root",
    tool: "LS",
    output: rootListing,
  }),
  timeline(4, "tool_use", {
    callId: "structure-read-pkg",
    tool: "Read",
    input: { path: "package.json" },
  }),
  timeline(5, "tool_result", {
    callId: "structure-read-pkg",
    tool: "Read",
    output: packageJson,
  }),
  timeline(6, "tool_use", {
    callId: "structure-ls-src",
    tool: "LS",
    input: { path: "src" },
  }),
  timeline(7, "tool_result", {
    callId: "structure-ls-src",
    tool: "LS",
    output: srcListing,
  }),
  timeline(8, "tool_use", {
    callId: "structure-ls-tauri",
    tool: "LS",
    input: { path: "src-tauri" },
  }),
  timeline(9, "tool_result", {
    callId: "structure-ls-tauri",
    tool: "LS",
    output: tauriListing,
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
    sessionId: "structure-replay",
    role,
    content,
    runId,
    agentId: role === "assistant" ? "miku" : null,
    audienceType: role === "assistant" ? "user" : null,
    audienceId: null,
    targetType: role === "user" ? "agent" : null,
    targetId: role === "user" ? "miku" : null,
    failureReason: null,
    elapsedMs: role === "assistant" && content ? 18420 : null,
    messageKind: "message",
    quickActions: [],
    timeline: items,
    attachments: [],
    createdAt: at,
  };
}

export interface StructureReplaySnapshot {
  title: string;
  detail: string;
  sessionTitle: string;
  composer: string;
  messages: ChatMessage[];
  busy: boolean;
  original: string;
  sent: string;
}

type Frame = Omit<StructureReplaySnapshot, "original" | "sent"> & {
  showPayload?: boolean;
};

const frames: Frame[] = [
  {
    title: "等待开始",
    detail: "空对话。Tab 仍是「新对话」，还没有原始提问。",
    sessionTitle: "新对话",
    composer: "",
    messages: [],
    busy: false,
  },
  {
    title: "输入原始提问",
    detail: "输入框写入原始数据：看一下这个项目结构。尚未发送。",
    sessionTitle: "新对话",
    composer: structureOriginalPrompt,
    messages: [],
    busy: false,
    showPayload: true,
  },
  {
    title: "组装发送数据",
    detail: "前端 formatAgentPrompt 后，后端再包上工作区限制，变成真正发给运行时的提示词。",
    sessionTitle: "新对话",
    composer: structureOriginalPrompt,
    messages: [],
    busy: false,
    showPayload: true,
  },
  {
    title: "发送用户消息",
    detail: "原始提问进入时间线，输入框清空。Tab 标题还没改。",
    sessionTitle: "新对话",
    composer: "",
    messages: [message("structure-user-1", "user", structureOriginalPrompt)],
    busy: false,
    showPayload: true,
  },
  {
    title: "设置 Tab 标题 · 截断",
    detail: "nameSessionFromFirstPrompt 先用 titleFromPrompt 截断原文，Tab 变成「看一下这个项目结构」。",
    sessionTitle: structureFallbackTitle,
    composer: "",
    messages: [message("structure-user-1", "user", structureOriginalPrompt)],
    busy: false,
    showPayload: true,
  },
  {
    title: "创建助手回合",
    detail: "排队/运行中。只显示「正在处理」，助手气泡不再重复写「正在整理回答」。",
    sessionTitle: structureFallbackTitle,
    composer: "",
    messages: [
      message("structure-user-1", "user", structureOriginalPrompt),
      message("structure-assistant-1", "assistant", "", "structure-run-1"),
    ],
    busy: true,
    showPayload: true,
  },
  {
    title: "设置 Tab 标题 · 生成",
    detail: "generateChatSessionTitle 返回短标题，Tab 从截断原文改成「查看项目结构」。",
    sessionTitle: structureGeneratedTitle,
    composer: "",
    messages: [
      message("structure-user-1", "user", structureOriginalPrompt),
      message("structure-assistant-1", "assistant", "", "structure-run-1"),
    ],
    busy: true,
    showPayload: true,
  },
  {
    title: "思考",
    detail: "运行时第一帧：thinking。准备先列根目录再读 package.json。",
    sessionTitle: structureGeneratedTitle,
    composer: "",
    messages: [
      message("structure-user-1", "user", structureOriginalPrompt),
      message("structure-assistant-1", "assistant", "", "structure-run-1", steps.slice(0, 1)),
    ],
    busy: true,
    showPayload: true,
  },
  {
    title: "列出根目录",
    detail: "调用 LS path=. 看仓库顶层。",
    sessionTitle: structureGeneratedTitle,
    composer: "",
    messages: [
      message("structure-user-1", "user", structureOriginalPrompt),
      message("structure-assistant-1", "assistant", "", "structure-run-1", steps.slice(0, 2)),
    ],
    busy: true,
    showPayload: true,
  },
  {
    title: "根目录结果",
    detail: "看到 src、src-tauri、prototypes、docs 和 package.json。",
    sessionTitle: structureGeneratedTitle,
    composer: "",
    messages: [
      message("structure-user-1", "user", structureOriginalPrompt),
      message("structure-assistant-1", "assistant", "", "structure-run-1", steps.slice(0, 3)),
    ],
    busy: true,
    showPayload: true,
  },
  {
    title: "读取 package.json",
    detail: "调用 Read 确认项目名和启动脚本。",
    sessionTitle: structureGeneratedTitle,
    composer: "",
    messages: [
      message("structure-user-1", "user", structureOriginalPrompt),
      message("structure-assistant-1", "assistant", "", "structure-run-1", steps.slice(0, 4)),
    ],
    busy: true,
    showPayload: true,
  },
  {
    title: "package.json 结果",
    detail: "name 为 multara，脚本走 vite / tauri。",
    sessionTitle: structureGeneratedTitle,
    composer: "",
    messages: [
      message("structure-user-1", "user", structureOriginalPrompt),
      message("structure-assistant-1", "assistant", "", "structure-run-1", steps.slice(0, 5)),
    ],
    busy: true,
    showPayload: true,
  },
  {
    title: "列出 src",
    detail: "调用 LS path=src，看前端入口和分层。",
    sessionTitle: structureGeneratedTitle,
    composer: "",
    messages: [
      message("structure-user-1", "user", structureOriginalPrompt),
      message("structure-assistant-1", "assistant", "", "structure-run-1", steps.slice(0, 6)),
    ],
    busy: true,
    showPayload: true,
  },
  {
    title: "src 结果",
    detail: "views、components、stores、domain、services 都在这一层。",
    sessionTitle: structureGeneratedTitle,
    composer: "",
    messages: [
      message("structure-user-1", "user", structureOriginalPrompt),
      message("structure-assistant-1", "assistant", "", "structure-run-1", steps.slice(0, 7)),
    ],
    busy: true,
    showPayload: true,
  },
  {
    title: "列出 src-tauri",
    detail: "调用 LS path=src-tauri，看 Rust 后端。",
    sessionTitle: structureGeneratedTitle,
    composer: "",
    messages: [
      message("structure-user-1", "user", structureOriginalPrompt),
      message("structure-assistant-1", "assistant", "", "structure-run-1", steps.slice(0, 8)),
    ],
    busy: true,
    showPayload: true,
  },
  {
    title: "src-tauri 结果",
    detail: "Cargo.toml 与 src/ 对应 Tauri 命令和执行器。",
    sessionTitle: structureGeneratedTitle,
    composer: "",
    messages: [
      message("structure-user-1", "user", structureOriginalPrompt),
      message("structure-assistant-1", "assistant", "", "structure-run-1", steps.slice(0, 9)),
    ],
    busy: true,
    showPayload: true,
  },
  {
    title: "最终回复",
    detail: "助手汇总分层。这一轮发送、改标题和每一步工具调用都已走完。",
    sessionTitle: structureGeneratedTitle,
    composer: "",
    messages: [
      message("structure-user-1", "user", structureOriginalPrompt),
      message("structure-assistant-1", "assistant", finalReply, "structure-run-1", steps),
    ],
    busy: false,
    showPayload: true,
  },
];

export const structureReplayStepCount = frames.length - 1;

export function structureReplaySnapshot(step: number): StructureReplaySnapshot {
  const current = Math.max(0, Math.min(structureReplayStepCount, step));
  const frame = frames[current];
  return {
    ...frame,
    original: frame.showPayload ? structureOriginalPrompt : "",
    sent: frame.showPayload ? structureSentPrompt : "",
  };
}
