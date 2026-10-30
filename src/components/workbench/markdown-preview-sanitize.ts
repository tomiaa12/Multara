import DOMPurify from "dompurify";

const REMOTE_URL = /^\s*(?:https?:)?\/\//i;

/** 独立实例：钩子只作用于仓库里的 Markdown 预览，不影响聊天消息的渲染。 */
const purify = DOMPurify(window);

// 仓库文件内容不可信：远程图片会被用来发信标，内联样式和表单能伪造界面。
purify.addHook("afterSanitizeAttributes", (node) => {
  if (node.tagName !== "IMG") return;
  if (REMOTE_URL.test(node.getAttribute("src") ?? "")) node.removeAttribute("src");
  if (/(?:https?:)?\/\//i.test(node.getAttribute("srcset") ?? "")) node.removeAttribute("srcset");
});

export function sanitizeMarkdownPreview(html: string) {
  return purify.sanitize(html, {
    USE_PROFILES: { html: true },
    FORBID_TAGS: ["style", "form", "input"],
    FORBID_ATTR: ["style"],
  });
}
