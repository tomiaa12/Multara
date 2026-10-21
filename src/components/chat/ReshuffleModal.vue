<script setup lang="ts">
import { ref } from "vue";
import { reshuffleHintText } from "../../stores/reask";

const props = defineProps<{
  afterCount: number;
}>();

const emit = defineEmits<{
  confirm: [rollback: boolean];
  cancel: [];
}>();

const choice = ref<"keep" | "rollback">("keep");
</script>

<template>
  <div class="modal-backdrop" @click.self="emit('cancel')">
    <div class="modal" role="dialog" aria-modal="true" aria-labelledby="reshuffle-title">
      <h2 id="reshuffle-title">重新提问</h2>
      <p>{{ reshuffleHintText(afterCount) }}</p>
      <p>文件状态如何处理？</p>
      <label class="check-row">
        <input v-model="choice" type="radio" name="reshuffle-files" value="rollback">
        <span>回滚文件 — 恢复到这条消息提问前的代码状态</span>
      </label>
      <label class="check-row">
        <input v-model="choice" type="radio" name="reshuffle-files" value="keep">
        <span>保持现有代码 — 文件不改变，只删除对话记录</span>
      </label>
      <div class="modal-actions">
        <UiButton @click="emit('cancel')">取消</UiButton>
        <UiButton variant="primary" @click="emit('confirm', choice === 'rollback')">确认并发送</UiButton>
      </div>
    </div>
  </div>
</template>
