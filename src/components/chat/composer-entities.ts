import type { ComposerEntityKind, ComposerNode, ComposerReferenceChipNode } from "./composer-content";

export type ComposerSuggestCategory = ComposerEntityKind;

export type ComposerEntityItem = {
  id: string;
  kind: ComposerEntityKind;
  label: string;
  description?: string;
  avatar?: string | null;
  group: string;
};

export type ComposerTrigger = {
  symbol: "@" | "/" | "#";
  category: ComposerSuggestCategory;
  query: string;
};

const TRIGGER_CATEGORIES: Record<ComposerTrigger["symbol"], ComposerSuggestCategory> = {
  "@": "agent",
  "/": "skill",
  "#": "task",
};

export function activeComposerTrigger(text: string): ComposerTrigger | null {
  const match = text.match(/(?:^|\s)([@/#])([^\s]*)$/);
  if (!match) return null;
  const symbol = match[1] as ComposerTrigger["symbol"];
  return { symbol, category: TRIGGER_CATEGORIES[symbol], query: match[2] ?? "" };
}

export function filterComposerEntities(items: ComposerEntityItem[], query: string) {
  const normalized = query.trim().toLowerCase();
  if (!normalized) return items;
  return items.filter((item) =>
    item.label.toLowerCase().includes(normalized)
    || item.description?.toLowerCase().includes(normalized),
  );
}

export function referenceNode(item: ComposerEntityItem): ComposerReferenceChipNode {
  return {
    type: "chip",
    id: `${item.kind}:${item.id}`,
    referenceId: item.id,
    referenceKind: item.kind,
    label: item.label,
  };
}

export function replaceTriggerWithReference(
  nodes: ComposerNode[],
  trigger: ComposerTrigger,
  item: ComposerEntityItem,
): ComposerNode[] {
  const next = nodes.map((node) => (node.type === "text" ? { ...node } : node));
  const tokenPattern = new RegExp(`(^|\\s)\\${trigger.symbol}[^\\s]*$`);
  for (let index = next.length - 1; index >= 0; index -= 1) {
    const node = next[index];
    if (node.type !== "text") continue;
    const match = node.value.match(tokenPattern);
    if (!match) continue;
    const token = match[0].slice(match[1].length);
    node.value = node.value.slice(0, node.value.length - token.length);
    next.splice(index + 1, 0, referenceNode(item), { type: "text", value: " " });
    return next;
  }
  next.push(referenceNode(item), { type: "text", value: " " });
  return next;
}
