import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, rmSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { setTimeout as sleep } from "node:timers/promises";

const projectRoot = join(dirname(fileURLToPath(import.meta.url)), "..");
const tauriConf = JSON.parse(readFileSync(join(projectRoot, "src-tauri", "tauri.conf.json"), "utf8"));
const identifier = String(tauriConf.identifier || "com.multara.desktop");
const productName = String(tauriConf.productName || "Multara");
const RETRIES = 8;

function home(...parts) {
  return join(homedir(), ...parts);
}

function envPath(name, fallback) {
  const value = process.env[name];
  return value && value.trim() ? value : fallback;
}

function appStateDirs() {
  if (process.platform === "win32") {
    const roaming = envPath("APPDATA", home("AppData", "Roaming"));
    const local = envPath("LOCALAPPDATA", home("AppData", "Local"));
    return [
      join(roaming, identifier),
      join(local, identifier),
      join(roaming, productName),
      join(local, productName),
    ];
  }
  if (process.platform === "darwin") {
    return [
      home("Library", "Application Support", identifier),
      home("Library", "Caches", identifier),
      home("Library", "WebKit", identifier),
      home("Library", "HTTPStorages", identifier),
      home("Library", "Application Support", productName),
      home("Library", "Caches", productName),
    ];
  }
  const data = envPath("XDG_DATA_HOME", home(".local", "share"));
  const config = envPath("XDG_CONFIG_HOME", home(".config"));
  const cache = envPath("XDG_CACHE_HOME", home(".cache"));
  return [
    join(data, identifier),
    join(config, identifier),
    join(cache, identifier),
    join(data, productName),
    join(config, productName),
    join(cache, productName),
  ];
}

function stopApp() {
  if (process.platform === "win32") {
    for (const name of ["Multara.exe", "multara.exe"]) {
      spawnSync("taskkill", ["/IM", name, "/F", "/T"], { stdio: "ignore", windowsHide: true });
    }
    return;
  }
  spawnSync("pkill", ["-x", productName], { stdio: "ignore" });
  spawnSync("pkill", ["-x", "multara"], { stdio: "ignore" });
}

function removeDir(path) {
  rmSync(path, { recursive: true, force: true, maxRetries: 3, retryDelay: 150 });
}

const targets = [...new Set(appStateDirs())].filter((path) => existsSync(path));

if (targets.length === 0) {
  console.log("没有找到 Multara 本地数据，已经是全新状态。");
  process.exit(0);
}

console.log("将删除以下本地数据（项目源码目录不会动）：");
for (const path of targets) {
  console.log(`  ${path}`);
}

console.log("正在退出 Multara…");
stopApp();
await sleep(400);

const leftover = [];
for (const path of targets) {
  let lastError;
  for (let attempt = 0; attempt < RETRIES; attempt++) {
    if (!existsSync(path)) {
      lastError = undefined;
      break;
    }
    try {
      removeDir(path);
      lastError = undefined;
      break;
    } catch (error) {
      lastError = error;
      if (attempt === 2) stopApp();
      await sleep(250 * (attempt + 1));
    }
  }
  if (lastError || existsSync(path)) {
    leftover.push({ path, error: lastError });
    continue;
  }
  console.log(`已删除 ${path}`);
}

if (leftover.length) {
  console.error("\n部分目录仍被占用，删除失败。");
  for (const { path, error } of leftover) {
    console.error(`  ${path}: ${error instanceof Error ? error.message : error ?? "仍存在"}`);
  }
  process.exit(1);
}

console.log("\n已重置。再次打开软件会像第一次安装一样。");
