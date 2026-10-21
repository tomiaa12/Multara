<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import DOMPurify from "dompurify";
import { marked } from "marked";
import hljs from "highlight.js";
import * as backend from "../../services/backend";
import { useAppStore } from "../../stores/app";
import { fileName } from "./timeline-display";
import { parseCodeFenceInfo, splitWorkspacePaths } from "./workspace-path";

const props = defineProps<{ content: string }>();
const root = ref<HTMLElement | null>(null);
const store = useAppStore();

marked.setOptions({ gfm: true, breaks: true });

const languageAlias: Record<string, string> = {
  vue: "xml",
  html: "xml",
  svelte: "xml",
  ts: "typescript",
  tsx: "typescript",
  jsx: "javascript",
  sh: "bash",
  zsh: "bash",
  shell: "bash",
  rs: "rust",
};

const html = computed(() =>
  DOMPurify.sanitize(marked.parse(props.content || "", { async: false }) as string, {
    USE_PROFILES: { html: true },
  }),
);

function fenceInfoOf(code: HTMLElement) {
  const raw = [...code.classList]
    .find((item) => item.startsWith("language-"))
    ?.slice("language-".length) ?? "";
  return parseCodeFenceInfo(raw);
}

function languageOf(code: HTMLElement) {
  const fence = fenceInfoOf(code);
  const name = fence.language;
  if (!name) return "";
  return languageAlias[name] ?? name;
}

function highlightCode(code: HTMLElement) {
  const language = languageOf(code);
  if (language) {
    for (const item of [...code.classList]) {
      if (item.startsWith("language-")) code.classList.remove(item);
    }
    code.classList.add(`language-${language}`);
  }
  try {
    hljs.highlightElement(code);
  } catch {
    code.classList.add("hljs");
  }
}

async function copyBlock(block: HTMLElement, button: HTMLButtonElement) {
  await navigator.clipboard.writeText(block.textContent ?? "");
  button.textContent = "已复制";
  window.setTimeout(() => {
    button.textContent = "复制";
  }, 1000);
}

function lineLabel(startLine?: number, endLine?: number) {
  if (!startLine || startLine < 1) return "";
  if (!endLine || endLine < 1 || endLine === startLine) return String(startLine);
  return `${startLine}-${endLine}`;
}

function wrapCodeBlock(block: HTMLElement, fence: ReturnType<typeof parseCodeFenceInfo>) {
  if (block.parentElement?.classList.contains("code-block")) return;
  const wrap = document.createElement("div");
  wrap.className = "code-block";
  if (fence.path) {
    wrap.dataset.path = fence.path;
    if (fence.startLine) wrap.dataset.start = String(fence.startLine);
    if (fence.endLine) wrap.dataset.end = String(fence.endLine);
    const head = document.createElement("button");
    head.type = "button";
    head.className = "code-block-head";
    head.dataset.path = fence.path;
    if (fence.startLine) head.dataset.start = String(fence.startLine);
    if (fence.endLine) head.dataset.end = String(fence.endLine);
    head.title = fence.path;
    const file = document.createElement("span");
    file.className = "code-block-file";
    file.textContent = fileName(fence.path);
    const lines = lineLabel(fence.startLine, fence.endLine);
    if (lines) {
      const range = document.createElement("span");
      range.className = "code-block-lines";
      range.textContent = lines;
      head.append(file, range);
    } else {
      head.append(file);
    }
    wrap.append(head);
  }
  const actions = document.createElement("div");
  actions.className = "code-block-actions";
  if (fence.path) {
    const apply = document.createElement("button");
    apply.type = "button";
    apply.className = "code-copy";
    apply.textContent = "应用到文件";
    apply.addEventListener("click", (event) => {
      event.preventDefault();
      event.stopPropagation();
      const path = fence.path!;
      const content = block.textContent ?? "";
      store.revealWorkspaceFile(path);
      const workspaces = store.data?.workspaces ?? [];
      void (async () => {
        for (const workspace of workspaces) {
          try {
            await backend.writeWorkspaceFile(workspace.id, path, content);
            apply.textContent = "已写入";
            window.setTimeout(() => {
              apply.textContent = "应用到文件";
            }, 1200);
            return;
          } catch {
            // 试下一个工作区
          }
        }
        apply.textContent = "未找到文件";
        window.setTimeout(() => {
          apply.textContent = "应用到文件";
        }, 1200);
      })();
    });
    actions.append(apply);
  }
  const copy = document.createElement("button");
  copy.type = "button";
  copy.className = "code-copy";
  copy.textContent = "复制";
  copy.addEventListener("click", (event) => {
    event.preventDefault();
    event.stopPropagation();
    void copyBlock(block, copy);
  });
  actions.append(copy);
  block.classList.add("vs-scroll");
  block.replaceWith(wrap);
  wrap.append(actions, block);
}

function linkWorkspacePaths(el: HTMLElement) {
  const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
  const nodes: Text[] = [];
  let current = walker.nextNode();
  while (current) {
    nodes.push(current as Text);
    current = walker.nextNode();
  }
  for (const node of nodes) {
    const parent = node.parentElement;
    if (!parent || parent.closest("pre, a, .workspace-path, .code-block")) continue;
    const text = node.textContent ?? "";
    if (!text.includes("/")) continue;
    const segments = splitWorkspacePaths(text);
    if (!segments.some((segment) => segment.type === "path")) continue;
    const fragment = document.createDocumentFragment();
    for (const segment of segments) {
      if (segment.type !== "path" || !segment.path) {
        fragment.append(segment.text);
        continue;
      }
      const span = document.createElement("span");
      span.className = "workspace-path";
      span.dataset.path = segment.path;
      if (segment.startLine) span.dataset.start = String(segment.startLine);
      if (segment.endLine) span.dataset.end = String(segment.endLine);
      span.textContent = segment.text;
      span.title = "点击打开";
      fragment.append(span);
    }
    node.replaceWith(fragment);
  }
}

function revealFromDataset(target: HTMLElement) {
  const path = target.dataset.path;
  if (!path) return;
  const start = Number(target.dataset.start || "");
  const end = Number(target.dataset.end || "");
  store.revealWorkspaceFile(
    path,
    Number.isFinite(start) && start > 0 ? start : undefined,
    Number.isFinite(end) && end > 0 ? end : undefined,
  );
}

function onRichClick(event: MouseEvent) {
  if (event.button !== 0) return;
  const el = event.target as HTMLElement | null;
  if (!el || !root.value) return;

  const file = el.closest(".code-block-head");
  if (file instanceof HTMLElement && root.value.contains(file)) {
    event.preventDefault();
    event.stopPropagation();
    revealFromDataset(file);
    return;
  }

  const path = el.closest(".workspace-path");
  if (!(path instanceof HTMLElement) || !root.value.contains(path)) return;
  event.preventDefault();
  event.stopPropagation();
  revealFromDataset(path);
}

function wrapTable(table: HTMLElement) {
  if (table.parentElement?.classList.contains("md-table-wrap")) return;
  const wrap = document.createElement("div");
  wrap.className = "md-table-wrap vs-scroll";
  table.replaceWith(wrap);
  wrap.append(table);
}

async function enhanceBlocks() {
  await nextTick();
  const el = root.value;
  if (!el) return;
  for (const block of [...el.querySelectorAll<HTMLElement>("pre:not([data-enhanced])")]) {
    block.dataset.enhanced = "true";
    const code = block.querySelector<HTMLElement>("code");
    if (!code) continue;
    if (code.classList.contains("language-mermaid")) {
      try {
        const mermaid = (await import("mermaid")).default;
        // 图表按渲染时的界面主题出图；已渲染的 SVG 不会随主题切换重绘
        const light = document.documentElement.dataset.theme === "light";
        mermaid.initialize({ startOnLoad: false, securityLevel: "strict", theme: light ? "default" : "dark" });
        const { svg } = await mermaid.render(`mermaid-${crypto.randomUUID()}`, code.textContent ?? "");
        block.outerHTML = `<div class="mermaid-preview vs-scroll">${DOMPurify.sanitize(svg, { USE_PROFILES: { svg: true, svgFilters: true } })}</div>`;
      } catch {
        block.dataset.diagramError = "无法渲染图表";
        const note = document.createElement("p");
        note.className = "diagram-error";
        note.textContent = "无法渲染图表";
        block.insertAdjacentElement("afterend", note);
      }
      continue;
    }
    if (code.classList.contains("language-html-preview")) {
      const preview = document.createElement("div");
      preview.className = "html-preview vs-scroll";
      preview.innerHTML = DOMPurify.sanitize(code.textContent ?? "", { USE_PROFILES: { html: true } });
      block.replaceWith(preview);
      continue;
    }
    const fence = fenceInfoOf(code);
    highlightCode(code);
    wrapCodeBlock(block, fence);
  }
  for (const table of [...el.querySelectorAll<HTMLElement>("table")]) {
    wrapTable(table);
  }
  linkWorkspacePaths(el);
  for (const link of [...el.querySelectorAll<HTMLAnchorElement>("a[href]")]) {
    if (link.dataset.externalBound) continue;
    const href = link.getAttribute("href") ?? "";
    if (!/^https?:\/\//i.test(href)) continue;
    link.dataset.externalBound = "true";
    link.target = "_blank";
    link.rel = "noopener noreferrer";
    link.addEventListener("click", (event) => {
      event.preventDefault();
      event.stopPropagation();
      void backend.openExternalUrl(href);
    });
  }
}

watch(html, () => void enhanceBlocks(), { immediate: true, flush: "post" });
</script>

<template>
  <div ref="root" class="rich-message" v-html="html" @click="onRichClick" />
</template>
