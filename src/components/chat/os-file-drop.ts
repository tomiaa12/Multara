/** 桌面端系统拖放。WebView 的 HTML5 drop 经常拿不到真实路径，改听 Tauri 的 drag-drop。 */

export const OS_DROP_EVENT = "multara-os-drop";

export type OsDropDetail = {
  paths: string[];
  x: number;
  y: number;
};

type HoverListener = (zone: Element | null, point: { x: number; y: number } | null) => void;

const hoverListeners = new Set<HoverListener>();
let starting: Promise<void> | null = null;

export function osDropClientPoint(position: { x: number; y: number }, scale: number) {
  const factor = scale > 0 ? scale : 1;
  return { x: position.x / factor, y: position.y / factor };
}

/**
 * wry 在 macOS 把 AppKit 的逻辑点标成了 PhysicalPosition。再除 devicePixelRatio
 * 会把落点缩到左上角，右下角聊天和居中弹层都对不上。Windows 的客户区坐标才是物理像素。
 */
export function osDropScale(platform: string, devicePixelRatio: number) {
  if (platform === "macos") return 1;
  return devicePixelRatio > 0 ? devicePixelRatio : 1;
}

function hostPlatform() {
  const platform = navigator.platform || "";
  const ua = navigator.userAgent || "";
  if (/Mac/.test(platform) || /Macintosh/.test(ua)) return "macos";
  if (/Win/.test(platform)) return "windows";
  return "other";
}

export function osDropZoneAt(x: number, y: number, doc: Document = document) {
  const stack = typeof doc.elementsFromPoint === "function" ? doc.elementsFromPoint(x, y) : [];
  for (const element of stack) {
    const zone = element.closest("[data-workspace-drop]");
    if (zone) return zone;
  }
  const hit = doc.elementFromPoint(x, y);
  return hit?.closest("[data-workspace-drop]") ?? null;
}

export function onOsDropHover(listener: HoverListener) {
  hoverListeners.add(listener);
  return () => {
    hoverListeners.delete(listener);
  };
}

function notifyHover(zone: Element | null, point: { x: number; y: number } | null) {
  hoverListeners.forEach((listener) => listener(zone, point));
}

export function startOsFileDrop() {
  if (starting || typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) return;
  starting = (async () => {
    const { getCurrentWebview } = await import("@tauri-apps/api/webview");
    await getCurrentWebview().onDragDropEvent((event) => {
      const payload = event.payload;
      if (payload.type === "leave") {
        notifyHover(null, null);
        return;
      }
      const point = osDropClientPoint(payload.position, osDropScale(hostPlatform(), window.devicePixelRatio || 1));
      const zone = osDropZoneAt(point.x, point.y);
      if (payload.type !== "drop") {
        notifyHover(zone, point);
        return;
      }
      notifyHover(null, null);
      if (!zone || !payload.paths.length) return;
      zone.dispatchEvent(new CustomEvent<OsDropDetail>(OS_DROP_EVENT, {
        bubbles: true,
        detail: { paths: payload.paths, x: point.x, y: point.y },
      }));
    });
  })().catch(() => {
    starting = null;
  });
}
