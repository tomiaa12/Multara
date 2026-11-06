<script setup lang="ts">
import { onBeforeUnmount, onMounted, provide, ref } from "vue";
import {
  PANE_HEADER,
  fitHeightsToTotal,
  layoutPaneHeights,
  paneMax,
  paneMin,
  resizeAroundSash,
  sashCursor,
  sashDisabled,
  type PaneBox,
} from "./pane-layout";

const SASH_HOVER_DELAY = 300;
const mac = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.userAgent);

const root = ref<HTMLElement | null>(null);
const animated = ref(false);
let frame = 0;
let hoverFrame = 0;
let hoverSash: HTMLElement | null = null;
let dragging = false;
let observer: ResizeObserver | undefined;
let childObserver: MutationObserver | undefined;

function shownCursor(cursor: "ns-resize" | "n-resize" | "s-resize") {
  return mac && cursor === "ns-resize" ? "row-resize" : cursor;
}

function clearSashHover() {
  window.clearTimeout(hoverFrame);
  hoverSash?.classList.remove("is-hover");
  hoverSash = null;
}

function armSashHover(sash: HTMLElement) {
  if (sash.classList.contains("is-disabled")) return;
  if (sash === hoverSash) return;
  clearSashHover();
  hoverSash = sash;
  if (sash.classList.contains("is-active")) {
    sash.classList.add("is-hover");
    return;
  }
  hoverFrame = window.setTimeout(() => sash.classList.add("is-hover"), SASH_HOVER_DELAY);
}

function childPanes() {
  return [...(root.value?.querySelectorAll(":scope > .ui-pane") ?? [])] as HTMLElement[];
}

function readNumber(value: string | undefined) {
  const number = Number(value);
  return Number.isFinite(number) ? number : undefined;
}

function storedSize(pane: HTMLElement) {
  const key = pane.dataset.sizeKey;
  if (!key) return undefined;
  const number = Number(localStorage.getItem(key));
  return Number.isFinite(number) && number > 0 ? number : undefined;
}

/** 内容固有高度。body 被 flex 拉高时 scrollHeight 会跟着变大，不能拿来当首选高度。 */
function intrinsicBodyHeight(body: HTMLElement) {
  const style = getComputedStyle(body);
  let height = (parseFloat(style.paddingTop) || 0) + (parseFloat(style.paddingBottom) || 0);
  for (const child of body.children) {
    if (!(child instanceof HTMLElement)) continue;
    const childStyle = getComputedStyle(child);
    if (childStyle.display === "none") continue;
    height += child.offsetHeight
      + (parseFloat(childStyle.marginTop) || 0)
      + (parseFloat(childStyle.marginBottom) || 0);
  }
  return height;
}

function preferredHeight(pane: HTMLElement, total: number, spec: PaneBox) {
  const stored = storedSize(pane);
  if (stored) return stored;
  const fallback = readNumber(pane.dataset.defaultSize);
  if (fallback && fallback > 0) return fallback;
  const body = pane.querySelector(":scope > .ui-pane-body") as HTMLElement | null;
  const content = PANE_HEADER + (body ? intrinsicBodyHeight(body) : 0);
  const cap = Math.max(paneMin(spec), Math.round(total * 0.4));
  return Math.min(Math.max(content, paneMin(spec)), cap);
}

function specsFor(list: HTMLElement[], total: number, holdHeights: boolean, target?: HTMLElement, targetHeight?: number) {
  return list.map((pane) => {
    const spec: PaneBox = {
      fill: pane.dataset.fill === "true",
      expanded: pane.dataset.expanded === "true",
      minBody: readNumber(pane.dataset.minBody) ?? 22,
      preferred: 0,
    };
    if (pane === target && targetHeight != null) spec.preferred = targetHeight;
    else if (spec.expanded && !spec.fill) {
      spec.preferred = holdHeights
        ? (pane.getBoundingClientRect().height || preferredHeight(pane, total, spec))
        : preferredHeight(pane, total, spec);
    }
    return spec;
  });
}

function paint(list: HTMLElement[], heights: number[], specs: PaneBox[], total: number, scrollable: boolean) {
  const sum = heights.reduce((acc, height) => acc + height, 0);
  list.forEach((pane, index) => {
    pane.style.flex = "0 0 auto";
    pane.style.maxHeight = "none";
    pane.style.height = `${heights[index]}px`;
    const sash = pane.querySelector(":scope > .ui-pane-sash") as HTMLElement | null;
    if (!sash || index === 0) return;
    const disabled = sashDisabled(specs, index);
    sash.classList.toggle("is-disabled", disabled);
    // 与 VS Code 一致：上侧还能被压 / 下侧还能被压，决定光标单向还是双向。
    // 收起项 max===min，不参与可压缩判断。
    let canShrinkUp = false;
    let canShrinkDown = false;
    for (let item = 0; item < index; item += 1) {
      if (!specs[item].expanded) continue;
      if (heights[item] > paneMin(specs[item]) + 0.5) canShrinkUp = true;
    }
    for (let item = index; item < specs.length; item += 1) {
      if (!specs[item].expanded) continue;
      if (heights[item] > paneMin(specs[item]) + 0.5) canShrinkDown = true;
    }
    const atMin = !canShrinkDown;
    const atMax = !canShrinkUp;
    const cursor = disabled ? "default" : shownCursor(sashCursor(true, atMin, atMax));
    sash.dataset.cursor = cursor;
    sash.dataset.scrollable = scrollable ? "1" : "0";
    sash.style.removeProperty("cursor");
  });
  return sum;
}

function layout(holdHeights = false) {
  const el = root.value;
  if (!el || dragging) return;
  const list = childPanes();
  const total = el.clientHeight;
  if (!list.length || total <= 0) return;
  const specs = specsFor(list, total, holdHeights);
  const heights = layoutPaneHeights(total, specs);
  const sum = heights.reduce((acc, height) => acc + height, 0);
  const scrollable = sum > total + 1;
  el.classList.toggle("is-scrollable", scrollable);
  paint(list, heights, specs, total, scrollable);
}

function relayout(animate = false) {
  const el = root.value;
  if (animate && el) {
    el.classList.add("is-animated");
    animated.value = true;
    window.clearTimeout(frame);
    frame = window.setTimeout(() => {
      el.classList.remove("is-animated");
      animated.value = false;
    }, 200);
  }
  layout(false);
}

function onPointerDown(event: PointerEvent) {
  const sash = (event.target as HTMLElement | null)?.closest?.(".ui-pane-sash") as HTMLElement | null;
  if (!sash || sash.classList.contains("is-disabled")) return;
  const pane = sash.parentElement as HTMLElement | null;
  const list = childPanes();
  const index = pane ? list.indexOf(pane) : -1;
  const host = root.value;
  if (!host || index <= 0) return;
  const specs = specsFor(list, host.clientHeight, true);
  if (sashDisabled(specs, index)) return;
  event.preventDefault();
  event.stopPropagation();
  try {
    sash.setPointerCapture(event.pointerId);
  } catch {
    // Synthetic events cannot capture the pointer. Real drags still do.
  }
  sash.classList.add("is-active", "is-hover");
  dragging = true;
  animated.value = false;
  const startY = event.clientY;
  const total = host.clientHeight;
  const mins = specs.map((spec) => paneMin(spec));
  const maxes = specs.map((spec) => paneMax(spec));
  // 对齐 VS Code：拖动时上下互借，先压回视口，避免继续撑出滚动条。
  const startHeights = fitHeightsToTotal(
    list.map((item) => item.getBoundingClientRect().height),
    mins,
    total,
    maxes,
  );
  document.body.classList.add("is-resizing-panes");
  host.classList.toggle("is-scrollable", false);
  paint(list, startHeights, specs, total, false);

  const move = (moveEvent: PointerEvent) => {
    const currentHost = root.value;
    if (!currentHost) return;
    const current = childPanes();
    if (current.length !== startHeights.length) return;
    const currentTotal = currentHost.clientHeight;
    const currentSpecs = specsFor(current, currentTotal, true);
    const currentMins = currentSpecs.map((spec) => paneMin(spec));
    const currentMaxes = currentSpecs.map((spec) => paneMax(spec));
    const delta = Math.round(moveEvent.clientY - startY);
    const heights = resizeAroundSash(startHeights, currentMins, index, delta, currentMaxes);
    currentHost.classList.toggle("is-scrollable", false);
    paint(current, heights, currentSpecs, currentTotal, false);
    const cursor = sash.dataset.cursor || "ns-resize";
    document.body.dataset.paneCursor = cursor;
  };

  const end = (endEvent: PointerEvent) => {
    sash.removeEventListener("pointermove", move);
    sash.removeEventListener("pointerup", end);
    sash.removeEventListener("pointercancel", end);
    sash.classList.remove("is-active");
    const hit = document.elementFromPoint(endEvent.clientX, endEvent.clientY);
    if (!(hit instanceof Element) || !sash.contains(hit)) {
      sash.classList.remove("is-hover");
      if (hoverSash === sash) hoverSash = null;
    }
    document.body.classList.remove("is-resizing-panes");
    delete document.body.dataset.paneCursor;
    // 非 fill 抽屉记下拖后高度；fill 仍吃剩余。
    const current = childPanes();
    for (const item of current) {
      if (item.dataset.fill === "true" || item.dataset.expanded !== "true") continue;
      const key = item.dataset.sizeKey;
      if (!key) continue;
      localStorage.setItem(key, String(Math.round(item.getBoundingClientRect().height)));
    }
    dragging = false;
    layout(true);
  };

  sash.addEventListener("pointermove", move);
  sash.addEventListener("pointerup", end);
  sash.addEventListener("pointercancel", end);
}

onMounted(() => {
  if (!root.value) return;
  observer = new ResizeObserver(() => layout(false));
  observer.observe(root.value);
  childObserver = new MutationObserver(() => relayout(true));
  childObserver.observe(root.value, { childList: true });
  layout(false);
});

function onPointerOver(event: PointerEvent) {
  const sash = (event.target as HTMLElement | null)?.closest?.(".ui-pane-sash") as HTMLElement | null;
  if (!sash) return;
  armSashHover(sash);
}

function onPointerOut(event: PointerEvent) {
  const sash = (event.target as HTMLElement | null)?.closest?.(".ui-pane-sash") as HTMLElement | null;
  if (!sash || sash !== hoverSash) return;
  const next = event.relatedTarget;
  if (next instanceof Node && sash.contains(next)) return;
  if (sash.classList.contains("is-active")) return;
  clearSashHover();
}

onBeforeUnmount(() => {
  observer?.disconnect();
  childObserver?.disconnect();
  window.clearTimeout(frame);
  clearSashHover();
  document.body.classList.remove("is-resizing-panes");
  delete document.body.dataset.paneCursor;
});

provide("uiPaneView", { relayout });
</script>

<template>
  <div
    ref="root"
    class="ui-pane-view"
    :class="{ 'is-animated': animated, 'is-mac': mac }"
    @pointerdown="onPointerDown"
    @pointerover="onPointerOver"
    @pointerout="onPointerOut"
  >
    <slot />
  </div>
</template>
