---
name: ui-design-system
description: Enforces Multara's existing compact dark-first Tauri desktop UI. Use when adding or changing Vue components, CSS, buttons, inputs, selects, dialogs, sidebars, workbench, chat, file tree, context menus, colors, typography, spacing, radius, shadows, icons, hover/focus/disabled states, empty/error/loading UI, or any visual style.
---

# Multara UI Design System

约束后续 AI 复用现有视觉语言，禁止重新设计。

权威源码（只读提取，不要另起炉灶）：

- Tokens / 全局样式：`src/styles.css` 的 `:root` 与 `[data-theme="light"]`
- 基础组件：`src/components/ui/`（`UiButton` `UiInput` `UiSelect` `UiTextarea` `ContextMenu`）
- 全局注册：`src/main.ts`
- 图标：`@lucide/vue`（UI）、`src/icons.ts` + `FileTypeIcon.vue`（文件类型）
- 原型 `prototype/css/` **不是** 生产规范，禁止按它改产品 UI

`prototype/` 更偏 VS Code 蓝灰；生产是 **#121212 深灰 + #1683ff 蓝**。以生产为准。

## 何时使用

任何新增 / 修改界面、样式、交互状态、图标、弹窗、列表、工作台、聊天面板时，先读本 Skill。

## 核心规则

1. 先复用，再扩展，最后才允许新 variant。禁止为单次需求写一套新视觉。
2. 颜色、间距、圆角、阴影、字号、过渡必须用 `src/styles.css` 里的 token。禁止新写 `#rrggbb` / `13px` / `10px radius`。
3. 按钮只用 `UiButton`，输入只用 `UiInput` / `UiSelect` / `UiTextarea`，右键只用 `ContextMenu`。
4. 这是紧凑型桌面软件，不是 SaaS 网页。默认小控件、高信息密度、可键盘、可右键、可拖拽。
5. 深色是默认主题。浅色只覆盖 token，不要为浅色另写一套组件样式。
6. 禁止引入 Tailwind / shadcn / Naive / Element / Ant Design 或新 icon 库。
7. 禁止玻璃拟态、大面积渐变、过大圆角、过大 padding、Material / Apple / shadcn 风格。
8. 项目符号 `.project-symbol` 的渐变是历史例外，不要扩散。

## 新增 UI 流程

1. 读本文件，再按需读下方文档。
2. 在 `src/components/ui/` 和 `src/styles.css` 找已有组件 / class。
3. 已有能力能覆盖 → 直接复用。
4. 差一点点 → 给现有组件加 variant / size，token 必须已存在。
5. 现有组件无法表达 → 才允许新组件，且必须用 token，并在 `components.md` 登记。
6. 对照相邻页面（同密度：页面 or 工作台），不要混用两套密度。

## 修改 UI 流程

1. 找到当前组件和它用的 class。
2. 对照 Design System 与相邻页面。
3. 只改行为 / 结构，不顺手换颜色、圆角、字号、阴影。
4. 禁止复制一份「更好看」的新样式覆盖旧组件。

## 密度（先选对再写样式）

| 密度 | 场景 | 行高 | 控件高 | 图标 |
|---|---|---|---|---|
| page | 设置、需求、智能体、任务看板、表单弹窗 | 卡片 12–16px padding | Button 34 / Input 36 | 14–16 |
| workbench | 文件树、Git、编辑器页签、会话页签、SCM | 行 22–26 | 图标按钮 22 | 12–13 |

工作台里不要用页面级大卡片。页面里不要用 22px 幽灵图标按钮当主操作。

## 组件选用

| 需求 | 使用 |
|---|---|
| 主操作 | `UiButton variant="primary"` |
| 次操作 | `UiButton`（default） |
| 弱操作 / 工具条 | `UiButton variant="ghost"` |
| 危险 | `UiButton variant="danger"`，破坏性再加 Dialog 确认 |
| 纯图标 | `UiButton size="icon"`，`title` 必填 |
| 页面小按钮 | `UiButton size="small"` |
| 单行输入 | `UiInput` |
| 多行 | `UiTextarea` |
| 少量固定选项 | `UiSelect`（原生 select，不要自造下拉） |
| 开关 | `UiInput type="checkbox"` + `.check-row` |
| 需要确认 | `.modal-backdrop` + `.modal` |
| 短暂成功 | `.save-success` 行内文字，不要做 Toast |
| 悬停说明 | `title`，不要做 Tooltip 组件 |
| 右键 | `ContextMenu` |
| 文件图标 | `FileTypeIcon` |
| 其他图标 | `@lucide/vue` 已有图标 |

没有的组件：Toast、Tooltip、Switch、Radio、自定义 Select、Command Palette。未明确要求时禁止新建。

## 禁止

1. 随意创建新颜色 / 字号 / 间距 / 圆角 / 阴影
2. 硬编码颜色代替 token
3. 为同一功能写第二种 Button / Input 样式
4. 手绘 SVG 图标；或同一功能换不同 Lucide 图标
5. 引入新 UI 框架或 icon 库
6. 为了好看改已有组件视觉
7. 混用 prototype 与生产两套 token
8. 使用渐变（`.project-symbol` 除外）
9. 使用 `border-radius: 9999px` 做按钮 / 卡片 / 输入框
10. 新建组件前不检查 `src/components/ui/`
11. 在工作台使用大卡片、大圆角、大按钮
12. 用 `button.primary` 新代码替代 `variant="primary"`（旧 class 仅兼容）
13. 浅色主题再写一套硬编码色
14. 复制 `prototype/css` 进生产

## 按需阅读

- [tokens.md](tokens.md) — 色、字、间距、圆角、阴影
- [components.md](components.md) — 组件规格与 API
- [interactions.md](interactions.md) — hover / active / focus / disabled / loading
- [desktop.md](desktop.md) — 标题栏、侧栏、工作台、右键、快捷键
- [patterns.md](patterns.md) — 页面模式、密度、重构项
- [examples.md](examples.md) — 正确 / 错误示例
