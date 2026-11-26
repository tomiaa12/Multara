import { describe, expect, it } from "vitest";
import {
  cleanupMarker,
  defaultCleanupChecked,
  formatDiskSize,
  matchPreferredBranch,
} from "./folder-cleanup";

describe("folder cleanup selection", () => {
  it("marks worktree, real branch, and folder apart", () => {
    expect(cleanupMarker({ kind: "git", isolationMode: "worktree" })).toBe("worktree");
    expect(cleanupMarker({ kind: "git", isolationMode: "direct" })).toBe("branch");
    expect(cleanupMarker({ kind: "folder", isolationMode: "direct" })).toBe("folder");
    expect(cleanupMarker({ kind: "folder", isolationMode: "copy" })).toBe("folder");
  });

  it("checks only a clean committed worktree", () => {
    const clean = {
      marker: "worktree" as const,
      sessionBound: false,
      missing: false,
      dirty: false,
      conflict: false,
    };
    expect(defaultCleanupChecked(clean)).toBe(true);
    expect(defaultCleanupChecked({ ...clean, marker: "branch" })).toBe(false);
    expect(defaultCleanupChecked({ ...clean, marker: "folder" })).toBe(false);
    expect(defaultCleanupChecked({ ...clean, dirty: true })).toBe(false);
    expect(defaultCleanupChecked({ ...clean, conflict: true })).toBe(false);
    expect(defaultCleanupChecked({ ...clean, sessionBound: true })).toBe(false);
    expect(defaultCleanupChecked({ ...clean, missing: true })).toBe(false);
  });

  it("prefers the upstream branch when it still exists", () => {
    expect(matchPreferredBranch(["main", "feat/inbox"], "origin/feat/inbox")).toBe("feat/inbox");
    expect(matchPreferredBranch(["main"], "origin/feat/inbox")).toBeNull();
    expect(matchPreferredBranch(["main"], "")).toBeNull();
  });

  it("formats folder size", () => {
    expect(formatDiskSize(0)).toBe("0 KB");
    expect(formatDiskSize(512)).toBe("512 B");
    expect(formatDiskSize(1536)).toBe("1.5 KB");
    expect(formatDiskSize(5 * 1024 * 1024)).toBe("5 MB");
    expect(formatDiskSize(1.5 * 1024 * 1024 * 1024)).toBe("1.5 GB");
  });
});
