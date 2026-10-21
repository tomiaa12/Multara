import type { ExplorerIncrementalNaming } from "./explorer-settings";

const MAX_SAFE_SMALL_INTEGER = 1 << 30;

function extname(name: string) {
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot) : "";
}

/** VS Code `fileActions.ts` 中 `incrementFileName` 的移植。 */
export function incrementFileName(name: string, isFolder: boolean, naming: "simple" | "smart"): string {
  if (naming === "simple") {
    let namePrefix = name;
    let extSuffix = "";
    if (!isFolder) {
      extSuffix = extname(name);
      namePrefix = extSuffix ? name.slice(0, -extSuffix.length) : name;
    }
    // name copy 5(.txt) => name copy 6(.txt)；name copy(.txt) => name copy 2(.txt)
    const suffixRegex = /^(.+ copy)( \d+)?$/;
    if (suffixRegex.test(namePrefix)) {
      return (
        namePrefix.replace(suffixRegex, (_match, g1: string, g2?: string) => {
          const number = g2 ? parseInt(g2, 10) : 1;
          return number === 0
            ? `${g1}`
            : number < MAX_SAFE_SMALL_INTEGER
              ? `${g1} ${number + 1}`
              : `${g1}${g2} copy`;
        }) + extSuffix
      );
    }
    return `${namePrefix} copy${extSuffix}`;
  }

  const separators = "[\\.\\-_]";
  const maxNumber = MAX_SAFE_SMALL_INTEGER;
  const bump = (digits: string) => String(parseInt(digits, 10) + 1).padStart(digits.length, "0");

  // file.1.txt => file.2.txt
  const suffixFileRegex = RegExp(`(.*${separators})(\\d+)(\\..*)$`);
  if (!isFolder && suffixFileRegex.test(name)) {
    return name.replace(suffixFileRegex, (_m, g1: string, g2: string, g3: string) =>
      parseInt(g2, 10) < maxNumber ? g1 + bump(g2) + g3 : `${g1}${g2}.1${g3}`,
    );
  }

  // 1.file.txt => 2.file.txt
  const prefixFileRegex = RegExp(`(\\d+)(${separators}.*)(\\..*)$`);
  if (!isFolder && prefixFileRegex.test(name)) {
    return name.replace(prefixFileRegex, (_m, g1: string, g2: string, g3: string) =>
      parseInt(g1, 10) < maxNumber ? bump(g1) + g2 + g3 : `${g1}${g2}.1${g3}`,
    );
  }

  // 1.txt => 2.txt
  const prefixFileNoNameRegex = RegExp("(\\d+)(\\..*)$");
  if (!isFolder && prefixFileNoNameRegex.test(name)) {
    return name.replace(prefixFileNoNameRegex, (_m, g1: string, g2: string) =>
      parseInt(g1, 10) < maxNumber ? bump(g1) + g2 : `${g1}.1${g2}`,
    );
  }

  // file.txt => file.1.txt
  const lastIndexOfDot = name.lastIndexOf(".");
  if (!isFolder && lastIndexOfDot >= 0) {
    return `${name.slice(0, lastIndexOfDot)}.1${name.slice(lastIndexOfDot)}`;
  }

  // 123 => 124
  const noNameNoExtensionRegex = RegExp("(\\d+)$");
  if (!isFolder && lastIndexOfDot === -1 && noNameNoExtensionRegex.test(name)) {
    return name.replace(noNameNoExtensionRegex, (_m, g1: string) =>
      parseInt(g1, 10) < maxNumber ? bump(g1) : `${g1}.1`,
    );
  }

  // file => file1；file1 => file2
  const noExtensionRegex = RegExp("(.*)(\\d*)$");
  if (!isFolder && lastIndexOfDot === -1 && noExtensionRegex.test(name)) {
    return name.replace(noExtensionRegex, (_m, g1: string, g2: string) => {
      let number = parseInt(g2, 10);
      if (Number.isNaN(number)) number = 0;
      return number < maxNumber ? g1 + String(number + 1).padStart(g2.length, "0") : `${g1}${g2}.1`;
    });
  }

  // folder.1 => folder.2
  if (isFolder && /(\d+)$/.test(name)) {
    return name.replace(/(\d+)$/, (_m, g1: string) => (parseInt(g1, 10) < maxNumber ? bump(g1) : `${g1}.1`));
  }

  // 1.folder => 2.folder
  if (isFolder && /^(\d+)/.test(name)) {
    return name.replace(/^(\d+)(.*)$/, (_m, g1: string, g2: string) =>
      parseInt(g1, 10) < maxNumber ? bump(g1) + g2 : `${g1}${g2}.1`,
    );
  }

  return `${name}.1`;
}

/** 粘贴/复制时找一个不冲突的名字；`disabled` 时原样返回，由调用方询问是否覆盖。 */
export function findAvailableName(
  name: string,
  isFolder: boolean,
  naming: ExplorerIncrementalNaming,
  exists: (name: string) => boolean,
): string {
  if (naming === "disabled") return name;
  let candidate = name;
  for (let guard = 0; exists(candidate) && guard < 10_000; guard += 1) {
    candidate = incrementFileName(candidate, isFolder, naming);
  }
  return candidate;
}

const WINDOWS_INVALID_FILE_CHARS = /[\\/:*?"<>|]/;
const UNIX_INVALID_FILE_CHARS = /[/]/;
const WINDOWS_FORBIDDEN_NAMES = /^(con|prn|aux|clock\$|nul|lpt[0-9]|com[0-9])(\.(.*?))?$/i;

/** VS Code `extpath.isValidBasename`。 */
export function isValidBasename(name: string | null | undefined, isWindows: boolean): boolean {
  if (!name || name.length === 0 || /^\s+$/.test(name)) return false;
  if ((isWindows ? WINDOWS_INVALID_FILE_CHARS : UNIX_INVALID_FILE_CHARS).test(name)) return false;
  if (isWindows && WINDOWS_FORBIDDEN_NAMES.test(name)) return false;
  if (name === "." || name === "..") return false;
  if (isWindows && name[name.length - 1] === ".") return false;
  if (isWindows && name.length !== name.trim().length) return false;
  if (name.length > 255) return false;
  return true;
}

/** 去掉首尾制表符与末尾斜杠，对应 VS Code `getWellFormedFileName`。 */
export function wellFormedFileName(name: string): string {
  if (!name) return name;
  return name.replace(/^\t+|\t+$/g, "").replace(/[\\/]+$/, "");
}

export interface FileNameProblem {
  message: string;
  severity: "error" | "warning";
}

function trimLongName(name: string) {
  return name.length > 255 ? `${name.slice(0, 255)}...` : name;
}

/**
 * VS Code `validateFileName`：名字可以含 `/` 表示多级（会自动创建中间文件夹），
 * 但不能以斜杠开头，也不能与同级已有项重名。
 */
export function validateFileName(
  rawName: string,
  options: { currentName?: string; siblingExists: (name: string) => boolean; isWindows: boolean },
): FileNameProblem | null {
  const name = wellFormedFileName(rawName);
  if (!name || /^\s+$/.test(name)) {
    return { message: "必须提供文件或文件夹名称。", severity: "error" };
  }
  if (name[0] === "/" || name[0] === "\\") {
    return { message: "文件或文件夹名称不能以斜杠开头。", severity: "error" };
  }
  const names = name.split(/[\\/]/).filter(Boolean);
  if (names.length === 1 && name !== options.currentName && options.siblingExists(name)) {
    return { message: `此位置已存在文件或文件夹 ${name}，请选择其他名称。`, severity: "error" };
  }
  if (names.some((segment) => !isValidBasename(segment, options.isWindows))) {
    return { message: `名称 ${trimLongName(name)} 不能用作文件或文件夹名称，请选择其他名称。`, severity: "error" };
  }
  if (names.some((segment) => /^\s|\s$/.test(segment))) {
    return { message: "文件或文件夹名称首尾有空白字符。", severity: "warning" };
  }
  return null;
}

/** 重命名时默认只选中主文件名（不含扩展名），与 VS Code 一致。 */
export function renameSelection(name: string, isDirectory: boolean): [number, number] {
  const lastDot = name.lastIndexOf(".");
  if (isDirectory || lastDot <= 0) return [0, name.length];
  return [0, lastDot];
}
