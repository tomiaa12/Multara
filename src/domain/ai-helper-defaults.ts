import { t } from "../i18n";

/** 与后端默认提示词语义一致；文案走 i18n，随界面语言变化。 */
export function defaultBranchPrompt(): string {
  return t("settings.gen.defaultBranchPrompt");
}

export function defaultCommitPrompt(): string {
  return t("settings.gen.defaultCommitPrompt");
}

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
