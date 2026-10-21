# Interactions

全项目只允许一套交互节奏。新组件必须抄现有状态，不要自创。

## 总则

| 属性 | 值 | 来源 |
|---|---|---|
| duration | `120ms` / `var(--duration)` | 已有（几乎所有 transition） |
| easing | `ease` / `var(--ease)` | 已有 |
| 可动画属性 | `background` `border-color` `box-shadow` `opacity` | 已有 |
| 禁止动画 | `width` `height` `padding` 布局跳动；不要加 bounce | 统一 |

`transform: translateY(-1px)` 只存在于 `.agent-tile`，禁止复制。

## Hover

按表面类型，不要混用。

| 表面 | Hover | 来源 |
|---|---|---|
| 默认按钮 | 边变亮、底 `#2c2c2c`（浅色 `#eef0f3`） | 已有 |
| Primary | `--blue-hover` | 已有 |
| Ghost / 工具条图标 | 底变浅或父级已有 hover，不加边框 | 已有 |
| 导航项 | 底 `#1b1b1b` 字 `#ddd` | 已有 |
| 页面卡片 | 边 `--border-hover` 底 `--surface-hover` | 已有 |
| 文件树行 | 底 `#242424` 字 `#eee` | 已有 |
| 编辑器 / 会话 tab | 底 `#222` 字 `#ddd` | 已有 |
| SCM 仓头 | 底 `#2a2d2e` | 已有 |
| 右键菜单项 | 底 `#094771` 字 `#fff`（仅此组件） | 已有 |
| 窗口关闭 | 底 `#c42b1c` 字 `#fff` | 已有 |
| 窗口 min/max | 底 `#2a2a2a` | 已有 |
| 可点卡片 | 可加 `cursor: pointer` | 已有 |

行内次要操作（`.user-turn-actions` `.tile-actions`）默认 `opacity: 0`，父级 hover / focus-within 显示。工作台树工具按钮常显。不要把两种混在同一列表。

## Active / Selected

| 表面 | Active | 来源 |
|---|---|---|
| 导航 | `.router-link-active` 底 `#292929` 字 `#fff` | 已有 |
| 树 | `.selected` | 已有 |
| File/Git 段 tab | `.active` 底 `--panel-2` | 已有 |
| 编辑器 tab | `.active` 底 `#1e1e1e` 字 `#f2f2f2` | 已有 |
| 会话 tab | `.active` 底 `#2a2a2a` 字 `#f2f2f2` | 已有 |

按下态不要做额外 scale。`:active` 未在生产统一，不要新加明显下沉。

## Focus

必须能键盘看见焦点。

| 控件 | Focus | 来源 |
|---|---|---|
| `UiButton` | `outline: 2px solid var(--control-ring); outline-offset: 2px` | 已有 |
| Input / Select / Textarea | 边 `--control-ring` + `box-shadow: 0 0 0 3px color-mix(in srgb, var(--control-ring) 20%, transparent)` | 已有 |
| 分栏拖拽条 | `::after` 变成 `--blue` | 已有 |
| 可点击卡片 | 部分旧代码 `outline: none` 只改边框（不一致，见 patterns） | 已有 |

新交互控件：按钮走 outline，字段走 ring shadow。不要 `outline: none` 又不给替代。

## Disabled

| 控件 | 表现 | 来源 |
|---|---|---|
| button / UiButton | opacity 0.45，`cursor: default` | 已有 |
| input / select / textarea | opacity 0.5，`cursor: not-allowed` | 已有 |
| 菜单项 | 字 `#6a6a6a`，不可点 | 已有 |
| 树工具按钮 | opacity 0.35 | 已有 |

Disabled 元素禁止再有 hover 色变（按钮已用 `:hover:not(:disabled)`）。

## Loading

| 场景 | 表现 | 来源 |
|---|---|---|
| 页面首次 | `.empty` 文案「正在加载本地工作区…」 | 已有 |
| 按钮提交 | `disabled` + 「保存中…」 | 已有 |
| 行内刷新 | `LoaderCircle` + class `spin`（`1s linear infinite`） | 已有 |
| 文件树 | 「正在读取文件…」`.tree-empty` | 已有 |
| Git | 「正在读取 Git 状态…」 | 已有 |
| 聊天生成 | 已有 thinking / spinner class | 已有 |

禁止新加进度条组件、骨架屏库、全屏 spinner。

## Cursor

| 操作 | cursor |
|---|---|
| 按钮 / 导航 | `pointer` |
| disabled | `default` 或 `not-allowed` |
| 分栏 | `col-resize` |
| 文本 | 默认 |
| 拖拽文件 | 已有 `copy` |
| 资源卡片 | `grab` |

## 键盘

| 键 | 行为 |
|---|---|
| Esc | 关 ContextMenu；新 Dialog 必须关 |
| Enter | 提交当前表单 / 发送（聊天已有） |
| 双击标题栏 | 最大化（Windows） |
| 右键 | 树 / 页签 / 需求行出现 `ContextMenu`，快捷键画在右侧 `kbd` |

新菜单项若有快捷键，用 `<kbd>` 显示，样式跟 `.context-menu kbd`（11px mono，`#858585`）。
