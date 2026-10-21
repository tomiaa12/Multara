import { computed, ref } from "vue";
import en from "./locales/en";
import ja from "./locales/ja";
import ko from "./locales/ko";
import zhCN from "./locales/zh-CN";
import zhTW from "./locales/zh-TW";
import type { Locale, MessageCatalog, MessageKey } from "./types";
import { LOCALES } from "./types";

export type { Locale, MessageCatalog, MessageKey };
export { LOCALES };

const catalogs: Record<Locale, MessageCatalog> = {
  "zh-CN": zhCN,
  en,
  "zh-TW": zhTW,
  ja,
  ko,
};

export const locale = ref<Locale>(resolveLocale("system"));

export function resolveLocale(language?: string): Locale {
  const raw = !language || language === "system"
    ? (typeof navigator !== "undefined" ? navigator.language : "en")
    : language;
  const lower = raw.toLowerCase().replaceAll("_", "-");
  if (lower === "zh-tw" || lower === "zh-hk" || lower === "zh-mo" || lower.startsWith("zh-hant")) {
    return "zh-TW";
  }
  if (lower === "zh-cn" || lower.startsWith("zh")) return "zh-CN";
  if (lower === "ja" || lower.startsWith("ja-")) return "ja";
  if (lower === "ko" || lower.startsWith("ko-")) return "ko";
  if (lower === "en" || lower.startsWith("en-")) return "en";
  return "en";
}

export function applyLanguageSetting(setting?: string) {
  locale.value = resolveLocale(setting);
  if (typeof document !== "undefined") {
    document.documentElement.lang = locale.value;
  }
}

export function translate(key: MessageKey, language?: string, vars?: Record<string, string | number>): string {
  const id = resolveLocale(language ?? locale.value);
  const template = catalogs[id][key] ?? catalogs.en[key] ?? String(key);
  return format(template, vars);
}

export function t(key: string, vars?: Record<string, string | number>): string {
  return translate(key, locale.value, vars);
}

export function useI18n() {
  const current = computed(() => locale.value);
  return {
    locale: current,
    t: (key: string, vars?: Record<string, string | number>) => translate(key, locale.value, vars),
  };
}

function format(template: string, vars?: Record<string, string | number>): string {
  if (!vars) return template;
  return template.replace(/\{(\w+)\}/g, (_, name: string) => (vars[name] == null ? `{${name}}` : String(vars[name])));
}
