import type { ChatMessage, TimelineItem } from "../../domain/types";

const at = "2026-09-07T14:20:00.000Z";

let seq = 0;
function event(partial: Partial<TimelineItem> & Pick<TimelineItem, "eventType">): TimelineItem {
  seq += 1;
  return {
    seq,
    callId: null,
    tool: null,
    content: null,
    input: null,
    output: null,
    createdAt: at,
    ...partial,
  };
}

function pair(tool: string, callId: string, input: Record<string, unknown>, output: string): TimelineItem[] {
  return [
    event({ eventType: "tool_use", tool, callId, input }),
    event({ eventType: "tool_result", tool, callId, output }),
  ];
}

const pasteImage =
  "data:image/svg+xml," +
  encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" width="240" height="160">
      <rect width="240" height="160" rx="12" fill="#2b4c73"/>
      <text x="120" y="86" text-anchor="middle" fill="#d6e8ff" font-size="18" font-family="sans-serif">粘贴截图</text>
    </svg>`,
  );

export const previewMessages: ChatMessage[] = [
  {
    id: "preview-user-1",
    sessionId: "preview",
    role: "user",
    targetType: "agent",
    targetId: "miku",
    content: [
      "[代码引用] src/views/ChatView.vue:12-13",
      "```",
      "import ChatPanel from \"../components/ChatPanel.vue\";",
      "</script>",
      "```",
      "",
      "[代码引用] src-tauri/src/webhook.rs:25-33",
      "```",
      "std::thread::spawn(move || {",
      "    let Ok(server) = Server::http(format!(\"127.0.0.1:{port}\")) else {",
      "        return;",
      "    };",
      "});",
      "```",
      "",
      "ChatView.vue (12-13) 测试 webhook.rs (25-33) 这段在做什么？",
    ].join("\n"),
    runId: null,
    failureReason: null,
    elapsedMs: null,
    messageKind: "message",
    quickActions: [],
    timeline: [],
    attachments: [],
    createdAt: at,
  },
  {
    id: "preview-assistant-1",
    sessionId: "preview",
    role: "assistant",
    agentId: "miku",
    audienceType: "user",
    audienceId: null,
    content: `这两处分别是聊天页入口和本地 Webhook 监听。

## 结论

- \`ChatView.vue\` 只负责挂上共享的 \`ChatPanel\`。
- \`webhook.rs\` 在后台线程绑定 \`127.0.0.1:{port}\`，接收自动化回调。

> 智能体改文件前会先 Read / Grep，确认调用点后再 Edit。

### 接下来可以做

1. 给 Webhook 加上健康检查
2. 把端口写进设置页
3. 用下面这张图核对请求流

\`\`\`mermaid
flowchart LR
  ChatView --> ChatPanel
  Webhook --> Automation
  Automation --> Agent
\`\`\`

前端入口：

\`\`\`vue
<template>
  <ChatPanel :project-id="selectedProjectId" show-session-list />
</template>
\`\`\`

按钮预览：

\`\`\`html-preview
<button style="padding:6px 12px;border:0;border-radius:8px;background:#1683ff;color:#fff">发送</button>
\`\`\`

### 任务拆分与分工

| 角色 | 负责 | 产出 |
| --- | --- | --- |
| **队员 A · 根目录与工程配置** | 根目录文件/目录职责；\`package.json\` scripts/依赖；\`tsconfig\`、eslint/prettier；\`vercel.json\`、firebase 相关 | 工程骨架一览（1 段） |
| **队员 B · \`src\` 内容架构** | \`src\` 一级目录含义；多语言页如何组织；工具页/文档与共享组件、布局、hooks、utils 的关系 | \`src\` 结构树 + 简短说明 |
| **队员 C · 周边目录** | \`build/\`、\`scripts/\`、\`extension/\`、\`node/\`、\`docs/\`、\`.cursor\`/\`github\`（点到为止） | 各目录一句话职责 |
| **收尾（建议队长或 A）** | 汇总 A/B/C；确认无越界、无改动；**仅在有文件修改时**跑类型检查 | 给用户的最终结构说明 |`,
    runId: "preview-run-1",
    failureReason: null,
    elapsedMs: 18420,
    messageKind: "message",
    quickActions: [],
    timeline: [
      event({ eventType: "thinking", content: "先看聊天入口，再核对 webhook 监听。" }),
      ...pair("Read", "r1", { path: "src/views/ChatView.vue", offset: 1, limit: 28 }, "ok"),
      ...pair("Read", "r2", { path: "src/components/ChatPanel.vue", offset: 140, limit: 50 }, "ok"),
      ...pair("Read", "r3", { path: "src-tauri/src/webhook.rs", offset: 25, limit: 9 }, "ok"),
      ...pair("Grep", "g1", { pattern: "incoming_requests|handle_request" }, "webhook.rs:30"),
      ...pair("Glob", "g2", { glob: "src-tauri/src/*.rs" }, "webhook.rs\nautomation.rs"),
      ...pair(
        "mcp_chrome-browser-plugin_browser_navigate",
        "b1",
        { url: "http://localhost:1420/preview" },
        "opened",
      ),
      ...pair("browser_snapshot", "b2", { title: "样式预览" }, "snapshot"),
      event({
        eventType: "thinking",
        content: "文件引用和 webhook 监听已经对上。接下来用命令确认服务能起来，再改一处提示文案，并补一个无关的 MCP 调用展示。这段思考需要足够长，界面才会把摘要显示成 Thought，而不是 Thought briefly。",
      }),
      ...pair("Bash", "s1", { command: "lsof -iTCP:1420 -sTCP:LISTEN", description: "检查预览端口" }, "node    89251 tomiaa   23u  IPv4 0x1  0t0  TCP 127.0.0.1:1420 (LISTEN)"),
      event({
        eventType: "tool_use",
        tool: "Edit",
        callId: "e1",
        input: {
          path: "src/views/ChatView.vue",
          old_string: [
            '        <div v-if="!sessionMessages.length" class="chat-welcome">',
            "          <h2>今天要完成什么？</h2>",
            "          <p>选择智能体或小队后开始对话；执行期间发送的新消息会自动排队。</p>",
            "        </div>",
          ].join("\n"),
          new_string: [
            '        <div v-if="!sessionMessages.length" class="chat-welcome">',
            "          <h2>从这条对话开始</h2>",
            "          <p>选择智能体或小队后开始对话；执行期间发送的新消息会自动排队。</p>",
            "        </div>",
          ].join("\n"),
        },
      }),
      event({ eventType: "tool_result", tool: "Edit", callId: "e1", output: "updated" }),
      event({
        eventType: "tool_use",
        tool: "Write",
        callId: "w1",
        input: {
          path: "src/views/ChatPreviewView.vue",
          contents: [
            "<template>",
            '  <div class="chat-preview-page">',
            "    <header>对话样式预览</header>",
            "    <ChatPanel compact preview />",
            "  </div>",
            "</template>",
            "",
          ].join("\n"),
        },
      }),
      event({ eventType: "tool_result", tool: "Write", callId: "w1", output: "wrote file" }),
      event({
        eventType: "tool_use",
        tool: "CallMcpTool",
        callId: "m1",
        input: { server: "docs", toolName: "get_doc_content", uri: "preview" },
      }),
      event({ eventType: "tool_result", tool: "CallMcpTool", callId: "m1", output: "{ \"ok\": true }" }),
    ],
    attachments: [],
    createdAt: at,
  },
  {
    id: "preview-user-2",
    sessionId: "preview",
    role: "user",
    targetType: "agent",
    targetId: "miku",
    content: "按这张图把输入框和附件条对齐，另外日志也一起看下。",
    runId: null,
    failureReason: null,
    elapsedMs: null,
    messageKind: "message",
    quickActions: [],
    timeline: [],
    attachments: [
      {
        id: "preview-paste",
        name: "paste.png",
        path: pasteImage,
        mimeType: "image/png",
        size: 18432,
      },
      {
        id: "preview-log",
        name: "runtime.log",
        path: "/tmp/runtime.log",
        mimeType: "text/plain",
        size: 8192,
      },
    ],
    createdAt: at,
  },
  {
    id: "preview-assistant-2",
    sessionId: "preview",
    role: "assistant",
    agentId: "miku",
    audienceType: "user",
    audienceId: null,
    content: "运行中断，没有生成完整回答。",
    runId: "preview-run-2",
    failureReason: "runtime claude exited with status 1\nError: Webhook 服务无法监听 127.0.0.1:9377",
    elapsedMs: 3200,
    messageKind: "message",
    quickActions: [],
    timeline: [
      event({ eventType: "thinking", content: "先核对附件和日志。" }),
      ...pair("Read", "r4", { path: "src/styles.css", offset: 190, limit: 20 }, "composer styles"),
      ...pair("Bash", "s2", { command: "tail -n 20 runtime.log" }, "Webhook 服务无法监听 127.0.0.1:9377"),
    ],
    attachments: [],
    createdAt: at,
  },
  {
    id: "preview-assistant-2b",
    sessionId: "preview",
    role: "assistant",
    agentId: "reviewer",
    audienceType: "user",
    audienceId: null,
    content: "运行中断，没有生成完整回答。",
    runId: "preview-run-2b",
    failureReason: "runtime claude exited with status 1\nError: Webhook 服务无法监听 127.0.0.1:9377",
    elapsedMs: 3200,
    messageKind: "message",
    quickActions: [],
    timeline: [],
    attachments: [],
    createdAt: at,
  },
  {
    id: "preview-user-3",
    sessionId: "preview",
    role: "user",
    targetType: "agent",
    targetId: "miku",
    content: "继续把 styles.css 和 Diff 卡片对一下。",
    runId: null,
    failureReason: null,
    elapsedMs: null,
    messageKind: "message",
    quickActions: [],
    timeline: [],
    attachments: [],
    createdAt: at,
  },
  {
    id: "preview-assistant-running",
    sessionId: "preview",
    role: "assistant",
    agentId: "miku",
    audienceType: "user",
    audienceId: null,
    content: "",
    runId: "preview-run-live",
    failureReason: null,
    elapsedMs: null,
    messageKind: "message",
    quickActions: [],
    timeline: [
      event({ eventType: "thinking", content: "先读样式，再核对 Diff 卡片。" }),
      event({ eventType: "tool_use", tool: "Read", callId: "live-r1", input: { path: "src/styles.css", offset: 170, limit: 40 } }),
      event({ eventType: "tool_use", tool: "Read", callId: "live-r2", input: { path: "src/components/chat/FileDiffCard.vue", offset: 1, limit: 46 } }),
    ],
    attachments: [],
    createdAt: at,
  },
  {
    id: "preview-squad-user",
    sessionId: "preview",
    role: "user",
    content: "登录验证码对不上，小队先查前端再补测试。",
    runId: null,
    agentId: null,
    audienceType: null,
    audienceId: null,
    targetType: "squad",
    targetId: "preview-squad",
    failureReason: null,
    elapsedMs: null,
    messageKind: "message",
    quickActions: [],
    timeline: [],
    attachments: [],
    createdAt: at,
  },
  {
    id: "preview-squad-lead",
    sessionId: "preview",
    role: "assistant",
    content: "我先拆一下：前端核对输入长度和提交字段，评审补过期和位数两条回归。",
    runId: "preview-run-lead",
    agentId: "miku",
    audienceType: "user",
    audienceId: null,
    targetType: null,
    targetId: null,
    failureReason: null,
    elapsedMs: 4200,
    messageKind: "message",
    quickActions: [],
    timeline: [event({ eventType: "thinking", content: "先拆给队员。" })],
    attachments: [],
    createdAt: at,
  },
  {
    id: "preview-squad-assign",
    sessionId: "preview",
    role: "assistant",
    content: "请按刚才的拆分处理你负责的部分，完成后向我汇报。",
    runId: null,
    agentId: "miku",
    audienceType: "agent",
    audienceId: "reviewer",
    targetType: null,
    targetId: null,
    failureReason: null,
    elapsedMs: null,
    messageKind: "message",
    quickActions: [],
    timeline: [],
    attachments: [],
    createdAt: at,
  },
  {
    id: "preview-squad-member",
    sessionId: "preview",
    role: "assistant",
    content: "两条回归都绿了，断言已经换成新文案。",
    runId: "preview-run-member",
    agentId: "reviewer",
    audienceType: "agent",
    audienceId: "miku",
    targetType: null,
    targetId: null,
    failureReason: null,
    elapsedMs: 9100,
    messageKind: "message",
    quickActions: [],
    timeline: [],
    attachments: [],
    createdAt: at,
  },
  {
    id: "preview-question",
    sessionId: "preview",
    role: "assistant",
    content: [
      "当前工作区绑的是需求：**【账户】万元入金开户驳回场景-允许驳回后修改账户类型(保证金→现金)**。",
      "",
      "```multara-questions",
      JSON.stringify({
        questions: [
          {
            header: "下一步",
            question: "你现在想先做哪一项？",
            options: [
              { label: "摸清现状", description: "先在 client / acct / uc 里查驳回改账户类型相关代码与接口，给一版结论" },
              { label: "直接改代码", description: "按需求把「驳回后允许保证金改现金」补上或修通" },
              { label: "拆成任务", description: "建任务并指派，方便留状态、多轮跟进" },
            ],
          },
          {
            header: "范围",
            question: "这次涉及哪些仓库？",
            multiSelect: true,
            options: ["client", "acct", "uc"],
          },
        ],
      }),
      "```",
    ].join("\n"),
    runId: null,
    agentId: "miku",
    audienceType: "user",
    audienceId: null,
    targetType: null,
    targetId: null,
    failureReason: null,
    elapsedMs: 4400,
    messageKind: "message",
    quickActions: [],
    timeline: [],
    attachments: [],
    createdAt: at,
  },
];
