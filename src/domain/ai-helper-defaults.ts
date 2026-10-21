/** 与后端 DEFAULT_BRANCH_PROMPT / DEFAULT_COMMIT_PROMPT 保持一致。 */
export const DEFAULT_BRANCH_PROMPT =
  "根据需求名起一个简短的 Git 分支名。不要把整段需求标题原样拼进去，提炼 2～5 个关键词。优先小写英文和连字符，专有名词可以保留原文。只输出一行分支名。";

export const DEFAULT_COMMIT_PROMPT =
  "只根据已暂存的 Git diff 写一条 Conventional Commit 信息。只输出一行，不超过 100 个字符，使用 feat/fix/refactor 等前缀，不要解释。";

export const GEN_MODE_AGENT = "agent";
export const GEN_MODE_API = "api";

export function normalizeGenMode(value: string | undefined | null): typeof GEN_MODE_AGENT | typeof GEN_MODE_API {
  return value === GEN_MODE_API ? GEN_MODE_API : GEN_MODE_AGENT;
}

export function displayPrompt(stored: string | undefined | null, fallback: string): string {
  if (!stored?.trim()) return fallback;
  return stored;
}

export function persistPrompt(edited: string, fallback: string): string {
  const text = edited.trim();
  if (!text || text === fallback.trim()) return "";
  return text;
}
