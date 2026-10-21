# Desktop

Multara 是 Tauri 桌面软件。按 IDE / 本地工作台设计，不按营销站或 SaaS Dashboard。

## Window

| 项 | 值 | 来源 |
|---|---|---|
| 壳 | `.app-shell` grid：`38px 1fr 24px / 154px 1fr` | 已有 |
| 底 | `#151515`（略深于 `--background`） | 已有 |
| 标题栏高 | `--titlebar-h` 38px | 已有 |
| 标题栏底 | `#121212`，底边 `--border` | 已有 |
| 标题栏字 | 11px `--muted` | 已有 |
| 品牌 | Lucide 15 + 「Multara」，字重 650 | 已有 |
| 拖拽 | `.titlebar-drag` + `data-tauri-drag-region` | 已有 |
| macOS | padding-left 78px 给系统灯 | 已有 |
| Windows 控件 | 宽 46px，高 100%，无圆角 | 已有 |
| Windows 关闭 hover | `#c42b1c` | 已有 |
| 状态栏 | `--statusbar-h` 24px，字 10px，左环境摘要，右运行数 + 当前文件语言 | 已有 |

不要做网页 Header（logo + 大导航 + 用户头像）。窗口控件只在 Windows 自绘。

## App Sidebar

见 `components.md` Sidebar。固定窄栏 154px，分组「工作区 / 配置」。图标 16 + 中文标签。

## Workbench

`WorkspaceWorkbench`：三列 + 4px 拖拽条。

| 列 | 约束 | 来源 |
|---|---|---|
| 左（树 / Git） | `minmax(160px, 20%)` 底 `#141414` | 已有 |
| 中（编辑器） | `minmax(300px, 1fr)` | 已有 |
| 右（聊天） | `minmax(260px, 50%)` | 已有 |
| 拖拽条 | 4px，hover/focus 现 `--blue` | 已有 |

工作台密度：

- Explorer 标题 35px，11px，字距 0.08em
- 工具图标按钮 22×22，透明底
- 树行 25px
- 编辑器 tab 35px、无圆角、脏点 7px
- 不要把页面 `.page` 的 28px 大留白带进工作台

## Editor

- Monaco 是编辑器，不要用 textarea 冒充代码编辑。
- 欢迎态 `.editor-welcome` 底 `#1e1e1e`，快捷键列表 13px `#6f6f6f`。
- Zoom HUD 已有，复用 `EditorZoomHud`。
- 面包屑用 `.editor-breadcrumbs`，大纲用左侧「大纲」页签。
- 状态栏右侧：行/列、UTF-8、LF/CRLF、缩进、语言、缩放。

## Chat

| 形态 | 规格 | 来源 |
|---|---|---|
| 停靠右栏 | 工作台第三列 | 已有 |
| 浮动窗 | `.floating-chat` 390×540，radius 12（例外），可 resize | 已有 |
| FAB | `.chat-fab` 48 圆，`--blue`，仅非工作台页 | 已有 |
| 会话 tab | 与 editor tab 同族 | 已有 |
| 作曲区 | 透明 textarea，发送 = primary icon + `ArrowUp` 16 | 已有 |

气泡 / 过程折叠已有 class，改聊天先复用 `src/components/chat/`。

## Context Menu

`ContextMenu.vue`：

- `min-width: 228px`，padding 4 0，radius 6，底 `#2d2d2d`，边 `#454545`，阴影 `--shadow-md`
- 项高 26px，padding 0 18，字 13
- hover：`#094771` / `#fff`
- 分隔 `hr` 1px `#454545`
- 右键 `kbd` 11px mono
- Esc / 点外面关闭；自动防出屏

新右键不要用 `UiButton` 堆出来。扩展 `ContextMenuItem`。

## File Tree

- 打开 / 折叠 chevron 12
- `FileTypeIcon` 15
- Git 色见 `tokens.md`
- 拖入工作区已有 ghost `.workspace-file-drag-ghost`
- 空态 `.tree-empty` 11px `--muted`

## Status / Inline alerts

- 全局状态栏放环境摘要和当前编辑器状态，不放 Toast
- SCM 错误用 `.scm-error`（10px，`--danger-bg`）
- 外部文件变更条已有，复用 `WorkspaceWorkbench` 里那组 small 按钮

## Keyboard / 鼠标

必须保持：

- 右键菜单
- 分栏拖拽
- 标题栏拖动 / 双击最大化
- Tab 页签关闭、中键关闭（若该处已有，不要删）
- 聊天发送快捷键（已有逻辑，不要改视觉来「升级」）

新功能优先快捷键 + 右键，不要只做隐藏的图标悬停菜单。

## Empty / Loading（桌面）

| 密度 | Empty | Loading |
|---|---|---|
| 页面 | `.empty-project` 居中，padding 72 24，可配 primary 按钮 | `.empty` 一句文案 |
| 工作台 | `.tree-empty` / `.editor-welcome` | 同一区域替换文案 |
| 需求首页 | `.req-empty` 现有插画节点，不要改成大插图营销页 | 已有动画，勿另做 |

## 不要带入的 Web 习惯

- 大卡片网格 + 48px padding
- 圆角 16–24 的对话框
- 底部固定大主按钮条
- 顶部全局搜索大条
- 面包屑 + 页面英雄区
- 移动端汉堡菜单（窄屏只压缩网格，不是另一套产品）
