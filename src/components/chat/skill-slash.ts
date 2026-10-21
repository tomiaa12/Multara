import type { ComposerNode } from "./composer-content";
import type { GlobalResource } from "../../domain/types";

export type SkillSlashHit = {
  id: string;
  name: string;
  description: string;
};

/** 当前行末尾的 `/查询`；没有空格才算正在输入斜杠命令。 */
export function activeSlashQuery(text: string): string | null {
  const match = text.match(/(?:^|\s)\/([^\s]*)$/);
  return match ? match[1] : null;
}

export function filterSkillSlash(skills: GlobalResource[], query: string): SkillSlashHit[] {
  const text = query.trim().toLowerCase();
  return skills
    .filter((skill) => skill.kind === "skill")
    .filter((skill) => {
      if (!text) return true;
      return skill.name.toLowerCase().includes(text) || skill.description.toLowerCase().includes(text);
    })
    .slice(0, 8)
    .map((skill) => ({ id: skill.id, name: skill.name, description: skill.description }));
}

export function replaceSlashToken(nodes: ComposerNode[], skillName: string): ComposerNode[] {
  const replacement = `/${skillName} `;
  const next = nodes.map((node) => (node.type === "text" ? { ...node } : node));
  for (let index = next.length - 1; index >= 0; index -= 1) {
    const node = next[index];
    if (node.type !== "text") continue;
    const match = node.value.match(/(^|[\s])(\/[^\s]*)$/);
    if (!match) continue;
    const token = match[2];
    node.value = `${node.value.slice(0, node.value.length - token.length)}${replacement}`;
    return next;
  }
  next.push({ type: "text", value: replacement });
  return next;
}
