<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import AddWorkspaceModal from "../components/dashboard/AddWorkspaceModal.vue";
import WorkspaceWorkbench from "../components/WorkspaceWorkbench.vue";
import * as backend from "../services/backend";
import { useAppStore } from "../stores/app";
import {
  forgetRestoreHints,
  pendingRestoreHints,
  rememberRestoreHints,
  type WorkspaceRestoreHint,
} from "./dashboard/workspace-restore";

defineOptions({ name: "ProjectWorkspaceView" });

const route = useRoute();
const router = useRouter();
const store = useAppStore();
/** keep-alive 共用同一实例，必须跟路由走，不能在 setup 里一次性读死。 */
const projectId = computed(() =>
  typeof route.params.projectId === "string" ? route.params.projectId : "",
);
const ready = ref(false);
const blocked = ref(false);
const paths = ref<string[]>([]);
const preferred = ref<Record<string, string>>({});

let ticket = 0;

function preferredOf(hints: WorkspaceRestoreHint[]) {
  const map: Record<string, string> = {};
  for (const hint of hints) {
    const branch = hint.upstream || hint.branch;
    if (branch) map[hint.sourcePath] = branch;
  }
  return map;
}

function showRestore(hints: WorkspaceRestoreHint[]) {
  paths.value = [...new Set(hints.map((hint) => hint.sourcePath))];
  preferred.value = preferredOf(hints);
  blocked.value = hints.length > 0;
}

watch(
  projectId,
  async (id) => {
    const current = ++ticket;
    if (!id) {
      blocked.value = false;
      ready.value = true;
      return;
    }
    const hints = pendingRestoreHints(id, store.data?.workspaces ?? []);
    showRestore(hints);
    ready.value = true;
    let gaps: backend.CheckoutGap[] = [];
    try {
      gaps = await backend.checkoutGaps(id);
    } catch {
      gaps = [];
    }
    if (current !== ticket) return;
    if (!hints.length && !gaps.length) {
      forgetRestoreHints(id);
      blocked.value = false;
      return;
    }
    rememberRestoreHints([
      ...hints,
      ...gaps.map((gap) => ({
        projectId: id,
        sourcePath: gap.sourcePath,
        branch: gap.branch,
        upstream: hints.find((hint) => hint.sourcePath === gap.sourcePath)?.upstream ?? null,
      })),
    ]);
    for (const gap of gaps) {
      try {
        await store.detachWorkspace(gap.workspaceId, {
          workspaceId: gap.workspaceId,
          deleteLocalBranch: false,
          deleteRemoteBranch: false,
          deleteCopyDirectory: false,
        });
      } catch {
        if (current !== ticket) return;
        void router.replace("/projects");
        return;
      }
    }
    if (current !== ticket) return;
    showRestore(pendingRestoreHints(id, store.data?.workspaces ?? []));
  },
  { immediate: true },
);

function onAdded() {
  forgetRestoreHints(projectId.value);
  blocked.value = false;
}

function onClose() {
  void router.replace("/projects");
}
</script>

<template>
  <AddWorkspaceModal
    v-if="ready && blocked"
    :project-id="projectId"
    :initial-paths="paths"
    :preferred-branches="preferred"
    @close="onClose"
    @added="onAdded"
  />
  <WorkspaceWorkbench v-else-if="ready" :project-id="projectId" />
</template>
