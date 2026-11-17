import { describe, expect, it } from "vitest";
import { dropBeforeId, edgeScrollSpeed } from "./task-card-drag";

const cards = [
  { id: "a", top: 0, height: 100 },
  { id: "b", top: 110, height: 100 },
  { id: "c", top: 220, height: 100 },
];

describe("dropBeforeId", () => {
  it("按卡片中线决定插入位置", () => {
    expect(dropBeforeId(cards, 10, "x")).toBe("a");
    expect(dropBeforeId(cards, 60, "x")).toBe("b");
    expect(dropBeforeId(cards, 200, "x")).toBe("c");
    expect(dropBeforeId(cards, 300, "x")).toBe("");
  });

  it("跳过被拖的卡片", () => {
    expect(dropBeforeId(cards, 140, "b")).toBe("c");
    expect(dropBeforeId(cards, 40, "a")).toBe("b");
  });

  it("空列落在列尾", () => {
    expect(dropBeforeId([], 50, "x")).toBe("");
  });
});

describe("edgeScrollSpeed", () => {
  it("中间不滚，贴边越近越快", () => {
    expect(edgeScrollSpeed(500, 0, 1000)).toBe(0);
    expect(edgeScrollSpeed(30, 0, 1000)).toBeLessThan(0);
    expect(edgeScrollSpeed(0, 0, 1000)).toBeLessThan(edgeScrollSpeed(30, 0, 1000));
    expect(edgeScrollSpeed(990, 0, 1000)).toBeGreaterThan(0);
  });

  it("区域太小不滚", () => {
    expect(edgeScrollSpeed(10, 0, 80)).toBe(0);
  });
});
