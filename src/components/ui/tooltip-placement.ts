export type TooltipPlacement = "top" | "bottom" | "left" | "right";

export type TooltipBox = {
  top: number;
  left: number;
  width: number;
  height: number;
};

export type TooltipSize = {
  width: number;
  height: number;
};

export type TooltipPosition = {
  placement: TooltipPlacement;
  top: number;
  left: number;
};

const ORDER: Record<TooltipPlacement, TooltipPlacement[]> = {
  top: ["top", "bottom", "right", "left"],
  bottom: ["bottom", "top", "right", "left"],
  left: ["left", "right", "top", "bottom"],
  right: ["right", "left", "top", "bottom"],
};

/** 按偏好方向选能放下 tip 的一侧；放不下则取空间最大的一侧。默认偏好 top。 */
export function pickTooltipPlacement(
  trigger: TooltipBox,
  tip: TooltipSize,
  preferred: TooltipPlacement = "top",
  gap = 6,
  margin = 8,
  viewport = { width: 0, height: 0 },
): TooltipPosition {
  const viewW = viewport.width || (typeof window !== "undefined" ? window.innerWidth : 0);
  const viewH = viewport.height || (typeof window !== "undefined" ? window.innerHeight : 0);
  const triggerBottom = trigger.top + trigger.height;
  const triggerRight = trigger.left + trigger.width;
  const space = {
    top: trigger.top - margin,
    bottom: viewH - triggerBottom - margin,
    left: trigger.left - margin,
    right: viewW - triggerRight - margin,
  };
  const need = {
    top: tip.height + gap,
    bottom: tip.height + gap,
    left: tip.width + gap,
    right: tip.width + gap,
  };
  const fits = (side: TooltipPlacement) => space[side] >= need[side];
  const placement =
    ORDER[preferred].find(fits)
    ?? ORDER[preferred].reduce((best, side) => (space[side] > space[best] ? side : best));

  const centerX = trigger.left + trigger.width / 2;
  const centerY = trigger.top + trigger.height / 2;
  let top = 0;
  let left = 0;
  if (placement === "top") {
    top = trigger.top - tip.height - gap;
    left = centerX - tip.width / 2;
  } else if (placement === "bottom") {
    top = triggerBottom + gap;
    left = centerX - tip.width / 2;
  } else if (placement === "left") {
    top = centerY - tip.height / 2;
    left = trigger.left - tip.width - gap;
  } else {
    top = centerY - tip.height / 2;
    left = triggerRight + gap;
  }

  left = Math.min(Math.max(left, margin), Math.max(margin, viewW - tip.width - margin));
  top = Math.min(Math.max(top, margin), Math.max(margin, viewH - tip.height - margin));
  return { placement, top, left };
}
