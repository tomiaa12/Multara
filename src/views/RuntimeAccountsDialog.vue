<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { X } from "@lucide/vue";
import type { Runtime } from "../domain/types";
import { t } from "../i18n";
import {
  getRuntimeAccountStatus,
  probeRuntimeUsage,
  type RuntimeAccountStatus,
  type RuntimeUsage,
} from "../services/backend";
import RuntimeAccountGroup from "./RuntimeAccountGroup.vue";
import RuntimeUsageBars from "./RuntimeUsageBars.vue";
import { supportsRuntimeUsage } from "./runtime-usages";

const props = defineProps<{ runtime: Runtime; status?: RuntimeAccountStatus }>();
const emit = defineEmits<{ update: [status: RuntimeAccountStatus]; login: []; close: [] }>();

const error = ref("");
const usage = ref<RuntimeUsage | null>(null);
const usageLoading = ref(false);
const showUsage = computed(() => supportsRuntimeUsage(props.runtime));

async function loadUsage() {
  if (!showUsage.value) return;
  usageLoading.value = true;
  try {
    usage.value = await probeRuntimeUsage(props.runtime.id);
  } catch (cause) {
    usage.value = {
      runtimeId: props.runtime.id,
      supported: true,
      checkedAt: new Date().toISOString(),
      windows: [],
      unavailable: {
        reason: "probeFailed",
        message: cause instanceof Error ? cause.message : String(cause),
      },
    };
  } finally {
    usageLoading.value = false;
  }
}

// 列表上的状态可能没读钥匙串，打开弹窗时读一次完整的（顺带收录当前登录）
onMounted(async () => {
  try {
    emit("update", await getRuntimeAccountStatus(props.runtime.id));
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  }
  void loadUsage();
});
</script>

<template>
  <UiModal @click.self="emit('close')">
    <section class="modal domain-modal">
      <header class="modal-heading">
        <h2>{{ t("runtimeAccount.title", { name: runtime.name }) }}</h2>
        <UiButton class="icon-button" @click="emit('close')"><X :size="16" /></UiButton>
      </header>
      <p v-if="error" class="form-error">{{ error }}</p>
      <RuntimeUsageBars
        v-if="showUsage"
        class="runtime-account-usage"
        show-refresh
        :usage="usage"
        :loading="usageLoading"
        @refresh="loadUsage"
      />
      <RuntimeAccountGroup
        v-if="status"
        :runtime="runtime"
        :status="status"
        @update="emit('update', $event)"
        @login="emit('login')"
      />
      <p v-else-if="!error" class="skill-create-hint">{{ t("runtimeAccount.loading") }}</p>
    </section>
  </UiModal>
</template>

<style scoped>
.runtime-account-usage {
  margin-bottom: 12px;
  padding-bottom: 12px;
  border-bottom: 1px solid var(--color-border);
}
</style>
