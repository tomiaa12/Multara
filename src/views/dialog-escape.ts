/** UiSelect、TagPicker、输入框建议菜单共用的浮层类名（都 Teleport 到 body）。 */
const OPEN_POPUP_SELECTOR = ".ds-select-popover";

/**
 * 文档级 Esc 是否该关闭对话框：已被别的组件处理（defaultPrevented）、输入法组字中、
 * 或还有下拉 / 建议浮层打开时，这次 Esc 属于它们，不关对话框。
 */
export function escapeClosesDialog(event: KeyboardEvent) {
  if (event.key !== "Escape" || event.defaultPrevented || event.isComposing) return false;
  return !document.querySelector(OPEN_POPUP_SELECTOR);
}
