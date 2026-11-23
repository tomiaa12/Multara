// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  WORKFLOW_DROP_ATTR,
  cancelWorkflowPaletteDrag,
  workflowDropPointAt,
} from "./palette-drag";

describe("palette-drag", () => {
  afterEach(() => {
    cancelWorkflowPaletteDrag();
    document.body.replaceChildren();
    vi.restoreAllMocks();
  });

  it("reports a drop point when the pointer is over the canvas zone", () => {
    const zone = document.createElement("div");
    zone.setAttribute(WORKFLOW_DROP_ATTR, "canvas");
    document.body.appendChild(zone);
    vi.spyOn(document, "elementFromPoint").mockReturnValue(zone);

    expect(workflowDropPointAt(40, 50)).toEqual({ x: 40, y: 50 });
  });

  it("returns null outside the canvas zone", () => {
    vi.spyOn(document, "elementFromPoint").mockReturnValue(document.body);
    expect(workflowDropPointAt(400, 400)).toBeNull();
  });

  it("treats nodes inside the canvas as a valid drop target", () => {
    const zone = document.createElement("div");
    zone.setAttribute(WORKFLOW_DROP_ATTR, "canvas");
    const node = document.createElement("div");
    zone.appendChild(node);
    document.body.appendChild(zone);
    vi.spyOn(document, "elementFromPoint").mockReturnValue(node);

    expect(workflowDropPointAt(40, 30)).toEqual({ x: 40, y: 30 });
  });
});
