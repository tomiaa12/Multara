/** 收件箱浏览器 → 聊天：图片附件事件（ChatPanel 监听）。 */

export const BROWSER_ATTACH_IMAGE = "multara:browser-attach-image";

export type BrowserAttachImageDetail = {
  sessionId: string;
  name: string;
  mimeType: string;
  base64: string;
};

export function attachBrowserImage(detail: BrowserAttachImageDetail) {
  window.dispatchEvent(new CustomEvent(BROWSER_ATTACH_IMAGE, { detail }));
}

/** 隐藏的气泡或被缓存的页面没有布局盒，不能抢走附件。 */
export function elementIsShown(el: HTMLElement | null | undefined): boolean {
  return Boolean(el && el.getClientRects().length > 0);
}

export function browserImageFile(detail: BrowserAttachImageDetail): File {
  const binary = atob(detail.base64);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index);
  return new File([bytes], detail.name || "browser.png", { type: detail.mimeType || "image/png" });
}
