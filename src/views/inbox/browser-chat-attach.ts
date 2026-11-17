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
