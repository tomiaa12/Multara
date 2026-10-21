# Patterns

## 两套密度

产品已经形成两套，而不是一套「平均」样式。

**Page**：`DashboardView` `SettingsView` `CollectionView` `TasksView` 详情页。

- 容器 `.page`：宽 `min(1180px, calc(100% - 48px))`，padding `28px 0 60px`
- 页头 `.page-header`：左标题右操作，gap 24，下边距 24
- `h1` 22 + `p` 13 muted
- 主按钮在页头右侧

**Workbench**：`ProjectWorkspaceView` / `WorkspaceWorkbench`。

- 占满 `main`，`overflow: hidden`
- 无 `.page` 外边距
- 控件更小、行更密

新 UI 先判断落在哪一层。跨层复用组件可以，但要改 size（page 用 default，workbench 工具条用 icon 22）。

## 表单

```vue
<label>名称<UiInput v-model="name" required /></label>
<label class="check-row"><UiInput v-model="flag" type="checkbox" />说明</label>
<div class="inline-field"><UiInput v-model="path" /><UiButton>检测</UiButton></div>
<small>辅助说明</small>
```

多字段弹窗加 `.modal-heading`（标题 + 关闭）和 `.form-row`。领域多选盒用 `fieldset`（已有 `.domain-modal fieldset`）。

## 空 / 错 / 成功

| 模式 | Class | 何时 |
|---|---|---|
| 页级空 | `.empty-project` / `.placeholder` / `.req-empty` | 无数据引导创建 |
| 页级加载 / 轻空 | `.empty` | 一句话即可 |
| 树空 | `.tree-empty` | 工作台 |
| 错误块 | `.error-card` | 加载失败、顶部错误 |
| 表单错 | `.form-error` | 弹窗校验 |
| SCM 错 | `.scm-error` | Git 面板 |
| 保存成功 | `.save-success` | 设置 / 详情页 |

## 重构项（已发现不一致，本次不改代码）

记录下来供以后收敛。新代码按「目标」写，不要继续复制「现状」里的散乱值。

| ID | 现状 | 目标 | 严重度 |
|---|---|---|---|
| I1 | 大量硬编码 `#1b1b1b` `#222` `#303030` 等 | 新代码改用 token | 高 |
| I2 | `button` 全局样式与 `.ui-button` 不完全相同 | 新按钮只用 `UiButton` | 高 |
| I3 | `class="primary"` 与 `variant="primary"` 并存 | 新代码只用 variant | 中 |
| I4 | radius 出现 4/5/6/7/8/9/10/11/12 | 新代码只用 token 表 | 高 |
| I5 | spacing 出现 3/5/7/9/11/13/15 | 新代码用 spacing scale | 中 |
| I6 | 浅色主题只覆盖部分硬编码，工作台大量深色写死 | 新代码用 token，旧页以后再迁 | 高 |
| I7 | `StopAiDialog` 使用不存在的 `.modal-card` | 应改为 `.modal` | 中 |
| I8 | danger 有 `#e78080` 与 `#ef9292` | 新代码 `--danger` | 低 |
| I9 | 卡片 hover 有的 `translateY(-1px)` 有的没有 | 不要扩散位移 | 低 |
| I10 | `.project-symbol` 渐变 | 保留，不扩散 | 低 |
| I11 | `floating-chat` radius 12、FAB 48 | 保留例外，不扩散 | 低 |
| I12 | 可点击卡片部分 `outline: none` | 新卡片提供 focus-visible 边框或 ring | 中 |
| I13 | 生产 token 与 `prototype/css/tokens.css` 两套 | 忽略 prototype | 高 |
| I14 | 图标 11–17 连续散落 | 新代码按 xs/sm/md/lg | 中 |
| I15 | 任务卡片 radius 8、资源卡 7 | 新卡片 9 | 低 |
| I16 | 无 Toast / Tooltip 却有人可能新造 | 继续用 title / save-success | 中 |
| I17 | 输入 7px radius vs 按钮 6px | 维持各自已有值 | 低 |
| I18 | 右键 hover 蓝 vs 其他 hover 灰 | 维持菜单特例 | 低 |

## Prototype

`prototype/` 是早期 IDE 视觉探索（`--accent: #3d8bfd`、`--radius: 6`、`--radius-lg: 10`、Segoe UI）。**不是**当前产品。禁止：

- `@import` prototype CSS
- 把 prototype 的 `.btn` 体系带进 `src/`
- 用 prototype 的色板覆盖 `--blue`
