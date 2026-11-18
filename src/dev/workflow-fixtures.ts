/** 浏览器预览模式（无 Tauri）下的工作流样例数据，仅在 `hasBackend()` 为 false 时使用。 */
import type {
  Diagnostic,
  EdgeDef,
  NodeDef,
  NodeRun,
  NodeTemplate,
  RunDetail,
  SecretName,
  Workflow,
  WorkflowDef,
  WorkflowDetail,
  WorkflowRun,
  WorkflowSummary,
  WorkflowTemplateMeta,
} from "../domain/workflow";

const NOW = "2026-11-18T08:00:00.000Z";

function node(partial: Partial<NodeDef> & Pick<NodeDef, "id" | "kind" | "title" | "position">): NodeDef {
  return { inputs: [], outputs: [], config: {}, workspaces: [], join: "all", onError: "fail_run", ...partial };
}

function edge(id: string, source: string, sourcePort: string, target: string, targetPort: string, maxIterations?: number): EdgeDef {
  return maxIterations ? { id, source, sourcePort, target, targetPort, maxIterations } : { id, source, sourcePort, target, targetPort };
}

export const fixtureDefinition = (): WorkflowDef => ({
  schemaVersion: 1,
  inputs: [
    { name: "doc_url", type: "url", required: true, label: "需求文档地址", primary: true },
    { name: "project", type: "text", required: true, label: "需求 id" },
  ],
  outputs: [{ name: "mr_url", type: "url" }],
  workspaces: [{ key: "app", label: "代码仓", project: "{{ run.inputs.project }}", isolation: "worktree_new", autoCommit: true }],
  nodes: [
    node({ id: "start", kind: "trigger", title: "开始", position: { x: 0, y: 160 }, config: { sources: ["manual", "automation", "channel"] } }),
    node({
      id: "prd", kind: "agent", title: "读取需求文档", position: { x: 280, y: 160 },
      inputs: [{ name: "url", type: "url", required: true }],
      outputs: [{ name: "spec", type: "json", required: true }],
      config: { target: { type: "agent", id: "miku" }, mode: "auto", maxRepairTurns: 2, continueSession: "same_node", prompt: "阅读需求文档 {{ inputs.url }}，整理成结构化需求。" },
    }),
    node({
      id: "code", kind: "agent", title: "编写代码", position: { x: 560, y: 160 },
      inputs: [
        { name: "spec", type: "json", required: true },
        { name: "feedback", type: "markdown" },
      ],
      outputs: [{ name: "summary", type: "markdown", required: true }],
      workspaces: [{ slot: "app", access: "write" }],
      config: { target: { type: "agent", id: "miku" }, mode: "plan", maxRepairTurns: 2, continueSession: "same_node", prompt: "按需求实现：{{ inputs.spec | tojson }}\n{% if inputs.feedback %}上一轮反馈：{{ inputs.feedback }}{% endif %}" },
    }),
    node({
      id: "test", kind: "script", title: "运行单测", position: { x: 840, y: 160 },
      workspaces: [{ slot: "app", access: "read" }],
      config: { command: "pnpm test", shell: "default", cwd: { slot: "app" }, env: {}, successExitCodes: [0], failOnNonZero: false, parseStdout: "none", maxOutputKB: 256 },
    }),
    node({
      id: "check", kind: "switch", title: "测试结果判断", position: { x: 1120, y: 160 },
      inputs: [{ name: "value", type: "any" }],
      config: { value: "inputs.value", cases: [{ port: "pass", when: "inputs.value.exit_code == 0" }], defaultPort: "fail", mode: "first" },
    }),
    node({
      id: "review", kind: "gate", title: "人工验收", position: { x: 1400, y: 100 },
      config: {
        title: "验收：{{ run.inputs.project }}", body: "请检查改动并给出结论。",
        decisions: [
          { port: "approve", label: "通过", primary: true },
          { port: "changes", label: "打回修改", requireComment: true },
          { port: "reject", label: "拒绝" },
        ],
        showDiffOf: ["app"], notify: {},
      },
    }),
    node({
      id: "ship", kind: "git", title: "推送", position: { x: 1680, y: 60 },
      config: { slot: "app", op: "push", remote: "origin", setUpstream: true },
    }),
    node({ id: "done", kind: "output", title: "输出", position: { x: 1960, y: 60 }, inputs: [{ name: "mr_url", type: "url" }] }),
    node({ id: "note_1", kind: "note", title: "说明", position: { x: 560, y: 340 }, config: { text: "单测失败最多回改 3 轮。" } }),
  ],
  edges: [
    edge("e1", "start", "doc_url", "prd", "url"),
    edge("e2", "prd", "spec", "code", "spec"),
    edge("e3", "code", "$out", "test", "$in"),
    edge("e4", "test", "$result", "check", "value"),
    edge("e5", "check", "pass", "review", "$in"),
    edge("e6", "check", "fail", "code", "feedback", 3),
    edge("e7", "review", "approve", "ship", "$in"),
    edge("e8", "ship", "head", "done", "mr_url"),
  ],
  settings: { ownerAgentId: "miku", maxParallelNodes: 4, concurrency: "queue" },
});

export const fixtureWorkflows = (): Workflow[] => [
  {
    id: "wf-req", name: "需求到上线", slug: "req-to-release", description: "读需求 → 写代码 → 单测 → 评审 → 推送",
    icon: "", publishedVersion: 3, enabled: true, draftUpdatedAt: NOW, createdAt: NOW, updatedAt: NOW, hasUnpublishedChanges: true,
  },
  {
    id: "wf-review", name: "每日代码巡检", slug: "daily-review", description: "每天扫描依赖更新并通知",
    icon: "", publishedVersion: 1, enabled: false, draftUpdatedAt: NOW, createdAt: NOW, updatedAt: NOW, hasUnpublishedChanges: false,
  },
  {
    id: "wf-blank", name: "空白工作流", slug: "blank", description: "",
    icon: "", publishedVersion: null, enabled: false, draftUpdatedAt: NOW, createdAt: NOW, updatedAt: NOW, hasUnpublishedChanges: true,
  },
];

export const fixtureSummaries = (): WorkflowSummary[] => {
  const status = ["running", "succeeded", null];
  return fixtureWorkflows().map((workflow, index) => ({
    ...workflow,
    lastRunStatus: status[index] ?? null,
    lastRunAt: status[index] ? NOW : null,
    nodeCount: index === 0 ? 9 : 4,
  }));
};

export const fixtureDiagnostics = (): Diagnostic[] => [
  { severity: "warning", code: "slot_parallel_writers", nodeId: "test", message: "节点 test 与 code 可能并行写同一个槽位，运行时会被串行化" },
  { severity: "warning", code: "gate_decision_unconnected", nodeId: "review", edgeId: undefined, message: "人工验收的「打回修改」没有连线，触发后该分支直接结束" },
];

export const fixtureDetail = (workflowId: string): WorkflowDetail => {
  const workflow = fixtureWorkflows().find((item) => item.id === workflowId) ?? fixtureWorkflows()[0];
  const full = workflow.id === "wf-req";
  const draft = full
    ? fixtureDefinition()
    : { ...fixtureDefinition(), nodes: fixtureDefinition().nodes.slice(0, 2), edges: [], inputs: [], outputs: [], workspaces: [] };
  return {
    workflow,
    draft,
    versions: workflow.publishedVersion
      ? Array.from({ length: workflow.publishedVersion }, (_, index) => ({
          version: workflow.publishedVersion! - index, note: index === 0 ? "加入人工验收" : "初版", createdAt: NOW,
        }))
      : [],
    diagnostics: full ? fixtureDiagnostics() : [],
  };
};

export const fixtureNodeTemplates = (): NodeTemplate[] => [
  {
    id: "builtin.read_prd", category: "requirement", name: "读取需求文档", description: "读取需求文档链接，整理成结构化需求",
    recommends: { skills: ["lark-doc-get"], mcp: [] }, builtin: true, edges: [],
    nodes: [node({
      id: "prd", kind: "agent", title: "读取需求文档", position: { x: 0, y: 0 },
      inputs: [{ name: "url", type: "url", required: true }],
      outputs: [{ name: "spec", type: "json", required: true }],
      config: { target: { type: "agent", id: "" }, mode: "auto", maxRepairTurns: 2, continueSession: "same_node", prompt: "阅读需求文档 {{ inputs.url }}" },
    })],
  },
  {
    id: "builtin.unit_test", category: "quality", name: "运行测试并判断", description: "跑单测，失败走 fail 分支",
    recommends: null, builtin: true,
    nodes: [
      node({ id: "test", kind: "script", title: "运行测试", position: { x: 0, y: 0 }, config: { command: "pnpm test", shell: "default", cwd: null, env: {}, successExitCodes: [0], failOnNonZero: false, parseStdout: "none", maxOutputKB: 256 } }),
      node({ id: "check", kind: "switch", title: "测试结果", position: { x: 280, y: 0 }, inputs: [{ name: "value", type: "any" }], config: { value: "inputs.value", cases: [{ port: "pass", when: "inputs.value.exit_code == 0" }], defaultPort: "fail", mode: "first" } }),
    ],
    edges: [edge("e1", "test", "$result", "check", "value")],
  },
  {
    id: "user.demo", category: "user", name: "我的审批卡", description: "自定义模板", recommends: null, builtin: false, edges: [],
    nodes: [node({ id: "gate", kind: "gate", title: "审批", position: { x: 0, y: 0 }, config: { title: "审批", body: "", decisions: [], showDiffOf: [], notify: {} } })],
  },
];

export const fixtureFlowTemplates = (): WorkflowTemplateMeta[] => [
  { id: "flow.requirement_to_release", name: "需求到上线", description: "读需求 → 写代码 → 单测 → 评审 → 推送", nodeCount: 9 },
];

export const fixtureSecrets = (): SecretName[] => [
  { name: "GITLAB_TOKEN", description: "推送与建 MR", updatedAt: NOW },
];

const nodeRun = (partial: Partial<NodeRun> & Pick<NodeRun, "id" | "nodeId" | "kind" | "status">): NodeRun => ({
  runId: "run-1", iteration: 0, attempt: 1, inputs: {}, firedPorts: [], carried: null, state: {}, sessionId: null,
  repairCount: 0, workspaceBefore: null, workspaceAfter: null, forkWorkspaceId: null, skipReason: null, errorCode: null,
  error: null, retryAt: null, deadlineAt: null, createdAt: NOW, startedAt: NOW, completedAt: NOW, ...partial,
});

export const fixtureRuns = (workflowId?: string): WorkflowRun[] => [
  {
    id: "run-1", workflowId: workflowId ?? "wf-req", version: 3, status: "waiting", inputs: { doc_url: "https://example.com/prd", project: "req-1" },
    outputs: {}, triggerSource: "manual", triggerRef: null, parentNodeRunId: null, mapIndex: null, extra: {},
    overviewSessionId: "wf-run:run-1", taskId: null, errorCode: null, error: null, createdAt: NOW, startedAt: NOW, completedAt: null,
  },
  {
    id: "run-2", workflowId: workflowId ?? "wf-req", version: 0, status: "failed", inputs: { doc_url: "https://example.com/x", project: "req-2" },
    outputs: {}, triggerSource: "automation", triggerRef: null, parentNodeRunId: null, mapIndex: null, extra: {},
    overviewSessionId: "wf-run:run-2", taskId: null, errorCode: "agent_failed", error: "agent failed", createdAt: NOW, startedAt: NOW, completedAt: NOW,
  },
  {
    id: "run-3", workflowId: workflowId ?? "wf-req", version: 3, status: "succeeded", inputs: { doc_url: "https://example.com/y", project: "req-3" },
    outputs: { mr_url: "https://git.example.com/mr/1" }, triggerSource: "manual", triggerRef: null, parentNodeRunId: null, mapIndex: null, extra: {},
    overviewSessionId: "wf-run:run-3", taskId: null, errorCode: null, error: null, createdAt: NOW, startedAt: NOW, completedAt: NOW,
  },
];

export const fixtureRunDetail = (runId: string): RunDetail => {
  const runs = fixtureRuns();
  const run = runs.find((item) => item.id === runId) ?? runs[0];
  return {
    run,
    workflow: fixtureWorkflows()[0],
    definition: fixtureDefinition(),
    nodeRuns: [
      nodeRun({ id: "nr-start", nodeId: "start", kind: "trigger", status: "succeeded", firedPorts: [] }),
      nodeRun({ id: "nr-prd", nodeId: "prd", kind: "agent", status: "succeeded", sessionId: "wf-node:nr-prd" }),
      nodeRun({ id: "nr-code-0", nodeId: "code", kind: "agent", status: "succeeded", sessionId: "wf-node:nr-code-0" }),
      nodeRun({ id: "nr-test-0", nodeId: "test", kind: "script", status: "succeeded" }),
      nodeRun({ id: "nr-check-0", nodeId: "check", kind: "switch", status: "succeeded", firedPorts: ["fail"] }),
      nodeRun({ id: "nr-code-1", nodeId: "code", kind: "agent", iteration: 1, status: "succeeded", sessionId: "wf-node:nr-code-0" }),
      nodeRun({ id: "nr-test-1", nodeId: "test", kind: "script", iteration: 1, status: "succeeded" }),
      nodeRun({ id: "nr-check-1", nodeId: "check", kind: "switch", iteration: 1, status: "succeeded", firedPorts: ["pass"] }),
      nodeRun({ id: "nr-review", nodeId: "review", kind: "gate", iteration: 1, status: run.status === "succeeded" ? "succeeded" : "waiting_human", completedAt: null }),
      ...(run.status === "failed" ? [nodeRun({ id: "nr-ship", nodeId: "ship", kind: "git", iteration: 1, status: "failed", error: "push rejected", errorCode: "git_failed" })] : []),
    ],
    values: [
      { id: "v1", nodeRunId: "nr-prd", nodeId: "prd", port: "spec", portType: "json", size: 40, source: "agent", preview: { title: "示例需求", acceptance: ["可登录"] } },
    ],
    workspaces: [{ runId: run.id, slotKey: "app", workspaceId: "ws-1", path: "/tmp/wf/app", branch: "wf/req-to-release-3f2a", projectId: "req-1", baseCommit: "a1b2c3d", headCommit: "e4f5a6b", owned: true }],
    gates: [],
    loopCounters: { e6: 1 },
    children: [],
    usage: { code: { agentRuns: 2, elapsedMs: 184000 } },
  };
};
