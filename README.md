# Multara

本地优先的多智能体开发工作区。桌面壳采用 Tauri 2，界面采用 Vue 3 + TypeScript + Pinia，系统能力和领域逻辑放在 Rust。

## 当前实现

- 应用导航：项目、聊天、自动化、智能体、小队、统计、运行时、Skills、MCP、设置。
- 右下角浮动聊天，以及独立聊天页面。
- SQLite 本地数据骨架：智能体、小队、成员、对话、消息和运行队列。
- 自动检测本机 Claude、Codex、Cursor、Copilot、OpenCode、Gemini、Qwen、Kimi CLI。
- 小队消息先交给队长，并为其他成员创建带父子关系的待分配任务。
- 浏览器预览使用 mock 数据；Tauri 内使用 Rust command 与 SQLite。

## 开发

要求 Node.js 22、pnpm、Rust stable 和 Tauri 对应平台依赖。

```bash
pnpm install
pnpm tauri:dev
```

仅运行前端：

```bash
pnpm dev
```

## 验证

```bash
pnpm build
cd src-tauri && cargo test
```

## 打包

与 Quka 相同，Tauri 生成 Windows NSIS/MSI 和 macOS DMG：

```bash
pnpm tauri:build:windows
pnpm tauri:build:macos-arm
pnpm tauri:build:macos-intel
```

## GitHub Releases 与自动更新

`.github/workflows/release.yml` 会在推送 `v*` 标签后：

1. 从标签同步 `package.json`、`Cargo.toml` 和 `tauri.conf.json` 版本。
2. 构建 Windows x64、macOS Apple Silicon、macOS Intel 安装包。
3. 上传到 GitHub Releases。
4. 生成签名更新包和 `latest.json`。

首次发布前必须单独生成 Multara 的 updater 密钥：

```bash
pnpm tauri signer generate -w ~/.tauri/multara.key
```

- 将私钥内容配置为 GitHub Actions Secret：`TAURI_SIGNING_PRIVATE_KEY`。
- 将密码配置为：`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`。
- 将输出的公钥替换 `src-tauri/tauri.conf.json` 中的 `__MULTARA_UPDATER_PUBLIC_KEY__`。
- 确认 updater endpoint 指向实际 GitHub 仓库。

私钥不得提交到仓库。当前占位公钥只用于开发构建，未替换前不能发布可自动更新的正式安装包。
