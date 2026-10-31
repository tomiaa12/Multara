# 聊天框语音输入（听写）：系统识别版

## 0. 范围与决策

**做**：聊天输入框听写——点麦克风，边说边把文字写进输入框。识别走 **macOS / Windows 系统语音识别**（WebView 的 `SpeechRecognition` / `webkitSpeechRecognition`），**不**捆绑本地 ASR 模型或 sherpa-onnx。

**不做**：
- 自托管本地模型（Foundry / sherpa-onnx / whisper.cpp）。
- 编辑器、终端里的听写。
- 语音模式（全双工实时对话、朗读回复）。
- LLM 润色。

**已拍板**

| # | 决策 |
|---|---|
| 引擎 | 系统听写（Mac + Windows） |
| 安装包 | **几乎不增加**（无原生 ASR 库、无随包模型） |
| 隐私 | 可能经系统厂商云端处理（尤其 macOS）；首次引导需说明 |
| 回车 | 先 `stop` 再发送（有超时则按已有文字） |
| Esc | 保留文字并停止 |
| 不支持时 | 隐藏麦克风；设置页说明 |

界面与写入器仍对照 VS Code 聊天听写（状态机、interim 灰显、手改接管、避开芯片），后端换成系统 API。

---

## 1. VS Code 怎么做的（对照，不照搬引擎）

VS Code 用 **Foundry Local + Nemotron** 在工具进程里做本机流式 ASR，**不用**系统听写；模型与运行库可按需下载。其 Node 原生插件不能用于 Tauri。

我们只复用其 **会话层形状**：麦克风 UI、累计文本 / 定稿前缀、写入器、引导与错误分类。

---

## 2. 目标架构

```text
WebView
┌──────────────────────────────────────────────┐
│ MicButton / MicMenu / 引导卡片               │
│ dictation-service（状态机）                  │
│  ├ system-speech（SpeechRecognition）        │
│  ├ transcript-reducer                        │
│  └ composer-inserter → ChatComposerField     │
│ 设置：启用、识别语言                         │
└──────────────────────────────────────────────┘
         │
         ▼ 系统听写（OS / 厂商实现，可能联网）
```

原则：

1. **无 Rust STT / 无模型下载**；麦克风权限仍要（macOS `NSMicrophoneUsageDescription`）。
2. 转写事件形状：`{ revision, text, finalizedText?, isFinal }`。
3. 全局一次一个会话；换输入框先停再开。
4. 系统 API 通常占用默认麦克风；设备列表用于权限检测与说明，识别本身跟系统默认输入走。

---

## 3. 前端模块

```text
src/dictation/
  dictation-service.ts
  system-speech.ts
  transcript-reducer.ts
  composer-inserter.ts
  dictation-language.ts
  dictation-errors.ts
src/components/dictation/
  MicButton.vue
  MicMenu.vue
  DictationOnboardingCard.vue
src/stores/dictation.ts
```

状态机：`Idle → Preparing → Recording → Transcribing → Idle`（系统听写 Preparing 很短，主要是权限；无下载环）。

`composer-inserter`：与原稿 4.3 相同（`data-dictation` span、interim 灰显、避开芯片、手改接管、IME 缓冲、直接改 DOM、CJK 不补空格、20 分钟上限）。

---

## 4. 界面与入口

- 麦克风在 `ChatComposer` 发送按钮左侧；prop `dictation` 默认关，主输入框打开，`UserPrompt` 历史气泡不传。
- 首次引导：权限、失败分类、**系统听写可能联网**说明。
- 快捷键：`Ctrl/⌘+Shift+M` 开始 / 停止。
- 设置「语音输入」：启用、识别语言（自动 / 具体语言）；无模型管理项。

---

## 5. 平台

| 平台 | 要做 | 状态 |
|---|---|---|
| macOS | `Info.plist` 已有 `NSMicrophoneUsageDescription`、`NSSpeechRecognitionUsageDescription`；WKWebView 的 `webkitSpeechRecognition` | 在本机裸 WKWebView 里确认 API 存在（`webkitSpeechRecognition` 为 function，标准的 `SpeechRecognition` 没有）；**真实识别效果与授权弹窗未在打包版里验证** |
| macOS 开发构建 | `tauri dev` 从终端 / 编辑器启动 | **一点麦克风应用就被系统杀掉。** 崩溃报告：`EXC_CRASH (SIGABRT)`，TCC 提示"app attempted to access privacy-sensitive data without a usage description … NSSpeechRecognitionUsageDescription"，责任进程是启动它的 Cursor。TCC 看的是**责任进程**的 Info.plist，不是我们的。所以开发构建里即使 `Info.plist` 有说明也没用。处理：后端命令 `dictation_environment` 只在应用运行于 `.app` 内、且其 `Info.plist` 同时含麦克风与语音识别说明时返回可用；否则前端隐藏麦克风，设置页说明原因。要在开发中调试听写，需要用打包后的 `.app`（从 Finder / `open` 启动） |
| Windows | WebView2 的 `webkitSpeechRecognition` | **未验证。** WebView2 是 Chromium，它的实现不是 Windows 自带听写，在非 Chrome 的嵌入环境里可能报 `network` 错误。上线前必须在 Windows 实测；不行就只在 macOS 显示麦克风，`isSystemSpeechSupported()` 之外还要加一次"试跑探测" |
| 不支持 | 隐藏麦克风 | — |

---

## 6. 边界情况与行为约定

| 场景 | 约定 | 实现 |
|---|---|---|
| 引擎在停顿后自行结束并重启 | 之前的文字必须保留，累计文本只增不减 | `segment-accumulator.ts`：重启前 `commit` 已有文字，新一轮结果接在后面；拉丁语系补一个空格，中日韩不补 |
| 识别报错 / 取消 | 已写入的文字保留，去掉听写标记（灰色样式、`data-dictation`），否则下一次会复用旧标记并覆盖 | `composer-inserter.dispose()` 展开标记 |
| 用户手改听写文字 | 接管前缀，后续只追加；灰色样式一并去掉 | `onInput` → `unwrapMarker` |
| 听写中按回车 / 点发送 | 先 `stop` 等最终结果（有超时）再发送 | `ChatComposer.onSubmit`；回车路径（`ChatComposerField` 的 `submit`）也走它 |
| Esc | 保留文字并停止；`preventDefault` 让对话框的 Esc 不再关闭 | `dictation-service`；`dialog-escape.ts` 会检查 `defaultPrevented` |
| 多输入框 | 全局单例，换输入框先停再开 | `dictation-service.start` |
| 气泡收起 / 隐藏、`MicButton` 卸载、`keep-alive` 页面被切走 | 停止并定稿 | `FloatingChat`、`MicButton`（`onBeforeUnmount` / `onDeactivated`） |
| 睡眠 / 窗口隐藏 | 停止 | `visibilitychange`；普通失焦不停止 |
| IME 组合输入中 | 缓冲最新文本，组合结束再写 | `composer-inserter.pending` |
| 隐私 | 系统识别可能经系统厂商云端处理（尤其 macOS）；首次引导已说明；音频不落盘，日志里不记录转写文字 | 引导卡片 |

## 7. 测试

- `segment-accumulator.test.ts`：重启后文字不丢（中文无分隔、英文补空格、多次重启只增不减）。
- `composer-inserter.test.ts`：`dispose` 保留文字并去标记、下一次不覆盖；`finish` 无灰色残留；手改后展开并追加。
- `transcript-reducer.test.ts`、`dictation-language.test.ts`：已有。
- **没有自动化覆盖**：真实麦克风与系统识别、Windows 行为、回车发送时的实际时序，需要手动验证。
