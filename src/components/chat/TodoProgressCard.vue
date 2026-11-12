<script setup lang="ts">
import Codicon from "../ui/Codicon.vue";
import type { PlanTodo } from "./timeline-display";

defineProps<{ todos: PlanTodo[] }>();

function tone(status: string) {
  if (status === "completed") return "ok";
  if (status === "in_progress") return "run";
  if (status === "cancelled") return "muted";
  return "wait";
}

function label(status: string) {
  if (status === "completed") return "已完成";
  if (status === "in_progress") return "进行中";
  if (status === "cancelled") return "取消";
  return "待办";
}
</script>

<template>
  <section class="todo-progress-card">
    <header>待办</header>
    <ul>
      <li v-for="todo in todos" :key="todo.id" :data-tone="tone(todo.status)">
        <Codicon v-if="todo.status === 'completed'" name="check" :size="12" />
        <i v-else />
        <span>{{ todo.text }}</span>
        <em>{{ label(todo.status) }}</em>
      </li>
    </ul>
  </section>
</template>
