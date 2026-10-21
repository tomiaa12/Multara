import { typescript } from "monaco-editor";
import { monaco } from "../monaco";
import * as backend from "../services/backend";
import type { LocatedFile } from "../components/workbench/workspace-file-location";
import { SCRIPT_EXTENSIONS } from "./definition-heuristics";
import { applyWorkspaceTypescript } from "./monaco-workspace";
import { mergeWorkspaceCompilerOptions, type WorkspaceCompilerOptions } from "./tsconfig-options";
import { workspaceModelUri } from "./workspace-models";

type WorkspaceEntry = {
  mtimes: Map<string, number>;
  typings: backend.TypingFileRecord[];
  refreshing: Promise<void> | null;
  queued: boolean;
};

/**
 * 让 Monaco 的 TS 服务看到整个工作区：源码建成模型（随磁盘变化增量刷新），
 * 依赖的 `.d.ts` 注册成 extraLib。跨文件的 Ctrl+点击、F12、Peek 都依赖这里。
 */
class WorkspaceCodeIndex {
  private readonly workspaces = new Map<string, WorkspaceEntry>();
  private readonly owners = new Map<object, string[]>();
  private readonly openCheckers = new Set<(workspaceId: string, path: string) => boolean>();
  private readonly compilerOptions = new Map<string, WorkspaceCompilerOptions>();
  private typingLocations = new Map<string, LocatedFile | null>();

  setWorkspaces(owner: object, workspaceIds: string[]) {
    this.owners.set(owner, workspaceIds);
    this.reconcile();
  }

  release(owner: object) {
    this.owners.delete(owner);
    this.reconcile();
  }

  /** 打开的标签页由工作台自己同步内容，索引不覆盖它们，避免冲掉未保存的修改。 */
  registerOpenChecker(check: (workspaceId: string, path: string) => boolean) {
    this.openCheckers.add(check);
    return () => this.openCheckers.delete(check);
  }

  setCompilerOptions(workspaceId: string, options: WorkspaceCompilerOptions) {
    this.compilerOptions.set(workspaceId, options);
    this.applyCompilerOptions();
  }

  refreshAll() {
    for (const id of this.workspaces.keys()) void this.refresh(id);
  }

  /**
   * extraLib 按 npm 扁平布局摆放；pnpm 下真实文件在 `.pnpm` 里。
   * `undefined`：不是类型声明；`null`：是声明但在工作区外，打不开。
   */
  typingLocation(uri: monaco.Uri): LocatedFile | null | undefined {
    const key = uri.path;
    return this.typingLocations.has(key) ? this.typingLocations.get(key)! : undefined;
  }

  private reconcile() {
    const wanted = new Set([...this.owners.values()].flat());
    for (const id of [...this.workspaces.keys()]) {
      if (!wanted.has(id)) this.remove(id);
    }
    let added = false;
    for (const id of wanted) {
      if (this.workspaces.has(id)) continue;
      added = true;
      this.workspaces.set(id, { mtimes: new Map(), typings: [], refreshing: null, queued: false });
      void this.refresh(id);
      void this.loadTypings(id);
    }
    if (added) this.applyCompilerOptions();
  }

  private remove(workspaceId: string) {
    const entry = this.workspaces.get(workspaceId);
    if (!entry) return;
    this.workspaces.delete(workspaceId);
    for (const path of entry.mtimes.keys()) {
      if (!this.isOpen(workspaceId, path)) monaco.editor.getModel(workspaceModelUri(workspaceId, path))?.dispose();
    }
    this.applyExtraLibs();
    this.applyCompilerOptions();
  }

  private refresh(workspaceId: string): Promise<void> {
    const entry = this.workspaces.get(workspaceId);
    if (!entry) return Promise.resolve();
    if (entry.refreshing) {
      entry.queued = true;
      return entry.refreshing;
    }
    entry.refreshing = this.syncSources(workspaceId, entry)
      .catch(() => undefined)
      .finally(() => {
        entry.refreshing = null;
        if (entry.queued) {
          entry.queued = false;
          void this.refresh(workspaceId);
        }
      });
    return entry.refreshing;
  }

  private async syncSources(workspaceId: string, entry: WorkspaceEntry) {
    const known: Record<string, number> = {};
    for (const [path, mtime] of entry.mtimes) {
      // 模型可能被编辑器卸载时顺手销毁，那就当作没见过，重新传内容。
      if (monaco.editor.getModel(workspaceModelUri(workspaceId, path))) known[path] = mtime;
    }
    const batch = await backend.collectSourceFiles(workspaceId, SCRIPT_EXTENSIONS, known);
    if (this.workspaces.get(workspaceId) !== entry) return;
    const seen = new Set<string>();
    for (const file of batch.files) {
      seen.add(file.path);
      entry.mtimes.set(file.path, file.mtime);
      if (file.content == null) continue;
      const uri = workspaceModelUri(workspaceId, file.path);
      const model = monaco.editor.getModel(uri);
      if (!model) {
        monaco.editor.createModel(file.content, undefined, uri);
      } else if (!this.isOpen(workspaceId, file.path) && model.getValue() !== file.content) {
        model.setValue(file.content);
      }
    }
    for (const path of [...entry.mtimes.keys()]) {
      if (seen.has(path)) continue;
      entry.mtimes.delete(path);
      if (!this.isOpen(workspaceId, path)) monaco.editor.getModel(workspaceModelUri(workspaceId, path))?.dispose();
    }
  }

  private async loadTypings(workspaceId: string) {
    let typings: backend.TypingFileRecord[] = [];
    try {
      typings = await backend.collectWorkspaceTypings(workspaceId);
    } catch {
      return;
    }
    const entry = this.workspaces.get(workspaceId);
    if (!entry) return;
    entry.typings = typings;
    this.applyExtraLibs();
  }

  private applyExtraLibs() {
    const libs: Array<{ content: string; filePath: string }> = [];
    const locations = new Map<string, LocatedFile | null>();
    for (const [workspaceId, entry] of this.workspaces) {
      for (const typing of entry.typings) {
        const path = `/${workspaceId}/${typing.path}`;
        // 不能用 Uri.toString()：它会把 `@scope` 编码成 `%40scope`，TS 拼出的模块路径就对不上了。
        libs.push({ content: typing.content, filePath: `file://${path}` });
        locations.set(path, typing.diskPath ? { workspaceId, path: typing.diskPath } : null);
      }
    }
    this.typingLocations = locations;
    typescript.typescriptDefaults.setExtraLibs(libs);
    typescript.javascriptDefaults.setExtraLibs(libs);
  }

  private applyCompilerOptions() {
    applyWorkspaceTypescript(
      mergeWorkspaceCompilerOptions(
        [...this.workspaces.keys()].map((workspaceId) => ({
          workspaceId,
          options: this.compilerOptions.get(workspaceId) ?? {},
        })),
      ),
    );
  }

  private isOpen(workspaceId: string, path: string) {
    for (const check of this.openCheckers) {
      if (check(workspaceId, path)) return true;
    }
    const model = monaco.editor.getModel(workspaceModelUri(workspaceId, path));
    return Boolean(model && monaco.editor.getEditors().some((editor) => editor.getModel() === model));
  }
}

export const codeIndex = new WorkspaceCodeIndex();
