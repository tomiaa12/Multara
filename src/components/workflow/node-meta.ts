import type { Component } from "vue";
import {
  Bot, Flag, Play,
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
};

export function kindLabelKey(kind: NodeKind): string {
  return `workflow.inspector.kind.${kind}`;
}

/** 画布 / 检查器显示用：有标题用标题，否则用节点类型文案（避免露出 start 这类 id） */
export function nodeDisplayTitle(
  node: { title?: string; kind: NodeKind } | null | undefined,
  fallbackId: string,
  translate: (key: string) => string,
): string {
  const title = node?.title?.trim();
  if (title) return title;
  if (node) return translate(kindLabelKey(node.kind));
  return fallbackId;
}
