import { monaco } from "../monaco";
import * as backend from "../services/backend";
import type { LocatedFile, WorkspacePathRoot } from "../components/workbench/workspace-file-location";
import { codeIndex } from "./code-index";
import {
  componentFileNames,
  definitionColumn,
  definitionPattern,
  findDefinitionLines,
  isScriptPath,
  resolveImportSpecifier,
  searchIncludeFor,
  stringLiteralAt,
} from "./definition-heuristics";
import { languageServers } from "./lsp/language-servers";
import {
  knownWorkspaces,
  locateModel,
  releaseWorkspaceRoots,
  setWorkspaceRoots,
  workspaceModelUri,
} from "./workspace-models";

export type NavigationTarget = LocatedFile & {
  startLine?: number;
  startColumn?: number;
  endLine?: number;
  endColumn?: number;
};

const MAX_SEARCH_RESULTS = 10;
const MAX_PEEK_MODELS = 10;
const SCRIPT_LANGUAGES = new Set(["typescript", "javascript"]);

let installed = false;
const fileLists = new Map<string, Promise<Set<string>>>();
const fileNameIndexes = new WeakMap<Set<string>, Map<string, string[]>>();
const peekModels = new Map<monaco.editor.ITextModel, number>();

/** 全局注册一次：定义提供器对所有语言生效，TS/JS 只补充它自己解析不了的 import（.vue/.css 等）。 */
export function installCodeNavigation() {
  if (installed) return;
  installed = true;
  languageServers.start();
  monaco.languages.registerDefinitionProvider({ scheme: "file" }, { provideDefinition });
}

export function setNavigationWorkspaces(owner: object, roots: WorkspacePathRoot[]) {
  setWorkspaceRoots(owner, roots);
  codeIndex.setWorkspaces(owner, roots.map((item) => item.id));
  languageServers.retain(new Set(knownWorkspaces().map((item) => item.id)));
}

export function releaseNavigationWorkspaces(owner: object) {
  releaseWorkspaceRoots(owner);
  codeIndex.release(owner);
  languageServers.retain(new Set(knownWorkspaces().map((item) => item.id)));
}

/** 磁盘有变化（保存、Agent 写文件、git 切分支）时调用。 */
export function refreshNavigationIndex() {
  fileLists.clear();
  codeIndex.refreshAll();
}

/**
 * 跳到别的文件时 Monaco 只会询问 opener；只接管自己编辑器发起的跳转，
 * 同一页面里其他 Monaco 实例（技能编辑器等）不受影响。
 */
export function registerWorkspaceOpener(options: {
  owns: (editor: monaco.editor.ICodeEditor) => boolean;
  open: (target: NavigationTarget) => Promise<void> | void;
}) {
  return monaco.editor.registerEditorOpener({
    openCodeEditor(source, resource, selectionOrPosition) {
      if (!options.owns(source)) return false;
      const typing = codeIndex.typingLocation(resource);
      if (typing === null) return false;
      const located = typing ?? locateModel(resource);
      if (!located) return false;
      const target: NavigationTarget = { ...located };
      if (monaco.Range.isIRange(selectionOrPosition)) {
        target.startLine = selectionOrPosition.startLineNumber;
        target.startColumn = selectionOrPosition.startColumn;
        target.endLine = selectionOrPosition.endLineNumber;
        target.endColumn = selectionOrPosition.endColumn;
      } else if (selectionOrPosition) {
        target.startLine = selectionOrPosition.lineNumber;
        target.startColumn = selectionOrPosition.column;
      }
      void options.open(target);
      return true;
    },
  });
}

async function provideDefinition(
  model: monaco.editor.ITextModel,
  position: monaco.Position,
  token: monaco.CancellationToken,
): Promise<monaco.languages.Definition | null> {
  const target = locateModel(model.uri);
  if (!target) return null;
  if (SCRIPT_LANGUAGES.has(model.getLanguageId())) {
    const imported = await importDefinition(model, position, target);
    return imported && !isScriptPath(imported.path) ? finish([lineOne(imported)]) : null;
  }
  const viaServer = await languageServers.definition(model, position).catch(() => null);
  if (viaServer?.length) return finish(viaServer);
  if (token.isCancellationRequested) return null;
  return finish(await heuristicDefinition(model, position, target, token));
}

async function heuristicDefinition(
  model: monaco.editor.ITextModel,
  position: monaco.Position,
  target: LocatedFile,
  token: monaco.CancellationToken,
): Promise<monaco.languages.Location[]> {
  const imported = await importDefinition(model, position, target);
  if (imported) return [lineOne(imported)];

  const word = model.getWordAtPosition(position)?.word;
  if (!word) return [];

  const components = componentFileNames(word);
  if (components.length) {
    const matches = filesNamed(await fileList(target.workspaceId), components);
    if (matches.length) {
      return matches.slice(0, MAX_SEARCH_RESULTS).map((path) => lineOne({ workspaceId: target.workspaceId, path }));
    }
  }

  if (!/^[A-Za-z_$][\w$]*$/.test(word)) return [];
  const local = findDefinitionLines(model.getLinesContent(), word).filter(
    (match) => match.line !== position.lineNumber,
  );
  if (local.length) return local.map((match) => wordLocation(model.uri, match.line, match.column, word));
  if (token.isCancellationRequested) return [];

  const response = await backend.searchWorkspace(target.workspaceId, {
    query: definitionPattern(word),
    caseSensitive: true,
    wholeWord: false,
    regex: true,
    include: searchIncludeFor(target.path),
    exclude: "",
  });
  const extension = target.path.split(".").pop();
  return response.matches
    .filter((match) => match.path !== target.path)
    .sort((a, b) => Number(b.path.endsWith(`.${extension}`)) - Number(a.path.endsWith(`.${extension}`)))
    .slice(0, MAX_SEARCH_RESULTS)
    .map((match) =>
      wordLocation(
        workspaceModelUri(target.workspaceId, match.path),
        match.line,
        definitionColumn(match.column, match.length, word),
        word,
      ),
    );
}

async function importDefinition(
  model: monaco.editor.ITextModel,
  position: monaco.Position,
  target: LocatedFile,
): Promise<LocatedFile | null> {
  const specifier = stringLiteralAt(model.getLineContent(position.lineNumber), position.column);
  if (!specifier) return null;
  const files = await fileList(target.workspaceId);
  const path = resolveImportSpecifier(specifier, target.path, (candidate) => files.has(candidate));
  return path ? { workspaceId: target.workspaceId, path } : null;
}

function fileList(workspaceId: string): Promise<Set<string>> {
  let list = fileLists.get(workspaceId);
  if (!list) {
    list = backend
      .listQuickOpenFiles(workspaceId)
      .then((entries) => new Set(entries.filter((entry) => !entry.isDirectory).map((entry) => entry.path.replaceAll("\\", "/"))))
      .catch(() => {
        fileLists.delete(workspaceId);
        return new Set<string>();
      });
    fileLists.set(workspaceId, list);
  }
  return list;
}

/** 按文件名建索引，跟着文件列表一起失效（refreshNavigationIndex 会换掉 Set）。 */
function filesNamed(files: Set<string>, names: string[]) {
  let index = fileNameIndexes.get(files);
  if (!index) {
    index = new Map<string, string[]>();
    for (const path of files) {
      const name = path.split("/").pop() ?? "";
      if (!index.has(name)) index.set(name, []);
      index.get(name)?.push(path);
    }
    fileNameIndexes.set(files, index);
  }
  return names.flatMap((name) => index.get(name) ?? []);
}

/**
 * 新的 Peek 会替换旧的，这时释放旧 Peek 临时建的模型。新结果还要用的、正挂在编辑器上的留到下一次；
 * 被改过的说明已经当标签页打开在编辑，不再归这里管。
 */
function releasePeekModels(keep: Set<string>) {
  for (const [model, version] of peekModels) {
    if (model.isDisposed() || model.getAlternativeVersionId() !== version) {
      peekModels.delete(model);
      continue;
    }
    if (keep.has(model.uri.toString()) || model.isAttachedToEditor()) continue;
    model.dispose();
    peekModels.delete(model);
  }
}

function lineOne(file: LocatedFile): monaco.languages.Location {
  return { uri: workspaceModelUri(file.workspaceId, file.path), range: new monaco.Range(1, 1, 1, 1) };
}

function wordLocation(uri: monaco.Uri, line: number, column: number, word: string): monaco.languages.Location {
  return { uri, range: new monaco.Range(line, column, line, column + word.length) };
}

/** 多个结果时 Monaco 用 Peek 展示，需要目标文件已有模型；单个结果直接交给 opener。 */
async function finish(locations: monaco.languages.Location[]): Promise<monaco.languages.Location[]> {
  if (locations.length < 2) return locations;
  const keys = new Set(locations.map((item) => item.uri.toString()));
  releasePeekModels(keys);
  const missing = [...keys]
    .map((key) => locations.find((item) => item.uri.toString() === key)!.uri)
    .filter((uri) => !monaco.editor.getModel(uri))
    .slice(0, MAX_PEEK_MODELS);
  await Promise.all(
    missing.map(async (uri) => {
      const located = locateModel(uri);
      if (!located) return;
      try {
        const file = await backend.readWorkspaceFile(located.workspaceId, located.path);
        if (monaco.editor.getModel(uri)) return;
        const model = monaco.editor.createModel(file.content, undefined, uri);
        peekModels.set(model, model.getAlternativeVersionId());
      } catch {
        // 文件读不到就让 Peek 显示空
      }
    }),
  );
  return locations;
}
