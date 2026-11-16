type CloseSurface = {
  element: () => HTMLElement | null;
  ignore?: (element: Element) => boolean;
  close: () => void | Promise<void>;
};

const surfaces: CloseSurface[] = [];

function hostElement(node: Node | null): Element | null {
  if (!node) return null;
  return node instanceof Element ? node : node.parentElement;
}

/** 聊天、编辑器各自注册。Command/Ctrl+W 只关闭当前焦点所在的那一块。 */
export function registerCloseSurface(surface: CloseSurface) {
  surfaces.push(surface);
  return () => {
    const index = surfaces.indexOf(surface);
    if (index >= 0) surfaces.splice(index, 1);
  };
}

export async function closeFocusedSurface(): Promise<boolean> {
  const element = hostElement(document.activeElement);
  const surface = surfaces.find((item) => {
    const root = item.element();
    if (!root || !element || !root.contains(element)) return false;
    return !item.ignore?.(element);
  });
  if (!surface) return false;
  await surface.close();
  return true;
}
