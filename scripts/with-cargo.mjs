import { spawn } from "node:child_process";
import { existsSync, readdirSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const delim = process.platform === "win32" ? ";" : ":";
const projectRoot = join(dirname(fileURLToPath(import.meta.url)), "..");
const localBin = join(projectRoot, "node_modules", ".bin");

function prependPath(pathValue, dir) {
  if (!dir || !existsSync(dir)) return pathValue;
  return `${dir}${delim}${pathValue}`;
}

function pathKey(env) {
  return Object.keys(env).find((key) => key.toLowerCase() === "path") ?? "PATH";
}

function parseVersion(name) {
  const match = name.replace(/^v/i, "").match(/^(\d+)\.(\d+)\.(\d+)/);
  if (!match) return null;
  return { major: Number(match[1]), minor: Number(match[2]), patch: Number(match[3]), dirName: name };
}

function meetsViteNode(version) {
  if (version.major === 20 && version.minor >= 19) return true;
  if (version.major === 22 && version.minor >= 12) return true;
  return version.major > 22;
}

function nvmNodeDir() {
  const root = process.platform === "win32"
    ? process.env.NVM_HOME || join(homedir(), "AppData", "Roaming", "nvm")
    : process.env.NVM_DIR || join(homedir(), ".nvm");
  if (!existsSync(root)) return "";

  const versions = readdirSync(root, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => parseVersion(entry.name))
    .filter((version) => version && meetsViteNode(version))
    .sort((left, right) =>
      left.major - right.major || left.minor - right.minor || left.patch - right.patch,
    );
  const best = versions.at(-1);
  if (!best) return "";

  if (process.platform === "win32") {
    const dir = join(root, best.dirName);
    return existsSync(join(dir, "node.exe")) ? dir : "";
  }

  const bin = join(root, "versions", "node", best.dirName, "bin");
  return existsSync(join(bin, "node")) ? bin : "";
}

const env = { ...process.env };
const key = pathKey(env);
let pathValue = env[key] ?? "";
pathValue = prependPath(pathValue, join(homedir(), ".cargo", "bin"));
pathValue = prependPath(pathValue, nvmNodeDir());
pathValue = prependPath(pathValue, localBin);

for (const name of Object.keys(env)) {
  if (name.toLowerCase() === "path") delete env[name];
}
env[key] = pathValue;

const child = spawn("tauri", process.argv.slice(2), {
  stdio: "inherit",
  shell: true,
  env,
  windowsHide: false,
});

function stopChild() {
  if (child.exitCode != null || child.signalCode) return;
  if (process.platform === "win32" && child.pid) {
    spawn("taskkill", ["/PID", String(child.pid), "/T"], {
      stdio: "ignore",
      windowsHide: true,
    });
    return;
  }
  child.kill("SIGTERM");
}

process.on("SIGINT", stopChild);
process.on("SIGTERM", stopChild);

child.on("exit", (code, signal) => {
  process.exit(signal ? 0 : code ?? 0);
});
