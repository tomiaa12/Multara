<script setup lang="ts">
/** 需求列表行：名称 + 谁在工作（执行模式侧栏 / 工作台切换需求下拉共用）。 */
import AgentLiveStatus from "../AgentLiveStatus.vue";
import type { AvatarStackItem } from "../ui/AvatarStack.vue";
import { t } from "../../i18n";

withDefaults(defineProps<{
  name: string;
  sessionIds: string[];
  workers?: AvatarStackItem[];
  idleLabel?: string;
  /** 头像边长；侧栏 / 下拉 16、表格 24 */
  size?: number;
  max?: number;
  /** 额外角标，例如「已归档」 */
  badge?: string;
}>(), {
  workers: () => [],
  idleLabel: "",
  size: 16,
  max: 3,
  badge: "",
});
</script>

<template>
  <span class="req-project-row">
    <span class="req-project-row-title">
      <strong class="req-project-row-name">{{ name }}</strong>
      <span v-if="badge" class="ds-badge">{{ badge }}</span>
    </span>
    <span class="req-project-row-workers">
      <AgentLiveStatus
        :session-ids="sessionIds"
        :fallback="workers"
        :idle-label="idleLabel"
        :max="max"
        :size="size"
        :dialog-title="t('dashboard.col.workers')"
      />
    </span>
  </span>
</template>
