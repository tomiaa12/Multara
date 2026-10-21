/**
 * VS Code 风格悬浮滚动条：原生 overflow 继续负责滚动，
 * 系统滚动条藏掉，滑块 fixed 叠在宿主上（宿主 transform 关进本地 stacking），
 * 不占布局、不撑开 scrollWidth；层级用 --z-overlay-scroll。
 *
 * 不包裹节点（Vue 会丢 vnode），滑块作为宿主最后一个子节点插入。
 * contenteditable / 表单控件跳过，避免滑块进文档内容。
 */

import type { Directive, Plugin } from "vue";

export const HOST_CLASS = "vs-scroll";
export const BAR_CLASS = "vs-overlay-scrollbar";
export const SLIDER_CLASS = "vs-overlay-slider";
export const MINIMUM_SLIDER_SIZE = 20;
export const HIDE_TIMEOUT_MS = 500;
export const POINTER_DRAG_RESET_DISTANCE = 140;
export const DEFAULT_BAR_SIZE = 10;

const ATTACHED_ATTR = "data-overlay-scroll";
const SKIP_CLOSEST = ".monaco-editor, .monaco-scrollable-element";

export type OverlayAxis = "vertical" | "horizontal";

export interface OverlaySliderMetrics {
  needed: boolean;
  size: number;
  position: number;
  ratio: number;
}

/**
 * @param visible 视口可滚动尺寸（clientWidth / clientHeight），决定真实行程 scrollSize - visible
 * @param trackLength 滑轨长度；预留角落给另一轴时小于 visible，默认等于 visible
 */
export function overlaySliderMetrics(
  visible: number,
  scrollSize: number,
  scrollPos: number,
  trackLength = visible,
): OverlaySliderMetrics {
  const vis = Math.max(0, visible);
  const total = Math.max(0, scrollSize);
  const track = Math.max(0, trackLength);
  if (total <= 0 || total <= vis || track <= 0) {
    return { needed: false, size: 0, position: 0, ratio: 0 };
  }
  const size = Math.max(MINIMUM_SLIDER_SIZE, Math.min(track, Math.floor((track * vis) / total)));
  const travel = total - vis;
  const thumbTravel = Math.max(0, track - size);
  const ratio = travel === 0 ? 0 : thumbTravel / travel;
  const position = Math.min(thumbTravel, Math.round(scrollPos * ratio));
  return { needed: true, size, position, ratio };
}

export function canAttachOverlayScroll(el: Element): el is HTMLElement {
  if (!(el instanceof HTMLElement)) return false;
  if (el.hasAttribute(ATTACHED_ATTR)) return false;
  if (el.classList.contains(BAR_CLASS) || el.closest(`.${BAR_CLASS}`)) return false;
  if (el.isContentEditable) return false;
  const tag = el.tagName;
  if (tag === "TEXTAREA" || tag === "INPUT" || tag === "SELECT" || tag === "P" || tag === "PRE") return false;
  if (el.closest(SKIP_CLOSEST)) return false;
  return true;
}

type AxisState = {
  bar: HTMLElement;
  slider: HTMLElement;
  metrics: OverlaySliderMetrics;
};

const attached = new WeakMap<HTMLElement, OverlayScroll>();

class OverlayScroll {
  private readonly vertical: AxisState;
  private readonly horizontal: AxisState;
  private readonly listenOn: HTMLElement;
  private readonly resizeObserver: ResizeObserver;
  private readonly mutationObserver: MutationObserver;
  private hideTimer = 0;
  private raf = 0;
  private dragging = false;
  private pointerOver = false;
  private barSize = DEFAULT_BAR_SIZE;
  private disposed = false;

  constructor(private readonly el: HTMLElement) {
    this.el.setAttribute(ATTACHED_ATTR, "");
    this.el.classList.add(HOST_CLASS);
    this.listenOn = this.el.classList.contains("xterm-viewport")
      ? (this.el.parentElement ?? this.el)
      : this.el;
    this.vertical = this.createAxis("vertical");
    this.horizontal = this.createAxis("horizontal");
    this.el.append(this.vertical.bar, this.horizontal.bar);

    this.onScroll = this.onScroll.bind(this);
    this.onPointerEnter = this.onPointerEnter.bind(this);
    this.onPointerLeave = this.onPointerLeave.bind(this);
    this.el.addEventListener("scroll", this.onScroll, { passive: true });
    this.listenOn.addEventListener("pointerenter", this.onPointerEnter);
    this.listenOn.addEventListener("pointerleave", this.onPointerLeave);

    this.resizeObserver = new ResizeObserver(() => this.scheduleRender());
    this.resizeObserver.observe(this.el);
    this.mutationObserver = new MutationObserver((records) => {
      if (records.every((record) => this.isOwnMutation(record))) return;
      this.ensureBars();
      this.scheduleRender();
    });
    this.mutationObserver.observe(this.el, { childList: true, subtree: true, characterData: true });
    this.scheduleRender();
  }

  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    window.clearTimeout(this.hideTimer);
    if (this.raf) cancelAnimationFrame(this.raf);
    this.resizeObserver.disconnect();
    this.mutationObserver.disconnect();
    this.el.removeEventListener("scroll", this.onScroll);
    this.listenOn.removeEventListener("pointerenter", this.onPointerEnter);
    this.listenOn.removeEventListener("pointerleave", this.onPointerLeave);
    this.vertical.bar.remove();
    this.horizontal.bar.remove();
    this.el.removeAttribute(ATTACHED_ATTR);
    attached.delete(this.el);
  }

  private createAxis(axis: OverlayAxis): AxisState {
    const bar = document.createElement("div");
    bar.className = `${BAR_CLASS} is-${axis}`;
    bar.setAttribute("role", "presentation");
    bar.setAttribute("aria-hidden", "true");
    const slider = document.createElement("div");
    slider.className = SLIDER_CLASS;
    bar.appendChild(slider);
    bar.addEventListener("pointerdown", (event) => this.onTrackPointerDown(axis, event));
    slider.addEventListener("pointerdown", (event) => this.onSliderPointerDown(axis, event));
    return { bar, slider, metrics: overlaySliderMetrics(0, 0, 0) };
  }

  private isOwnMutation(record: MutationRecord): boolean {
    const nodes = [...record.addedNodes, ...record.removedNodes];
    if (!nodes.length) return record.target === this.vertical.bar
      || record.target === this.horizontal.bar
      || record.target === this.vertical.slider
      || record.target === this.horizontal.slider;
    return nodes.every((node) => node instanceof Element && (
      node === this.vertical.bar
      || node === this.horizontal.bar
      || node.classList.contains(BAR_CLASS)
    ));
  }

  private ensureBars(): void {
    if (this.disposed) return;
    if (this.vertical.bar.parentElement !== this.el) this.el.appendChild(this.vertical.bar);
    if (this.horizontal.bar.parentElement !== this.el) this.el.appendChild(this.horizontal.bar);
  }

  private onScroll(): void {
    this.scheduleRender();
    this.reveal();
  }

  private onPointerEnter(): void {
    this.pointerOver = true;
    this.reveal();
  }

  private onPointerLeave(): void {
    this.pointerOver = false;
    this.scheduleHide();
  }

  private reveal(): void {
    this.vertical.bar.classList.add("is-visible");
    this.horizontal.bar.classList.add("is-visible");
    this.scheduleHide();
  }

  private scheduleHide(): void {
    window.clearTimeout(this.hideTimer);
    if (this.pointerOver || this.dragging) return;
    this.hideTimer = window.setTimeout(() => {
      if (this.pointerOver || this.dragging) return;
      this.vertical.bar.classList.remove("is-visible");
      this.horizontal.bar.classList.remove("is-visible");
    }, HIDE_TIMEOUT_MS);
  }

  private scheduleRender(): void {
    if (this.raf || this.disposed) return;
    this.raf = requestAnimationFrame(() => {
      this.raf = 0;
      this.render();
    });
  }

  private readBarSize(): number {
    const raw = getComputedStyle(this.el).getPropertyValue("--scrollbar-size").trim();
    const parsed = Number.parseFloat(raw);
    return Number.isFinite(parsed) && parsed > 0 ? parsed : DEFAULT_BAR_SIZE;
  }

  private render(): void {
    if (this.disposed) return;
    this.ensureBars();
    this.barSize = this.readBarSize();
    const visH = this.el.clientHeight;
    const visW = this.el.clientWidth;
    const totalH = this.el.scrollHeight;
    const totalW = this.el.scrollWidth;
    const top = this.el.scrollTop;
    const left = this.el.scrollLeft;

    // 行程必须按 client 尺寸算；轨道可因另一轴预留角落而更短，不能拿轨道长当 visible
    this.vertical.metrics = overlaySliderMetrics(visH, totalH, top);
    const hTrack = this.vertical.metrics.needed ? Math.max(0, visW - this.barSize) : visW;
    // overflow-x: clip/hidden 时 scrollWidth 仍可能偏大，但不应出现横向滑块
    const overflowX = getComputedStyle(this.el).overflowX;
    const allowHorizontal = overflowX !== "hidden" && overflowX !== "clip";
    this.horizontal.metrics = allowHorizontal
      ? overlaySliderMetrics(visW, totalW, left, hTrack)
      : { needed: false, size: 0, position: 0, ratio: 0 };

    // 宿主有 transform 时 fixed 相对宿主贴边（勿再用 scrollLeft/Top，会撑开可滚范围）
    this.layoutBar("vertical", this.vertical, {
      top: 0,
      left: Math.max(0, visW - this.barSize),
      width: this.barSize,
      height: visH,
    });
    this.layoutBar("horizontal", this.horizontal, {
      top: Math.max(0, visH - this.barSize),
      left: 0,
      width: hTrack,
      height: this.barSize,
    });
  }

  private layoutBar(
    axis: OverlayAxis,
    state: AxisState,
    box: { top: number; left: number; width: number; height: number },
  ): void {
    state.bar.classList.toggle("is-needed", state.metrics.needed);
    if (!state.metrics.needed) return;
    state.bar.style.top = `${box.top}px`;
    state.bar.style.left = `${box.left}px`;
    state.bar.style.width = `${box.width}px`;
    state.bar.style.height = `${box.height}px`;
    if (axis === "vertical") {
      state.slider.style.height = `${state.metrics.size}px`;
      state.slider.style.top = `${state.metrics.position}px`;
      state.slider.style.left = "";
      state.slider.style.width = "";
    } else {
      state.slider.style.width = `${state.metrics.size}px`;
      state.slider.style.left = `${state.metrics.position}px`;
      state.slider.style.top = "";
      state.slider.style.height = "";
    }
  }

  private onTrackPointerDown(axis: OverlayAxis, event: PointerEvent): void {
    if (event.button !== 0 || event.target !== event.currentTarget) return;
    event.preventDefault();
    const state = axis === "vertical" ? this.vertical : this.horizontal;
    if (!state.metrics.needed || state.metrics.ratio === 0) return;
    const rect = state.bar.getBoundingClientRect();
    const offset = axis === "vertical" ? event.clientY - rect.top : event.clientX - rect.left;
    this.writeScroll(axis, (offset - state.metrics.size / 2) / state.metrics.ratio);
    this.onSliderPointerDown(axis, event);
  }

  private onSliderPointerDown(axis: OverlayAxis, event: PointerEvent): void {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    const state = axis === "vertical" ? this.vertical : this.horizontal;
    if (!state.metrics.needed || state.metrics.ratio === 0) return;
    const start = axis === "vertical" ? event.pageY : event.pageX;
    const startOrtho = axis === "vertical" ? event.pageX : event.pageY;
    const startScroll = axis === "vertical" ? this.el.scrollTop : this.el.scrollLeft;
    const ratio = state.metrics.ratio;
    this.dragging = true;
    state.slider.classList.add("is-active");
    this.reveal();
    const target = event.currentTarget instanceof HTMLElement ? event.currentTarget : state.slider;
    target.setPointerCapture(event.pointerId);

    const onMove = (move: PointerEvent) => {
      const ortho = axis === "vertical" ? move.pageX : move.pageY;
      if (Math.abs(ortho - startOrtho) > POINTER_DRAG_RESET_DISTANCE) {
        this.writeScroll(axis, startScroll);
        return;
      }
      const delta = (axis === "vertical" ? move.pageY : move.pageX) - start;
      this.writeScroll(axis, startScroll + delta / ratio);
    };
    const onUp = () => {
      target.removeEventListener("pointermove", onMove);
      target.removeEventListener("pointerup", onUp);
      target.removeEventListener("pointercancel", onUp);
      state.slider.classList.remove("is-active");
      this.dragging = false;
      this.scheduleHide();
    };
    target.addEventListener("pointermove", onMove);
    target.addEventListener("pointerup", onUp);
    target.addEventListener("pointercancel", onUp);
  }

  private writeScroll(axis: OverlayAxis, value: number): void {
    if (axis === "vertical") {
      const max = Math.max(0, this.el.scrollHeight - this.el.clientHeight);
      this.el.scrollTop = Math.min(max, Math.max(0, value));
      return;
    }
    const max = Math.max(0, this.el.scrollWidth - this.el.clientWidth);
    this.el.scrollLeft = Math.min(max, Math.max(0, value));
  }
}

export function attachOverlayScroll(el: HTMLElement): () => void {
  const existing = attached.get(el);
  if (existing) return () => existing.dispose();
  if (!canAttachOverlayScroll(el)) return () => undefined;
  const instance = new OverlayScroll(el);
  attached.set(el, instance);
  return () => instance.dispose();
}

export function detachOverlayScroll(el: HTMLElement): void {
  attached.get(el)?.dispose();
}

function walkOverlayHosts(root: ParentNode, visit: (el: HTMLElement) => void): void {
  if (root instanceof Element && root.classList.contains(HOST_CLASS)) visit(root);
  root.querySelectorAll?.(`.${HOST_CLASS}`).forEach((node) => visit(node as HTMLElement));
}

export function attachOverlayScrollTree(root: ParentNode): void {
  walkOverlayHosts(root, (el) => {
    if (canAttachOverlayScroll(el)) attachOverlayScroll(el);
  });
}

export function detachOverlayScrollTree(root: ParentNode): void {
  walkOverlayHosts(root, detachOverlayScroll);
}

let observer: MutationObserver | null = null;

export function startOverlayScrollObserver(root: ParentNode = document): () => void {
  const target = root instanceof Document ? root.body ?? root.documentElement : root;
  if (!target) return () => undefined;
  stopOverlayScrollObserver();
  attachOverlayScrollTree(target);
  observer = new MutationObserver((records) => {
    for (const record of records) {
      for (const node of record.addedNodes) {
        if (node instanceof Element || node instanceof DocumentFragment) attachOverlayScrollTree(node);
      }
      for (const node of record.removedNodes) {
        if (node instanceof Element || node instanceof DocumentFragment) detachOverlayScrollTree(node);
      }
      if (record.type === "attributes" && record.target instanceof HTMLElement) {
        if (record.target.classList.contains(HOST_CLASS)) attachOverlayScroll(record.target);
        else detachOverlayScroll(record.target);
      }
    }
  });
  observer.observe(target, {
    childList: true,
    subtree: true,
    attributes: true,
    attributeFilter: ["class"],
  });
  return stopOverlayScrollObserver;
}

export function stopOverlayScrollObserver(): void {
  observer?.disconnect();
  observer = null;
}

export const overlayScrollDirective: Directive<HTMLElement> = {
  mounted(el) {
    el.classList.add(HOST_CLASS);
    attachOverlayScroll(el);
  },
  updated(el) {
    el.classList.add(HOST_CLASS);
    attachOverlayScroll(el);
  },
  unmounted(el) {
    detachOverlayScroll(el);
  },
};

export const overlayScrollPlugin: Plugin = {
  install(app) {
    app.directive("overlay-scroll", overlayScrollDirective);
    if (typeof document === "undefined") return;
    const start = () => startOverlayScrollObserver();
    if (document.readyState === "loading") {
      document.addEventListener("DOMContentLoaded", start, { once: true });
    } else {
      start();
    }
  },
};
