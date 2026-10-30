/** 按可用宽度做中间省略：首尾保留，中间 `...`。装得下则原样返回。 */
export function fitMiddleEllipsis(
  text: string,
  maxWidth: number,
  measure: (value: string) => number,
  ellipsis = "...",
): string {
  if (!text || maxWidth <= 0) return text;
  if (measure(text) <= maxWidth) return text;
  if (measure(ellipsis) >= maxWidth) return ellipsis;

  let low = 0;
  let high = text.length;
  let best = ellipsis;
  while (low <= high) {
    const keep = Math.floor((low + high) / 2);
    if (keep <= 0) {
      high = keep - 1;
      continue;
    }
    // 路径更在意末尾目录名，后半略多一点
    const front = Math.max(1, Math.floor(keep * 0.4));
    const back = Math.max(1, keep - front);
    if (front + back >= text.length) {
      low = keep + 1;
      continue;
    }
    const candidate = `${text.slice(0, front)}${ellipsis}${text.slice(text.length - back)}`;
    if (measure(candidate) <= maxWidth) {
      best = candidate;
      low = keep + 1;
    } else {
      high = keep - 1;
    }
  }
  return best;
}

let sharedContext: CanvasRenderingContext2D | null | undefined;
let sharedFont = "";

/** 用元素计算样式建测量函数；所有实例共用一个 canvas，测量前切到各自的字体。 */
export function createTextMeasurer(el: Element): (value: string) => number {
  sharedContext ??= document.createElement("canvas").getContext("2d");
  const ctx = sharedContext;
  if (!ctx) return (value) => value.length;
  const style = getComputedStyle(el);
  const font = [
    style.fontStyle,
    style.fontWeight,
    style.fontSize,
    style.fontFamily,
  ].join(" ");
  return (value) => {
    if (sharedFont !== font) {
      ctx.font = font;
      sharedFont = font;
    }
    return ctx.measureText(value).width;
  };
}

/** @deprecated 段级缩略；新 UI 请用 fitMiddleEllipsis */
export function middleEllipsisPath(path: string, keepStart = 1, keepEnd = 2): string {
  const sep = path.includes("\\") ? "\\" : "/";
  const parts = path.split(/[/\\]/).filter(Boolean);
  if (parts.length <= keepStart + keepEnd) return path;
  return `${parts.slice(0, keepStart).join(sep)}${sep}...${sep}${parts.slice(-keepEnd).join(sep)}`;
}
