import manifest from "material-icon-theme/dist/material-icons.json";
import { createMaterialResolver, type MaterialManifest } from "./material-resolver";

// 1200+ 个 SVG 只收集地址，由 <img> 按需加载；no-inline 防止小图标被内联成 base64 撑大脚本。
const svgUrls = import.meta.glob<string>("../../node_modules/material-icon-theme/icons/*.svg", {
  eager: true,
  import: "default",
  query: "?no-inline",
});

const urlByFile = new Map(
  Object.entries(svgUrls).map(([path, url]) => [path.slice(path.lastIndexOf("/") + 1), url]),
);

// 清单里的 iconPath 形如 "./../icons/git.svg"，只取文件名去匹配
export const resolver = createMaterialResolver(
  manifest as MaterialManifest,
  (iconPath) => urlByFile.get(iconPath.slice(iconPath.lastIndexOf("/") + 1)),
);
