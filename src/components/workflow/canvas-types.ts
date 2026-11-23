/** FlowCanvas 的公共类型：运行视图的状态覆盖层。 */
export interface NodeOverlay {
  /** 当前节点运行状态（pending / running / waiting_human / succeeded / failed / skipped / cancelled / ...） */
  status: string;
  /** 当前节点运行的 iteration（从 0 开始）、attempt（从 1 开始） */
  iteration: number;
  attempt: number;
  /** 该节点有多轮（iteration>0）或多次尝试（attempt>1）时才显示角标 */
  multiple: boolean;
  /** 当前节点运行的会话，供卡片上的智能体状态使用 */
  sessionId?: string | null;
  /** 刚流入这张卡片的值，避免画在连线中点被节点挡住 */
  incoming?: string;
}

export interface EdgeOverlay {
  /** live 高亮；dead 淡化；pending 默认 */
  state: "live" | "dead" | "pending";
  /** 下一步还在进行时，只让这一条线流动 */
  flowing?: boolean;
  /** 线上的值摘要（完整值放 tooltip） */
  label?: string;
  tooltip?: string;
  /** 回边运行时计数 k（显示「k/N」） */
  count?: number;
}

export interface RunOverlay {
  nodes: Record<string, NodeOverlay>;
  edges: Record<string, EdgeOverlay>;
}
