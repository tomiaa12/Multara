<script setup lang="ts">
import { onMounted, ref } from "vue";
import { X } from "@lucide/vue";
import type { Runtime } from "../domain/types";
import { t } from "../i18n";
import { getRuntimeAccountStatus, type RuntimeAccountStatus } from "../services/backend";
import RuntimeAccountGroup from "./RuntimeAccountGroup.vue";

const props = defineProps<{ runtime: Runtime; status?: RuntimeAccountStatus }>();
const emit = defineEmits<{ update: [status: RuntimeAccountStatus]; login: []; close: [] }>();

const error = ref("");

// 列表上的状态可能没读钥匙串，打开弹窗时读一次完整的（顺带收录当前登录）
onMounted(async () => {
  try {
    emit("update", await getRuntimeAccountStatus(props.runtime.id));
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
  }
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
