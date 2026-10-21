# Components

基础组件在 `src/components/ui/`，全局注册于 `src/main.ts`。新页面直接写标签，不必再 import（除 `ContextMenu`）。

## 清单

| 组件 | 文件 | 状态 |
|---|---|---|
| Button | `UiButton.vue` + `.ui-button` | 规范组件 |
| Input | `UiInput.vue` + `.ui-input` | 规范组件 |
| Select | `UiSelect.vue` + `.ui-select` | 原生 select |
| Textarea | `UiTextarea.vue` + `.ui-textarea` | 规范组件 |
| Checkbox | `UiInput type="checkbox"` + `.ui-checkbox` | 规范组件 |
| Context menu | `ContextMenu.vue` + `.context-menu` | 规范组件 |
| Dialog | `.modal-backdrop` + `.modal` | 无独立 Vue 封装，复用 class |
| Tabs | `.editor-tab` / `.chat-session-tab` / `.workspace-sidebar-tabs` | 按场景复用 class |
| Sidebar | `.sidebar` `.nav-item` | App 导航 |
| File tree | `.file-tree` | 工作台 |
| Card | `.panel` `.tile` `.list-card` `.project-card` | 页面密度 |
| Tag | `.tag` | 药丸状态 |
| Empty | `.empty` `.empty-project` `.tree-empty` `.req-empty` | 按密度选 |
| Error | `.error-card` `.form-error` `.scm-error` | 按场景选 |
| Icon button | `UiButton size="icon"` | 规范 |
| File icon | `FileTypeIcon.vue` | 仅文件 / 文件夹，来自 VS Code 文件图标 |
| Codicon | `Codicon.vue` | 工作台抽屉、文件树箭头和视图工具条，字体来自 VS Code Codicons |
| Pane | `UiPaneView.vue` + `UiPane.vue` | 侧栏上下抽屉，对应 VS Code PaneView |
| 聊天审阅条 | `chat/ChatEditingBar.vue` | 贴在输入框上方，列出工作区里仍待保留/撤销的文件 |

没有：Toast、Tooltip、Switch、Radio、Combobox、Stepper。

## Button

```vue
<UiButton>次要</UiButton>
<UiButton variant="primary">主要</UiButton>
<UiButton variant="danger">危险</UiButton>
<UiButton variant="ghost">弱操作</UiButton>
<UiButton size="small">小</UiButton>
<UiButton size="icon" title="说明"><Icon :size="13" /></UiButton>
```

Props：`variant`: `default | primary | danger | ghost`；`size`: `default | small | icon`；`type`: `button | submit | reset`。

| | default | small | icon |
|---|---|---|---|
| min-height | 34px | 28px | 32px（工作台工具条改 22px） |
| padding | `7px 11px`（全局 button） | `4px 8px` | `0` |
| font-size | 继承 13 | 11px | — |
| radius | `--radius-md` 6px | 同 | 同 |
| icon | 16 页面主操作 / 14 次操作 | 14 | 12–13 |
| gap | 7px（已有） | 6px | 0 |

视觉（已有）：

| Variant | 底 | 边 | 字 | Hover |
|---|---|---|---|---|
| default | `--control-bg-hover` | `--control-border` | `--text` | 边 `#4a4a4a` 底 `#2c2c2c`（浅色见 light override） |
| primary | `--blue` | `--blue` | `#fff` | `--blue-hover` |
| danger | 同 default | 同 default | `--danger` | 同 default（只改字色，不是实心底） |
| ghost | transparent | transparent | 继承 | 由父级决定，不要加描边 |

Disabled：`opacity: 0.45`，`cursor: default`。Loading：按钮 `disabled` + 文案「保存中…」或 `LoaderCircle` + `.spin`。不要加 spinner 组件库。

旧兼容：`class="primary"`、`class="icon-button"`、`class="danger"` 仍然生效。**新代码用 props**。

禁止：第二种主按钮、实心红底 danger（prototype 有，生产没有）、`border-radius: 999px` 按钮、高度 40+ 的大按钮（FAB `.chat-fab` 48 是唯一例外）。

## Input / Textarea / Select / Checkbox

```vue
<UiInput v-model="name" placeholder="…" />
<UiInput v-model.number="port" type="number" />
<UiInput v-model="on" type="checkbox" />
<UiSelect v-model="theme"><option value="dark">深色</option></UiSelect>
<UiTextarea v-model="bio" rows="3" />
```

| | Input / Select | Textarea | Checkbox |
|---|---|---|---|
| height | min-height 36px | min-height 76px | 15×15 |
| padding | 8×10 / select 7 32 7 10 | 8×10 | 0 |
| radius | **7px**（已有例外） | 7px | 浏览器默认 |
| border | `--control-border` | 同 | — |
| bg | `--control-bg` | 同 | accent `--blue` |
| hover 边 | `#4a4a4a` | 同 | — |
| focus | 边 `--control-ring` + `0 0 0 3px color-mix(in srgb, var(--control-ring) 20%, transparent)` | 同 | — |
| placeholder | `--placeholder` | 同 | — |
| disabled | opacity 0.5，`not-allowed` | 同 | — |

Select 是原生 `<select>`，自定义箭头（两个 5px 三角）。禁止做 portal 下拉，除非现有 `UiSelect` 无法满足（大量可搜索选项目前也仍用原生或工作台已有 picker）。

表单 label（已有）：`display: grid; gap: 7px; font-size: 12px`。Checkbox 行用 `.check-row`。

Readonly：沿用 disabled 视觉或去掉边框透明底（聊天 composer 是例外）。

Error：外面包 `.form-error` 或在字段下用 `--danger-text`，不要给 input 发明新的红边 variant，除非复用已有 `border-color: var(--danger-border)`。

## Dialog / Modal

结构（已有，见 `UnsavedChangesDialog.vue`、`AddWorkspaceModal.vue`）：

```vue
<div class="modal-backdrop" @click.self="close">
  <div class="modal" role="dialog" aria-modal="true">
    <h2>标题</h2>
    <p>说明</p>
    <!-- 字段 -->
    <div class="modal-actions">
      <UiButton @click="close">取消</UiButton>
      <UiButton variant="primary" :disabled="busy">{{ busy ? "保存中…" : "保存" }}</UiButton>
    </div>
  </div>
</div>
```

| 项 | 值 | 来源 |
|---|---|---|
| overlay | `--overlay`，`place-items: center`，padding 20 | 已有 |
| 默认宽 | `min(460px, 100%)` | 已有 |
| 工作区选择 | `min(760px, 100%)` + `.workspace-picker-modal` | 已有 |
| 领域表单 | `min(660px, 100%)` + `.domain-modal` | 已有 |
| padding | 20px | 已有 |
| gap | 16px | 已有 |
| radius | `--radius-xl` 11px | 已有 |
| 边框 | `--border-strong` | 已有 |
| 底 | `#1b1b1b`（接近 `--panel`） | 已有 |
| 阴影 | `--shadow-lg` | 已有 |
| 标题 | `h2` 17px，无关闭也可；有关闭用 `size="icon"` + `X` 16 | 已有 |
| footer | `.modal-actions` 右对齐，gap 8 | 已有 |
| 按钮序 | 取消（default）→ 破坏（default/danger 字）→ 主操作 primary 最右 | 已有 |
| 关 | 点遮罩、取消、Esc（右键菜单已有 Esc；Dialog 新代码应支持 Esc） | 部分已有 |

`StopAiDialog` 误用了不存在的 `.modal-card`。新 dialog 必须用 `.modal`。多操作且每个都重要时，footer 可改成纵向 `gap: 8`（该对话框已有），不要另发明样式。

## Tabs

三种已有形态，禁止第四种：

| 场景 | Class | 高 | 字号 | 激活 |
|---|---|---|---|---|
| 编辑器页签 | `.editor-tab` | 35px | 12 | 底 `#1e1e1e` 字 `--text-strong`，无圆角 |
| 会话页签 | `.chat-session-tab` | 35px | 12 | 底 `#2a2a2a` |
| 侧栏 File/Git | `.workspace-sidebar-tabs .ui-button` | 28px | 继承 | `.active` 底 `--panel-2` |

关闭钮 `.tab-close`：18×18，radius 4，默认隐藏，hover/active 显示。脏文件用 7px 圆点。

## Sidebar（App 导航）

| 项 | 值 | 来源 |
|---|---|---|
| 宽 | `--sidebar-w` 154px（grid 列） | 已有 |
| padding | 12px 7px | 已有 |
| 分组间距 | margin-bottom 15px，组内 gap 3 | 已有 |
| 组标签 | `.nav-label` 10px `--text-faint` | 已有 |
| 项高 | min-height 34px | 已有 |
| 项 padding | 0 9px，radius 6，字 13，gap 9 | 已有 |
| 图标 | Lucide 16 | 已有 |
| hover | 底 `#1b1b1b` 字 `#ddd` | 已有 |
| active | `.router-link-active` 底 `#292929` 字 `#fff` | 已有 |

不要做成 240px+ 的宽 SaaS 侧栏，不要在项上加彩色左边条（那是 SCM 仓库类型标记，不是导航）。

## List / Tree

| 项 | 值 | 来源 |
|---|---|---|
| 树行高 | min-height 25px | 已有 |
| 树字号 | 13 | 已有 |
| 树图标 | `FileTypeIcon` 默认 15，chevron 12 | 已有 |
| 树 hover | `#242424` | 已有 |
| 树选中 | 已有 `.selected` | 已有 |
| 缩进 | 用层级 padding（现有 tree 逻辑），不要改成 20px 一档 | 已有 |
| 页面列表行 | `.list-card` padding 12 gap 12 | 已有 |
| 需求行 | `.project-card` padding 15 radius 9 | 已有 |
| SCM 仓头 | min-height 40，字 11 / 10 | 已有 |

次要文字用 `small` / `.muted`（11px `--muted`）。状态用 Git 色或 `.tag`。

## Badge / Status

| 场景 | 用法 |
|---|---|
| 在线 / 中性药丸 | `.tag` / `.tag.online`（绿字 + 绿边） |
| 成功文字 | `--green` 或 `.save-success` |
| 警告条 | `--warning` + `--warning-bg` |
| 错误条 | `.error-card` / `.form-error` / `.scm-error` |
| 计数 | SCM / 看板 header 小 pill，10px |

不要做实心彩色大徽章。

## Card

`.panel` `.tile` `.list-card` `.metric-grid article`：`1px solid var(--border)`，radius 9，底 `--panel`。

页面卡片 padding 16 或 12。看板卡片 padding 12 radius 8（历史，新卡片用 9）。

可点击卡片 hover：边 `--border-hover`，底 `--surface-hover`，过渡 120ms。`agent-tile` 的 `translateY(-1px)` 不要扩散。

## Icon

| 档 | 尺寸 | 场景 |
|---|---|---|
| icon-xs | 11–12 | 窗口控件、树 chevron、tab 关闭、Git 行内 |
| icon-sm | 13–14 | 列表图标按钮、次操作 |
| icon-md | 15–16 | 导航、页面主按钮 |
| icon-lg | 18–22 | 空状态、FAB（22） |

规则：

- 页面和聊天仍用 `@lucide/vue`。工作台侧栏抽屉、文件树箭头、打开的编辑器、时间线用 `Codicon`（VS Code 同一套字形）。
- 文件 / 文件夹只用 `FileTypeIcon`，禁止 Lucide `File` 冒充类型图标（工作台空态占位除外）。
- 禁止手写装饰 SVG。`AvatarPicker` 生成头像是唯一例外。
- stroke 用 Lucide 默认，不要改 `stroke-width`。
- 与文字 gap：按钮 7px，树 3–6px。
- 图标按钮必须 `title`。

## 使用规则（何时用什么）

| 情境 | 用 | 不用 |
|---|---|---|
| 提交表单 | primary Button | 链接冒充按钮 |
| 工具条图标 | ghost + icon | 每个都做成 primary |
| 删除 | danger 字色；不可逆先 Dialog | 实心红按钮 |
| 2–8 个固定选项 | `UiSelect` | 自造菜单 |
| 是/否设置 | checkbox + `.check-row` | Switch |
| 阻断确认 | Dialog | 自定义全屏页 |
| 保存成功 | `.save-success` | Toast / alert |
| 字段提示 | `small` | 气泡 Tooltip |
| 行操作菜单 | `ContextMenu` | 每行三个点再自造面板 |
| 短暂进程 | `LoaderCircle` `.spin` 或「正在…」 | 骨架屏框架 |
