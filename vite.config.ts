/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import { e2eVitePlugin } from "./scripts/e2e-vite-plugin";

const host = process.env.TAURI_DEV_HOST;

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
  test: {
    include: ["src/**/*.test.ts"],
    environment: "node",
    exclude: ["src/e2e/**"],
  },
});
