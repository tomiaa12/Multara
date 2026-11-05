import type { Agent, ChatMessage, Task, TaskRunRecord, TaskStatus } from "../domain/types";
import type { AssignmentSubtaskRef } from "../components/chat/message-speaker";

const at = "2026-11-04T06:01:14.248Z";
const later = "2026-11-04T06:02:19.356Z";

export type TaskDetailPreviewChild = {
  id: string;
  title: string;
  status: TaskStatus;
  agentId: string;
  reply?: string;
  pending?: boolean;
};

export type TaskDetailPreviewSide = {
  projectName: string;
  priority: "none" | "urgent" | "high" | "medium" | "low";
  labels: string[];
  checklist: Array<{ id: string; text: string; done: boolean }>;
  children: TaskDetailPreviewChild[];
  prUrl: string;
  runs: TaskRunRecord[];
};

export type TaskDetailPreviewScenario = {
  id: string;
  label: string;
  detail: string;
  missing?: boolean;
  task: Task | null;
  agents: Agent[];
  messages: ChatMessage[];
  side: TaskDetailPreviewSide;
  /** 嵌套卡预览：按子任务 id 覆盖 SubtaskReplyEmbed */
  embedPreviewById?: Record<string, NonNullable<AssignmentSubtaskRef["preview"]>>;
};

function agent(partial: Pick<Agent, "id" | "name" | "avatar">): Agent {
  return {
    description: "",
    instructions: "",
    runtimeId: "cursor",
    model: "auto",
    maxConcurrentTasks: 3,
    skillIds: [],
    mcpIds: [],
    ...partial,
  };
}

function task(partial: Partial<Task> & Pick<Task, "id" | "title" | "status" | "agentId" | "sessionId">): Task {
  return {
    description: "",
    createdAt: at,
    updatedAt: later,
    priority: "none",
    labels: [],
    checklist: [],
    ...partial,
  };
}

function message(
  partial: Partial<ChatMessage> & Pick<ChatMessage, "id" | "role" | "content">,
): ChatMessage {
  return {
    sessionId: "preview-task-session",
    runId: null,
    failureReason: null,
    elapsedMs: null,
    messageKind: "message",
    quickActions: [],
    timeline: [],
    attachments: [],
    createdAt: at,
    agentId: null,
    audienceType: null,
    audienceId: null,
    ...partial,
  };
}

const agents: Agent[] = [
  agent({ id: "miku", name: "Miku", avatar: "/miku-avatar.jpg" }),
  agent({ id: "coder", name: "Coder", avatar: "C" }),
];

const emptySide = (): TaskDetailPreviewSide => ({
  projectName: "开户审核",
  priority: "none",
  labels: [],
  checklist: [],
  children: [],
  prUrl: "",
  runs: [],
});

const CHILD_ID = "39187a5c-2538-419d-aec5-18e846787ca6";
const CHILD_TITLE = "实现：交收审驳回后认证审自动驳回";

export const taskDetailPreviewScenarios: TaskDetailPreviewScenario[] = [
  {
    id: "missing",
    label: "任务不存在",
    detail: "路由指向已删除或错误 id 时的空态。",
    missing: true,
    task: null,
    agents,
    messages: [],
    side: emptySide(),
  },
  {
    id: "empty",
    label: "新建空任务",
    detail: "仅有标题，无描述、无活动、侧栏为空。",
    task: task({
      id: "preview-task-empty",
      sessionId: "preview-task-session-empty",
      title: "核对交收审驳回链路",
      status: "todo",
      agentId: "miku",
    }),
    agents,
    messages: [],
    side: emptySide(),
  },
  {
    id: "running",
    label: "执行中",
    detail: "助手正在跑工具，侧栏有一次 running。",
    task: task({
      id: "preview-task-running",
      sessionId: "preview-task-session-running",
      title: "排查详情页状态未回退",
      description: "入金驳回后认证审应回到开户审核态。",
      status: "in_progress",
      agentId: "miku",
      priority: "high",
    }),
    agents,
    messages: [
      message({
        id: "preview-task-u1",
        sessionId: "preview-task-session-running",
        role: "user",
        targetType: "agent",
        targetId: "miku",
        content: "一审通过后再驳回交收审，详情页状态为什么没回去？",
      }),
      message({
        id: "preview-task-a1",
        sessionId: "preview-task-session-running",
        role: "assistant",
        agentId: "miku",
        audienceType: "user",
        content: "",
        runId: "preview-task-run-live",
        timeline: [
          {
            seq: 1,
            callId: null,
            tool: null,
            eventType: "thinking",
            content: "先对一下详情页取状态的入口。",
            input: null,
            output: null,
            createdAt: at,
          },
          {
            seq: 2,
            callId: "r1",
            tool: "Read",
            eventType: "tool_use",
            content: null,
            input: { path: "internal/acctAdmin/service/kyc/kyc.go", offset: 120, limit: 40 },
            output: null,
            createdAt: at,
          },
        ],
      }),
    ],
    side: {
      ...emptySide(),
      priority: "high",
      labels: ["回归", "开户"],
      runs: [
        {
          id: "preview-task-run-live",
          agentId: "miku",
          runtimeId: "cursor",
          status: "running",
          assignment: "排查详情页状态",
          createdAt: at,
        },
      ],
    },
  },
  {
    id: "failed",
    label: "失败回复",
    detail: "助手失败态 + 侧栏 failed run。",
    task: task({
      id: "preview-task-failed",
      sessionId: "preview-task-session-failed",
      title: "补交收审驳回单测",
      status: "todo",
      agentId: "miku",
      priority: "medium",
    }),
    agents,
    messages: [
      message({
        id: "preview-task-fail-u",
        sessionId: "preview-task-session-failed",
        role: "user",
        targetType: "agent",
        targetId: "miku",
        content: "把自动驳回的单测补上。",
      }),
      message({
        id: "preview-task-fail-a",
        sessionId: "preview-task-session-failed",
        role: "assistant",
        agentId: "miku",
        audienceType: "user",
        content: "构建失败，单测没跑完。",
        runId: "preview-task-run-fail",
        failureReason: "go test ./internal/acctAdmin/...: exit status 1",
        elapsedMs: 8200,
      }),
    ],
    side: {
      ...emptySide(),
      priority: "medium",
      runs: [
        {
          id: "preview-task-run-fail",
          agentId: "miku",
          runtimeId: "cursor",
          status: "failed",
          assignment: "补单测",
          createdAt: at,
        },
      ],
    },
  },
  {
    id: "delegation",
    label: "主任务派子任务",
    detail: "A @ B 派活 → A 说明 → B @ A 回复 → A 汇总。",
    task: task({
      id: "preview-task-parent",
      sessionId: "preview-task-session-parent",
      title: "【开户审核】交收审驳回后认证审自动驳回",
      description: "交收审驳回后，认证审对所有入金方式自动驳回。",
      status: "in_review",
      agentId: "miku",
      priority: "urgent",
      labels: ["需求", "后端"],
      checklist: [
        { id: "c1", text: "还原原仓库改动", done: true },
        { id: "c2", text: "在需求任务里实现", done: true },
        { id: "c3", text: "回归验证", done: false },
      ],
      prUrl: "https://github.com/example/uc/pull/128",
    }),
    agents,
    messages: [
      message({
        id: "preview-task-del-u",
        sessionId: "preview-task-session-parent",
        role: "user",
        targetType: "agent",
        targetId: "miku",
        content: "原仓库还原，在需求中修改，这次允许修改需求中的项目",
      }),
      message({
        id: "preview-task-del-asg",
        sessionId: "preview-task-session-parent",
        role: "assistant",
        agentId: "miku",
        audienceType: "agent",
        audienceId: "coder",
        messageKind: "assignment",
        content: `请处理子任务「${CHILD_TITLE}」（\`${CHILD_ID}\`），完成后向我汇报。`,
      }),
      message({
        id: "preview-task-del-a1",
        sessionId: "preview-task-session-parent",
        role: "assistant",
        agentId: "miku",
        audienceType: "user",
        content: "原仓库已经还原，实现已经转到需求里的任务去做，我这边还没改代码。",
        runId: "preview-task-run-a1",
        elapsedMs: 4100,
        createdAt: at,
      }),
      message({
        id: `bridge:${CHILD_ID}`,
        sessionId: "preview-task-session-parent",
        role: "assistant",
        agentId: "coder",
        audienceType: "agent",
        audienceId: "miku",
        content: "补丁已应用到工作区，构建和指定测试都通过了。\n\n- **改动文件:** `internal/acctAdmin/service/kyc/kyc.go`\n- 新增单测 2 个，均已通过。",
        elapsedMs: 9100,
        createdAt: "2026-11-04T06:01:59.531Z",
      }),
      message({
        id: "preview-task-del-a2",
        sessionId: "preview-task-session-parent",
        role: "assistant",
        agentId: "miku",
        audienceType: "user",
        content: "父任务已改为 `in_review`。子任务补丁已应用，构建和指定测试都通过了。",
        runId: "preview-task-run-a2",
        elapsedMs: 6800,
        createdAt: later,
      }),
    ],
    side: {
      projectName: "开户审核",
      priority: "urgent",
      labels: ["需求", "后端"],
      checklist: [
        { id: "c1", text: "还原原仓库改动", done: true },
        { id: "c2", text: "在需求任务里实现", done: true },
        { id: "c3", text: "回归验证", done: false },
      ],
      children: [
        {
          id: CHILD_ID,
          title: CHILD_TITLE,
          status: "in_review",
          agentId: "coder",
          reply: "补丁已应用到工作区，构建和指定测试都通过了。",
        },
      ],
      prUrl: "https://github.com/example/uc/pull/128",
      runs: [
        {
          id: "preview-task-run-a1",
          agentId: "miku",
          runtimeId: "cursor",
          status: "completed",
          assignment: "派活并说明",
          createdAt: at,
        },
        {
          id: "preview-task-run-a2",
          agentId: "miku",
          runtimeId: "cursor",
          status: "completed",
          assignment: "汇总子任务",
          createdAt: later,
        },
      ],
    },
  },
  {
    id: "delegation-pending",
    label: "子任务等待回复",
    detail: "已派活，A 已答，B 尚未回复（嵌套卡空态）。",
    task: task({
      id: "preview-task-pending",
      sessionId: "preview-task-session-pending",
      title: "拆实现给 Coder",
      status: "in_progress",
      agentId: "miku",
    }),
    agents,
    messages: [
      message({
        id: "preview-task-pend-asg",
        sessionId: "preview-task-session-pending",
        role: "assistant",
        agentId: "miku",
        audienceType: "agent",
        audienceId: "coder",
        messageKind: "assignment",
        content: `请处理子任务「${CHILD_TITLE}」（\`${CHILD_ID}\`），完成后向我汇报。`,
      }),
      message({
        id: "preview-task-pend-a1",
        sessionId: "preview-task-session-pending",
        role: "assistant",
        agentId: "miku",
        audienceType: "user",
        content: "已建子任务，等 Coder 回传结果。",
        elapsedMs: 2200,
      }),
    ],
    side: {
      ...emptySide(),
      children: [
        {
          id: CHILD_ID,
          title: CHILD_TITLE,
          status: "in_progress",
          agentId: "coder",
          pending: true,
        },
      ],
      runs: [
        {
          id: "preview-task-run-pend",
          agentId: "coder",
          runtimeId: "cursor",
          status: "running",
          assignment: CHILD_TITLE,
          createdAt: at,
        },
      ],
    },
    embedPreviewById: {
      [CHILD_ID]: {
        status: "in_progress",
        agent: { id: "coder", name: "Coder", avatar: "C" },
        pending: true,
      },
    },
  },
  {
    id: "rich-side",
    label: "侧栏满数据",
    detail: "标签、清单、子任务、PR、多次 run 都填满。",
    task: task({
      id: "preview-task-rich",
      sessionId: "preview-task-session-rich",
      title: "发布前核对清单",
      description: "核对发布范围与回归用例。",
      status: "blocked",
      agentId: "miku",
      priority: "high",
      labels: ["发布", "SIT"],
      checklist: [
        { id: "r1", text: "服务范围确认", done: true },
        { id: "r2", text: "数据脚本准备", done: false },
        { id: "r3", text: "回滚方案", done: false },
      ],
      prUrl: "https://github.com/example/uc/pull/130",
    }),
    agents,
    messages: [
      message({
        id: "preview-task-rich-a",
        sessionId: "preview-task-session-rich",
        role: "assistant",
        agentId: "miku",
        audienceType: "user",
        content: "**只需要发布 `service-admin-account`。** 数据脚本还缺一条驳回样例。",
        elapsedMs: 5100,
      }),
    ],
    side: {
      projectName: "开户审核 · 发布",
      priority: "high",
      labels: ["发布", "SIT"],
      checklist: [
        { id: "r1", text: "服务范围确认", done: true },
        { id: "r2", text: "数据脚本准备", done: false },
        { id: "r3", text: "回滚方案", done: false },
      ],
      children: [
        {
          id: "preview-child-docs",
          title: "补发布说明",
          status: "todo",
          agentId: "coder",
        },
        {
          id: "preview-child-data",
          title: "准备驳回样例数据",
          status: "in_progress",
          agentId: "miku",
        },
      ],
      prUrl: "https://github.com/example/uc/pull/130",
      runs: [
        {
          id: "preview-task-run-rich-1",
          agentId: "miku",
          runtimeId: "cursor",
          status: "completed",
          assignment: "核对发布范围",
          createdAt: at,
        },
        {
          id: "preview-task-run-rich-2",
          agentId: "miku",
          runtimeId: "cursor",
          status: "completed",
          assignment: "列回归点",
          createdAt: later,
        },
      ],
    },
  },
  {
    id: "done",
    label: "已完成",
    detail: "done 态，活动区有结论，侧栏有 PR。",
    task: task({
      id: "preview-task-done",
      sessionId: "preview-task-session-done",
      title: "交收审自动驳回已上线",
      status: "done",
      agentId: "miku",
      priority: "low",
      prUrl: "https://github.com/example/uc/pull/128",
    }),
    agents,
    messages: [
      message({
        id: "preview-task-done-a",
        sessionId: "preview-task-session-done",
        role: "assistant",
        agentId: "miku",
        audienceType: "user",
        content: "SIT 驳回样例符合预期，可以关单。",
        elapsedMs: 1800,
      }),
    ],
    side: {
      ...emptySide(),
      priority: "low",
      prUrl: "https://github.com/example/uc/pull/128",
      runs: [
        {
          id: "preview-task-run-done",
          agentId: "miku",
          runtimeId: "cursor",
          status: "completed",
          assignment: "验收结论",
          createdAt: later,
        },
      ],
    },
  },
];

export const taskDetailPreviewScenarioOptions = taskDetailPreviewScenarios.map((item) => ({
  value: item.id,
  label: item.label,
}));

export function taskDetailPreviewScenario(id: string) {
  return taskDetailPreviewScenarios.find((item) => item.id === id) ?? taskDetailPreviewScenarios[0]!;
}
