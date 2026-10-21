/**
 * 浮层 z-index 分配（对齐 Element Plus useZIndex / 旧 PopupManager）：
 * 1. 弹层 Teleport 到 body，脱离父级 overflow / stacking
 * 2. 打开时 nextZIndex()，后开的盖住先开的
 * 3. 起步值高于 modal（200）与 run-log（240），默认 300，与 .ds-select-popover 一致
 */
const DEFAULT_BASE = 300;

let current = 0;

export function peekPopupZIndex(base = DEFAULT_BASE): number {
  return base + current;
}

export function nextPopupZIndex(base = DEFAULT_BASE): number {
  current += 1;
  return base + current;
}

/** 测试用 */
export function resetPopupZIndex() {
  current = 0;
}
