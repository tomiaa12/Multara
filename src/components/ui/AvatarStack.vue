<script setup lang="ts">
import { computed, ref } from "vue";
import { X } from "@lucide/vue";
import AgentAvatar from "../AgentAvatar.vue";
import { t } from "../../i18n";

export type AvatarStackItem = {
  key: string;
  name: string;
  /** 交给 AgentAvatar 的 value：图片 / emoji / 空 */
  image?: string;
  title?: string;
};

const props = withDefaults(
  defineProps<{
    items: AvatarStackItem[];
    /** 可见头像数，超出显示 +N */
    max?: number;
    size?: number;
    dialogTitle?: string;
  }>(),
  {
    max: 3,
    size: 24,
    dialogTitle: "",
  },
);

const showAll = ref(false);

const visible = computed(() => props.items.slice(0, Math.max(1, props.max)));
const overflow = computed(() => Math.max(0, props.items.length - visible.value.length));
const title = computed(() => props.dialogTitle || t("avatarStack.dialogTitle"));

function openOverflow(event: MouseEvent) {
  event.stopPropagation();
  showAll.value = true;
}
</script>

<template>
  <span v-if="items.length" class="ds-avatar-stack" @click.stop>
    <AgentAvatar
      v-for="item in visible"
      :key="item.key"
      class="ds-avatar"
      :value="item.image || ''"
      :name="item.name"
      :size="size"
    />
    <button
      v-if="overflow"
      type="button"
      class="ds-avatar ds-avatar-stack-more"
      :style="{ width: `${size}px`, height: `${size}px`, fontSize: `${Math.max(10, size * 0.42)}px` }"
      :title="t('avatarStack.moreTip', { n: overflow })"
      @click="openOverflow"
    >
      +{{ overflow }}
    </button>
  </span>
  <span v-else class="avatar-stack-empty">—</span>

  <UiModal v-if="showAll" @click.self="showAll = false">
    <div class="modal avatar-stack-dialog" role="dialog" aria-modal="true">
      <header class="modal-heading">
        <h2>{{ title }}</h2>
        <UiButton class="icon-button" size="icon" :tip="t('common.close')" @click="showAll = false">
          <X :size="15" />
        </UiButton>
      </header>
      <div class="modal-body vs-scroll">
        <ul class="avatar-stack-list">
          <li v-for="item in items" :key="item.key">
            <AgentAvatar :value="item.image || ''" :name="item.name" :size="28" />
            <div class="avatar-stack-meta">
              <strong>{{ item.name }}</strong>
              <small v-if="item.title && item.title !== item.name">{{ item.title }}</small>
            </div>
          </li>
        </ul>
      </div>
      <div class="modal-actions">
        <UiButton @click="showAll = false">{{ t("common.close") }}</UiButton>
      </div>
    </div>
  </UiModal>
</template>
