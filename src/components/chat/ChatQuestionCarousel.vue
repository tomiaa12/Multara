<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { Check, ChevronDown, ChevronUp, MessageCircleQuestion } from "@lucide/vue";
import {
  formatAnswers,
  isAnswered,
  type ChatQuestion,
  type ChatQuestionAnswer,
} from "./question-block";
import { t } from "../../i18n";

const props = defineProps<{
  questions: ChatQuestion[];
  agentName?: string;
}>();
const emit = defineEmits<{
  submit: [text: string];
  skip: [];
}>();

const root = ref<HTMLElement | null>(null);
const body = ref<HTMLElement | null>(null);
/** 每道题一个容器 / 自定义输入框，按题目下标存，用于滚动定位和聚焦 */
const itemEls = ref<HTMLElement[]>([]);
const customEls = ref<Record<string, HTMLTextAreaElement>>({});
const index = ref(0);
const answers = ref<Record<string, ChatQuestionAnswer>>({});
const focused = ref(0);
const collapsed = ref(false);
const isMac = /Mac|iPhone|iPad/.test(navigator.platform);
const agentFallback = computed(() => props.agentName ?? t("chat.role.agent"));
const submitMod = computed(() => (isMac ? "⌘" : "Ctrl"));

const current = computed(() => props.questions[index.value]);
const total = computed(() => props.questions.length);
const multiple = computed(() => total.value > 1);
const isLast = computed(() => index.value >= total.value - 1);
const answeredCount = computed(() =>
  props.questions.filter((question) => isAnswered(answers.value[question.id])).length,
);

watch(() => props.questions, () => {
  index.value = 0;
  answers.value = {};
  focused.value = 0;
});

function answerOf(question: ChatQuestion): ChatQuestionAnswer {
  return answers.value[question.id] ?? { selected: [] };
}

function setAnswer(question: ChatQuestion, next: ChatQuestionAnswer) {
  answers.value = { ...answers.value, [question.id]: next };
}

function isSelected(question: ChatQuestion, label: string) {
  return answers.value[question.id]?.selected.includes(label) ?? false;
}

function setCustomEl(id: string, el: unknown) {
  if (el instanceof HTMLTextAreaElement) customEls.value[id] = el;
  else delete customEls.value[id];
}

/** 切到第 n 题：高亮并滚到可见；keepFocus 时不抢自定义输入框的焦点 */
function activate(next: number, options: { keepFocus?: boolean } = {}) {
  if (next < 0 || next >= total.value) return;
  if (next !== index.value) focused.value = 0;
  index.value = next;
  void nextTick(() => {
    itemEls.value[next]?.scrollIntoView({ block: "nearest", behavior: "smooth" });
    if (!options.keepFocus) root.value?.focus({ preventScroll: true });
  });
}

/** 单选答完后跳到下一道还没答的题；都答了就停在原地等提交 */
function advanceToUnanswered() {
  for (let step = 1; step < total.value; step += 1) {
    const candidate = (index.value + step) % total.value;
    if (!isAnswered(answers.value[props.questions[candidate].id])) {
      activate(candidate);
      return;
    }
  }
}

function pick(questionIndex: number, optionIndex: number) {
  const question = props.questions[questionIndex];
  const option = question?.options[optionIndex];
  if (!question || !option) return;
  if (questionIndex !== index.value) index.value = questionIndex;
  focused.value = optionIndex;
  const previous = answerOf(question);
  if (question.multiSelect) {
    const selected = previous.selected.includes(option.label)
      ? previous.selected.filter((item) => item !== option.label)
      : [...previous.selected, option.label];
    setAnswer(question, { ...previous, selected });
    return;
  }
  setAnswer(question, { selected: [option.label] });
  // 多道题时选完自动进下一道未答题；只有一道题时仍需回车 / 点提交，避免误触直接发出
  if (multiple.value) advanceToUnanswered();
  else void nextTick(() => root.value?.focus());
}

/** 回车：单选题还没选时先选中高亮项，再进下一题或提交 */
function confirmCurrent() {
  const question = current.value;
  if (!question) return;
  const options = question.options;
  if (!question.multiSelect && !isAnswered(answerOf(question)) && focused.value < options.length) {
    setAnswer(question, { selected: [options[focused.value].label] });
  }
  if (isLast.value || answeredCount.value === total.value) {
    if (answeredCount.value) submit();
    return;
  }
  activate(index.value + 1);
}

function onCustomInput(question: ChatQuestion, event: Event) {
  const value = (event.target as HTMLTextAreaElement).value;
  const previous = answerOf(question);
  setAnswer(question, {
    selected: question.multiSelect || !value.trim() ? previous.selected : [],
    custom: value,
  });
}

function submit() {
  if (!answeredCount.value) return;
  emit("submit", formatAnswers(props.questions, answers.value));
}

function onKeydown(event: KeyboardEvent) {
  const typing = event.target instanceof HTMLTextAreaElement;
  if (event.key === "Escape") {
    event.preventDefault();
    emit("skip");
    return;
  }
  if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
    event.preventDefault();
    submit();
    return;
  }
  if (typing) {
    if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
      event.preventDefault();
      confirmCurrent();
    }
    return;
  }
  const options = current.value?.options ?? [];
  if (/^[1-9]$/.test(event.key)) {
    const target = Number(event.key) - 1;
    if (target < options.length) {
      event.preventDefault();
      pick(index.value, target);
    }
    return;
  }
  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    event.preventDefault();
    const span = options.length + (current.value?.allowCustom ? 1 : 0);
    if (!span) return;
    focused.value = (focused.value + (event.key === "ArrowDown" ? 1 : -1) + span) % span;
    if (focused.value === options.length && current.value) customEls.value[current.value.id]?.focus();
    return;
  }
  // 左右切题（多道题时）
  if ((event.key === "ArrowLeft" || event.key === "ArrowRight") && multiple.value) {
    event.preventDefault();
    activate(index.value + (event.key === "ArrowLeft" ? -1 : 1));
    return;
  }
  if (event.key === "Enter" && !event.shiftKey) {
    // 焦点在卡片里的按钮（跳过、翻页等）上时交给按钮自己处理
    if (event.target instanceof HTMLElement && event.target !== root.value && event.target.closest("button:not(.chat-question-option)")) return;
    event.preventDefault();
    confirmCurrent();
    return;
  }
  if (event.key === " " && focused.value < options.length) {
    event.preventDefault();
    pick(index.value, focused.value);
  }
}

onMounted(() => {
  // 只在用户没有聚焦其他输入时接管键盘，避免打断正在写的草稿
  const active = document.activeElement as HTMLElement | null;
  if (!active || active === document.body || !active.closest("input, textarea, [contenteditable='true']")) {
    root.value?.focus({ preventScroll: true });
  }
});

defineExpose({ focus: () => root.value?.focus() });
</script>

<template>
  <section
    v-if="current"
    ref="root"
    class="chat-question-card"
    :class="{ collapsed, 'is-multiple': multiple }"
    tabindex="0"
    role="group"
    :aria-label="t('chat.question.aria', { name: agentFallback, question: current.question })"
    @keydown="onKeydown"
  >
    <header class="chat-question-head">
      <MessageCircleQuestion :size="14" />
      <span class="chat-question-title">
        {{ t("chat.question.waitingTitle", { name: agentFallback }) }}<template v-if="multiple">{{ t("chat.question.multiSuffix", { n: total }) }}</template>
      </span>
      <template v-if="multiple">
        <UiButton
          class="icon-button"
          variant="ghost"
          size="icon"
          :title="t('chat.question.prev')"
          :disabled="index === 0"
          @click="activate(index - 1)"
        ><ChevronUp :size="14" /></UiButton>
        <span class="chat-question-step">{{ index + 1 }} / {{ total }}</span>
        <UiButton
          class="icon-button"
          variant="ghost"
          size="icon"
          :title="t('chat.question.next')"
          :disabled="isLast"
          @click="activate(index + 1)"
        ><ChevronDown :size="14" /></UiButton>
      </template>
      <UiButton
        class="icon-button chat-question-collapse"
        variant="ghost"
        size="icon"
        :title="collapsed ? t('chat.question.expand') : t('chat.question.collapse')"
        :aria-expanded="!collapsed"
        @click="collapsed = !collapsed"
      >
        <ChevronUp v-if="collapsed" :size="14" />
        <ChevronDown v-else :size="14" />
      </UiButton>
    </header>
    <div v-show="!collapsed" ref="body" class="chat-question-body vs-scroll">
      <div
        v-for="(question, questionIndex) in questions"
        :key="question.id"
        :ref="(el) => { if (el) itemEls[questionIndex] = el as HTMLElement; }"
        class="chat-question-item"
        :class="{ active: questionIndex === index, answered: isAnswered(answers[question.id]) }"
        @click="questionIndex !== index && activate(questionIndex, { keepFocus: true })"
      >
        <p class="chat-question-text">
          <span v-if="multiple" class="chat-question-no">{{ questionIndex + 1 }}.</span>
          <span>
            <small v-if="question.header" class="chat-question-header">{{ question.header }}</small>
            {{ question.question }}
          </span>
          <Check v-if="multiple && isAnswered(answers[question.id])" class="chat-question-done" :size="13" />
        </p>
        <ul
          v-if="question.options.length"
          class="chat-question-options"
          :role="question.multiSelect ? 'group' : 'radiogroup'"
        >
          <li v-for="(option, optionIndex) in question.options" :key="option.label">
            <button
              type="button"
              class="chat-question-option"
              :class="{
                selected: isSelected(question, option.label),
                focused: questionIndex === index && focused === optionIndex,
              }"
              :role="question.multiSelect ? 'checkbox' : 'radio'"
              :aria-checked="isSelected(question, option.label)"
              tabindex="-1"
              @click.stop="pick(questionIndex, optionIndex)"
              @mouseenter="questionIndex === index && (focused = optionIndex)"
            >
              <span class="chat-question-key">
                <Check v-if="question.multiSelect && isSelected(question, option.label)" :size="11" />
                <template v-else>{{ optionIndex + 1 }}</template>
              </span>
              <span class="chat-question-option-text">
                <b>{{ option.label }}</b>
                <small v-if="option.description">{{ option.description }}</small>
              </span>
            </button>
          </li>
        </ul>
        <textarea
          v-if="question.allowCustom"
          :ref="(el) => setCustomEl(question.id, el)"
          class="chat-question-custom"
          rows="1"
          :value="answers[question.id]?.custom ?? ''"
          :placeholder="question.options.length ? t('chat.question.customWithOptions') : t('chat.question.customOnly')"
          @input="onCustomInput(question, $event)"
          @focus="index = questionIndex; focused = question.options.length"
        />
      </div>
    </div>
    <footer v-show="!collapsed" class="chat-question-foot">
      <span class="chat-question-hint">
        <template v-if="current.options.length">{{ t("chat.question.hintPick", { range: `1–${Math.min(current.options.length, 9)}` }) }}</template>
        <template v-if="multiple">{{ t("chat.question.hintSwitch") }}</template>Enter {{ isLast || answeredCount === total ? t("chat.question.hintEnterSubmit") : t("chat.question.hintEnterNext") }}{{ t("chat.question.hintEsc") }}
      </span>
      <div class="chat-question-actions">
        <UiButton variant="ghost" size="small" @click="emit('skip')">{{ t("chat.question.skip") }}</UiButton>
        <UiButton
          class="primary"
          size="small"
          :title="t('chat.question.submitShortcut', { mod: submitMod })"
          :disabled="!answeredCount"
          @click="submit"
        >{{ multiple ? t("chat.question.submitProgress", { answered: answeredCount, total }) : t("chat.question.submit") }}</UiButton>
      </div>
    </footer>
  </section>
</template>
