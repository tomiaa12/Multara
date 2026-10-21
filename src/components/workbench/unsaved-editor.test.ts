import { describe, expect, test } from "vitest";
import {
  confirmLeaveUnsavedEditors,
  confirmRollbackIfDirty,
  formatRollbackDirtyMessage,
  listDirtyEditorPaths,
  registerDirtyEditorPaths,
  registerUnsavedDirtyChecker,
  registerUnsavedLeaveHandler,
} from "./unsaved-editor";

describe("confirmLeaveUnsavedEditors", () => {
  test("allows close when nothing is dirty", async () => {
    registerUnsavedDirtyChecker(() => false);
    registerUnsavedLeaveHandler(async () => false);
    await expect(confirmLeaveUnsavedEditors()).resolves.toBe(true);
  });

  test("asks the workbench handler when files are dirty", async () => {
    registerUnsavedDirtyChecker(() => true);
    registerUnsavedLeaveHandler(async () => false);
    await expect(confirmLeaveUnsavedEditors()).resolves.toBe(false);
    registerUnsavedLeaveHandler(async () => true);
    await expect(confirmLeaveUnsavedEditors()).resolves.toBe(true);
  });

  test("does not block close if the dirty checker throws", async () => {
    registerUnsavedDirtyChecker(() => {
      throw new Error("editor gone");
    });
    registerUnsavedLeaveHandler(async () => false);
    await expect(confirmLeaveUnsavedEditors()).resolves.toBe(true);
  });
});

describe("T-E07 rollback dirty prompt", () => {
  test("formats the documented overwrite warning", () => {
    expect(formatRollbackDirtyMessage(["src/a.ts"])).toBe(
      "以下文件有未保存修改，Rollback 将覆盖：src/a.ts",
    );
  });

  test("lists dirty editor paths from the workbench registry", () => {
    registerDirtyEditorPaths(() => ["src/a.ts", "src/a.ts"]);
    expect(listDirtyEditorPaths()).toEqual(["src/a.ts"]);
    registerDirtyEditorPaths(null);
  });

  test("asks before rollback and honors cancel", async () => {
    const ask = async () => false;
    await expect(confirmRollbackIfDirty(["src/a.ts"], ask)).resolves.toBe(false);
    await expect(confirmRollbackIfDirty([], ask)).resolves.toBe(true);
  });
});
