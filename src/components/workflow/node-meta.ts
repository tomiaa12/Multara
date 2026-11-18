import type { Component } from "vue";
import {
  Bell, Bot, Flag, GitBranch, Globe, Layers, Play, ShieldCheck, Shuffle, Split, StickyNote, Terminal, Workflow,
} from "@lucide/vue";
import type { NodeKind } from "../../domain/workflow";

export interface NodeKindMeta {
  icon: Component;
  /** 左侧强调条与图标颜色（只用设计 token） */
  color: string;
}

export const NODE_KIND_META: Record<NodeKind, NodeKindMeta> = {
  trigger: { icon: Play, color: "var(--color-success)" },
  output: { icon: Flag, color: "var(--color-success)" },
  agent: { icon: Bot, color: "var(--color-purple)" },
  script: { icon: Terminal, color: "var(--color-accent)" },
  gate: { icon: ShieldCheck, color: "var(--color-warning)" },
  switch: { icon: Split, color: "var(--color-info)" },
  transform: { icon: Shuffle, color: "var(--color-text-secondary)" },
  http: { icon: Globe, color: "var(--color-info)" },
  git: { icon: GitBranch, color: "var(--color-git-modified)" },
  notify: { icon: Bell, color: "var(--color-warning)" },
  subflow: { icon: Workflow, color: "var(--color-purple)" },
  map: { icon: Layers, color: "var(--color-purple)" },
  note: { icon: StickyNote, color: "var(--color-text-tertiary)" },
};

export function kindLabelKey(kind: NodeKind): string {
  return `workflow.inspector.kind.${kind}`;
}
