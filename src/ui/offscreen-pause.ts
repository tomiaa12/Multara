/** 离屏（含 keep-alive 隐藏、display:none）时通知调用方，用来暂停动画或延迟加载。全局共用一个观察器；不支持时视为可见。 */
const listeners = new WeakMap<Element, (offscreen: boolean) => void>();
let observer: IntersectionObserver | null = null;

export function watchOffscreen(el: Element, onChange: (offscreen: boolean) => void): () => void {
  if (typeof IntersectionObserver === "undefined") {
    onChange(false);
    return () => undefined;
  }
  observer ??= new IntersectionObserver((entries) => {
    for (const entry of entries) listeners.get(entry.target)?.(!entry.isIntersecting);
  });
  listeners.set(el, onChange);
  observer.observe(el);
  return () => {
    observer?.unobserve(el);
    listeners.delete(el);
  };
}
