<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { Folder, FolderKanban, GitBranch, Plus } from "@lucide/vue";
import gsap from "gsap";

const emit = defineEmits<{ create: [] }>();

const leaves = [
  { kind: "git" as const, label: "Git 项目" },
  { kind: "git" as const, label: "Git 项目" },
  { kind: "folder" as const, label: "文件夹" },
];

const rootRef = ref<HTMLElement | null>(null);
const mapRef = ref<HTMLElement | null>(null);
const hubRef = ref<HTMLElement | null>(null);
const leafRefs = ref<HTMLElement[]>([]);
const pathRefs = ref<SVGPathElement[]>([]);
const titleRef = ref<HTMLElement | null>(null);
const descRef = ref<HTMLElement | null>(null);
const actionRef = ref<HTMLElement | null>(null);
const svgSize = ref({ w: 0, h: 0 });
const pathDs = ref<string[]>(["", "", ""]);

let cancelled = false;
let playing = false;
let resizeObserver: ResizeObserver | null = null;

function bindLeaf(el: unknown, index: number) {
  if (el instanceof HTMLElement) leafRefs.value[index] = el;
}

function bindPath(el: unknown, index: number) {
  if (el instanceof SVGPathElement) pathRefs.value[index] = el;
}

function reducedMotion() {
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

function rel(el: HTMLElement, root: HTMLElement) {
  const a = el.getBoundingClientRect();
  const b = root.getBoundingClientRect();
  return {
    x: a.left - b.left,
    y: a.top - b.top,
    w: a.width,
    h: a.height,
  };
}

function layoutLines() {
  const map = mapRef.value;
  const hub = hubRef.value;
  if (!map || !hub || leafRefs.value.length !== leaves.length) return false;

  const width = map.clientWidth;
  const height = map.clientHeight;
  svgSize.value = { w: width, h: height };

  const hubBox = rel(hub, map);
  const hubX = hubBox.x + hubBox.w / 2;
  const hubY = hubBox.y + hubBox.h;
  const leafBoxes = leafRefs.value.map((leaf) => rel(leaf, map));
  const jointY = hubY + Math.max(18, (Math.min(...leafBoxes.map((box) => box.y)) - hubY) * 0.5);

  pathDs.value = leafBoxes.map((box) => {
    const leafX = box.x + box.w / 2;
    const leafY = box.y;
    if (Math.abs(leafX - hubX) < 20) {
      return `M ${hubX} ${hubY} L ${hubX} ${leafY}`;
    }
    return `M ${hubX} ${hubY} L ${hubX} ${jointY} L ${leafX} ${jointY} L ${leafX} ${leafY}`;
  });
  return true;
}

function hideStroke(path: SVGPathElement) {
  const length = Math.max(path.getTotalLength(), 1);
  path.style.strokeDasharray = `${length}`;
  path.style.strokeDashoffset = `${length}`;
  return length;
}

function revealStroke(path: SVGPathElement) {
  const length = Math.max(path.getTotalLength(), 1);
  path.style.strokeDasharray = `${length}`;
  path.style.strokeDashoffset = "0";
}

function showFinal() {
  layoutLines();
  rootRef.value?.classList.add("is-ready");
  gsap.set([hubRef.value, ...leafRefs.value, titleRef.value, descRef.value, actionRef.value], {
    opacity: 1,
    y: 0,
    clearProps: "transform",
  });
  nextTick(() => pathRefs.value.forEach(revealStroke));
}

async function play() {
  const hub = hubRef.value;
  const title = titleRef.value;
  const desc = descRef.value;
  const action = actionRef.value;
  if (!hub || !title || !desc || !action) return;

  if (reducedMotion()) {
    showFinal();
    return;
  }

  gsap.set(hub, { opacity: 0 });
  gsap.set(leafRefs.value, { opacity: 0 });
  gsap.set([title, desc, action], { opacity: 0, y: 14 });

  playing = true;
  await gsap.to(hub, { opacity: 1, duration: 0.55, ease: "power2.out" });
  if (cancelled) return;

  layoutLines();
  await nextTick();
  const paths = pathRefs.value.filter(Boolean);
  const lengths = paths.map(hideStroke);
  await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));

  for (const [index, path] of paths.entries()) {
    if (cancelled) return;
    const leaf = leafRefs.value[index];
    await gsap.fromTo(
      path,
      { strokeDashoffset: lengths[index] },
      { strokeDashoffset: 0, duration: 0.95, ease: "power2.inOut" },
    );
    if (cancelled || !leaf) return;
    await gsap.to(leaf, { opacity: 1, duration: 0.4, ease: "power2.out" });
    await new Promise((resolve) => window.setTimeout(resolve, 180));
  }

  if (cancelled) return;
  await gsap.to(title, { opacity: 1, y: 0, duration: 0.55, ease: "power2.out" });
  if (cancelled) return;
  await gsap.to(desc, { opacity: 1, y: 0, duration: 0.65, ease: "power2.out" });
  if (cancelled) return;
  await gsap.to(action, { opacity: 1, y: 0, duration: 0.5, ease: "power2.out" });
  playing = false;
  rootRef.value?.classList.add("is-ready");
}

onMounted(async () => {
  await nextTick();
  requestAnimationFrame(() => {
    void play();
    if (!mapRef.value) return;
    resizeObserver = new ResizeObserver(() => {
      if (playing) return;
      layoutLines();
      nextTick(() => pathRefs.value.forEach(revealStroke));
    });
    resizeObserver.observe(mapRef.value);
  });
});

onBeforeUnmount(() => {
  cancelled = true;
  playing = false;
  resizeObserver?.disconnect();
});
</script>

<template>
  <section ref="rootRef" class="req-empty">
    <div ref="mapRef" class="req-empty-map" aria-hidden="true">
      <svg
        class="req-empty-svg"
        :width="svgSize.w"
        :height="svgSize.h"
        :viewBox="`0 0 ${svgSize.w} ${svgSize.h}`"
      >
        <path
          v-for="(d, index) in pathDs"
          :key="`line-${index}`"
          :ref="(el) => bindPath(el, index)"
          class="req-empty-path"
          :d="d"
        />
      </svg>
      <div ref="hubRef" class="req-empty-node is-hub">
        <FolderKanban :size="16" />
        <span>需求</span>
      </div>
      <div class="req-empty-leaves">
        <div
          v-for="(leaf, index) in leaves"
          :key="`${leaf.kind}-${index}`"
          :ref="(el) => bindLeaf(el, index)"
          class="req-empty-node"
          :class="leaf.kind === 'git' ? 'is-git' : 'is-folder'"
        >
          <GitBranch v-if="leaf.kind === 'git'" :size="14" />
          <Folder v-else :size="14" />
          <span>{{ leaf.label }}</span>
        </div>
      </div>
    </div>
    <h2 ref="titleRef">从一条需求开始</h2>
    <ul ref="descRef" class="req-empty-points">
      <li>
        <div>
          <strong>一条需求，挂齐所有仓库</strong>
          <span>前端、后端、脚本、配置——多个 Git 项目或普通文件夹都能挂在同一条需求上。智能体一次看全、一次改完，不用来回切项目。</span>
        </div>
      </li>
      <li>
        <div>
          <strong>同一仓库，并行推进多条需求</strong>
          <span>每个需求从所选分支迁出独立 Worktree。一个 Git 项目可以同时开发多条需求，互不抢分支，也不用 stash。</span>
        </div>
      </li>
      <li>
        <div>
          <strong>原仓库不被打断</strong>
          <span>改动落在独立工作区，当前分支保持原样。普通文件夹可复制到隔离目录，也可以直接使用原目录。</span>
        </div>
      </li>
    </ul>
    <div ref="actionRef" class="req-empty-action">
      <UiButton variant="primary" @click="emit('create')"><Plus :size="16" /> 新建需求</UiButton>
    </div>
  </section>
</template>
