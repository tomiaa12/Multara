import { ref, shallowRef } from "vue";
import type { FileIconResolver, ResolvedFileIcon } from "./types";

export type { ResolvedFileIcon } from "./types";

// id 与设置项 fileIconTheme 的取值一致，Rust 端 validate_settings 里有同一份白名单。
export const FILE_ICON_THEMES = [
  { value: "material-icon-theme", label: "Material Icon Theme" },
  { value: "vscode-icons", label: "VSCode Icons" },
] as const;

export type FileIconThemeId = (typeof FILE_ICON_THEMES)[number]["value"];

export const DEFAULT_FILE_ICON_THEME: FileIconThemeId = "material-icon-theme";

// 每套主题的清单和图标集都比较大，按需动态加载，只下载当前选中的那套。
const loaders: Record<FileIconThemeId, () => Promise<{ resolver: FileIconResolver }>> = {
  "material-icon-theme": () => import("./material"),
  "vscode-icons": () => import("./vscode"),
};

export function normalizeFileIconTheme(value: string | null | undefined): FileIconThemeId {
  return FILE_ICON_THEMES.some((theme) => theme.value === value)
    ? (value as FileIconThemeId)
    : DEFAULT_FILE_ICON_THEME;
}

const activeResolver = shallowRef<FileIconResolver | null>(null);
const lightMode = ref(false);

let requested: FileIconThemeId | null = null;

export function setFileIconTheme(value: string | null | undefined) {
  const next = normalizeFileIconTheme(value);
  if (requested === next) return;
  requested = next;
  void loaders[next]().then(({ resolver }) => {
    // 快速切换时只保留最后一次请求的结果
    if (requested !== next) return;
    activeResolver.value = resolver;
  });
}

if (typeof document !== "undefined") {
  const syncLight = () => {
    lightMode.value = document.documentElement.dataset.theme === "light";
  };
  syncLight();
  new MutationObserver(syncLight).observe(document.documentElement, {
    attributes: true,
    attributeFilter: ["data-theme"],
  });
}

/** 在 computed 里调用：主题加载完成或明暗切换后会自动重新计算。主题未就绪时返回 null。 */
export function resolveFileIcon(
  path: string,
  options: { directory?: boolean; open?: boolean } = {},
): ResolvedFileIcon | null {
  const resolver = activeResolver.value;
  if (!resolver) {
    setFileIconTheme(requested ?? DEFAULT_FILE_ICON_THEME);
    return null;
  }
  const name = path.split(/[\\/]/).pop() || path;
  return resolver({ name, ...options, light: lightMode.value });
}