import type { FileIconQuery, FileIconResolver } from "./types";

type IconMap = Record<string, string>;

interface ManifestMaps {
  fileExtensions?: IconMap;
  fileNames?: IconMap;
  folderNames?: IconMap;
  folderNamesExpanded?: IconMap;
}

/** `material-icon-theme/dist/material-icons.json` 中用到的字段（VS Code 文件图标主题格式）。 */
export interface MaterialManifest extends ManifestMaps {
  iconDefinitions: Record<string, { iconPath: string }>;
  file: string;
  folder: string;
  folderExpanded: string;
  light?: ManifestMaps;
}

interface NormalizedMaps {
  fileExtensions: Map<string, string>;
  fileNames: Map<string, string>;
  folderNames: Map<string, string>;
  folderNamesExpanded: Map<string, string>;
}

// VS Code 按小写文件名匹配图标，清单里少量带大写的键也统一转小写。
function normalize(maps: ManifestMaps | undefined): NormalizedMaps {
  const lower = (map: IconMap | undefined) =>
    new Map(Object.entries(map ?? {}).map(([key, icon]) => [key.toLowerCase(), icon]));
  return {
    fileExtensions: lower(maps?.fileExtensions),
    fileNames: lower(maps?.fileNames),
    folderNames: lower(maps?.folderNames),
    folderNamesExpanded: lower(maps?.folderNamesExpanded),
  };
}

function lookup(maps: NormalizedMaps, query: FileIconQuery, name: string): string | undefined {
  if (query.directory) {
    return (query.open ? maps.folderNamesExpanded : maps.folderNames).get(name);
  }
  const byName = maps.fileNames.get(name);
  if (byName) return byName;
  // 复合扩展名优先：foo.spec.ts 依次尝试 spec.ts、ts
  const parts = name.split(".");
  for (let index = 1; index < parts.length; index += 1) {
    const icon = maps.fileExtensions.get(parts.slice(index).join("."));
    if (icon) return icon;
  }
  return undefined;
}

/**
 * 按 VS Code 的匹配顺序解析 Material Icon Theme 图标：
 * 浅色覆盖表 → 默认表 → 主题默认文件 / 文件夹图标。
 * `urlFor` 把清单里的 iconPath 映射成可加载的地址。
 */
export function createMaterialResolver(
  manifest: MaterialManifest,
  urlFor: (iconPath: string) => string | undefined,
): FileIconResolver {
  const dark = normalize(manifest);
  const light = normalize(manifest.light);
  const fallbackUrl = (id: string) => urlFor(manifest.iconDefinitions[id]?.iconPath ?? "");

  return (query) => {
    const name = query.name.toLowerCase();
    const fallback = query.directory
      ? (query.open ? manifest.folderExpanded : manifest.folder)
      : manifest.file;
    const id = (query.light ? lookup(light, query, name) : undefined)
      ?? lookup(dark, query, name)
      ?? fallback;
    return { src: fallbackUrl(id) ?? fallbackUrl(fallback) ?? "" };
  };
}
