import { t } from "../../i18n";

export type GitStatusLetter = "M" | "U" | "A" | "D" | "R" | "C" | "";

function isConflict(indexStatus: string, worktreeStatus: string) {
  if (indexStatus === "U" || worktreeStatus === "U") return true;
  return (
    (indexStatus === "A" && worktreeStatus === "A") ||
    (indexStatus === "D" && worktreeStatus === "D")
  );
}

export function gitStatusLetter(status: {
  worktreeStatus: string;
  indexStatus: string;
}): GitStatusLetter {
  if (isConflict(status.indexStatus, status.worktreeStatus)) return "C";
  const value =
    status.worktreeStatus !== " " ? status.worktreeStatus : status.indexStatus;
  if (value === "?") return "U";
  if (value === "C") return "A";
  if (value === "M" || value === "A" || value === "D" || value === "R") return value;
  return value.trim() ? "M" : "";
}

export function gitStatusLetterFor(
  status: { worktreeStatus: string; indexStatus: string },
  side: "index" | "worktree",
): GitStatusLetter {
  if (isConflict(status.indexStatus, status.worktreeStatus)) return "C";
  if (side === "index") {
    return gitStatusLetter({ indexStatus: status.indexStatus, worktreeStatus: " " });
  }
  return gitStatusLetter({ indexStatus: " ", worktreeStatus: status.worktreeStatus });
}

export function gitStatusClass(letter: string) {
  if (letter === "U") return "git-untracked";
  if (letter === "A") return "git-added";
  if (letter === "D") return "git-deleted";
  if (letter === "R") return "git-renamed";
  if (letter === "C") return "git-conflict";
  return letter ? "git-modified" : "";
}

export function gitStatusTitle(letter: string) {
  if (!letter || letter === "") return "";
  return t(`git.status.${letter}`);
}

export function folderGitStatus(letters: string[]): GitStatusLetter {
  const present = letters.filter(Boolean);
  if (present.some((letter) => letter === "C")) return "C";
  if (present.some((letter) => letter === "A" || letter === "U")) return "U";
  if (present.some((letter) => letter === "D")) return "D";
  if (present.some((letter) => letter === "R")) return "R";
  return present.length ? "M" : "";
}
