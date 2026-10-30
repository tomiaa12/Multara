/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import { e2eVitePlugin } from "./scripts/e2e-vite-plugin";

const host = process.env.TAURI_DEV_HOST;

/**
 * 大依赖各自成 chunk：入口不再是单个巨型文件，改业务代码时这些 chunk 的缓存也不失效。
 * mermaid 不在这里：它本来就是动态导入并自动拆分，强行合并会和入口共享依赖而被预加载。
 */
const VENDOR_CHUNKS: Array<[chunk: string, packages: string[]]> = [
  ["monaco", ["monaco-editor", "@guolao/vue-monaco-editor"]],
  ["xterm", ["@xterm/"]],
  ["highlight", ["highlight.js"]],
  ["dicebear", ["@dicebear/"]],
  ["markdown", ["marked", "dompurify"]],
];

function vendorChunk(id: string) {
  const path = id.split("node_modules/").at(-1);
  if (!path || path === id) return undefined;
  return VENDOR_CHUNKS.find(([, packages]) =>
    packages.some((name) => path.startsWith(name.endsWith("/") ? name : `${name}/`)),
  )?.[0];
}

export default defineConfig({
  plugins: [vue(), e2eVitePlugin()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    rollupOptions: {
      output: { manualChunks: vendorChunk },
    },
  },
  test: {
    include: ["src/**/*.test.ts"],
    environment: "node",
    exclude: ["src/e2e/**"],
  },
});
