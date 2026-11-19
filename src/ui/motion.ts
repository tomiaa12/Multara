/**
 * 公共动效（脚本部分）。样式与预设说明见 src/design/motion.css。
 *
 * - v-motion：元素挂载播入场、卸载前留「残影」播退场。v-if 在外层（调用方卸载整个组件，
 *   或元素在 Teleport 里）、<Transition> 包不住时用。只对 fixed / absolute 浮层生效退场，
 *   在文档流里的元素留残影会占位跳动，那种用 <UiCollapse> 或 <Transition>。
 * - collapseTransition()：给 <Transition :css="false"> 的高度 0 ↔ auto 钩子，<UiCollapse> 用它。
 * - vDetailsMotion：原生 <details> 展开 / 收起时动画高度。
 *
 * 全部尊重系统「减少动态效果」；测试环境（vitest）默认关闭，避免残影干扰 DOM 断言。
 */
import type { ObjectDirective } from "vue";

export type MotionPreset =
  | "fade"
  | "pop"
  | "scale"
  | "dropdown"
  | "dropup"
  | "slide-right"
  | "slide-left"
  | "slide-up"
  | "slide-down"
  /** 只有入场：页面 / 分区切换的淡入 */
  | "page";

export type MotionValue =
  | MotionPreset
  | false
  | null
  | undefined
  | { enter?: MotionPreset | false; leave?: MotionPreset | false };

let forcedEnabled: boolean | null = null;

/** 测试里需要验证动效时打开；传 null 恢复默认判断 */
export function setMotionEnabled(value: boolean | null) {
  forcedEnabled = value;
}

export function motionEnabled() {
  if (forcedEnabled != null) return forcedEnabled;
  if (import.meta.env.MODE === "test") return false;
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") return false;
  return !window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

function resolve(value: MotionValue): { enter: MotionPreset | false; leave: MotionPreset | false } {
  if (!value) return { enter: false, leave: false };
  if (typeof value === "string") return { enter: value, leave: value };
  return { enter: value.enter ?? false, leave: value.leave ?? false };
}

const ENTER_PREFIX = "ds-motion-in-";

/** 读元素上动画的总时长（ms），给 animationend 兜底计时用 */
function animationTotalMs(el: Element) {
  const style = getComputedStyle(el);
  const parse = (list: string) =>
    list.split(",").map((part) => {
      const value = part.trim();
      if (value.endsWith("ms")) return Number.parseFloat(value);
      if (value.endsWith("s")) return Number.parseFloat(value) * 1000;
      return 0;
    });
  const durations = parse(style.animationDuration || "0s");
  const delays = parse(style.animationDelay || "0s");
  let max = 0;
  durations.forEach((duration, index) => {
    max = Math.max(max, duration + (delays[index] ?? delays[0] ?? 0));
  });
  return max;
}

function onAnimationDone(el: HTMLElement, done: () => void) {
  let finished = false;
  const finish = () => {
    if (finished) return;
    finished = true;
    el.removeEventListener("animationend", onEnd);
    el.removeEventListener("animationcancel", onEnd);
    window.clearTimeout(timer);
    done();
  };
  const onEnd = (event: Event) => {
    if (event.target === el) finish();
  };
  el.addEventListener("animationend", onEnd);
  el.addEventListener("animationcancel", onEnd);
  // animationend 偶尔不来（元素被隐藏、动画被别的样式覆盖）；按实际时长兜底
  const timer = window.setTimeout(finish, animationTotalMs(el) + 80);
}

/** 播一次入场：挂上 ds-motion-in-<preset>，结束后摘掉，不影响元素原有 animation */
export function playEnter(el: HTMLElement, preset: MotionPreset) {
  if (!motionEnabled()) return;
  const className = `${ENTER_PREFIX}${preset}`;
  el.classList.add(className);
  onAnimationDone(el, () => el.classList.remove(className));
}

/** 把滚动位置、输入框内容同步到克隆体，避免退场时内容跳回顶部 / 清空 */
function syncLiveState(source: HTMLElement, ghost: HTMLElement) {
  const from = [source, ...source.querySelectorAll<HTMLElement>("*")];
  const to = [ghost, ...ghost.querySelectorAll<HTMLElement>("*")];
  if (from.length !== to.length) return;
  from.forEach((node, index) => {
    const copy = to[index];
    if (node.scrollTop || node.scrollLeft) {
      copy.scrollTop = node.scrollTop;
      copy.scrollLeft = node.scrollLeft;
    }
    if (node instanceof HTMLInputElement || node instanceof HTMLTextAreaElement) {
      (copy as HTMLInputElement).value = node.value;
    }
  });
}

/**
 * 卸载前调用：克隆元素留在原位置播退场，播完移除。原元素照常被 Vue 移除。
 * 不在文档流里（fixed / absolute）才留残影；隐藏的（v-show 关着、尺寸为 0）不留。
 */
export function playLeaveGhost(el: HTMLElement, preset: MotionPreset) {
  if (!motionEnabled() || !el.isConnected || !el.parentNode) return;
  const position = getComputedStyle(el).position;
  if (position !== "fixed" && position !== "absolute") return;
  const rect = el.getBoundingClientRect();
  if (rect.width === 0 && rect.height === 0) return;

  const ghost = el.cloneNode(true) as HTMLElement;
  for (const name of [...ghost.classList]) {
    if (name.startsWith(ENTER_PREFIX)) ghost.classList.remove(name);
  }
  ghost.classList.add("ds-motion-ghost", `ds-motion-out-${preset}`);
  ghost.removeAttribute("id");
  ghost.setAttribute("aria-hidden", "true");
  ghost.setAttribute("inert", "");
  el.parentNode.insertBefore(ghost, el.nextSibling);
  syncLiveState(el, ghost);
  onAnimationDone(ghost, () => ghost.remove());
}

/**
 * v-motion="'dropdown'"：入场 + 退场同一预设；
 * v-motion="{ enter: false, leave: 'pop' }"：元素 CSS 已有入场动画，只补退场。
 */
export const vMotion: ObjectDirective<HTMLElement, MotionValue> = {
  mounted(el, { value }) {
    const { enter } = resolve(value);
    if (enter) playEnter(el, enter);
  },
  beforeUnmount(el, { value }) {
    const { leave } = resolve(value);
    if (leave) playLeaveGhost(el, leave);
  },
};

/* ── 折叠：高度 0 ↔ auto ─────────────────────────────────────── */

const BOX_PROPS = ["height", "paddingTop", "paddingBottom", "marginTop", "marginBottom"] as const;
type BoxFrame = Partial<Record<(typeof BOX_PROPS)[number] | "opacity", string>>;

function measureBox(el: HTMLElement): BoxFrame {
  const style = getComputedStyle(el);
  return {
    height: `${el.offsetHeight}px`,
    paddingTop: style.paddingTop,
    paddingBottom: style.paddingBottom,
    marginTop: style.marginTop,
    marginBottom: style.marginBottom,
    opacity: "1",
  };
}

const COLLAPSED: BoxFrame = {
  height: "0px",
  paddingTop: "0px",
  paddingBottom: "0px",
  marginTop: "0px",
  marginBottom: "0px",
  opacity: "0",
};

const running = new WeakMap<Element, Animation>();

function cssDuration(name: string, fallback: number) {
  if (typeof document === "undefined") return fallback;
  const raw = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  if (raw.endsWith("ms")) return Number.parseFloat(raw) || fallback;
  if (raw.endsWith("s")) return Number.parseFloat(raw) * 1000 || fallback;
  return fallback;
}

function cssEasing(name: string, fallback: string) {
  if (typeof document === "undefined") return fallback;
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback;
}

function animateBox(el: HTMLElement, frames: BoxFrame[], duration: number, easing: string, done: () => void) {
  running.get(el)?.cancel();
  if (!motionEnabled() || typeof el.animate !== "function") {
    done();
    return;
  }
  const previousOverflow = el.style.overflow;
  el.style.overflow = "hidden";
  const animation = el.animate(frames as Keyframe[], { duration, easing });
  running.set(el, animation);
  const finish = () => {
    if (running.get(el) === animation) running.delete(el);
    el.style.overflow = previousOverflow;
    done();
  };
  animation.onfinish = finish;
  animation.oncancel = () => {
    el.style.overflow = previousOverflow;
  };
}

export type CollapseOptions = {
  /** 展开时长，默认 --motion-duration-normal */
  duration?: number;
  /** 是否同时淡入淡出，默认 true */
  fade?: boolean;
};

/** <Transition :css="false" v-bind="collapseTransition()">：子元素 v-if / v-show 高度平滑展开收起 */
export function collapseTransition(options: CollapseOptions = {}) {
  const fade = options.fade ?? true;
  const frame = (box: BoxFrame) => (fade ? box : { ...box, opacity: undefined });
  const duration = () => options.duration ?? cssDuration("--motion-duration-normal", 180);
  const easing = () => cssEasing("--motion-ease", "ease");
  return {
    css: false,
    onEnter(el: Element, done: () => void) {
      const target = el as HTMLElement;
      animateBox(target, [frame(COLLAPSED), frame(measureBox(target))], duration(), easing(), done);
    },
    onLeave(el: Element, done: () => void) {
      const target = el as HTMLElement;
      animateBox(target, [frame(measureBox(target)), frame(COLLAPSED)], Math.round(duration() * 0.8), easing(), done);
    },
    onEnterCancelled(el: Element) {
      running.get(el)?.cancel();
    },
    onLeaveCancelled(el: Element) {
      running.get(el)?.cancel();
    },
  };
}

/* ── 原生 <details> ─────────────────────────────────────────── */

function closedDetailsHeight(details: HTMLDetailsElement, summary: HTMLElement) {
  const style = getComputedStyle(details);
  const top = details.getBoundingClientRect().top;
  const bottom = summary.getBoundingClientRect().bottom;
  return (
    bottom - top + Number.parseFloat(style.paddingBottom || "0") + Number.parseFloat(style.borderBottomWidth || "0")
  );
}

function animateDetails(details: HTMLDetailsElement, summary: HTMLElement) {
  const opening = !details.open || details.dataset.motionClosing === "1";
  const start = details.offsetHeight;
  const duration = cssDuration("--motion-duration-normal", 180);
  const easing = cssEasing("--motion-ease", "ease");
  running.get(details)?.cancel();

  const play = (from: number, to: number, onFinish: () => void) => {
    details.style.overflow = "hidden";
    const animation = details.animate({ height: [`${from}px`, `${to}px`] }, { duration, easing });
    running.set(details, animation);
    const clear = () => {
      if (running.get(details) === animation) running.delete(details);
      details.style.overflow = "";
      details.style.height = "";
    };
    animation.onfinish = () => {
      clear();
      onFinish();
    };
    animation.oncancel = clear;
  };

  if (opening) {
    delete details.dataset.motionClosing;
    // 先钉住当前高度再打开，内容（含 @toggle 后 v-if 才渲染的）在裁剪下出来，不闪
    details.style.height = `${start}px`;
    details.style.overflow = "hidden";
    if (!details.open) details.open = true;
    // toggle 事件是异步派发的；等它和 Vue 的渲染都落地后再量最终高度
    requestAnimationFrame(() =>
      requestAnimationFrame(() => {
        details.style.height = "";
        if (!details.open) {
          details.style.overflow = "";
          return;
        }
        const end = details.offsetHeight;
        details.style.height = `${start}px`;
        play(start, end, () => {});
      }),
    );
    return;
  }

  details.dataset.motionClosing = "1";
  play(start, closedDetailsHeight(details, summary), () => {
    if (details.dataset.motionClosing !== "1") return;
    delete details.dataset.motionClosing;
    details.open = false;
  });
}

function onDetailsClick(event: MouseEvent) {
  if (event.defaultPrevented || !motionEnabled()) return;
  const details = event.currentTarget as HTMLDetailsElement;
  const summary = (event.target as Element | null)?.closest("summary");
  if (!summary || summary.parentElement !== details) return;
  // summary 里的按钮 / 链接自己处理点击，不当成展开收起
  const interactive = (event.target as Element).closest("a, button, input, select, textarea, [role='button']");
  if (interactive && summary.contains(interactive) && interactive !== summary) return;
  if (typeof details.animate !== "function") return;
  event.preventDefault();
  animateDetails(details, summary as HTMLElement);
}

/** <details v-details-motion>：点 summary 展开 / 收起时平滑过渡高度（键盘 Enter / 空格同样生效） */
export const vDetailsMotion: ObjectDirective<HTMLDetailsElement> = {
  mounted(el) {
    el.addEventListener("click", onDetailsClick);
  },
  beforeUnmount(el) {
    el.removeEventListener("click", onDetailsClick);
    running.get(el)?.cancel();
  },
};
