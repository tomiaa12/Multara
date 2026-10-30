<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { marked } from "marked";
import * as backend from "../../services/backend";
import { sanitizeMarkdownPreview } from "./markdown-preview-sanitize";

const RENDER_DELAY_MS = 150;

const props = defineProps<{
  content: string;
  path: string;
}>();

const emit = defineEmits<{
  open: [path: string];
}>();

const source = ref(props.content);
let renderTimer = 0;

watch(() => props.content, (content) => {
  window.clearTimeout(renderTimer);
  renderTimer = window.setTimeout(() => {
    source.value = content;
  }, RENDER_DELAY_MS);
});

onBeforeUnmount(() => window.clearTimeout(renderTimer));

const html = computed(() =>
  sanitizeMarkdownPreview(marked.parse(source.value || "", { async: false, gfm: true, breaks: false }) as string),
);

function resolveRelative(href: string) {
  const pathPart = href.split("#")[0]?.split("?")[0] ?? "";
  if (!pathPart || pathPart.startsWith("#") || /^[a-z][a-z0-9+.-]*:/i.test(href)) return "";
  const base = props.path.split("/").slice(0, -1);
  const parts = [...base];
  for (const segment of pathPart.split("/")) {
    if (!segment || segment === ".") continue;
    if (segment === "..") parts.pop();
    else parts.push(safeDecode(segment));
  }
  return parts.join("/");
}

function safeDecode(segment: string) {
  try {
    return decodeURIComponent(segment);
  } catch {
    return segment;
  }
}

function onClick(event: MouseEvent) {
  const link = (event.target as HTMLElement | null)?.closest("a[href]");
  if (!(link instanceof HTMLAnchorElement)) return;
  const href = link.getAttribute("href") ?? "";
  if (!href || href.startsWith("#")) return;
  event.preventDefault();
  if (/^https?:\/\//i.test(href)) {
    void backend.openExternalUrl(href);
    return;
  }
  const next = resolveRelative(href);
  if (next) emit("open", next);
}
</script>

<template>
  <div class="md-preview vs-scroll" @click="onClick">
    <article class="rich-message" v-html="html" />
  </div>
</template>
