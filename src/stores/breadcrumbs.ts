import { shallowRef } from "vue";
import type { RouteLocationNormalizedLoaded, RouteLocationRaw, Router } from "vue-router";
import { t } from "../i18n";

export interface Crumb {
  path: string;
  fullPath: string;
  label: string;
}

/**
 * 全局面包屑：记录用户实际走过的路径，而不是固定的路由层级。
 * - 点侧边栏菜单 / 进入一级页面：从该页重新开始（详情页带上路由 meta.parent 作为上一级）
 * - 页面内跳转：追加一级，如「智能体 > 智能体详情 > 任务详情」
 * - 回到路径中已有的页面（点面包屑、返回、浏览器后退）：截断到那一级
 */
export const crumbs = shallowRef<Crumb[]>([]);

/** 路径过深时只保留最近几级，避免标题栏被挤满 */
const MAX_DEPTH = 6;
let resetOnNext = false;

function toCrumb(route: RouteLocationNormalizedLoaded | ReturnType<Router["resolve"]>): Crumb {
  return {
    path: route.path,
    fullPath: route.fullPath,
    label: t(String(route.meta.crumbKey ?? route.meta.titleKey ?? route.meta.crumb ?? route.meta.title ?? route.path)),
  };
}

function staticTrail(router: Router, route: RouteLocationNormalizedLoaded): Crumb[] {
  const parent = route.meta.parent as string | undefined;
  return parent ? [toCrumb(router.resolve(parent)), toCrumb(route)] : [toCrumb(route)];
}

export function installBreadcrumbs(router: Router) {
  router.afterEach((to, _from, failure) => {
    const reset = resetOnNext;
    resetOnNext = false;
    if (failure) return;
    // keepTrail：从页面内打开的一级页（如工作台里的 Git 设置）要能按面包屑返回
    if (reset || (!to.meta.parent && !to.meta.keepTrail) || !crumbs.value.length) {
      crumbs.value = staticTrail(router, to);
      return;
    }
    const index = crumbs.value.findIndex((crumb) => crumb.path === to.path);
    const base = index >= 0 ? crumbs.value.slice(0, index) : crumbs.value;
    crumbs.value = [...base, toCrumb(to)].slice(-MAX_DEPTH);
  });
}

/** 侧边栏菜单点击：本次导航从该菜单重新开始计算面包屑 */
export function markMenuNavigation(router: Router, to: RouteLocationRaw) {
  const target = router.resolve(to);
  const current = router.currentRoute.value;
  // 点的就是当前页时不会触发导航，直接在这里重置
  if (target.path === current.path) {
    crumbs.value = staticTrail(router, current);
    return;
  }
  resetOnNext = true;
}

export function backTarget() {
  return crumbs.value.length > 1 ? crumbs.value[crumbs.value.length - 2] : null;
}
