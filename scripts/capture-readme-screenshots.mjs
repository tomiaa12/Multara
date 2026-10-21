import { chromium } from "playwright";
import { mkdir } from "node:fs/promises";
import path from "node:path";
import { spawn } from "node:child_process";

const BASE = process.env.MULTARA_URL || "http://localhost:1420";
const OUT = path.resolve("screenshots");
const CHANNEL = process.env.PLAYWRIGHT_CHANNEL || "chrome";

const LOCALES = ["en", "zh-CN", "zh-TW", "ja", "ko"];
const VIEWPORTS = [
  { name: "1920", width: 1920, height: 1080 },
  { name: "567", width: 567, height: 1080 },
];

const PAGES = [
  { id: "projects", hash: "#/projects" },
  { id: "workbench", hash: "#/projects/preview-project" },
  { id: "inbox", hash: "#/inbox" },
  { id: "tasks", hash: "#/tasks" },
  { id: "task-detail", hash: "#/tasks/task-1" },
  { id: "automation", hash: "#/automation" },
  { id: "agents", hash: "#/agents" },
  { id: "agent-detail", hash: "#/agents/miku" },
  { id: "squads", hash: "#/squads" },
  { id: "squad-detail", hash: "#/squads/preview-squad" },
  { id: "stats", hash: "#/stats" },
  { id: "runtimes", hash: "#/runtimes" },
  { id: "skills", hash: "#/skills" },
  { id: "mcp", hash: "#/mcp" },
  { id: "settings", hash: "#/settings" },
];

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function isServerUp() {
  try {
    const response = await fetch(BASE, { method: "GET" });
    return response.ok;
  } catch {
    return false;
  }
}

async function waitForServer(timeoutMs = 60000) {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    if (await isServerUp()) return;
    await sleep(400);
  }
  throw new Error(`Timed out waiting for ${BASE}`);
}

async function startDevServer() {
  const child = spawn("pnpm", ["dev"], {
    cwd: path.resolve("."),
    stdio: ["ignore", "pipe", "pipe"],
    env: process.env,
  });
  let output = "";
  child.stdout?.on("data", (chunk) => {
    output += String(chunk);
  });
  child.stderr?.on("data", (chunk) => {
    output += String(chunk);
  });
  child.on("exit", (code) => {
    if (code && code !== 0) {
      console.error(output);
    }
  });
  await waitForServer();
  return child;
}

async function setLanguage(page, locale) {
  await page.evaluate(async (lang) => {
    const app = document.querySelector("#app")?.__vue_app__;
    const pinia = app?.config?.globalProperties?.$pinia;
    const store = pinia?._s?.get("app");
    if (!store?.saveSettings) throw new Error("app store missing");
    await store.saveSettings({ language: lang, theme: "dark" });
  }, locale);
  await page.waitForTimeout(350);
}

async function openHash(page, hash) {
  await page.evaluate((h) => {
    location.hash = h;
  }, hash);
  await page.waitForTimeout(500);
}

async function capture() {
  const browser = await chromium.launch({ headless: true, channel: CHANNEL });
  const context = await browser.newContext({
    deviceScaleFactor: 1,
    colorScheme: "dark",
  });
  const page = await context.newPage();
  await page.goto(`${BASE}/#/projects`, { waitUntil: "domcontentloaded" });
  await page.waitForSelector("#app", { timeout: 20000 });
  await page.waitForTimeout(1200);

  let count = 0;
  for (const viewport of VIEWPORTS) {
    await page.setViewportSize({ width: viewport.width, height: viewport.height });
    for (const locale of LOCALES) {
      const dir = path.join(OUT, locale, viewport.name);
      await mkdir(dir, { recursive: true });
      await setLanguage(page, locale);
      for (const entry of PAGES) {
        await openHash(page, entry.hash);
        const file = path.join(dir, `${entry.id}.png`);
        await page.screenshot({ path: file, fullPage: false });
        count += 1;
        console.log(file);
      }
    }
  }

  await browser.close();
  console.log(`Done: ${count} screenshots overwritten under ${OUT}`);
}

async function main() {
  const alreadyUp = await isServerUp();
  let child = null;
  if (!alreadyUp) {
    console.log(`Starting mock web UI at ${BASE} …`);
    child = await startDevServer();
  } else {
    console.log(`Using existing server ${BASE}`);
  }

  try {
    await capture();
  } finally {
    if (child && !child.killed) {
      child.kill("SIGTERM");
    }
  }
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
