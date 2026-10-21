<script setup lang="ts">
import { computed } from "vue";
import DOMPurify from "dompurify";
import { marked } from "marked";
import * as backend from "../../services/backend";

const props = defineProps<{
  content: string;
  path: string;
}>();

const emit = defineEmits<{
  open: [path: string];
}>();

const html = computed(() =>
  DOMPurify.sanitize(
    marked.parse(props.content || "", { async: false, gfm: true, breaks: false }) as string,
    { USE_PROFILES: { html: true } },
  ),
);

function resolveRelative(href: string) {
  const pathPart = href.split("#")[0]?.split("?")[0] ?? "";
  if (!pathPart || pathPart.startsWith("#") || /^[a-z][a-z0-9+.-]*:/i.test(href)) return "";
  const base = props.path.split("/").slice(0, -1);
  const parts = [...base];
  for (const segment of pathPart.split("/")) {
    if (!segment || segment === ".") continue;
    if (segment === "..") parts.pop();
    else parts.push(decodeURIComponent(segment));
  }
  return parts.join("/");
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
