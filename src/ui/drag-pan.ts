/**
 * 按住拖动横向滚动：中键在宿主内任意位置按下都能拖，左键只在空白处按下才拖。
 * 卡片、按钮、输入框、可拖拽元素和悬浮滚动条保留各自的左键行为。
 * 拖动超过阈值后吞掉紧随的 click / auxclick，避免松手时误触卡片。
 */

import type { Directive } from "vue";
import { BAR_CLASS } from "./overlay-scroll";

export const DRAG_PAN_THRESHOLD = 4;
export const DRAG_PAN_ACTIVE_CLASS = "is-drag-panning";

const INTERACTIVE_SELECTOR = [
  "button",
  "a[href]",
  "input",
  "textarea",
  "select",
  "label",
  "[contenteditable]:not([contenteditable='false'])",
  "[draggable='true']",
  "[data-pan-ignore]",
  "[role='button']",
  "[role='menuitem']",
  `.${BAR_CLASS}`,
].join(",");

/** 左键按在可交互元素上时不接管；中键只避开滚动条。 */
export function canStartPan(button: number, target: Element | null, host: Element): boolean {
  if (button !== 0 && button !== 1) {
    return false;
  }
  if (!target || !host.contains(target)) {
    return false;
  }
  const blocked = button === 0 ? INTERACTIVE_SELECTOR : `.${BAR_CLASS}`;
  const hit = target.closest(blocked);
  return !hit || !host.contains(hit);
}

interface PanState {
  cleanup: () => void;
}

const states = new WeakMap<HTMLElement, PanState>();

function attach(host: HTMLElement): PanState {
  let pointerId: number | null = null;
  let startX = 0;
  let startScroll = 0;
  let moved = false;

  const swallowClick = (event: Event) => {
    event.preventDefault();
    event.stopPropagation();
  };

  const onMove = (event: PointerEvent) => {
    if (event.pointerId !== pointerId) {
      return;
    }
    const delta = event.clientX - startX;
    if (!moved && Math.abs(delta) < DRAG_PAN_THRESHOLD) {
      return;
    }
    if (!moved) {
      moved = true;
      host.classList.add(DRAG_PAN_ACTIVE_CLASS);
      host.setPointerCapture?.(event.pointerId);
      // 按下前已有的选区也一并清掉，拖动过程里不出现高亮。
      window.getSelection()?.removeAllRanges();
    }
    host.scrollLeft = startScroll - delta;
  };

  const finish = (event: PointerEvent) => {
    if (event.pointerId !== pointerId) {
      return;
    }
    pointerId = null;
    host.removeEventListener("pointermove", onMove);
    host.removeEventListener("pointerup", finish);
    host.removeEventListener("pointercancel", finish);
    if (host.hasPointerCapture?.(event.pointerId)) {
      host.releasePointerCapture(event.pointerId);
    }
    host.classList.remove(DRAG_PAN_ACTIVE_CLASS);
    if (moved) {
      // click / auxclick 在 pointerup 之后同步派发，下一帧前撤掉拦截。
      window.addEventListener("click", swallowClick, { capture: true, once: true });
      window.addEventListener("auxclick", swallowClick, { capture: true, once: true });
      requestAnimationFrame(() => {
        window.removeEventListener("click", swallowClick, { capture: true });
        window.removeEventListener("auxclick", swallowClick, { capture: true });
      });
    }
  };

  const onDown = (event: PointerEvent) => {
    if (pointerId !== null || event.pointerType !== "mouse") {
      return;
    }
    if (!canStartPan(event.button, event.target as Element | null, host)) {
      return;
    }
    if (host.scrollWidth <= host.clientWidth) {
      return;
    }
    if (event.button === 1) {
      // 阻止中键自动滚动 / 粘贴。
      event.preventDefault();
    }
    pointerId = event.pointerId;
    startX = event.clientX;
    startScroll = host.scrollLeft;
    moved = false;
    host.addEventListener("pointermove", onMove);
    host.addEventListener("pointerup", finish);
    host.addEventListener("pointercancel", finish);
  };

  // 中键按下时 WebKit 仍可能触发 mousedown 默认行为，单独拦一次。
  const onMouseDown = (event: MouseEvent) => {
    if (event.button === 1 && canStartPan(1, event.target as Element | null, host)) {
      event.preventDefault();
    }
  };

  // 可能开始拖动时不让浏览器开始选中文字；不拦 mousedown，免得影响焦点和外部点击关闭菜单。
  const onSelectStart = (event: Event) => {
    if (pointerId !== null) {
      event.preventDefault();
    }
  };

  host.addEventListener("pointerdown", onDown);
  host.addEventListener("mousedown", onMouseDown);
  host.addEventListener("selectstart", onSelectStart);
  return {
    cleanup() {
      host.removeEventListener("pointerdown", onDown);
      host.removeEventListener("mousedown", onMouseDown);
      host.removeEventListener("selectstart", onSelectStart);
      host.removeEventListener("pointermove", onMove);
      host.removeEventListener("pointerup", finish);
      host.removeEventListener("pointercancel", finish);
      host.classList.remove(DRAG_PAN_ACTIVE_CLASS);
    },
  };
}

export const vDragPan: Directive<HTMLElement> = {
  mounted(el) {
    states.set(el, attach(el));
  },
  beforeUnmount(el) {
    states.get(el)?.cleanup();
    states.delete(el);
  },
};
