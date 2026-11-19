<script setup lang="ts">
import { computed } from "vue";
import { useRoute } from "vue-router";
import WorkspaceWorkbench from "../components/WorkspaceWorkbench.vue";

defineOptions({ name: "ProjectWorkspaceView" });

const route = useRoute();
/** keep-alive 共用同一实例，必须跟路由走，不能在 setup 里一次性读死。 */
const projectId = computed(() =>
  typeof route.params.projectId === "string" ? route.params.projectId : "",
);
</script>

<template>
  <WorkspaceWorkbench :project-id="projectId" />
</template>
