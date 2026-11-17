// @vitest-environment happy-dom
import { describe, expect, it } from "vitest";
import { canStartPan } from "./drag-pan";

function board() {
  const host = document.createElement("div");
  host.innerHTML = `
    <section class="col">
      <header><strong class="title">Todo</strong><button class="btn"><svg class="icon"></svg></button></header>
      <div class="list"><article class="card" draggable="true"><h3 class="card-title">x</h3></article></div>
    </section>
    <div class="vs-overlay-scrollbar"><div class="slider"></div></div>`;
  return host;
}

describe("canStartPan", () => {
  it("左键在空白处可拖", () => {
    const host = board();
    for (const selector of [".col", ".list", "header", ".title"]) {
      expect(canStartPan(0, host.querySelector(selector), host)).toBe(true);
    }
    expect(canStartPan(0, host, host)).toBe(true);
  });

  it("左键在卡片、按钮、滚动条上不接管", () => {
    const host = board();
    for (const selector of [".card", ".card-title", ".btn", ".icon", ".slider"]) {
      expect(canStartPan(0, host.querySelector(selector), host)).toBe(false);
    }
  });

  it("中键除滚动条外都可拖", () => {
    const host = board();
    expect(canStartPan(1, host.querySelector(".card-title"), host)).toBe(true);
    expect(canStartPan(1, host.querySelector(".btn"), host)).toBe(true);
    expect(canStartPan(1, host.querySelector(".slider"), host)).toBe(false);
  });

  it("右键和宿主外的目标不接管", () => {
    const host = board();
    expect(canStartPan(2, host, host)).toBe(false);
    expect(canStartPan(0, document.createElement("div"), host)).toBe(false);
  });
});
