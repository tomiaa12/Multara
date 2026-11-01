/** 启动闪屏与设置共用的主题解析；`localStorage` 让首屏在 bootstrap 前就能上色。 */

export const THEME_STORAGE_KEY = "multara.theme";

export type ThemeSetting = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";

export function normalizeThemeSetting(value: string | undefined | null): ThemeSetting {
  if (value === "light" || value === "dark" || value === "system") return value;
  return "system";
}

export function resolveTheme(setting: string | undefined | null): ResolvedTheme {
  const normalized = normalizeThemeSetting(setting);
  if (normalized === "system") {
    return window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
  }
  return normalized;
}

/** 写入 `<html data-theme>`，并缓存设置值（含 system）供下次启动。 */
export function applyThemeSetting(setting: string | undefined | null): ResolvedTheme {
  const normalized = normalizeThemeSetting(setting);
  const resolved = resolveTheme(normalized);
  document.documentElement.dataset.theme = resolved;
  try {
    localStorage.setItem(THEME_STORAGE_KEY, normalized);
  } catch {
    /* 隐私模式等写不了也不挡界面 */
  }
  return resolved;
}
