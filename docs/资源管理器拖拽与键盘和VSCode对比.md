# 工作台左侧文件列表：拖拽与键盘，和 VS Code 的差距

对比对象：Multara 的资源管理器（`WorkspaceWorkbench.vue`、`workbench/explorer/*`、`explorer-keys.ts`、`chat/workspace-file-drag.ts`）与 VS Code 的 `src/vs/workbench/contrib/files/browser/`（`views/explorerViewer.ts` 的 `FileDragAndDrop`、`fileActions.contribution.ts`、`fileCommands.ts`、`files.contribution.ts`）。

**方法与边界**：只读源码做静态对比，没有运行 Multara，也没有运行 VS Code。结论标记为 **[已读代码]**（两边代码我都看过）或 **[推断]**（按机制判断，需要实际操作确认）。

## 0. 先说结论

Multara 的资源管理器已经移植得相当完整，不是"缺很多"：

- 键位表（`explorer-keys.ts`）几乎照搬了 VS Code 的默认键位，含 Mac / Windows 差异、`Ctrl+K Ctrl+Shift+C` 的和弦键。
- 内部拖放的判定函数 `canDropInExplorer` 是 VS Code `handleDragOver` 的移植；复制修饰键（Mac 用 Option，其它用 Ctrl）、500ms 悬停自动展开、移动确认框、覆盖确认、撤销 / 重做、剪切复制粘贴、类型跳转、文件嵌套、紧凑文件夹、排序选项、`explorer.*` 的大部分设置也都有。
- `explorer.*` 设置对比：VS Code 有而 Multara 没有的只有 `expandSingleFolderWorkspaces` 和三个 `openEditors.*`。

真正的差距集中在三类：**拖拽是自绘的（不是原生 DnD）所带来的限制**、**几个拖放落点没处理**、**少量列表级键位**。

## 1. 拖拽

### 1.1 根因：拖拽是用指针事件自绘的，不是原生拖放 [已读代码]

`WorkspaceWorkbench.vue` 里文件行是 `:draggable="false"`，拖动靠 `chat/workspace-file-drag.ts` 的 `beginWorkspaceFilePointerDrag`（`pointerdown` + `setPointerCapture` + 一个跟随鼠标的 ghost 元素，移动超过 5px 才算开始）。VS Code 用的是浏览器原生 DnD（`dataTransfer`）。这样做的好处是能和聊天输入框、终端、编辑器自定义联动（拖进聊天会变成引用，拖进终端会插入路径），也避开了 WebView 的原生拖放兼容问题；代价就是下面几条。

### 1.2 缺失 / 不同

| # | 功能 | VS Code | Multara | 依据 |
|---|---|---|---|---|
| 1 | **拖到应用外**（Finder / 资源管理器、其它应用、其它窗口） | `onDragStart` 里 `fillEditorsDragData` 并写入 `CodeDataTransfers.FILES`（文件路径 JSON），可拖到系统和别的 VS Code 窗口 | 没有任何原生 `dataTransfer` 数据，自绘 ghost 只在本窗口内可见，拖出窗口无效 | [已读代码] `explorerViewer.ts:1696-1710`；`workspace-file-drag.ts` 全文 |
| 2 | **拖到空白处 = 移到工作区根目录** | `target` 为空（拖到列表空白）时接受，移动到根（除非已在根下） | `treeItemFromPoint` 在空白处返回 null；`finishTreeDrag` 对 `explorer` 区且无目标的情况只 `endDrag()`，并弹出"拖到聊天框"的提示 `composerDropHint`。内部拖放落在空白处什么都不做 | [已读代码] `explorerViewer.ts:1587-1602`；`WorkspaceWorkbench.vue` `finishTreeDrag` |
| 3 | **拖到边缘自动滚动** | 列表控件内建，鼠标靠近上下边缘时自动滚 | 自绘拖拽里没有任何滚动逻辑（`workspace-file-drag.ts` 里没有 scroll 相关代码），文件很多时拖不到屏外的目标 | [已读代码] |
| 4 | **多根工作区里拖动根文件夹来排序** | 根可拖动重排，落点有"前 / 后"插入线（`ListDragOverEffectPosition`） | 工作区标题行（`tree-workspace-head`）没有 `pointerdown` 拖动处理，也没有排序逻辑 | [已读代码] `explorerViewer.ts:1632-1655`；Multara 的 `onTreePointerDown` 只挂在文件行上 |
| 5 | **拖放目标是紧凑文件夹的某一段** | 紧凑文件夹 `a/b/c` 可以把文件拖到中间的 `b` 段上，并高亮该段（`getCompressedStatFromDragEvent`） | 每一段只有 `@click` 处理（`onCompactClick`），拖放用 `data-tree-key` 找目标，整行都当作最深的那一级 | [已读代码] `explorerViewer.ts:1536-1556`；`WorkspaceWorkbench.vue:5522-5525` |
| 6 | **拖放落点的反馈** | 移动 / 复制的光标效果（`ListDragOverEffectType`），行内插入线，拖拽标签用 `getDragLabel` | 有高亮（`is-drop` / `is-drop-invalid`）和数量标签；没有区分移动 / 复制的光标样式 | [推断] 没找到对应的光标样式代码 |
| 7 | 拖到编辑器区 | 会按落点在编辑器组里打开（可分屏） | 直接固定打开（`openEditorFile(..., { pin: true })`），没有分屏落点 | [已读代码] `finishTreeDrag` |
| 8 | 从系统拖入 | `doHandleExternalDrop`，同名冲突询问 | 有（`importDropped`，同名会询问是否替换，`autoOpenDroppedFile` 也移植了） | [已读代码] 差距不大 |

补充说明：

- 第 3 条在大仓库里最容易被碰到——想把文件拖到当前屏幕外的文件夹，只能先手动展开 / 滚动到目标可见。
- 第 2 条和第 4 条是行为上"拖了没反应"的情况；用户很可能以为拖放坏了。
- 第 1 条无法用小改动补上，需要在拖动开始时同时发起原生拖放（Tauri 里要引入拖出文件的能力，如 `tauri-plugin-drag`），并要处理与自绘 ghost 并存的冲突。

## 2. 键盘

### 2.1 已有（与 VS Code 一致）[已读代码]

`F2`（Mac 为 `Enter`）重命名；`Delete` / `Shift+Delete`（Mac：`⌘⌫` / `⌥⌘⌫`）删除与永久删除；`Ctrl/⌘ +C / X / V` 复制 / 剪切 / 粘贴；`Esc` 取消剪切；`Ctrl/⌘+Z`、`Shift+Ctrl/⌘+Z`、`Ctrl+Y` 撤销重做；`Ctrl/⌘+A` 全选；`Ctrl+Alt+F` 查找；`Enter`（Mac：`⌘↓`）打开；`Ctrl/⌘+Enter` 在侧边打开；`Space` 打开但保留焦点；`Ctrl/⌘+←` 全部折叠；`←` / `→` 折叠展开与跳到父级；方向键、`Home`、`End`、`PageUp`、`PageDown`、`Shift` 扩选；`Shift+Alt+C` / `⌥⌘C` 复制路径，`Ctrl+Shift+Alt+C` 复制相对路径，`Ctrl+K Ctrl+Shift+C` 和弦；`Shift+Alt+R` / `⌥⌘R` 在系统文件管理器中显示；`Alt+点击` 递归展开；类型跳转。

### 2.2 缺失 / 不同

| # | 功能 | VS Code | Multara | 依据 |
|---|---|---|---|---|
| 1 | **不改变选择地移动焦点**：`Ctrl+↑ / ↓`；再配合 `Ctrl+Space` 切换当前项的选中 | VS Code 列表控件的 `list.focusDown` 加修饰键 / `list.toggleSelection`，用来做不连续多选 | `explorerKey` 里没有这几个组合（`Ctrl` 单独加方向键被判定为"无匹配"，落入类型跳转的单字符分支后因为带修饰键又被丢弃） | [已读代码] `explorer-keys.ts`；VS Code 侧的列表键位定义在 `listCommands`，我这次没有展开读，所以具体键位名称为 [推断] |
| 2 | **上下文菜单键 / `Shift+F10`** | 键盘直接打开右键菜单 | 没有对应的键位处理；浏览器对聚焦元素通常会触发 `contextmenu` 事件，但菜单位置会落在鼠标位置而不是行上 | [推断] 没有实际按键验证 |
| 3 | **`Enter` 在文件夹上的行为（Windows / Linux）** | 展开 / 折叠文件夹 | `Enter` 映射到 `open`，对文件夹最终是 `toggleExpandable`，行为一致 | [已读代码] 无差距，列出以免误报 |
| 4 | **"在编辑器中显示当前文件"的快捷键** | 有对应命令（`workbench.files.action.showActiveFileInExplorer`），可绑定键 | 工具栏有"定位当前文件"按钮（`revealActiveExplorerFile`），`Ctrl+Shift+E` 聚焦资源管理器；没有单独的"在资源管理器中显示"键位 | [已读代码] 对应 `WorkspaceWorkbench.vue:3062`、`3124` |
| 5 | **拖拽中的修饰键提示** | 光标 + 提示显示"复制"或"移动" | 修饰键生效（`isCopyDrag`），但没有提示（见 1.2 第 6 条） | [已读代码] |
| 6 | 新建文件 / 文件夹的快捷键 | 命令面板与菜单，默认无键位 | 菜单有，默认无键位 | 无差距 |

结论：键位表本身基本没有缺口，只有**不连续多选的两个组合键**（`Ctrl+方向键`、`Ctrl+Space`）和**键盘打开右键菜单**值得补。

## 3. 右键菜单

Multara 的菜单项比 VS Code 默认多不少（"添加到聊天"、"在新窗口中添加项目"、git 的暂存 / 取消暂存 / 丢弃 / 忽略、复制远程链接等）。对照 VS Code 的 `ExplorerContext` 菜单，**VS Code 有而 Multara 没有**的：

| 菜单项 | 说明 |
|---|---|
| **打开方式…（Open With…）** | 选择用哪个编辑器打开，Multara 只有"打开"、"固定打开"、"用系统程序打开" |
| 下载 / 上传（`5b_importexport`） | 仅 Web 版有，桌面版不需要 |
| 与剪贴板比较 | 这是编辑器菜单项，资源管理器里没有，列出以免误报 |

其它菜单差异（"选择以比较 / 与…比较 / 比较已选项"、"在集成终端中打开"、"在文件夹中查找"、"打开时间线"、"复制路径 / 相对路径"）两边都有。

## 4. 设置项

VS Code `explorer.*`（`files.contribution.ts`）里 Multara 没有的：

| 设置 | 作用 |
|---|---|
| `explorer.expandSingleFolderWorkspaces` | 打开只含一个文件夹的工作区时自动展开 |
| `explorer.openEditors.visible` / `minVisible` / `sortOrder` | "打开的编辑器"区块的可见行数与排序（Multara 有这个区块，见 `openEditorsMenuItems`，但没有这三个可配置项） |

## 5. 建议的优先级

按"用户会明显感觉到拖放坏了"的程度排：

1. **拖到空白处 = 移到根**（1.2 第 2 条）：小改动，`finishTreeDrag` 在 `explorer` 区且无目标时回退到当前工作区根；多工作区时用最后悬停的工作区，没有就不动。
2. **拖拽到边缘自动滚动**（1.2 第 3 条）：在 `onMove` 里判断指针与 `.file-tree` 上下边缘的距离，用 `requestAnimationFrame` 滚动。
3. **紧凑文件夹分段作为拖放目标**（1.2 第 5 条）：给每一段加 `data-tree-key`，并在 `treeItemFromPoint` 里识别。
4. **`Ctrl+方向键` / `Ctrl+Space`**（2.2 第 1 条）：在 `explorerKey` 加两个动作，只移动焦点或切换选中。
5. **根文件夹拖动排序**（1.2 第 4 条）：只在多工作区时有意义，需要先确认工作区顺序在哪里持久化。
6. **拖到应用外**（1.2 第 1 条）：改动最大，涉及 Tauri 原生拖出，建议单独立项。

## 6. 没有核对的部分

- 没有运行两边的程序，所有对比来自读代码；"光标效果"、"上下文菜单键的实际表现"两处标了推断。
- VS Code 的列表级键位（`Ctrl+↑/↓`、`Ctrl+Space` 等）定义在通用列表控件里（`base/browser/ui/list`、`platform/list/browser/listService.ts`），我这次没有展开读，2.2 第 1 条里的具体键位是按 VS Code 的常见行为写的，实施前请对照那两个文件确认。
- Multara 的 `WorkspaceWorkbench.vue` 有近 6000 行，我只读了拖放、键盘、右键菜单相关的部分；资源管理器里其它交互（例如行内重命名的校验、嵌套文件显示）没有逐项对比。
