import { beforeAll, describe, expect, test } from "vitest";
import { applyLanguageSetting } from "../../i18n";
import {
  activateUnsavedEditor,
  confirmLeaveUnsavedEditors,
  confirmRollbackIfDirty,
  formatRollbackDirtyMessage,
  hasUnsavedEditors,
  listDirtyEditorPaths,
  registerUnsavedEditor,
  type UnsavedEditorOwner,
} from "./unsaved-editor";

beforeAll(() => applyLanguageSetting("zh-CN"));

function owner(dirty: () => boolean, leave: () => Promise<boolean>, paths: string[] = []): UnsavedEditorOwner {
  return { dirty, leave, paths: () => paths };
}

describe("confirmLeaveUnsavedEditors", () => {
  test("allows close when nothing is dirty", async () => {
    const stop = registerUnsavedEditor(owner(() => false, async () => false));
    await expect(confirmLeaveUnsavedEditors()).resolves.toBe(true);
    stop();
  });

  test("asks the workbench handler when files are dirty", async () => {
    const stopCancel = registerUnsavedEditor(owner(() => true, async () => false));
    await expect(confirmLeaveUnsavedEditors()).resolves.toBe(false);
    stopCancel();
    const stopAllow = registerUnsavedEditor(owner(() => true, async () => true));
    await expect(confirmLeaveUnsavedEditors()).resolves.toBe(true);
    stopAllow();
  });

  test("does not block close if the dirty checker throws", async () => {
    const stop = registerUnsavedEditor(owner(() => {
      throw new Error("editor gone");
    }, async () => false));
    await expect(confirmLeaveUnsavedEditors()).resolves.toBe(true);
    stop();
  });

  test("unmounting one workbench keeps another workbench registered", () => {
    const first = owner(() => true, async () => false, ["a.ts"]);
    const second = owner(() => false, async () => true, ["b.ts"]);
    const stopFirst = registerUnsavedEditor(first);
    const stopSecond = registerUnsavedEditor(second);
    expect(listDirtyEditorPaths()).toEqual(["b.ts"]);
    activateUnsavedEditor(first);
    expect(listDirtyEditorPaths()).toEqual(["a.ts"]);
    stopSecond();
    expect(hasUnsavedEditors()).toBe(true);
    stopFirst();
    expect(hasUnsavedEditors()).toBe(false);
  });
});

describe("T-E07 rollback dirty prompt", () => {
  test("formats the documented overwrite warning", () => {
    expect(formatRollbackDirtyMessage(["src/a.ts"])).toBe(
      "以下文件有未保存修改，Rollback 将覆盖：src/a.ts",
    );
  });

  test("lists dirty editor paths from the workbench registry", () => {
    const stop = registerUnsavedEditor(owner(() => true, async () => true, ["src/a.ts", "src/a.ts"]));
    expect(listDirtyEditorPaths()).toEqual(["src/a.ts"]);
    stop();
  });

  test("asks before rollback and honors cancel", async () => {
    const ask = async () => false;
    await expect(confirmRollbackIfDirty(["src/a.ts"], ask)).resolves.toBe(false);
    await expect(confirmRollbackIfDirty([], ask)).resolves.toBe(true);
  });
});
