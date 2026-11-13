<script setup lang="ts">
import { ref } from "vue";
import { useRouter } from "vue-router";
import type { FeishuSendView } from "../../domain/types";
import { t } from "../../i18n";
import * as backend from "../../services/backend";
import { useAppStore } from "../../stores/app";
import UiButton from "../ui/UiButton.vue";

const props = defineProps<{
  sends: FeishuSendView[];
  sessionId: string;
}>();

const store = useAppStore();
const router = useRouter();
const pendingIds = ref(new Set<string>());

function openInbox(sessionId: string) {
  void router.push({ path: "/inbox", query: { session: sessionId } });
}

async function resend(id: string) {
  const send = props.sends.find((item) => item.id === id);
  if (!send?.resendable || pendingIds.value.has(id)) return;
  pendingIds.value = new Set([...pendingIds.value, id]);
  try {
    await backend.resendFeishuSend(id);
    await store.loadMessages(props.sessionId);
  } catch (cause) {
    store.reportComposerNotice(cause instanceof Error ? cause.message : String(cause));
  } finally {
    const next = new Set(pendingIds.value);
    next.delete(id);
    pendingIds.value = next;
  }
}
</script>

<template>
  <article
    v-for="send in sends"
    :key="send.id"
    class="plan-artifact-card"
    :data-tone="send.status === 'sent' ? 'ok' : 'fail'"
  >
    <div class="plan-artifact-head">
      <b class="plan-artifact-title">{{ t("chat.feishu.sentTo", { name: send.label }) }}</b>
      <span class="plan-artifact-status">{{ send.installationName }}</span>
    </div>
    <p class="plan-artifact-meta">{{ send.text }}</p>
    <p v-if="send.status !== 'sent' && send.error" class="plan-artifact-meta">{{ send.error }}</p>
    <div class="feishu-send-actions">
      <UiButton
        v-if="send.inboxSessionId"
        type="button"
        variant="ghost"
        size="small"
        @click="openInbox(send.inboxSessionId)"
      >{{ t("chat.feishu.viewInbox") }}</UiButton>
      <UiButton
        v-if="send.status !== 'sent'"
        type="button"
        variant="ghost"
        size="small"
        :disabled="!send.resendable || pendingIds.has(send.id)"
        @click="resend(send.id)"
      >{{ t("chat.feishu.resend") }}</UiButton>
      <span v-if="send.revoked" class="plan-artifact-meta">{{ t("chat.feishu.revoked") }}</span>
    </div>
  </article>
</template>

<style scoped>
.plan-artifact-head {
  cursor: default;
}
.feishu-send-actions {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
}
.plan-artifact-meta {
  white-space: pre-wrap;
}
</style>
