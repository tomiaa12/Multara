# Tokens

来源标记：

- **已有**：生产 `src/styles.css` 里本来就有的变量或重复出现的值
- **统一**：从重复硬编码中抽出，供新代码使用；未回改旧样式

新 CSS 只允许 `var(--name)`。没有对应 token 就先加 token，再写样式。

## Color

| Token | Dark | Light | 用途 | 来源 |
|---|---|---|---|---|
| `--background` | `#121212` | `#f4f5f7` | 窗口底 | 已有 |
| `--panel` | `#1a1a1a` | `#fff` | 卡片 / 面板 | 已有 |
| `--panel-2` | `#202020` | `#f5f6f8` | 嵌套面 / 次级块 | 已有 |
| `--surface-hover` | `#1c1c1c` | `#f0f1f3` | 卡片 / 行 hover | 统一 |
| `--surface-active` | `#292929` | `#e8eaee` | 选中导航 / 激活行 | 统一 |
| `--control-bg` | `#1c1c1c` | `#fff` | 输入底 | 已有 |
| `--control-bg-hover` | `#242424` | `#f7f8fa` | 默认按钮底 | 已有 |
| `--text` | `#e8e8e8` | `#202124` | 主文字 | 统一（原 `:root color`） |
| `--text-strong` | `#f2f2f2` | `#111111` | 标题 / 激活 tab | 统一 |
| `--muted` | `#8f8f8f` | `#6f737a` | 次要文字 | 已有 |
| `--text-faint` | `#626262` | `#9a9a9a` | 分组标签 / 更弱 | 统一 |
| `--placeholder` | `#6f6f6f` | `#8a8e96` | placeholder | 统一 |
| `--border` | `#2b2b2b` | `#dfe1e5` | 默认边框 / 分割线 | 已有 |
| `--border-hover` | `#3b3b3b` | `#c8ccd2` | 卡片 / 控件 hover 边 | 统一 |
| `--border-strong` | `#414141` | `#c8c8c8` | 弹窗边框 | 统一 |
| `--control-border` | `#383838` | `#cfd3d8` | 按钮 / 输入边框 | 已有 |
| `--blue` | `#1683ff` | `#1683ff` | Primary | 已有 |
| `--blue-hover` | `#3796ff` | `#1474d4` | Primary hover | 统一 |
| `--control-ring` | `#4b93d1` | `#2278c9` | Focus | 已有 |
| `--green` | `#39c274` | `#39c274` | Success | 已有 |
| `--warning` | `#e8c582` | `#8a6a12` | Warning 文字 | 统一 |
| `--warning-bg` | `#332918` | `#f6eed8` | Warning 底 | 统一 |
| `--danger` | `#ef9292` | `#c45656` | Danger 按钮字 | 统一 |
| `--danger-text` | `#ff8f8f` | `#b42318` | Error 文案 | 统一 |
| `--danger-bg` | `#351c1c` | `#f8e8e8` | Error 底 | 统一 |
| `--danger-border` | `#6c3030` | `#e0b4b4` | Error 边 | 统一 |
| `--info` | `#79b8ff` | `#2278c9` | Info / 链接强调 | 统一 |
| `--overlay` | `#0009` | `rgb(20 20 20 / 40%)` | 模态遮罩 | 统一 |
| `--wt` | `#4ec9b0` | 同 | Worktree 标记 | 已有（原局部） |
| `--direct` | `#e8c582` | 同 | 直接改目录标记 | 已有（原局部） |
| `--folder` | `#9a9a9a` | 同 | 普通文件夹标记 | 已有（原局部） |
| `--orange` | `#d7b765` | 同 | Git modified | 已有（原局部） |

映射（用户要求的语义名 → 本项目 token）：

| 语义 | Token |
|---|---|
| background | `--background` |
| surface | `--panel` |
| surface-hover | `--surface-hover` |
| surface-active | `--surface-active` |
| foreground | `--text` |
| foreground-secondary | `--muted` |
| foreground-muted | `--text-faint` |
| border | `--border` |
| border-hover | `--border-hover` |
| primary | `--blue` |
| primary-hover | `--blue-hover` |
| success | `--green` |
| warning | `--warning` |
| error | `--danger` / `--danger-text` |
| info | `--info` |
| focus | `--control-ring` |

Git 状态色（仅文件树 / SCM，不要当通用色）：

| 状态 | 色 | 来源 |
|---|---|---|
| modified | `#d7b765` | 已有 |
| added | `#74bd7f` | 已有 |
| deleted | `#d97878` | 已有 |
| renamed | `#79b8ff` | 已有 |
| conflict | `#e0884a` | 已有 |

右键菜单 hover `#094771` 是 VS Code 选择蓝，只存在于 `.context-menu`。不要用到按钮或卡片。

## Typography

字体（已有）：

```text
--font-ui: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif
--font-mono: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace
```

中文回退已包含在系统栈里。不要引入新字体。

| 角色 | Size | Weight | Line | 场景 | 来源 |
|---|---|---|---|---|---|
| Display | 24px | 650–800 | 1.2 | 指标数字 | 已有 |
| Heading | 22px | 默认 | 1.25 | 页面 `h1` | 已有 |
| Title | 17px | 默认 | 1.3 | `h2` | 已有 |
| Subtitle | 14px | 默认 | 1.4 | `h3` / 聊天气泡 | 已有 |
| Body | 13px | 400 | 1.55 | 正文、导航、树、菜单 | 已有 |
| Body Small | 12px | 400 | 1.45 | 卡片次文、表单 label、tab | 已有 |
| Caption | 11px | 400 | 1.4 | 辅助、标题栏、small 按钮 | 已有 |
| Label | 10px | 400–600 | 1.3 | 状态栏、分组名、计数徽标 | 已有 |
| Code | 继承 mono | 400 | 1.45 | 快捷键 `kbd`、代码块 | 已有 |

字重：正文 400，按钮 520，强调 600–650，品牌 650，项目缩写 800。不要使用 300 / 900。

项目没有独立 Display 海报级。24px 只用于 metric，不要当页面标题。

禁止新字号：`9px` `15px` `16px` `18px` `20px` 仅出现在旧代码（头像字、个别标题）。新代码靠上表取最近档。

## Spacing

```text
--space-1  2px
--space-2  4px
--space-3  6px
--space-4  8px
--space-5  10px
--space-6  12px
--space-7  16px
--space-8  20px
--space-9  24px
--space-10 32px
```

来源：从现有 padding / gap 统计后取整。旧代码里的 `3 / 5 / 7 / 9 / 11 / 13 / 15px` 视为历史值，新代码取最近 token。

| 用途 | Token |
|---|---|
| 图标与文字 | `--space-3` 或 `--space-4` |
| 控件内边距 | `--space-4` / `--space-5` |
| 卡片内边距 | `--space-6` / `--space-7` |
| 弹窗内边距 | `--space-8` |
| 页面区块 | `--space-9` |
| 空状态大留白 | 72px（仅空状态，不做成 token 滥用） |

## Radius

| Token | 值 | 用于 | 来源 |
|---|---|---|---|
| `--radius-sm` | 4px | 页签关闭、微条、细滚动条 | 已有 |
| `--radius-md` | 6px | 按钮、导航项、右键菜单、树拖拽幽灵 | 已有 |
| `--radius-lg` | 9px | 卡片、面板、tile、看板列 | 已有 |
| `--radius-xl` | 11px | Dialog | 已有 |
| `--radius-full` | 999px | `.tag` 药丸 | 已有 |
| `50%` | 圆 | 头像、FAB | 已有 |

输入框现为 **7px**（已有例外）。新 input 继续 7px，或写 `border-radius: 7px`，不要改成 6 或 8。不要新增 5 / 8 / 10 / 12 / 16px radius。

`floating-chat` 现为 12px，属例外，见 `patterns.md`。

## Border / Shadow / Motion

边框宽度一律 **1px**。分割线用 `1px solid var(--border)`。

| Token | 值 | 用于 | 来源 |
|---|---|---|---|
| `--shadow-sm` | `0 1px 2px rgb(0 0 0 / 24%)` | Primary 按钮 | 统一 |
| `--shadow-md` | `0 8px 28px #0008` | Context menu、中层浮层 | 统一 |
| `--shadow-lg` | `0 24px 80px #000b` | Modal | 统一 |

控件内阴影（不要用于卡片）：

```css
/* button */
box-shadow: inset 0 1px rgb(255 255 255 / 3%);
/* input */
box-shadow: inset 0 1px 1px rgb(0 0 0 / 12%);
```

过渡只允许：

```css
transition: border-color var(--duration) var(--ease),
  background var(--duration) var(--ease),
  box-shadow var(--duration) var(--ease),
  opacity var(--duration) var(--ease);
```

`--duration` = `120ms`，`--ease` = `ease`。禁止 100 / 200 / 250 / 300 / 500ms。

加载旋转：`.spin` + `chat-spin`（1s linear infinite）。这是唯一允许的非 120ms 动画。
