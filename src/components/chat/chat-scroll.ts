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
