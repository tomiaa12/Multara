/** 是否还有未滚到底的内容（用于底部渐变提示，对齐 req-clip / composer 裁切渐变）。 */
export function hasMoreBelow(el: HTMLElement | null, tolerancePx = 2): boolean {
  if (!el) return false;
  return el.scrollHeight - el.scrollTop - el.clientHeight > tolerancePx;
}
