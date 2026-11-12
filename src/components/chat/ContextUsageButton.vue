<script setup lang="ts">
import { computed, inject, onBeforeUnmount, ref, watch } from "vue";
import { ChartNoAxesColumn, X } from "@lucide/vue";
import AgentAvatar from "../AgentAvatar.vue";
import { bindDismissableLayer } from "../ui/dismissable-layer";
import { t } from "../../i18n";
import { useAppStore } from "../../stores/app";
import {
  chatMessagesKey,
  estimateContextUsage,
  formatTokenCount,
  listContextAgentIds,
} from "./context-usage";

const props = withDefaults(defineProps<{
  placement?: "up" | "down";
}>(), {
  placement: "up",
});

const store = useAppStore();
const messages = inject(chatMessagesKey, null);
const open = ref(false);
const root = ref<HTMLElement | null>(null);
const agentFilter = ref<string | null>(null);

const agentIds = computed(() => listContextAgentIds(messages?.value ?? []));
const agents = computed(() => {
  const list = store.data?.agents ?? [];
  return agentIds.value.map((id) => {
    const agent = list.find((item) => item.id === id);
    return {
      id,
      name: agent?.name || id,
      avatar: agent?.avatar || "",
    };
  });
});

watch(agentIds, (ids) => {
  if (agentFilter.value && !ids.includes(agentFilter.value)) agentFilter.value = null;
});

const usage = computed(() => estimateContextUsage(messages?.value ?? [], agentFilter.value));
const barTotal = computed(() => usage.value.slices.reduce((sum, slice) => sum + slice.tokens, 0) || 1);

let unbindDismiss: (() => void) | undefined;

watch(open, (value) => {
  unbindDismiss?.();
  unbindDismiss = undefined;
  if (!value) return;
  unbindDismiss = bindDismissableLayer({
    contains: (target) => Boolean(root.value?.contains(target)),
    onDismiss: () => {
      open.value = false;
    },
  });
});

onBeforeUnmount(() => {
  unbindDismiss?.();
});
</script>

<template>
  <div ref="root" class="context-usage">
    <UiTooltip :content="t('chat.usage.open')">
      <button
        type="button"
        class="context-usage-button"
        :aria-expanded="open"
        :aria-label="t('chat.usage.open')"
        @click.stop="open = !open"
      >
        <ChartNoAxesColumn :size="16" aria-hidden="true" />
      </button>
    </UiTooltip>
    <div
      v-if="open"
      class="context-usage-popover"
      :class="props.placement === 'down' ? 'is-down' : 'is-up'"
      role="dialog"
      :aria-label="t('chat.usage.title')"
    >
      <header>
        <strong>{{ t("chat.usage.title") }}</strong>
        <button
          type="button"
          class="context-usage-close"
          :aria-label="t('common.close')"
          @click.stop="open = false"
        >
          <X :size="14" />
        </button>
      </header>

      <div
        v-if="agents.length"
        class="context-usage-agents"
        role="tablist"
        :aria-label="t('chat.usage.agents')"
      >
        <button
          type="button"
          role="tab"
          class="context-usage-agent"
          :class="{ active: !agentFilter }"
          :aria-selected="!agentFilter"
          @click.stop="agentFilter = null"
        >
          {{ t("chat.usage.all") }}
        </button>
        <button
          v-for="agent in agents"
          :key="agent.id"
          type="button"
          role="tab"
          class="context-usage-agent"
          :class="{ active: agentFilter === agent.id }"
          :aria-selected="agentFilter === agent.id"
          :title="agent.name"
          @click.stop="agentFilter = agent.id"
        >
          <AgentAvatar :value="agent.avatar" :name="agent.name" :size="18" />
          <span>{{ agent.name }}</span>
        </button>
      </div>

      <div class="context-usage-bar" aria-hidden="true">
        <span
          v-for="slice in usage.slices"
          :key="slice.id"
          :style="{
            width: `${(slice.tokens / barTotal) * 100}%`,
            background: slice.color,
          }"
        />
      </div>
      <ul>
        <li v-for="slice in usage.slices" :key="slice.id">
          <i :style="{ background: slice.color }" />
          <span>{{ slice.label }}</span>
          <b>{{ formatTokenCount(slice.tokens) }}</b>
        </li>
      </ul>
    </div>
  </div>
</template>
