<script setup lang="ts">
import { computed, onBeforeUnmount, reactive } from "vue";
import RichContent from "../RichContent.vue";
import { genUiBlockId, type GenUiSegment, type GenUiSpec } from "./block";
import { genUiEntry } from "./registry";
import { genUiStateKey, readGenUiState, writeGenUiState } from "./state";
import type { GenUiSubmission } from "../ui-outbound";
import { t } from "../../../i18n";

const props = defineProps<{
  segment: Exclude<GenUiSegment, { kind: "md" }>;
  /** 落库后的消息 id：用于 blockId，回传给智能体 */
  messageId: string;
  /**
   * 局部状态（排序、草稿）的缓存键。流式时消息 id 是 run:<runId>，落库后换成正式 id；
   * 用 runId 做键，两边是同一份，不需要迁移。
   */
  stateId?: string;
  /** 超过单条消息上限：整块按代码显示 */
  overLimit?: boolean;
  submissions?: Map<string, GenUiSubmission>;
  /** 已点击、还在发送队列里的 blockId */
  sending?: Set<string>;
  /** 会话正忙 / 预览：交互按钮置灰 */
  disabled?: boolean;
  disabledReason?: string;
  sessionId?: string;
}>();
const emit = defineEmits<{
  action: [payload: { blockId: string; type: string; title?: string; action: string; values?: unknown }];
}>();

interface Rendered {
  key: string;
  blockId: string;
  spec: GenUiSpec;
  stateKey: string;
  ok: boolean;
  props?: unknown;
  truncated?: { shown: number; total: number };
  fallback?: string;
}

const items = computed<Rendered[]>(() => {
  const segment = props.segment;
  if (segment.kind !== "ui" || !segment.specs || props.overLimit) return [];
  return segment.specs
    .map((spec, sub) => ({ spec, sub }))
    // question 块走输入框上方的提问卡片，不在正文里画；序号先取再过滤，blockId 不随之偏移
    .filter(({ spec }) => spec.type !== "question")
    .map(({ spec, sub }) => {
      const blockId = genUiBlockId(props.messageId, segment.index, sub);
      const base = {
        key: `${segment.index}.${sub}`,
        blockId,
        spec,
        stateKey: genUiStateKey(props.stateId || props.messageId, segment.index, spec.type, sub),
      };
      const entry = genUiEntry(spec.type);
      const result = entry?.component ? entry.validate(spec.props) : null;
      if (!result?.ok) {
        return { ...base, ok: false, fallback: `\`\`\`json\n${JSON.stringify(spec, null, 2)}\n\`\`\`` };
      }
      return { ...base, ok: true, props: result.props, ...(result.truncated ? { truncated: result.truncated } : {}) };
    });
});

/** JSON 坏了或超过上限：原样显示成代码块 */
const wholeFallback = computed(() => {
  const segment = props.segment;
  return segment.kind === "ui" && (!segment.specs || props.overLimit) ? segment.source : "";
});

/*
 * 高度缓存：组件是异步加载的，重建时（长会话滚回来、流式结束换正式 id）加载完之前高度为 0，页面会跳。
 * 量到的高度记进 state.ts；重建后先用上次的高度占位，组件内容出来就撤掉。
 */
const measured = reactive(new Set<string>());
const observers = new Map<string, { el: HTMLElement; observer: ResizeObserver }>();

function placeholderStyle(stateKey: string) {
  if (measured.has(stateKey)) return undefined;
  const height = readGenUiState<number>(stateKey, "height");
  return height ? { minHeight: `${height}px` } : undefined;
}

function observeBody(stateKey: string, el: unknown) {
  const current = observers.get(stateKey);
  // 函数 ref 每次渲染都会调；同一个元素不重复挂
  if (current && current.el === el) return;
  current?.observer.disconnect();
  observers.delete(stateKey);
  if (!(el instanceof HTMLElement) || typeof ResizeObserver === "undefined") return;
  const observer = new ResizeObserver(() => {
    // 组件还没加载出来时 body 为空，不记
    if (!el.offsetHeight) return;
    measured.add(stateKey);
    const section = el.parentElement;
    if (section) writeGenUiState(stateKey, "height", section.offsetHeight);
  });
  observer.observe(el);
  observers.set(stateKey, { el, observer });
}

onBeforeUnmount(() => {
  for (const { observer } of observers.values()) observer.disconnect();
  observers.clear();
});

function onAction(item: Rendered, action: string, values?: unknown) {
  emit("action", {
    blockId: item.blockId,
    type: item.spec.type,
    ...(item.spec.title ? { title: item.spec.title } : {}),
    action,
    ...(values !== undefined ? { values } : {}),
  });
}
</script>

<template>
  <div v-if="segment.kind === 'pending'" class="genui-block genui-skeleton" aria-busy="true">
    <span>{{ t("genui.rendering") }}</span>
  </div>
  <div v-else-if="wholeFallback" class="genui-fallback">
    <RichContent :content="wholeFallback" />
    <p class="genui-fallback-note">{{ overLimit ? t("genui.overLimit") : t("genui.cannotRender") }}</p>
  </div>
  <template v-else>
    <template v-for="item in items" :key="item.key">
      <section
        v-if="item.ok"
        class="genui-block"
        :data-type="item.spec.type"
        :style="placeholderStyle(item.stateKey)"
      >
        <header v-if="item.spec.title" class="genui-title">{{ item.spec.title }}</header>
        <div :ref="(el) => observeBody(item.stateKey, el)" class="genui-body">
          <component
            :is="genUiEntry(item.spec.type)!.component!"
            :data="item.props"
            :state-key="item.stateKey"
            :title="item.spec.title"
            :submitted="submissions?.get(item.blockId) ?? null"
            :disabled="disabled || Boolean(sending?.has(item.blockId))"
            :disabled-reason="sending?.has(item.blockId) ? t('genui.sending') : disabledReason"
            :session-id="sessionId"
            @action="(action: string, values?: unknown) => onAction(item, action, values)"
          />
        </div>
        <p v-if="item.truncated" class="genui-truncated">
          {{ t("genui.truncated", { shown: item.truncated.shown, total: item.truncated.total }) }}
        </p>
      </section>
      <div v-else class="genui-fallback">
        <RichContent :content="item.fallback!" />
        <p class="genui-fallback-note">{{ t("genui.cannotRender") }}</p>
      </div>
    </template>
  </template>
</template>
