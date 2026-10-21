<script setup lang="ts">
import { useRoute } from "vue-router";
import WorkspaceWorkbench from "../components/WorkspaceWorkbench.vue";

defineOptions({ name: "ProjectWorkspaceView" });

const route = useRoute();
const projectId = typeof route.params.projectId === "string" ? route.params.projectId : "";
</script>

<template>
  <WorkspaceWorkbench :project-id="projectId" />
</template>
