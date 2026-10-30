import { computed, ref, shallowReactive } from "vue";
import en from "./locales/en";
import zhCN from "./locales/zh-CN";
import type { Locale, MessageCatalog, MessageKey } from "./types";
import { LOCALES } from "./types";

export type { Locale, MessageCatalog, MessageKey };
export { LOCALES };

/** 默认语言和回退语言（en）随入口加载，其余语言包按需加载；加载完成后依赖文案的界面自动刷新。 */
const catalogs = shallowReactive<Partial<Record<Locale, MessageCatalog>>>({
  "zh-CN": zhCN,
  en,
});

const lazyCatalogs: Partial<Record<Locale, () => Promise<{ default: MessageCatalog }>>> = {
  "zh-TW": () => import("./locales/zh-TW"),
  ja: () => import("./locales/ja"),
  ko: () => import("./locales/ko"),
};

export async function loadLocale(id: Locale) {
  if (catalogs[id]) return;
  const loaded = await lazyCatalogs[id]?.().catch(() => undefined);
  if (loaded) catalogs[id] = loaded.default;
}

// 测试里会同步断言各语言文案；生产构建中 MODE 是常量，这段会被摇树去掉。
if (import.meta.env.MODE === "test") await Promise.all(LOCALES.map(loadLocale));

export const locale = ref<Locale>(resolveLocale("system"));
void loadLocale(locale.value);
let requestedLocale = locale.value;

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

function switchLocale(next: Locale) {
  locale.value = next;
  if (typeof document !== "undefined") {
    document.documentElement.lang = locale.value;
  }
}

/** 已加载的语言同步切换；未加载的先加载再切换，避免瞬间回退成英文；连续切换时只认最后一次。 */
export function applyLanguageSetting(setting?: string): Promise<void> {
  const next = resolveLocale(setting);
  requestedLocale = next;
  if (catalogs[next]) {
    switchLocale(next);
    return Promise.resolve();
  }
  return loadLocale(next).then(() => {
    if (requestedLocale === next) switchLocale(next);
  });
}

export function translate(key: MessageKey, language?: string, vars?: Record<string, string | number>): string {
  const id = resolveLocale(language ?? locale.value);
  const template = catalogs[id]?.[key] ?? catalogs.en?.[key] ?? String(key);
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
