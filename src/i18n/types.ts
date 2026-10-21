export type Locale = "zh-CN" | "en" | "zh-TW" | "ja" | "ko";

export type MessageCatalog = Record<string, string>;

export type MessageKey = string;

export const LOCALES: Locale[] = ["zh-CN", "en", "zh-TW", "ja", "ko"];
