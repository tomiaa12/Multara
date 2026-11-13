---
name: release-tag
description: >-
  发布 Multara 的 git tag 版本并触发 GitHub Release。用户说发版、发布 tag、打 tag、
  release、push tag，或要求按上次的方式发一个版本时使用。
---

# 发布 tag 版本

版本号以 `v*` tag 为准。`package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml` 里的 `1.0.9` 是占位，不要为发版去改它们，也不要改 `Cargo.lock`。CI 在打 tag 后执行 `node scripts/set-version.mjs <tag>`，只改构建机上的那三份文件。

## 选版本

1. `git tag -l 'v*' --sort=-v:refname | head -5` 看最新 tag。`v1.0.x` 是旧序列，从 `v1.1.0` 起才是当前序列。
2. `git log <最新tag>..HEAD --oneline` 看这次要发的提交。
3. 自 `v1.1.0` 起，带功能的发版升一位次版本号：`v1.9.0` 的下一版是 `v1.10.0`。上次 tag 之后只有修复、或用户明确说补丁版时，才加修订号（先例：`v1.7.1`）。用户指定了版本号就用用户的。

## 发之前

- 当前分支是 `main`，且要发的提交已经在 `origin/main`。工作区里还有未提交改动时，先问用户哪些要进这个版本，不要把无关改动一起提交。
- 跑 `pnpm exec vue-tsc --noEmit`。发布工作流的 `pnpm build` 第一步就是它；`v1.9.0` 就是在这里失败的，tag 已经推上去但没有生成 Release。类型检查失败就停，不要打 tag。
- 目标 tag 不能已经存在：`git rev-parse -q --verify refs/tags/vX.Y.Z`。已有的 tag 不覆盖、不删除、不 force push。

## 打 tag 并推送

附注 tag，说明只有一行，和既有 tag 一样：

```bash
git tag -a vX.Y.Z -m "Multara X.Y.Z"
git push origin main
git push origin vX.Y.Z
```

不要 `git tag` 不带 `-a`，不要自己 `gh release create`。`.github/workflows/release.yml` 在 `v*` tag 推上去之后会打包 Windows（nsis）和 Apple Silicon Mac（app、dmg），先建 draft，成功后再取消 draft。`publish` 只在 tag 推送时运行。

## 推送之后

用 `gh run list --workflow=release.yml --limit 3` 确认这次 tag 的 run 已经出现，把 run 链接告诉用户。完整打包大约一刻钟，不必空等结束。失败时看 `gh run view <id> --log-failed`，在新提交上修好后发下一个版本；移动或重打同一个 tag 要用户明确要求。
