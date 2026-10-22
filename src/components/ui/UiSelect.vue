<script setup lang="ts" generic="T">
/*
 * 自定义下拉框（设计系统 Dropdown Select 的 Vue 实现，样式见 design/components.css 的 .ds-select-*）
 *
 * - 单选 / 多选（触发器内标签按宽度自适应折叠为 +N、上限、全选、清空）
 * - 搜索：支持拼音首字母 / 全拼 / 混拼，命中高亮；也搜索描述与 keywords
 * - 头像、描述、右侧附加信息、分组小标题
 * - 可清空、可创建、远程搜索（防抖 + 丢弃过期结果）
 * - 浮层 Teleport 到 body 并使用 fixed 定位，避免被弹窗或 overflow 容器裁剪
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, useAttrs, watch } from "vue";
import { Check, ChevronDown, Plus, Search, X } from "@lucide/vue";
import { matchText, pinyinInitial, splitByRange, type MatchRange } from "../../utils/pinyin-match";
import type { SelectGroup, SelectOption, SelectValue } from "./select-types";
import AgentAvatar from "../AgentAvatar.vue";
import { useI18n } from "../../i18n";

defineOptions({ inheritAttrs: false });

const props = withDefaults(defineProps<{
  /** 单选为选项 value；多选为 value 数组。泛型 T 跟随 v-model 变量推断 */
  modelValue?: T;
  options?: SelectOption[];
  groups?: SelectGroup[];
  placeholder?: string;
  /** 不传时选项数量 ≥ SEARCH_THRESHOLD 自动开启搜索 */
  searchable?: boolean;
  searchPlaceholder?: string;
  multiple?: boolean;
  /** 触发器最多展示的标签数；超出以 +N 折叠。实际展示还会按触发器宽度自动再折叠 */
  maxTagCount?: number;
  max?: number;
  clearable?: boolean;
  creatable?: boolean;
  load?: (query: string) => Promise<SelectOption[]>;
  disabled?: boolean;
  invalid?: boolean;
  required?: boolean;
  size?: "sm" | "md" | "lg";
  /** ghost：无边框透明背景，用于属性面板、工具栏等行内场景 */
  variant?: "default" | "ghost";
  emptyText?: string;
}>(), {
  modelValue: undefined,
  options: undefined,
  groups: undefined,
  placeholder: undefined,
  searchable: undefined,
  searchPlaceholder: undefined,
  maxTagCount: Number.POSITIVE_INFINITY,
  max: undefined,
  load: undefined,
  size: "md",
  variant: "default",
  emptyText: undefined,
});

const { t } = useI18n();
const resolvedPlaceholder = computed(() => props.placeholder ?? t("select.placeholder"));
const resolvedSearchPlaceholder = computed(() => props.searchPlaceholder ?? t("select.searchPlaceholder"));
const resolvedEmptyText = computed(() => props.emptyText ?? t("select.empty"));

const emit = defineEmits<{
  "update:modelValue": [value: T];
  change: [value: T];
  create: [label: string];
}>();

const attrs = useAttrs();
// class / style 留在根节点，其余（aria-label、title 等）透传到触发器
const rootAttrs = computed(() => ({ class: attrs.class, style: attrs.style }));
const triggerAttrs = computed(() => {
  const { class: _class, style: _style, ...rest } = attrs;
  return rest;
});

// 选项数量达到该值时默认显示搜索框
const SEARCH_THRESHOLD = 8;
// 下方剩余空间小于该值且上方更宽裕时向上展开（约等于列表最大高度 + 搜索框）
const FLIP_THRESHOLD = 320;
// 浮层与触发器、视口边缘的间距
const POPOVER_GAP = 4;
const VIEWPORT_MARGIN = 12;

const id = `ui-select-${Math.random().toString(36).slice(2, 9)}`;

const rootEl = ref<HTMLElement | null>(null);
const triggerEl = ref<HTMLElement | null>(null);
const tagsEl = ref<HTMLElement | null>(null);
const measureEl = ref<HTMLElement | null>(null);
const restMeasureEl = ref<HTMLElement | null>(null);
const popoverEl = ref<HTMLElement | null>(null);
const listEl = ref<HTMLElement | null>(null);
const inputEl = ref<HTMLInputElement | null>(null);
/** 按触发器宽度算出的可见标签数（再与 maxTagCount 取小）；初始全量，首帧后由 updateTagFit 收敛 */
const fitTagCount = ref(Number.POSITIVE_INFINITY);
const TAG_GAP_FALLBACK = 4;
const MIN_TAG_WIDTH = 24;
let tagFitObserver: ResizeObserver | undefined;

const open = ref(false);
const query = ref("");
const activeIndex = ref(-1);
const placement = ref<"bottom" | "top">("bottom");
const popoverStyle = ref<Record<string, string>>({});
const loading = ref(false);
const remoteOptions = ref<SelectOption[] | null>(null);
// 远程选中项 / 新创建项：保留下来，保证关闭后触发器仍能显示其标签
const extraOptions = ref<SelectOption[]>([]);

const sourceGroups = computed<SelectGroup[]>(() => {
  const groups = props.groups ?? [{ label: "", options: props.options ?? [] }];
  const created = extraOptions.value.filter((option) => !groups.some((group) => group.options.some((item) => item.value === option.value)));
  if (!created.length) return groups;
  const last = groups[groups.length - 1];
  return [...groups.slice(0, -1), { ...last, options: [...last.options, ...created] }];
});
const allOptions = computed(() => [...sourceGroups.value.flatMap((group) => group.options), ...(remoteOptions.value ?? [])]);
const isSearchable = computed(() => Boolean(props.load) || (props.searchable ?? allOptions.value.length >= SEARCH_THRESHOLD));

// 内部统一按 SelectValue 处理，对外通过泛型 T 保持调用方的类型
const model = computed(() => props.modelValue as unknown as SelectValue | SelectValue[] | undefined);
const selectedValues = computed<SelectValue[]>(() => {
  if (props.multiple) return Array.isArray(model.value) ? model.value : [];
  return model.value === undefined ? [] : [model.value as SelectValue];
});
const findOption = (value: SelectValue) =>
  allOptions.value.find((option) => option.value === value) ?? extraOptions.value.find((option) => option.value === value);
const isSelected = (value: SelectValue) => selectedValues.value.includes(value);
// 空字符串 / null 视为「未选择」，但若存在 value 为 "" 的选项（如「不关联」），则按选中该项显示
const singleOption = computed(() => (props.multiple ? undefined : findOption(model.value as SelectValue)));
const hasValue = computed(() =>
  props.multiple
    ? selectedValues.value.length > 0
    : model.value != null && model.value !== "",
);
const showsValue = computed(() => hasValue.value || Boolean(singleOption.value));
const reachedMax = computed(() => props.multiple && props.max != null && selectedValues.value.length >= props.max);

const selectedTagOptions = computed(() =>
  selectedValues.value.map((value) => findOption(value) ?? { value, label: String(value) }),
);
const shownTags = computed(() => {
  const cap = Math.min(fitTagCount.value, props.maxTagCount, selectedTagOptions.value.length);
  return selectedTagOptions.value.slice(0, Math.max(0, cap));
});
const restTags = computed(() => selectedTagOptions.value.slice(shownTags.value.length).map((option) => option.label));

function updateTagFit() {
  if (!props.multiple) return;
  const total = selectedTagOptions.value.length;
  if (!total) {
    fitTagCount.value = 0;
    return;
  }
  const hardMax = Math.min(total, props.maxTagCount);
  const container = tagsEl.value;
  const measure = measureEl.value;
  const restProbe = restMeasureEl.value;
  if (!container || !measure) {
    fitTagCount.value = hardMax;
    return;
  }
  const available = container.clientWidth;
  if (available <= 0) {
    fitTagCount.value = hardMax;
    return;
  }
  const styles = getComputedStyle(measure);
  const gap = Number.parseFloat(styles.columnGap || styles.gap) || TAG_GAP_FALLBACK;
  const tagNodes = [...measure.querySelectorAll<HTMLElement>(".ds-select-tag:not(.is-rest)")];
  let used = 0;
  let count = 0;
  for (let i = 0; i < hardMax; i += 1) {
    const width = tagNodes[i]?.offsetWidth ?? 0;
    const rest = total - i - 1;
    if (restProbe) restProbe.textContent = rest > 0 ? `+${rest}` : "+0";
    const restWidth = rest > 0 && restProbe ? restProbe.offsetWidth + gap : 0;
    const next = used + (count > 0 ? gap : 0) + width;
    if (next + restWidth <= available) {
      used = next;
      count += 1;
      continue;
    }
    if (count === 0) {
      const minNeed = MIN_TAG_WIDTH + restWidth;
      if (minNeed <= available || (rest > 0 && restProbe && restProbe.offsetWidth <= available)) {
        count = minNeed <= available ? 1 : 0;
      }
    }
    break;
  }
  fitTagCount.value = count;
}

function bindTagFitObserver() {
  tagFitObserver?.disconnect();
  tagFitObserver = undefined;
  if (!props.multiple || !triggerEl.value) return;
  tagFitObserver = new ResizeObserver(() => {
    void nextTick(updateTagFit);
  });
  tagFitObserver.observe(triggerEl.value);
}

watch(
  () => [props.multiple, props.maxTagCount, props.size, selectedValues.value] as const,
  () => {
    if (!props.multiple) return;
    void nextTick(() => {
      bindTagFitObserver();
      updateTagFit();
    });
  },
  { flush: "post" },
);

onMounted(() => {
  if (!props.multiple) return;
  bindTagFitObserver();
  void nextTick(updateTagFit);
});

/* ── 列表：按搜索词过滤，并扁平化为 items 供键盘导航 ───────────────── */
interface ViewOption {
  option: SelectOption;
  index: number;
  disabled: boolean;
  label: [string, string, string];
  description?: [string, string, string];
}
interface ListItem {
  option?: SelectOption;
  create?: string;
  disabled: boolean;
}

const view = computed(() => {
  const keyword = query.value.trim();
  const groups = remoteOptions.value ? [{ label: "", options: remoteOptions.value }] : sourceGroups.value;
  const items: ListItem[] = [];
  const visibleGroups: Array<{ label: string; options: ViewOption[] }> = [];
  for (const group of groups) {
    const options: ViewOption[] = [];
    for (const option of group.options) {
      let range: MatchRange | null = null;
      let descRange: MatchRange | null = null;
      // 远程搜索的结果由服务端过滤，本地不再筛选
      if (keyword && !props.load) {
        range = matchText(option.label, keyword, option.pinyin);
        if (!range && option.description) descRange = matchText(option.description, keyword);
        const keywordHit = !range && !descRange && option.keywords?.some((item) => matchText(item, keyword));
        if (!range && !descRange && !keywordHit) continue;
      }
      const disabled = Boolean(option.disabled) || (reachedMax.value && !isSelected(option.value));
      items.push({ option, disabled });
      options.push({
        option,
        index: items.length - 1,
        disabled,
        label: splitByRange(option.label, range),
        description: option.description ? splitByRange(option.description, descRange) : undefined,
      });
    }
    if (options.length) visibleGroups.push({ label: group.label, options });
  }
  // 可创建：输入内容与任何选项都不完全相同时，追加「创建」项
  let createIndex = -1;
  const exists = allOptions.value.some((option) => option.label.toLowerCase() === keyword.toLowerCase());
  if (props.creatable && keyword && !exists && !reachedMax.value) {
    items.push({ create: keyword, disabled: false });
    createIndex = items.length - 1;
  }
  return { groups: visibleGroups, items, createIndex };
});

const emptyMessage = computed(() => (
  props.load && !query.value.trim() ? t("select.searchHint") : resolvedEmptyText.value
));
const selectedCountLabel = computed(() =>
  t("select.selectedCount", {
    n: selectedValues.value.length,
    max: props.max != null ? t("select.selectedMax", { max: props.max }) : "",
  }),
);

function nextEnabled(from: number, step: 1 | -1) {
  const { items } = view.value;
  const count = items.length;
  for (let i = 1; i <= count; i += 1) {
    const index = (((from + step * i) % count) + count) % count;
    if (!items[index]?.disabled) return index;
  }
  return -1;
}

function setActive(index: number, scroll = true) {
  activeIndex.value = index;
  if (!scroll || index < 0) return;
  nextTick(() => listEl.value?.querySelector(`[data-index="${index}"]`)?.scrollIntoView({ block: "nearest" }));
}

// 过滤结果变化后，保证活动项仍然合法
watch(() => view.value.items, (items) => {
  if (activeIndex.value >= items.length || items[activeIndex.value]?.disabled) setActive(nextEnabled(-1, 1));
});

/* ── 展开 / 收起与定位 ──────────────────────────────────────────── */
function updatePosition() {
  const trigger = triggerEl.value;
  if (!trigger) return;
  const rect = trigger.getBoundingClientRect();
  const below = window.innerHeight - rect.bottom;
  placement.value = below < FLIP_THRESHOLD && rect.top > below ? "top" : "bottom";
  const popoverWidth = popoverEl.value?.offsetWidth ?? rect.width;
  const left = Math.max(VIEWPORT_MARGIN, Math.min(rect.left, window.innerWidth - popoverWidth - VIEWPORT_MARGIN));
  popoverStyle.value = {
    left: `${left}px`,
    minWidth: `${rect.width}px`,
    ...(placement.value === "top"
      ? { bottom: `${window.innerHeight - rect.top + POPOVER_GAP}px` }
      : { top: `${rect.bottom + POPOVER_GAP}px` }),
  };
}

function onOutsidePointer(event: PointerEvent) {
  const target = event.target as Node;
  if (rootEl.value?.contains(target) || popoverEl.value?.contains(target)) return;
  // 点击外部关闭时不抢回焦点，让焦点自然落到用户点击的位置
  setOpen(false, { restoreFocus: false });
}

function setOpen(next: boolean, { restoreFocus = true } = {}) {
  if (next === open.value || (next && props.disabled)) return;
  open.value = next;
  if (next) {
    query.value = "";
    remoteOptions.value = null;
    updatePosition();
    const selectedIndex = view.value.items.findIndex((item) => item.option && isSelected(item.option.value));
    setActive(selectedIndex >= 0 ? selectedIndex : nextEnabled(-1, 1));
    document.addEventListener("pointerdown", onOutsidePointer, true);
    window.addEventListener("resize", updatePosition);
    window.addEventListener("scroll", updatePosition, true);
    nextTick(() => {
      // 浮层渲染后才知道真实宽度，需要再校正一次水平位置
      updatePosition();
      inputEl.value?.focus();
    });
  } else {
    clearTimeout(loadTimer);
    loading.value = false;
    remoteOptions.value = null;
    removeListeners();
    const active = document.activeElement;
    if (restoreFocus && (popoverEl.value?.contains(active) || rootEl.value?.contains(active))) triggerEl.value?.focus();
  }
}

function removeListeners() {
  document.removeEventListener("pointerdown", onOutsidePointer, true);
  window.removeEventListener("resize", updatePosition);
  window.removeEventListener("scroll", updatePosition, true);
}

watch(() => props.disabled, (disabled) => {
  if (disabled) setOpen(false, { restoreFocus: false });
});

/* ── 选择 ──────────────────────────────────────────────────────── */
function commit(value: SelectValue | SelectValue[]) {
  emit("update:modelValue", value as T);
  emit("change", value as T);
}

function rememberRemote(value: SelectValue) {
  const option = remoteOptions.value?.find((item) => item.value === value);
  if (option && !extraOptions.value.some((item) => item.value === value)) extraOptions.value = [...extraOptions.value, option];
}

function toggle(value: SelectValue) {
  rememberRemote(value);
  if (props.multiple) {
    const current = selectedValues.value;
    if (current.includes(value)) commit(current.filter((item) => item !== value));
    else if (!reachedMax.value) commit([...current, value]);
    query.value = "";
    inputEl.value?.focus();
  } else {
    if (value !== model.value) commit(value);
    setOpen(false);
  }
}

function choose(index: number) {
  const item = view.value.items[index];
  if (!item || item.disabled) return;
  if (item.create) {
    const option: SelectOption = { value: item.create, label: item.create };
    extraOptions.value = [...extraOptions.value, option];
    emit("create", item.create);
    toggle(option.value);
    return;
  }
  if (item.option) toggle(item.option.value);
}

function clear() {
  commit(props.multiple ? [] : null);
}

function selectAllVisible() {
  const values = view.value.items.filter((item) => item.option && !item.option.disabled).map((item) => item.option!.value);
  const merged = [...new Set([...selectedValues.value, ...values])];
  commit(props.max != null ? merged.slice(0, props.max) : merged);
}

/* ── 远程搜索 ──────────────────────────────────────────────────── */
let loadTimer: ReturnType<typeof setTimeout> | undefined;
let loadToken = 0;

function onSearchInput(event: Event) {
  query.value = (event.target as HTMLInputElement).value;
  activeIndex.value = 0;
  if (!props.load) return;
  clearTimeout(loadTimer);
  const keyword = query.value.trim();
  if (!keyword) {
    loading.value = false;
    remoteOptions.value = null;
    return;
  }
  loading.value = true;
  // 防抖 300ms；用自增 token 丢弃过期请求的结果
  const token = (loadToken += 1);
  loadTimer = setTimeout(async () => {
    try {
      const options = await props.load!(keyword);
      if (token !== loadToken || !open.value) return;
      remoteOptions.value = options;
      setActive(nextEnabled(-1, 1));
    } finally {
      if (token === loadToken) loading.value = false;
    }
  }, 300);
}

/* ── 键盘 ──────────────────────────────────────────────────────── */
// 不可搜索时支持「按字母跳转」：按拼音首字母定位到匹配的选项
function typeahead(key: string) {
  const { items } = view.value;
  for (let i = 1; i <= items.length; i += 1) {
    const index = (activeIndex.value + i) % items.length;
    const first = items[index].option && [...items[index].option!.label][0];
    if (!items[index].disabled && first && pinyinInitial(first) === key.toLowerCase()) {
      setActive(index);
      return;
    }
  }
}

function onKeydown(event: KeyboardEvent) {
  if (props.disabled) return;
  const { key } = event;
  const fromInput = event.target === inputEl.value;
  if (!open.value) {
    if (["Enter", " ", "ArrowDown", "ArrowUp"].includes(key)) {
      event.preventDefault();
      setOpen(true);
    } else if (key === "Backspace" && props.clearable && hasValue.value) {
      clear();
    } else if (!isSearchable.value && key.length === 1 && /\S/.test(key)) {
      setOpen(true);
      typeahead(key);
    }
    return;
  }
  switch (key) {
    case "ArrowDown":
    case "ArrowUp":
      event.preventDefault();
      setActive(nextEnabled(activeIndex.value, key === "ArrowDown" ? 1 : -1));
      break;
    case "Home":
    case "End":
      if (fromInput) break;
      event.preventDefault();
      setActive(key === "Home" ? nextEnabled(-1, 1) : nextEnabled(view.value.items.length, -1));
      break;
    case "Enter":
      event.preventDefault();
      if (activeIndex.value >= 0) choose(activeIndex.value);
      break;
    case " ":
      if (fromInput) break;
      event.preventDefault();
      if (activeIndex.value >= 0) choose(activeIndex.value);
      break;
    case "Escape":
      event.preventDefault();
      event.stopPropagation();
      setOpen(false);
      break;
    case "Tab":
      // 焦点回到触发器，浏览器再按 Tab 的默认行为移到下一个控件
      setOpen(false);
      break;
    case "Backspace":
      if (fromInput && !query.value && props.multiple && selectedValues.value.length) {
        toggle(selectedValues.value[selectedValues.value.length - 1]);
      }
      break;
    default:
      if (!isSearchable.value && key.length === 1 && /\S/.test(key)) typeahead(key);
  }
}

function onTriggerClick(event: MouseEvent) {
  if (props.disabled) return;
  const target = event.target as HTMLElement;
  const remove = target.closest<HTMLElement>("[data-remove-index]");
  if (remove) {
    toggle(selectedValues.value[Number(remove.dataset.removeIndex)]);
    return;
  }
  if (target.closest(".ds-select-clear")) {
    clear();
    return;
  }
  setOpen(!open.value);
}

// 表单校验失败时，把焦点交给触发器（隐藏输入框本身不可见）
function onNativeInvalid() {
  triggerEl.value?.focus();
}

onBeforeUnmount(() => {
  clearTimeout(loadTimer);
  removeListeners();
  tagFitObserver?.disconnect();
  tagFitObserver = undefined;
});
</script>

<template>
  <div
    ref="rootEl"
    v-bind="rootAttrs"
    class="ds-select"
    :data-open="open ? '' : undefined"
    :data-disabled="disabled ? '' : undefined"
    :data-invalid="invalid ? '' : undefined"
    :data-clearable="clearable ? '' : undefined"
    :data-has-value="hasValue ? '' : undefined"
    :data-size="size === 'md' ? undefined : size"
    :data-variant="variant === 'default' ? undefined : variant"
    :data-placement="placement"
  >
    <div
      ref="triggerEl"
      v-bind="triggerAttrs"
      class="ds-select-trigger"
      role="combobox"
      :tabindex="disabled ? -1 : 0"
      aria-haspopup="listbox"
      :aria-expanded="open"
      :aria-controls="`${id}-list`"
      :aria-disabled="disabled || undefined"
      :aria-invalid="invalid || undefined"
      :aria-required="required || undefined"
      :aria-activedescendant="open && !isSearchable && activeIndex >= 0 ? `${id}-opt-${activeIndex}` : undefined"
      @click="onTriggerClick"
      @keydown="onKeydown"
    >
      <span v-if="$slots.prefix" class="ds-select-prefix"><slot name="prefix" /></span>
      <span v-if="multiple && hasValue" ref="tagsEl" class="ds-select-tags">
        <span v-for="(option, index) in shownTags" :key="String(option.value)" class="ds-select-tag">
          <AgentAvatar v-if="option.avatar" class="ds-avatar" :value="option.avatar" :size="null" />
          <span class="ds-select-text">{{ option.label }}</span>
          <span
            v-if="!disabled"
            class="ds-select-tag-remove"
            role="button"
            :data-remove-index="index"
            :aria-label="t('select.removeTag', { label: option.label })"
          ><X :size="12" /></span>
        </span>
        <span v-if="restTags.length" class="ds-select-tag is-rest" :title="restTags.join('、')">+{{ restTags.length }}</span>
      </span>
      <span
        v-if="multiple && hasValue"
        ref="measureEl"
        class="ds-select-tags-measure"
        aria-hidden="true"
      >
        <span v-for="option in selectedTagOptions" :key="`m-${String(option.value)}`" class="ds-select-tag">
          <AgentAvatar v-if="option.avatar" class="ds-avatar" :value="option.avatar" :size="null" />
          <span class="ds-select-text">{{ option.label }}</span>
          <span v-if="!disabled" class="ds-select-tag-remove"><X :size="12" /></span>
        </span>
        <span ref="restMeasureEl" class="ds-select-tag is-rest">+0</span>
      </span>
      <span v-else class="ds-select-value">
        <template v-if="!multiple && showsValue">
          <slot name="option-leading" :option="singleOption">
            <AgentAvatar
              v-if="singleOption?.avatar"
              class="ds-avatar"
              :value="singleOption.avatar"
              :size="null"
              style="width: 20px; height: 20px; font-size: 11px"
            />
          </slot>
          <span class="ds-select-text">{{ singleOption?.label ?? String(modelValue) }}</span>
        </template>
        <span v-else class="ds-select-placeholder">{{ resolvedPlaceholder }}</span>
      </span>
      <span class="ds-select-suffix">
        <span class="ds-select-clear" role="button" :aria-label="t('select.clear')"><X :size="13" /></span>
        <span class="ds-select-arrow"><ChevronDown :size="15" /></span>
      </span>
    </div>

    <!-- 隐藏输入框：让 required 参与原生表单校验 -->
    <input
      v-if="required"
      class="ds-select-native"
      tabindex="-1"
      aria-hidden="true"
      required
      :value="hasValue ? '1' : ''"
      @invalid="onNativeInvalid"
    />

    <Teleport to="body">
      <div
        v-if="open"
        ref="popoverEl"
        class="ds-select-popover is-floating"
        :data-placement="placement"
        :style="popoverStyle"
        @keydown="onKeydown"
      >
        <label v-if="isSearchable" class="ds-select-search">
          <Search :size="14" />
          <input
            ref="inputEl"
            type="text"
            :value="query"
            :placeholder="resolvedSearchPlaceholder"
            :aria-controls="`${id}-list`"
            :aria-activedescendant="activeIndex >= 0 ? `${id}-opt-${activeIndex}` : undefined"
            autocomplete="off"
            spellcheck="false"
            @input="onSearchInput"
          />
        </label>
        <!-- mousedown.prevent：选择时不让搜索框失焦，避免闪烁 -->
        <div
          :id="`${id}-list`"
          ref="listEl"
          class="ds-select-list"
          role="listbox"
          :aria-multiselectable="multiple || undefined"
          @mousedown.prevent
        >
          <div v-if="loading" class="ds-select-empty"><span class="ds-spinner" />{{ t("select.loading") }}</div>
          <template v-else>
            <div
              v-for="(group, groupIndex) in view.groups"
              :key="group.label || groupIndex"
              class="ds-select-group"
              role="group"
              :aria-label="group.label || undefined"
            >
              <div v-if="group.label" class="ds-select-group-label">
                <span>{{ group.label }}</span><span>{{ group.options.length }}</span>
              </div>
              <div
                v-for="item in group.options"
                :id="`${id}-opt-${item.index}`"
                :key="String(item.option.value)"
                class="ds-select-option"
                :class="{ 'has-description': item.option.description }"
                role="option"
                :data-index="item.index"
                :data-active="item.index === activeIndex ? '' : undefined"
                :aria-selected="isSelected(item.option.value)"
                :aria-disabled="item.disabled || undefined"
                @mousemove="item.index !== activeIndex && setActive(item.index, false)"
                @click="choose(item.index)"
              >
                <slot name="option-leading" :option="item.option">
                  <AgentAvatar v-if="item.option.avatar" class="ds-avatar" :value="item.option.avatar" :size="null" />
                </slot>
                <span class="ds-select-option-body">
                  <span class="ds-select-option-label">{{ item.label[0] }}<mark v-if="item.label[1]">{{ item.label[1] }}</mark>{{ item.label[2] }}</span>
                  <span v-if="item.description" class="ds-select-option-desc">{{ item.description[0] }}<mark v-if="item.description[1]">{{ item.description[1] }}</mark>{{ item.description[2] }}</span>
                </span>
                <span v-if="item.option.meta" class="ds-select-option-meta">{{ item.option.meta }}</span>
                <span class="ds-select-check"><Check :size="14" /></span>
              </div>
            </div>
            <div
              v-if="view.createIndex >= 0"
              :id="`${id}-opt-${view.createIndex}`"
              class="ds-select-option is-create"
              role="option"
              :data-index="view.createIndex"
              :data-active="view.createIndex === activeIndex ? '' : undefined"
              aria-selected="false"
              @mousemove="view.createIndex !== activeIndex && setActive(view.createIndex, false)"
              @click="choose(view.createIndex)"
            >
              <Plus :size="14" />
              <span class="ds-select-option-body"><span class="ds-select-option-label">{{ t("select.create", { label: query.trim() }) }}</span></span>
            </div>
            <div v-if="!view.items.length" class="ds-select-empty"><Search :size="18" />{{ emptyMessage }}</div>
          </template>
        </div>
        <div v-if="multiple" class="ds-select-footer">
          <span>{{ selectedCountLabel }}</span>
          <div>
            <button type="button" class="ui-button" data-variant="ghost" data-size="small" @mousedown.prevent @click="selectAllVisible">{{ t("select.selectAll") }}</button>
            <button type="button" class="ui-button" data-variant="ghost" data-size="small" :disabled="!hasValue" @mousedown.prevent @click="clear">{{ t("select.clear") }}</button>
          </div>
        </div>
      </div>
    </Teleport>
  </div>
</template>
