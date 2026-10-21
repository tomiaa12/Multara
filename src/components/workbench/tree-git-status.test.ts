import { beforeEach, describe, expect, it } from "vitest";
import { applyLanguageSetting } from "../../i18n";
import {
  folderGitStatus,
  gitStatusClass,
  gitStatusLetter,
  gitStatusLetterFor,
  gitStatusTitle,
} from "./tree-git-status";

beforeEach(() => {
  applyLanguageSetting("zh-CN");
});

describe("gitStatusLetter", () => {
  it("maps untracked to U", () => {
    expect(gitStatusLetter({ worktreeStatus: "?", indexStatus: "?" })).toBe("U");
  });

  it("maps porcelain unmerged U to conflict C", () => {
    expect(gitStatusLetter({ worktreeStatus: "U", indexStatus: "U" })).toBe("C");
    expect(gitStatusLetter({ worktreeStatus: "U", indexStatus: "A" })).toBe("C");
  });

  it("maps both-added and both-deleted unmerged states to C", () => {
    expect(gitStatusLetter({ worktreeStatus: "A", indexStatus: "A" })).toBe("C");
    expect(gitStatusLetter({ worktreeStatus: "D", indexStatus: "D" })).toBe("C");
  });

  it("keeps added, deleted, renamed and modified", () => {
    expect(gitStatusLetter({ worktreeStatus: " ", indexStatus: "A" })).toBe("A");
    expect(gitStatusLetter({ worktreeStatus: "D", indexStatus: " " })).toBe("D");
    expect(gitStatusLetter({ worktreeStatus: " ", indexStatus: "R" })).toBe("R");
    expect(gitStatusLetter({ worktreeStatus: "M", indexStatus: " " })).toBe("M");
  });

  it("maps git copied C to added", () => {
    expect(gitStatusLetter({ worktreeStatus: " ", indexStatus: "C" })).toBe("A");
  });

  it("prefers worktree status over index when not conflicted", () => {
    expect(gitStatusLetter({ worktreeStatus: "M", indexStatus: "A" })).toBe("M");
  });

  it("splits a staged-and-modified file by list", () => {
    const status = { indexStatus: "A", worktreeStatus: "M" };
    expect(gitStatusLetterFor(status, "index")).toBe("A");
    expect(gitStatusLetterFor(status, "worktree")).toBe("M");
    expect(gitStatusLetterFor({ indexStatus: "M", worktreeStatus: "M" }, "index")).toBe("M");
    expect(gitStatusLetterFor({ indexStatus: "M", worktreeStatus: "M" }, "worktree")).toBe("M");
  });
});

describe("gitStatusClass and title", () => {
  it("covers every displayed letter", () => {
    expect(gitStatusClass("M")).toBe("git-modified");
    expect(gitStatusClass("U")).toBe("git-untracked");
    expect(gitStatusClass("A")).toBe("git-added");
    expect(gitStatusClass("D")).toBe("git-deleted");
    expect(gitStatusClass("R")).toBe("git-renamed");
    expect(gitStatusClass("C")).toBe("git-conflict");
    expect(gitStatusTitle("M")).toBe("修改");
    expect(gitStatusTitle("U")).toBe("未跟踪");
    expect(gitStatusTitle("A")).toBe("新增");
    expect(gitStatusTitle("D")).toBe("删除");
    expect(gitStatusTitle("R")).toBe("重命名");
    expect(gitStatusTitle("C")).toBe("冲突");
  });
});

describe("folderGitStatus", () => {
  it("prefers conflict, then added, then deleted, then renamed", () => {
    expect(folderGitStatus(["M", "U", "C"])).toBe("C");
    expect(folderGitStatus(["M", "U", "D"])).toBe("U");
    expect(folderGitStatus(["M", "A"])).toBe("U");
    expect(folderGitStatus(["M", "D"])).toBe("D");
    expect(folderGitStatus(["M", "R"])).toBe("R");
  });

  it("falls back to modified", () => {
    expect(folderGitStatus(["M"])).toBe("M");
  });

  it("is empty without child changes", () => {
    expect(folderGitStatus([])).toBe("");
  });
});
