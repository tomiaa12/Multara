import { folderGitStatus, gitStatusLetter, type GitStatusLetter } from "./tree-git-status";

export interface TreeGitFolderStatus {
  letter: GitStatusLetter;
  count: number;
}

export interface TreeGitIndex {
  files: Map<string, GitStatusLetter>;
  folders: Map<string, TreeGitFolderStatus>;
}

type GitChange = { path: string; worktreeStatus: string; indexStatus: string };

/** 目录匹配规则与逐行 filter 时相同：变更路径等于目录本身，或以 `目录/` 开头。 */
function folderKeys(path: string) {
  const keys = [path];
  for (let index = path.indexOf("/"); index >= 0; index = path.indexOf("/", index + 1)) {
    if (index) keys.push(path.slice(0, index));
  }
  return keys;
}

/** 把一个仓库的变更一次性聚合成按文件、按祖先目录查表的索引，文件树每行只查表。 */
export function buildTreeGitIndex(changes: GitChange[]): TreeGitIndex {
  const files = new Map<string, GitStatusLetter>();
  const letters = new Map<string, { letters: Set<GitStatusLetter>; count: number }>();
  for (const change of changes) {
    const letter = gitStatusLetter(change);
    if (!files.has(change.path)) files.set(change.path, letter);
    for (const key of new Set(folderKeys(change.path))) {
      const folder = letters.get(key) ?? { letters: new Set<GitStatusLetter>(), count: 0 };
      folder.letters.add(letter);
      folder.count += 1;
      letters.set(key, folder);
    }
  }
  const folders = new Map<string, TreeGitFolderStatus>();
  for (const [key, folder] of letters) {
    folders.set(key, { letter: folderGitStatus([...folder.letters]), count: folder.count });
  }
  return { files, folders };
}
