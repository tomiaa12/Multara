import { createRouter, createWebHashHistory } from "vue-router";
import { confirmLeaveUnsavedEditors } from "./components/workbench/unsaved-editor";
import { installBreadcrumbs } from "./stores/breadcrumbs";
import { lastAppRoute } from "./views/dashboard/last-app-route";
import DashboardView from "./views/DashboardView.vue";

const CollectionView = () => import("./views/CollectionView.vue");
const ProjectWorkspaceView = () => import("./views/ProjectWorkspaceView.vue");

export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: "/", redirect: () => lastAppRoute("/projects") },
    { path: "/projects", component: DashboardView, meta: { titleKey: "nav.projects" } },
    {
      path: "/projects/:projectId",
      component: ProjectWorkspaceView,
      // parent：直接打开详情页（或从菜单进入）时面包屑的上一级；crumb：面包屑里的短名称
      meta: { titleKey: "route.workbench", crumbKey: "route.workbenchCrumb", parent: "/projects" },
    },
    { path: "/chat", redirect: "/tasks" },
    { path: "/inbox", component: () => import("./views/InboxView.vue"), meta: { titleKey: "nav.inbox" } },
    { path: "/tasks", component: () => import("./views/TasksView.vue"), meta: { titleKey: "nav.tasks" } },
    {
      path: "/tasks/:taskId",
      component: () => import("./views/TaskDetailView.vue"),
      meta: { titleKey: "route.taskDetail", parent: "/tasks" },
    },
    { path: "/workflows", component: () => import("./views/workflows/WorkflowsView.vue"), meta: { titleKey: "nav.workflows" } },
    {
      path: "/workflows/:workflowId",
      component: () => import("./views/workflows/WorkflowEditorView.vue"),
      meta: { titleKey: "route.workflowEditor", parent: "/workflows" },
    },
    {
      path: "/workflows/:workflowId/runs",
      component: () => import("./views/workflows/WorkflowRunsView.vue"),
      // 面包屑 parent 不能带参数；实际走过的路径由 breadcrumbs 记录
      meta: { titleKey: "route.workflowRuns", parent: "/workflows" },
    },
    {
      path: "/workflows/:workflowId/runs/:runId",
      component: () => import("./views/workflows/WorkflowRunView.vue"),
      meta: { titleKey: "route.workflowRun", parent: "/workflows" },
    },
    { path: "/automation", component: () => import("./views/AutomationView.vue"), meta: { titleKey: "nav.automation" } },
    { path: "/channels", component: () => import("./views/ChannelsView.vue"), meta: { titleKey: "nav.channels" } },
    { path: "/remote", component: () => import("./views/remote/RemoteView.vue"), meta: { titleKey: "nav.remote" } },
    { path: "/agents", component: CollectionView, meta: { titleKey: "nav.agents", kind: "agents" } },
    {
      path: "/agents/:agentId",
      component: () => import("./views/AgentDetailView.vue"),
      meta: { titleKey: "route.agentDetail", parent: "/agents" },
    },
    { path: "/squads", component: CollectionView, meta: { titleKey: "nav.squads", kind: "squads" } },
    {
      path: "/squads/:squadId",
      component: () => import("./views/SquadDetailView.vue"),
      meta: { titleKey: "route.squadDetail", parent: "/squads" },
    },
    { path: "/stats", component: () => import("./views/StatsView.vue"), meta: { titleKey: "nav.stats" } },
    { path: "/runtimes", component: CollectionView, meta: { titleKey: "nav.runtimes", kind: "runtimes" } },
    { path: "/skills", component: () => import("./views/SkillsView.vue"), meta: { titleKey: "nav.skills" } },
    {
      path: "/skills/:resourceId",
      component: () => import("./views/SkillDetailView.vue"),
      meta: { titleKey: "route.skillDetail", parent: "/skills" },
    },
    { path: "/mcp", component: () => import("./views/McpView.vue"), meta: { titleKey: "nav.mcp" } },
    { path: "/settings", component: () => import("./views/SettingsView.vue"), meta: { titleKey: "nav.settings", keepTrail: true } },
    { path: "/preview", component: () => import("./views/ChatPreviewView.vue"), meta: { titleKey: "nav.preview" } },
  ],
});

installBreadcrumbs(router);

export function isWorkbenchPath(path: string) {
  return path.startsWith("/projects/") || path === "/preview";
}

router.beforeEach(async (to, from) => {
  if (to.path === "/preview" && !import.meta.env.DEV) return "/projects";
  if (to.path === from.path) return true;
  if (
    !isWorkbenchPath(from.path)
    || (isWorkbenchPath(to.path) && to.params.projectId === from.params.projectId)
  ) {
    return true;
  }
  return confirmLeaveUnsavedEditors();
});
