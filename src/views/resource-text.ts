import type { GlobalResource } from "../domain/types";
import { matchText, splitByRange } from "../utils/pinyin-match";

export function highlightParts(text: string, query: string): [string, string, string] {
  return splitByRange(text, query.trim() ? matchText(text, query.trim()) : null);
}

export function resourceGroups(items: GlobalResource[]) {
  const groups = new Map<string, { key: string; label: string; scope: string; items: GlobalResource[] }>();
  for (const item of items) {
    const key = item.owner || "multara";
    const existing = groups.get(key);
    if (existing) existing.items.push(item);
    else {
      groups.set(key, {
        key,
        label: item.sourceLabel || key,
        scope: item.scope || "global",
        items: [item],
      });
    }
  }
  return [...groups.values()].sort((left, right) => {
    if (left.scope !== right.scope) return left.scope === "workspace" ? -1 : 1;
    return left.label.localeCompare(right.label, "zh");
  });
}
