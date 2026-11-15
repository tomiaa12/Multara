/**
 * 浏览器 DEV：起真实 Tauri 后端，但主窗口保持隐藏。
 * 打开 http://localhost:1420 即可；桌面 UI 请用 pnpm tauri:dev。
 */
process.env.MULTARA_BROWSER_DEV = "1";
process.argv.splice(2, process.argv.length, "dev");
console.log("Multara browser-dev: 窗口隐藏，请打开 http://localhost:1420/");
await import("./with-cargo.mjs");
