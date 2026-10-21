/**
 * VS Code `explorerFileNestingTrie.ts` 的移植：父模式可含一个 `*`，子模式里用
 * `${capture}`/`${basename}`/`${extname}`/`${dirname}` 引用父文件的对应部分。
 */
type FilenameAttributes = {
  basename: string;
  extname: string;
  dirname: string;
};

const SUBSTITUTION = /\$[({](capture|basename|dirname|extname)[)}]/g;

type SubstitutionType = "capture" | "basename" | "dirname" | "extname";

class SubstitutionString {
  private tokens: Array<string | { capture: SubstitutionType }> = [];

  constructor(pattern: string) {
    SUBSTITUTION.lastIndex = 0;
    let token: RegExpExecArray | null;
    let lastIndex = 0;
    while ((token = SUBSTITUTION.exec(pattern))) {
      this.tokens.push(pattern.slice(lastIndex, token.index));
      this.tokens.push({ capture: token[1] as SubstitutionType });
      lastIndex = token.index + token[0].length;
    }
    if (lastIndex !== pattern.length) this.tokens.push(pattern.slice(lastIndex));
  }

  substitute(attributes: FilenameAttributes, capture?: string): string {
    return this.tokens
      .map((token) => {
        if (typeof token === "string") return token;
        switch (token.capture) {
          case "basename":
            return attributes.basename;
          case "dirname":
            return attributes.dirname;
          case "extname":
            return attributes.extname;
          case "capture":
            return capture || "";
        }
      })
      .join("");
  }
}

export class SufTrie {
  private star: SubstitutionString[] = [];
  private epsilon: SubstitutionString[] = [];
  private map = new Map<string, SufTrie>();
  hasItems = false;

  add(key: string, value: string) {
    this.hasItems = true;
    if (key === "*") {
      this.star.push(new SubstitutionString(value));
    } else if (key === "") {
      this.epsilon.push(new SubstitutionString(value));
    } else {
      const tail = key[key.length - 1];
      const rest = key.slice(0, -1);
      if (tail === "*") throw new Error(`Unexpected star in SufTrie key: ${key}`);
      let existing = this.map.get(tail);
      if (!existing) this.map.set(tail, (existing = new SufTrie()));
      existing.add(rest, value);
    }
  }

  get(key: string, attributes: FilenameAttributes): string[] {
    const results: string[] = [];
    if (key === "") results.push(...this.epsilon.map((item) => item.substitute(attributes)));
    if (this.star.length) results.push(...this.star.map((item) => item.substitute(attributes, key)));
    const existing = this.map.get(key[key.length - 1]);
    if (existing) results.push(...existing.get(key.slice(0, -1), attributes));
    return results;
  }
}

export class PreTrie {
  private value = new SufTrie();
  private map = new Map<string, PreTrie>();

  add(key: string, value: string) {
    if (key === "" || key[0] === "*") {
      this.value.add(key, value);
      return;
    }
    let existing = this.map.get(key[0]);
    if (!existing) this.map.set(key[0], (existing = new PreTrie()));
    existing.add(key.slice(1), value);
  }

  get(key: string, attributes: FilenameAttributes): string[] {
    const results = [...this.value.get(key, attributes)];
    const existing = this.map.get(key[0]);
    if (existing) results.push(...existing.get(key.slice(1), attributes));
    return results;
  }
}

export class ExplorerFileNestingTrie {
  private root = new PreTrie();

  constructor(config: Array<[string, string[]]>) {
    for (const [parentPattern, childPatterns] of config) {
      for (const childPattern of childPatterns) this.root.add(parentPattern, childPattern);
    }
  }

  private attributes(filename: string, dirname: string): FilenameAttributes {
    const lastDot = filename.lastIndexOf(".");
    if (lastDot < 1) return { dirname, basename: filename, extname: "" };
    return { dirname, basename: filename.slice(0, lastDot), extname: filename.slice(lastDot + 1) };
  }

  nest(files: string[], dirname: string): Map<string, Set<string>> {
    const parentFinder = new PreTrie();
    for (const potentialParent of files) {
      const children = this.root.get(potentialParent, this.attributes(potentialParent, dirname));
      for (const child of children) parentFinder.add(child, potentialParent);
    }

    const findAllRootAncestors = (file: string, seen = new Set<string>()): string[] => {
      if (seen.has(file)) return [];
      seen.add(file);
      const ancestors = new Set(parentFinder.get(file, this.attributes(file, dirname)));
      ancestors.delete(file);
      if (ancestors.size === 0) return [file];
      return [...ancestors].flatMap((ancestor) => findAllRootAncestors(ancestor, seen));
    };

    const result = new Map<string, Set<string>>();
    for (const file of files) {
      let ancestors = findAllRootAncestors(file);
      if (ancestors.length === 0) ancestors = [file];
      for (const ancestor of ancestors) {
        let existing = result.get(ancestor);
        if (!existing) result.set(ancestor, (existing = new Set()));
        if (file !== ancestor) existing.add(file);
      }
    }
    return result;
  }
}

/** 与 `explorerModel.ts` 一致：值按逗号拆成多个子模式，空键或空值忽略。 */
export function nestingConfig(patterns: Record<string, string>): Array<[string, string[]]> {
  return Object.entries(patterns)
    .filter(([parent, children]) => typeof parent === "string" && typeof children === "string" && parent && children)
    .map(([parent, children]) => [
      parent.trim(),
      children
        .split(",")
        .map((child) => child.trim().replace(/\u200b/g, "").trim())
        .filter(Boolean),
    ]);
}

const trieCache = new Map<string, ExplorerFileNestingTrie>();

export function nestingTrie(patterns: Record<string, string>): ExplorerFileNestingTrie {
  const key = JSON.stringify(patterns);
  let trie = trieCache.get(key);
  if (!trie) {
    trie = new ExplorerFileNestingTrie(nestingConfig(patterns));
    trieCache.clear();
    trieCache.set(key, trie);
  }
  return trie;
}

/**
 * 只对同一目录下的文件做嵌套；返回 父文件名 → 子文件名列表，没有子文件的不出现。
 * 文件夹不参与（VS Code 同样只嵌套文件）。
 */
export function nestDirectoryFiles(
  fileNames: string[],
  dirname: string,
  patterns: Record<string, string>,
): Map<string, string[]> {
  const nested = nestingTrie(patterns).nest(fileNames, dirname);
  const result = new Map<string, string[]>();
  for (const [parent, children] of nested) {
    if (children.size) result.set(parent, [...children]);
  }
  return result;
}
