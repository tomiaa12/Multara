<script setup lang="ts">
import type { TimelineItem } from "../../domain/types";
import { useAppStore } from "../../stores/app";
import { inferToolName, permissionOptions } from "./timeline-display";

const props = defineProps<{ item: TimelineItem; runId?: string | null }>();
const store = useAppStore();
const options = permissionOptions(props.item);
const name = inferToolName(props.item) || props.item.tool || "工具";

function pick(optionId: string) {
  const id = props.item.callId;
  if (!id) return;
  void store.resolvePermission(id, optionId, props.runId);
}
</script>

<template>
  <section class="permission-card">
    <header>需要确认：{{ name }}</header>
    <p v-if="item.status && item.status !== 'pending'">已处理</p>
    <div v-else class="permission-actions">
      <UiButton
        v-for="option in options"
        :key="option.id"
        type="button"
        size="small"
        :variant="option.kind?.toLowerCase().includes('reject') ? 'ghost' : undefined"
        @click="pick(option.id)"
      >{{ option.label }}</UiButton>
    </div>
  </section>
</template>
