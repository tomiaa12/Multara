import type { ComposerNode } from "./composer-content";
import type { Agent, Task } from "../../domain/types";

export type MentionHit = {
  id: string;
  kind: "agent" | "task" | "command";
  label: string;
  detail: string;
  insert: string;
  avatar?: string | null;
};

export function activeMentionQuery(text: string): { kind: "@" | "#"; query: string } | null {
  const match = text.match(/(?:^|\s)([@#])([^\s]*)$/);
  return match ? { kind: match[1] as "@" | "#", query: match[2] } : null;
}

export function filterMentions(
  kind: "@" | "#",
  query: string,
  agents: Agent[],
  tasks: Task[],
): MentionHit[] {
  const text = query.trim().toLowerCase();
  if (kind === "@") {
    return agents
      .filter((agent) => !text || agent.name.toLowerCase().includes(text))
      .slice(0, 8)
      .map((agent) => ({
        id: agent.id,
        kind: "agent" as const,
        label: agent.name,
        detail: agent.model || "",
        insert: `@${agent.name} `,
        avatar: agent.avatar,
      }));
  }
  return tasks
    .filter((task) => !text || task.title.toLowerCase().includes(text))
    .slice(0, 8)
    .map((task) => ({
      id: task.id,
      kind: "task" as const,
      label: task.title,
      detail: task.status,
      insert: `#${task.title} `,
    }));
}

export function replaceMentionToken(nodes: ComposerNode[], insert: string): ComposerNode[] {
  const next = nodes.map((node) => (node.type === "text" ? { ...node } : node));
  for (let index = next.length - 1; index >= 0; index -= 1) {
    const node = next[index];
    if (node.type !== "text") continue;
    const match = node.value.match(/(^|[\s])([@#][^\s]*)$/);
    if (!match) continue;
    const token = match[2];
    node.value = `${node.value.slice(0, node.value.length - token.length)}${insert}`;
    return next;
  }
  next.push({ type: "text", value: insert });
  return next;
}

export const COMPOSER_COMMANDS: MentionHit[] = [
  { id: "compact", kind: "command", label: "compact", detail: "压缩当前对话上下文", insert: "/compact" },
  { id: "fork", kind: "command", label: "fork", detail: "复制当前会话", insert: "/fork" },
];

export function filterCommands(query: string): MentionHit[] {
  const text = query.trim().toLowerCase();
  return COMPOSER_COMMANDS.filter((item) => !text || item.label.includes(text) || item.detail.includes(text));
}
