/** 列表是否贴在「实时底部」：绝对滚底，或最后一条真实内容已落在视口下沿（兼容末轮 min-height:100% 空白）。 */
export function listIsAtBottom(element: HTMLElement, threshold = 80): boolean {
  const distance = element.scrollHeight - element.scrollTop - element.clientHeight;
  if (distance < threshold) return true;
  const lastContent = element.querySelector(".message-exchange:last-child")?.lastElementChild;
  if (!lastContent) return false;
  return lastContent.getBoundingClientRect().bottom - element.getBoundingClientRect().bottom < threshold;
}

/** 把滚动容器贴到底；调用方应在 followBottom 为真时使用。 */
export function stickListToBottom(element: HTMLElement, behavior: ScrollBehavior = "auto") {
  if (behavior === "smooth") {
    element.scrollTo({ top: element.scrollHeight, behavior });
    return;
  }
  element.scrollTop = element.scrollHeight;
}

/** smooth 动画通常远长于两帧；超时兜底，避免一直压住 follow 检测。 */
export const SMOOTH_SCROLL_FOLLOW_SUPPRESS_MS = 800;

/**
 * 程序化滚底结束后再放开 scroll 跟随检测。
 * smooth 必须等 scrollend（或超时），否则中途 scroll 事件会把 followBottom 关掉，
 * 「回到底部」表现为点一次只滚一截、要连点几次。
 */
export function afterProgrammaticStick(
  element: HTMLElement,
  behavior: ScrollBehavior,
  done: () => void,
): () => void {
  if (behavior !== "smooth") {
    let cancelled = false;
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        if (!cancelled) done();
      });
    });
    return () => {
      cancelled = true;
    };
  }
  let settled = false;
  const finish = () => {
    if (settled) return;
    settled = true;
    element.removeEventListener("scrollend", finish);
    window.clearTimeout(timer);
    done();
  };
  const timer = window.setTimeout(finish, SMOOTH_SCROLL_FOLLOW_SUPPRESS_MS);
  element.addEventListener("scrollend", finish);
  return finish;
}

/** 列表内容区高度（去掉上下 padding），末轮撑满用这个，避免比视口更高导致贴底后顶部溢出。 */
export function listContentHeight(list: HTMLElement) {
  const style = getComputedStyle(list);
  const pad = (parseFloat(style.paddingTop) || 0) + (parseFloat(style.paddingBottom) || 0);
  return Math.max(0, list.clientHeight - pad);
}

/**
 * 粘性提问：末轮用像素 min-height 撑满列表内容区。
 * overflow 容器里 CSS `min-height: 100%` 经常算不成明确高度，发送瞬间末轮仍很矮，
 * stick-to-bottom 会把短气泡贴到视口下沿，随后撑高就闪动。
 */
export function syncStickyExchangeMinHeight(list: HTMLElement, enabled: boolean) {
  const exchanges = list.querySelectorAll<HTMLElement>(".message-exchange");
  const last = exchanges[exchanges.length - 1];
  for (const el of exchanges) {
    if (el !== last) el.style.minHeight = "";
  }
  if (!enabled || !last) {
    if (last) last.style.minHeight = "";
    return null;
  }
  last.style.minHeight = `${listContentHeight(list)}px`;
  return last;
}

/**
 * 发送后把最新一轮留在视口顶：末轮已是一屏高时，贴底即可（sticky 提问停在顶）。
 * 不要再按 getBoundingClientRect 做 delta 二次校正——末轮略高时 last.top < list.top，
 * 会把 scrollTop 减回去，出现「先冲到对话顶再弹回底部」的闪动。
 */
export function pinLastExchangeToTop(list: HTMLElement, _last: HTMLElement) {
  stickListToBottom(list, "auto");
}

const USER_TURN_SELECTOR = ".user-turn";

/**
 * 计算粘性提问被下一轮顶上来时的 top 偏移。
 * nextTop 相对滚动容器顶；当下一轮进入上一轮所占粘性区域时，上一轮 top 变为负值被推走。
 */
export function stickyUserTurnPushOffsets(
  scrollerTop: number,
  turns: Array<{ height: number; top: number }>,
): number[] {
  const offsets = turns.map(() => 0);
  for (let index = 0; index < turns.length - 1; index += 1) {
    const prev = turns[index];
    const next = turns[index + 1];
    if (!prev || !next) continue;
    const push = next.top - scrollerTop - prev.height;
    if (push < 0) offsets[index] = push;
  }
  return offsets;
}

function scrollParentOf(el: HTMLElement): HTMLElement | null {
  let node: HTMLElement | null = el.parentElement;
  while (node && node !== document.documentElement) {
    const { overflowY } = getComputedStyle(node);
    if (overflowY === "auto" || overflowY === "scroll" || overflowY === "overlay") return node;
    node = node.parentElement;
  }
  return null;
}

/**
 * 最新粘性提问把上一轮往上推，避免短提问盖不住长提问时露馅；
 * 滚到某一轮回答时仍能看到对应提问（各轮保持 sticky）。
 */
export function syncStickyUserTurnPush(root: HTMLElement, enabled: boolean) {
  const stickies = [...root.querySelectorAll<HTMLElement>(USER_TURN_SELECTOR)];
  if (!enabled || stickies.length === 0) {
    for (const el of stickies) el.style.top = "";
    return;
  }
  const scroller = scrollParentOf(stickies[0]!) ?? root;
  const scrollerTop = scroller.getBoundingClientRect().top;
  for (const el of stickies) el.style.top = "0px";
  const turns = stickies.map((el) => {
    const rect = el.getBoundingClientRect();
    return { height: el.offsetHeight, top: rect.top };
  });
  const offsets = stickyUserTurnPushOffsets(scrollerTop, turns);
  offsets.forEach((offset, index) => {
    const el = stickies[index];
    if (el) el.style.top = offset === 0 ? "0px" : `${offset}px`;
  });
}
