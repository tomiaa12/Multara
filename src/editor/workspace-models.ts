import { monaco } from "../monaco";
import {
  matchLongestWorkspaceRoot,
  type LocatedFile,
  type WorkspacePathRoot,
} from "../components/workbench/workspace-file-location";
import { normalizeDriveLetter } from "./definition-heuristics";

/** 必须和 `EditorWithDiff` 里 `VueMonacoEditor` 的 `:path` 一致，否则同一文件会有两份模型。 */
export function workspaceModelUri(workspaceId: string, path: string) {
  return monaco.Uri.parse(`/${workspaceId}/${path}`);
}

const rootsByOwner = new Map<object, WorkspacePathRoot[]>();
/** 每次建模型、每次跳转定义都会查，按工作区根变化失效。 */
let knownCache: { list: WorkspacePathRoot[]; ids: Set<string> } | null = null;

export function setWorkspaceRoots(owner: object, roots: WorkspacePathRoot[]) {
  rootsByOwner.set(owner, roots);
  knownCache = null;
}

export function releaseWorkspaceRoots(owner: object) {
  rootsByOwner.delete(owner);
  knownCache = null;
}

function known() {
  if (knownCache) return knownCache;
  const byId = new Map<string, WorkspacePathRoot>();
  for (const roots of rootsByOwner.values()) {
    for (const root of roots) byId.set(root.id, root);
  }
  knownCache = { list: [...byId.values()], ids: new Set(byId.keys()) };
  return knownCache;
}

export function knownWorkspaces(): WorkspacePathRoot[] {
  return [...known().list];
}

export function workspaceRootPath(workspaceId: string): string | null {
  return knownWorkspaces().find((item) => item.id === workspaceId)?.roots[0] ?? null;
}

export function locateModel(uri: monaco.Uri): LocatedFile | null {
  if (uri.scheme !== "file") return null;
  const [, workspaceId, ...rest] = uri.path.split("/");
  if (!workspaceId || !rest.length) return null;
  if (!known().ids.has(workspaceId)) return null;
  return { workspaceId, path: rest.join("/") };
}

export function locateFsPath(path: string): LocatedFile | null {
  const roots = knownWorkspaces().map((item) => ({
    ...item,
    roots: item.roots.map(normalizeDriveLetter),
  }));
  return matchLongestWorkspaceRoot(normalizeDriveLetter(path), roots);
}
