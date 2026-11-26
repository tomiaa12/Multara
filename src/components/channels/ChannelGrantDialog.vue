<script setup lang="ts">
import { computed } from "vue";
import { X } from "@lucide/vue";
import type { ChannelGrant } from "../../domain/channel-grant";
import { APP_GRANT_TOGGLES, channelPermissionKeys } from "../../domain/channel-grant";
import type { ChannelType } from "../../domain/types";
import { t } from "../../i18n";
import { channelSourceLabel } from "../chat/message-speaker";
import ChatTargetSelect from "../ChatTargetSelect.vue";
import UiButton from "../ui/UiButton.vue";
import UiModal from "../ui/UiModal.vue";
import UiSelect from "../ui/UiSelect.vue";

const props = defineProps<{
  title: string;
  channelType: ChannelType;
  grant: ChannelGrant;
  showTarget?: boolean;
  targetValue?: string;
  projects: Array<{ id: string; name: string }>;
}>();

const emit = defineEmits<{
  close: [];
  save: [grant: ChannelGrant, targetValue: string];
  "update:grant": [grant: ChannelGrant];
  "update:targetValue": [value: string];
}>();

const projectOptions = computed(() => props.projects.map((project) => ({ value: project.id, label: project.name })));

function patch(next: Partial<ChannelGrant>) {
  emit("update:grant", { ...props.grant, ...next });
}

function toggleApp(key: (typeof APP_GRANT_TOGGLES)[number]["key"]) {
  patch({ [key]: !props.grant[key] });
}

function toggleChannel(key: string) {
  patch({ channel: { ...props.grant.channel, [key]: props.grant.channel[key] === false } });
}

function onProjects(value: unknown) {
  const ids = Array.isArray(value) ? value.map(String) : [];
  patch({ projectIds: ids });
}
</script>

<template>
  <UiModal @click.self="emit('close')">
    <form class="modal domain-modal" @submit.prevent="emit('save', grant, targetValue ?? '')">
      <header class="modal-heading">
        <h2>{{ title }}</h2>
        <UiButton type="button" variant="ghost" size="icon" :tip="t('common.close')" @click="emit('close')">
          <X :size="16" />
        </UiButton>
      </header>
      <div class="modal-body">
        <label v-if="showTarget">
          {{ t("channels.defaultTarget") }}
          <small class="channel-hint">{{ t("channels.defaultTargetHint") }}</small>
          <ChatTargetSelect
            :model-value="targetValue ?? ''"
            :placeholder="t('channels.userTargetNone')"
            :none-label="t('channels.userTargetNone')"
            :aria-label="t('channels.defaultTarget')"
            @update:model-value="emit('update:targetValue', String($event ?? ''))"
          />
        </label>
        <section>
          <h3>{{ t("channels.perm.app") }}</h3>
          <label v-for="item in APP_GRANT_TOGGLES" :key="item.key">
            <input class="ui-checkbox" type="checkbox" :checked="grant[item.key]" @change="toggleApp(item.key)" />
            <span>{{ t(item.labelKey) }}</span>
          </label>
        </section>
        <section>
          <h3>{{ t("channels.perm.projects") }}</h3>
          <label>
            <input
              class="ui-checkbox"
              type="checkbox"
              :checked="grant.projectsUnrestricted"
              @change="patch({ projectsUnrestricted: !grant.projectsUnrestricted })"
            />
            <span>{{ t("channels.perm.unrestricted") }}</span>
          </label>
          <UiSelect
            v-if="!grant.projectsUnrestricted"
            :model-value="grant.projectIds"
            multiple
            searchable
            clearable
            :placeholder="t('channels.perm.pickProjects')"
            :aria-label="t('channels.perm.projects')"
            :options="projectOptions"
            @update:model-value="onProjects"
          />
        </section>
        <section>
          <h3>{{ t("channels.perm.channel", { name: channelSourceLabel(channelType) }) }}</h3>
          <label v-for="item in channelPermissionKeys(channelType)" :key="item.key">
            <input
              class="ui-checkbox"
              type="checkbox"
              :checked="grant.channel[item.key] !== false"
              @change="toggleChannel(item.key)"
            />
            <span>{{ t(item.labelKey) }}</span>
          </label>
        </section>
      </div>
      <div class="modal-actions">
        <UiButton type="button" @click="emit('close')">{{ t("common.cancel") }}</UiButton>
        <UiButton type="submit" variant="primary">{{ t("common.save") }}</UiButton>
      </div>
    </form>
  </UiModal>
</template>

<style scoped>
.modal-body h3 {
  margin: 4px 0 0;
  font-size: var(--font-size-sm);
}
.modal-body section {
  display: grid;
  gap: 6px;
}
.channel-hint {
  font-weight: var(--font-weight-regular);
  color: var(--color-text-tertiary);
}
</style>
