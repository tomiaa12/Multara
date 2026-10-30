import DOMPurify from "dompurify";

const REMOTE_URL = /^\s*(?:https?:)?\/\//i;

/** 独立实例：钩子只作用于聊天消息，不影响 mermaid SVG 等其它净化。 */
const purify = DOMPurify(window);

// 智能体 / MCP / webhook 文本不可信：远程图片会被用来发信标，内联样式和表单能伪造界面。与 MarkdownPreview 一致。
purify.addHook("afterSanitizeAttributes", (node) => {
  if (node.tagName !== "IMG") return;
  if (REMOTE_URL.test(node.getAttribute("src") ?? "")) node.removeAttribute("src");
  if (/(?:https?:)?\/\//i.test(node.getAttribute("srcset") ?? "")) node.removeAttribute("srcset");
});

export function sanitizeRichHtml(html: string) {
  return purify.sanitize(html, {
    USE_PROFILES: { html: true },
    FORBID_TAGS: ["style", "form", "input"],
    FORBID_ATTR: ["style"],
  });
}
