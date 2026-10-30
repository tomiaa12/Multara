import type { ComposerNode } from "./composer-content";

export const COMPOSER_CLIPBOARD_ATTR = "data-multara-composer";

export function encodeComposerClipboard(nodes: ComposerNode[]) {
  return encodeURIComponent(JSON.stringify(nodes));
}

const HTML_ESCAPES: Record<string, string> = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" };

function escapeHtml(value: string) {
  return value.replace(/[&<>"']/g, (char) => HTML_ESCAPES[char]);
}

export function composerClipboardHtml(nodes: ComposerNode[], plain: string) {
  return `<span ${COMPOSER_CLIPBOARD_ATTR}="${encodeComposerClipboard(nodes)}">${escapeHtml(plain)}</span>`;
}

export function decodeComposerClipboard(raw: string): ComposerNode[] | null {
  if (!raw) return null;
  const fromHtml = raw.match(new RegExp(`${COMPOSER_CLIPBOARD_ATTR}="([^"]*)"`))?.[1] ?? raw;
  try {
    const parsed = JSON.parse(decodeURIComponent(fromHtml)) as unknown;
    if (!Array.isArray(parsed)) return null;
    const nodes: ComposerNode[] = [];
    for (const item of parsed) {
      if (!item || typeof item !== "object") continue;
      const node = item as Record<string, unknown>;
      if (node.type === "text" && typeof node.value === "string") {
        nodes.push({ type: "text", value: node.value });
        continue;
      }
      if (
        node.type === "chip"
        && typeof node.referenceId === "string"
        && typeof node.label === "string"
        && ["agent", "squad", "skill", "task", "requirement"].includes(String(node.referenceKind))
      ) {
        nodes.push({
          type: "chip",
          id: `${String(node.referenceKind)}:${node.referenceId}`,
          referenceId: node.referenceId,
          referenceKind: node.referenceKind as "agent" | "squad" | "skill" | "task" | "requirement",
          label: node.label,
        });
        continue;
      }
      if (node.type !== "chip" || typeof node.path !== "string") continue;
      nodes.push({
        type: "chip",
        id: typeof node.id === "string" ? node.id : "",
        path: node.path,
        startLine: Number(node.startLine) || 0,
        endLine: Number(node.endLine) || 0,
        text: typeof node.text === "string" ? node.text : "",
        ...(node.isDirectory ? { isDirectory: true } : {}),
        ...(node.source === "terminal" ? { source: "terminal" as const } : {}),
      });
    }
    return nodes.length ? nodes : null;
  } catch {
    return null;
  }
}
