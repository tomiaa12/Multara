---
name: multara-chat-surfaces
description: >-
  Multara 有三个聊天入口：右下角聊天气泡、任务详情、需求工作台。修改聊天相关代码时使用：
  ChatPanel、ChatComposer、UserPrompt、AssistantMessage、TaskActivity、components/chat/ 下的任何文件、
  store 里的会话/消息/runStates、聊天相关样式或 chat.* 文案。用户说"聊天"、"气泡"、"输入框"、
  "消息"、"任务回复"、"工作台聊天"时也要用，改完要核对三个环境。
---

# 三个聊天环境

改任何共享的聊天代码前，先确认这次改动会落到下面哪几个环境，再逐个核对。只改了一个入口却没检查另外两个，是这块最常见的回归来源。

## 三个环境和入口

| 环境 | 宿主 | 渲染组件 | 关键 props |
| --- | --- | --- | --- |
| 右下角气泡 | `src/components/FloatingChat.vue`（在 `App.vue` 挂载） | `ChatPanel` | `compact` `session-commands` `:show-editing-bar="false"`，`:key="projectId ?? 'global'"` |
| 需求工作台 | `src/components/WorkspaceWorkbench.vue` 的 `<aside class="project-chat">`（路由 `/projects/:projectId`，开发态还有 `/preview`） | `ChatPanel` | `compact` `session-commands` `preview*` `:allow-target-change="!preview"` |
| 任务详情 | `src/views/TaskDetailView.vue`（路由 `/tasks/:taskId`） | `src/views/TaskActivity.vue` | `session-id` `agent-id`，没有 `thread` |

顺带受影响的入口：
- 收件箱 `InboxView.vue`：任务类条目用 `TaskActivity thread`，其他条目用 `ChatPanel embedded`。
- 新建任务 `TaskCreateDialog.vue`，以及任务描述那块只读的 `ChatComposer`（`readonly embedded`），都直接使用 `ChatComposer`。

## 组件共享关系

- 气泡和工作台是同一个 `ChatPanel`，差别只来自 props。`ChatPanel` 里根据 `props.compact` / `embedded` / `sessionCommands` / `showEditingBar` / `preview` 走分支的地方，都要两边一起看。
- 任务详情**不经过 `ChatPanel`**。`TaskActivity` 自己组合 `UserPrompt`、`AssistantMessage`、`ChatComposer`、`ChatQuestionCarousel`，发送、停止、粘贴图片、拖文件、提问卡片、工作中状态这些逻辑也各写了一份。只在 `ChatPanel` 里加的功能，任务详情里看不到；要两边都有，就得在 `TaskActivity` 里也接上，或者把逻辑抽到 `components/chat/*.ts` 里共用。
- 三个环境共用的底层：`ChatComposer`、`UserPrompt`、`AssistantMessage`、`components/chat/*.ts` 里的纯逻辑（`message-speaker`、`timeline-display`、`question-block`、`chat-scroll`、`composer-content` 等），以及 `useAppStore()` 的 `messages`、`runStates`、`sessionProcessing`、`send`、`stopSessionRuns`。改这些等于三个环境一起改。

## 场景标识

`ChatPanel` 的 `resolvedPlaceholderPlace` 会推断当前场景，推断结果同时决定输入框占位文案 `chat.placeholder.<place>`，以及最近路径的存储桶 `recent-place`：

- `embedded` 推断为 `inbox`
- `compact` 且 `!showEditingBar` 推断为 `float`（气泡）
- `compact` 推断为 `workbench`
- 其他情况为 `chat`

`TaskActivity` 和 `TaskCreateDialog` 固定传 `recent-place="task"`。新增 props、修改 `compact` 或 `showEditingBar` 的默认值时，会连带改变场景推断；新增场景时要同时改 `ComposerRecentPlace`（`src/stores/composer-recent-paths.ts`）和全部五个语言包。

## 必查的行为差异

1. **气泡在工作台里只是隐藏，不会卸载。** `App.vue` 在 `isWorkbenchPath(route.path)` 或 `/inbox` 时给气泡传 `hidden`；首次打开后 `retained` 会让 `ChatPanel` 一直挂载。所以在工作台页面上，同一个 `projectId` 可能同时挂着两个 `ChatPanel`。新增的全局监听（键盘快捷键、`window` 事件、听写、canvas host、自动聚焦）要能看出自己处在隐藏的那个实例里，否则会触发两次，或者被看不见的那个实例抢走。
2. **会话标签按项目共享。** 打开的会话标签存在 `multara.chat.openTabs`，按 `tabScope(projectId)` 分组，由 `chat-open-tabs.ts` 和 `store.openedSessionIds` 管理。气泡和工作台在同一个项目下读写的是同一组标签；气泡上的 `pendingPlan` 红点也读这份数据。
3. **keep-alive。** `App.vue` 用 `<keep-alive :max="24">` 缓存页面。`TaskDetailView` / `TaskActivity` 依赖 `onActivated` / `onDeactivated` 暂停跟随滚动和秒表；新加的计时器或 watcher 也要处理页面停用时暂停，否则后台页面会一直在跑。
4. **滚动归属不一样。** 任务详情由外层 `.task-issue-scroll` 负责滚动和贴底，`TaskActivity` 只在 `thread` 模式下自己滚动；`ChatPanel` 用自己的 `.message-list`。改粘性提问（`syncStickyUserTurnPush`）或贴底逻辑时，三处都要验证。
5. **功能开关不对称。** `session-commands`（`/compact`、`/fork`）只在气泡和工作台开启；待审改文件条只在工作台显示，气泡关掉了；任务详情的 `UserPrompt` 是 `:enable-replay="false"`，`AssistantMessage` 是 `:allow-continue="false"`；工作台的 `preview` 模式会跳过大部分 store 调用（`if (props.preview) return`）。
6. **尺寸。** 气泡是窄浮层，可以展开（`.floating-chat.expanded`）；工作台聊天栏可以拖拽调宽；任务详情是全宽正文。长文本、代码块、提问卡片、diff 卡片都要在窄宽度下检查。

## 样式

样式集中在 `src/styles.css`，按根选择器区分环境：

- 气泡：`.floating-chat`、`.chat-fab`
- 气泡和工作台共有：`.shared-chat-panel.compact ...`
- 工作台：`.project-chat`、`.project-chat > .shared-chat-panel`
- 任务详情：`.task-issue-*`、`.task-activity ...`；收件箱线程：`.task-activity.is-thread`、`.inbox-thread`

改通用类（`.chat-turn`、`.user-turn`、`.assistant-working`、`.chat-composer`、`.chat-question-*`）时，要搜索上面这些作用域里有没有覆盖规则，确认改动在三处表现一致；只想改一处时，就挂在对应的根选择器下面。

## 改完后的核对清单

- [ ] 用 `rg` 搜改动涉及的组件或函数名，列出它在 `ChatPanel`、`TaskActivity`、`FloatingChat`、`WorkspaceWorkbench`、`InboxView`、`TaskCreateDialog` 里的所有用法
- [ ] 功能需要在任务详情里生效时，确认 `TaskActivity` 也接上了
- [ ] 新增的全局副作用在“工作台页面 + 隐藏的气泡实例”同时挂载时不会重复触发
- [ ] 新增文案五个语言包都补齐（`src/i18n/locales/{zh-CN,zh-TW,en,ja,ko}.ts`）
- [ ] `pnpm test`（`src/components/chat/*.test.ts` 覆盖了大部分纯逻辑）和 `pnpm build`（会跑 `vue-tsc`）
- [ ] 能跑 `pnpm tauri:dev` 时，三个环境各手动看一遍：`/tasks/:id` 详情、`/projects/:id` 右侧栏、其他页面的右下角气泡；需要演示数据可以用开发态的 `/preview`
