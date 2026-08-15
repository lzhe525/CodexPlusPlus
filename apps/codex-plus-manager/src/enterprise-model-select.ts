export type ModelMenuDirection = "up" | "down";

export type ModelMenuPlacement = {
  direction: ModelMenuDirection;
  left: number;
  width: number;
  maxHeight: number;
  top?: number;
  bottom?: number;
};

type TriggerRect = Pick<DOMRect, "bottom" | "left" | "top" | "width">;

export function calculateModelMenuPlacement(
  trigger: TriggerRect,
  viewportWidth: number,
  viewportHeight: number,
): ModelMenuPlacement {
  const margin = 8;
  const gap = 6;
  const preferredMaxHeight = 360;
  const availableBelow = Math.max(0, viewportHeight - trigger.bottom - gap - margin);
  const availableAbove = Math.max(0, trigger.top - gap - margin);
  const direction: ModelMenuDirection = availableBelow >= Math.min(240, availableAbove) ? "down" : "up";
  const availableHeight = direction === "down" ? availableBelow : availableAbove;
  const maxHeight = Math.min(preferredMaxHeight, availableHeight);
  const width = Math.min(Math.max(trigger.width, 240), Math.max(0, viewportWidth - margin * 2));
  const left = Math.min(Math.max(margin, trigger.left), Math.max(margin, viewportWidth - margin - width));

  return direction === "down"
    ? { direction, left, width, maxHeight, top: trigger.bottom + gap }
    : { direction, left, width, maxHeight, bottom: viewportHeight - trigger.top + gap };
}

export function nextModelIndex(
  currentIndex: number,
  optionCount: number,
  key: "ArrowDown" | "ArrowUp" | "End" | "Home",
): number {
  if (optionCount <= 0) return -1;
  if (key === "Home") return 0;
  if (key === "End") return optionCount - 1;
  if (key === "ArrowDown") return currentIndex < 0 ? 0 : (currentIndex + 1) % optionCount;
  return currentIndex < 0 ? optionCount - 1 : (currentIndex - 1 + optionCount) % optionCount;
}
