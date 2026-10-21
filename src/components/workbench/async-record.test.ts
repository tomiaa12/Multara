import { describe, expect, it } from "vitest";
import { mergeFetchedRecordEntry, recordFromFetchedKeys } from "./async-record";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

describe("mergeFetchedRecordEntry", () => {
  it("keeps sibling keys when two fetches finish out of order", async () => {
    let record: Record<string, string[]> = {};
    const first = deferred<string[]>();
    const second = deferred<string[]>();
    const writes = Promise.all([
      mergeFetchedRecordEntry(() => record, "workspace-1", first.promise).then(
        (next) => {
          record = next;
        },
      ),
      mergeFetchedRecordEntry(() => record, "workspace-2", second.promise).then(
        (next) => {
          record = next;
        },
      ),
    ]);
    second.resolve(["lib"]);
    await Promise.resolve();
    first.resolve(["src"]);
    await writes;
    expect(record).toEqual({
      "workspace-1": ["src"],
      "workspace-2": ["lib"],
    });
  });
});

describe("recordFromFetchedKeys", () => {
  it("assigns every workspace after all listings resolve", async () => {
    const first = deferred<string[]>();
    const second = deferred<string[]>();
    const pending = recordFromFetchedKeys(
      ["workspace-1", "workspace-2"],
      (key) => (key === "workspace-1" ? first.promise : second.promise),
    );
    second.resolve(["workspace-2"]);
    first.resolve(["workspace-1"]);
    await expect(pending).resolves.toEqual({
      "workspace-1": ["workspace-1"],
      "workspace-2": ["workspace-2"],
    });
  });
});
