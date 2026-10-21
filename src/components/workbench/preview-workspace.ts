import type {
  FileContent,
  FileEntry,
  GitRepositoryStatus,
  Workspace,
} from "../../domain/types";

export const PREVIEW_WORKSPACE_ID = "preview-workspace";
export const PREVIEW_WORKSPACE_B_ID = "preview-workspace-b";
export const PREVIEW_PROJECT_ID = "preview-project";

export const previewWorkspace: Workspace = {
  id: PREVIEW_WORKSPACE_ID,
  projectId: PREVIEW_PROJECT_ID,
  name: "preview-workspace",
  kind: "git",
  sourcePath: "/preview",
  workPath: "/preview",
  branch: "main",
  isolationMode: "direct",
  createdAt: "",
};

export const previewGitRepository: GitRepositoryStatus = {
  workspaceId: PREVIEW_WORKSPACE_ID,
  name: "preview-workspace",
  path: "/preview",
  branch: "main",
  upstream: null,
  ahead: 0,
  behind: 0,
  hasRemote: false,
  merging: false,
  changes: [
    { path: "src/composer.ts", indexStatus: " ", worktreeStatus: "M", staged: false },
    { path: "src/router.ts", indexStatus: " ", worktreeStatus: "?", staged: false },
    { path: "src/App.vue", indexStatus: "U", worktreeStatus: "U", staged: true },
    { path: "docs/guide.md", indexStatus: " ", worktreeStatus: "D", staged: false },
    { path: "package.json", indexStatus: "R", worktreeStatus: " ", staged: true },
    { path: "README.md", indexStatus: "A", worktreeStatus: " ", staged: true },
  ],
  branches: ["main"],
  remoteBranches: [],
};

export const previewRootEntries: FileEntry[] = [
  { name: "src", path: "src", isDirectory: true, depth: 0 },
  { name: "src-tauri", path: "src-tauri", isDirectory: true, depth: 0 },
  { name: "docs", path: "docs", isDirectory: true, depth: 0 },
  { name: "package.json", path: "package.json", isDirectory: false, depth: 0 },
  { name: "README.md", path: "README.md", isDirectory: false, depth: 0 },
  { name: "multara-agent-protocol-test.txt", path: "multara-agent-protocol-test.txt", isDirectory: false, depth: 0 },
];

export const previewDirectoryChildren: Record<string, FileEntry[]> = {
  [`${PREVIEW_WORKSPACE_ID}:src`]: [
    { name: "App.vue", path: "src/App.vue", isDirectory: false, depth: 0 },
    { name: "composer.ts", path: "src/composer.ts", isDirectory: false, depth: 0 },
    { name: "router.ts", path: "src/router.ts", isDirectory: false, depth: 0 },
  ],
  [`${PREVIEW_WORKSPACE_ID}:src-tauri`]: [
    { name: "Cargo.toml", path: "src-tauri/Cargo.toml", isDirectory: false, depth: 0 },
    { name: "src", path: "src-tauri/src", isDirectory: true, depth: 0 },
  ],
  [`${PREVIEW_WORKSPACE_ID}:docs`]: [
    { name: "guide.md", path: "docs/guide.md", isDirectory: false, depth: 0 },
  ],
  [`${PREVIEW_WORKSPACE_B_ID}:src`]: [
    { name: "main.ts", path: "src/main.ts", isDirectory: false, depth: 0 },
  ],
};

const files: Record<string, string> = {
  "package.json": `{
  "name": "multara",
  "private": true,
  "scripts": { "dev": "vite", "tauri": "node scripts/with-cargo.mjs" }
}
`,
  "README.md": "# Multara\n\n本地多智能体工作区。\n",
  "src-tauri/Cargo.toml": "[package]\nname = \"multara\"\n",
  "src/App.vue": `<script setup lang="ts">
import ChatPanel from "../components/ChatPanel.vue";
</script>
`,
  "src/composer.ts": `export function formatAgentPrompt(nodes) {
  return nodes.map((node) => node.value).join("");
}
`,
  "docs/guide.md": `# 工作台预览\n\n从左侧拖文件或文件夹到输入框。\n`,
  "multara-agent-protocol-test.txt": "round-1: tool-and-terminal-complete\n",
  "src/main.ts": "export const title = \"admin\"\n",
};

function languageFor(path: string) {
  if (path.endsWith(".vue")) return "html";
  if (path.endsWith(".ts") || path.endsWith(".js")) return "typescript";
  if (path.endsWith(".json")) return "json";
  if (path.endsWith(".md")) return "markdown";
  return "plaintext";
}

export const previewWorkspaceB: Workspace = {
  id: PREVIEW_WORKSPACE_B_ID,
  projectId: PREVIEW_PROJECT_ID,
  name: "admin",
  kind: "git",
  sourcePath: "/preview-admin",
  workPath: "/preview-admin",
  branch: "dev",
  isolationMode: "worktree",
  createdAt: "",
};

export const previewGitRepositoryB: GitRepositoryStatus = {
  workspaceId: PREVIEW_WORKSPACE_B_ID,
  name: "admin",
  path: "/preview-admin",
  branch: "dev",
  upstream: "origin/dev",
  ahead: 1,
  behind: 0,
  hasRemote: true,
  merging: false,
  changes: [
    { path: "src/main.ts", indexStatus: " ", worktreeStatus: "M", staged: false },
  ],
  branches: ["dev", "main"],
  remoteBranches: ["origin/dev", "origin/main"],
};

const ADMIN_ONLY = new Set(["src/main.ts"]);

export function previewSearchFiles(workspaceId: string): Array<{ path: string; content: string }> {
  if (workspaceId === PREVIEW_WORKSPACE_B_ID) {
    return [{ path: "src/main.ts", content: files["src/main.ts"] ?? "" }];
  }
  return Object.entries(files)
    .filter(([path]) => !ADMIN_ONLY.has(path))
    .map(([path, content]) => ({ path, content }));
}

export function setPreviewFileContent(path: string, content: string) {
  files[path] = content;
}

export function previewFileContent(path: string, workspaceId = PREVIEW_WORKSPACE_ID): FileContent {
  const content = files[path] ?? "";
  return {
    workspaceId,
    path,
    content,
    language: languageFor(path),
    size: content.length,
    readOnly: false,
  };
}
