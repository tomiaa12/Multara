<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { RemoteConnectionAttempt } from "../../domain/remote";
import UiRelativeTime from "../../components/ui/UiRelativeTime.vue";
import { t } from "../../i18n";
import * as remoteApi from "../../services/remote-backend";

const attempts = ref<RemoteConnectionAttempt[]>([]);

async function reload() {
  attempts.value = (await remoteApi.connectionLog().catch(() => [])) ?? [];
}

let stop: UnlistenFn | undefined;
onMounted(async () => {
  await reload();
  stop = await listen("remote-connections-changed", () => void reload());
});
onBeforeUnmount(() => stop?.());
</script>

<template>
  <section class="settings-card">
    <div class="settings-row">
      <div>
        <strong>{{ t("remote.network.log") }}</strong>
        <p>{{ t("remote.network.logHint") }}</p>
      </div>
    </div>
    <p v-if="!attempts.length" class="settings-hint">{{ t("remote.network.logEmpty") }}</p>
    <div v-else class="remote-log">
      <table class="ds-table">
        <thead>
          <tr>
            <th>{{ t("remote.network.time") }}</th>
            <th>IP</th>
            <th>{{ t("remote.network.source") }}</th>
            <th>{{ t("remote.device.name") }}</th>
            <th>{{ t("remote.network.result") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="attempt in attempts" :key="`${attempt.at}-${attempt.ip}`">
            <td class="remote-nowrap"><UiRelativeTime :value="attempt.at" /></td>
            <td><code>{{ attempt.ip }}</code></td>
            <td>{{ attempt.cidr ?? t("remote.network.other") }}</td>
            <td>{{ attempt.device ?? t("remote.network.unpaired") }}</td>
            <td>
              <span class="ds-badge" :data-tone="attempt.allowed ? 'success' : 'error'">
                {{ attempt.allowed ? t("remote.network.allowed") : t("remote.network.rejected") }}
              </span>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </section>
</template>

<style scoped>
.remote-log { overflow: auto; max-height: 320px; }
.remote-log code { font-family: var(--font-family-mono); }
.remote-nowrap { white-space: nowrap; }
</style>
