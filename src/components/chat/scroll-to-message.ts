/** 把消息列表滚到某条消息（或它上方的「新消息」分割线）的顶部。 */
export function scrollListToMessage(list: HTMLElement | null, messageId: string | null): boolean {
  if (!list || !messageId) return false;
  const target = list.querySelector<HTMLElement>(`[data-message-id="${CSS.escape(messageId)}"]`);
  if (!target) return false;
  const top = target.getBoundingClientRect().top - list.getBoundingClientRect().top + list.scrollTop;
  list.scrollTop = Math.max(0, top);
  return true;
}
