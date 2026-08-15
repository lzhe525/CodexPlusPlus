import assert from "node:assert/strict";
import test from "node:test";
import { calculateModelMenuPlacement, nextModelIndex } from "./enterprise-model-select.ts";

test("model menu opens toward the side with enough viewport space", () => {
  const down = calculateModelMenuPlacement(
    { bottom: 140, left: 40, top: 100, width: 320 },
    1000,
    800,
  );
  assert.equal(down.direction, "down");
  assert.equal(down.top, 146);
  assert.equal(down.maxHeight, 360);

  const up = calculateModelMenuPlacement(
    { bottom: 750, left: 850, top: 710, width: 320 },
    1000,
    800,
  );
  assert.equal(up.direction, "up");
  assert.equal(up.bottom, 96);
  assert.equal(up.left, 672);
  assert.equal(up.width, 320);
});

test("model menu remains inside a narrow viewport", () => {
  const placement = calculateModelMenuPlacement(
    { bottom: 120, left: -20, top: 80, width: 500 },
    360,
    300,
  );
  assert.equal(placement.left, 8);
  assert.equal(placement.width, 344);
  assert.equal(placement.maxHeight, 166);
});

test("model keyboard navigation wraps and handles boundaries", () => {
  assert.equal(nextModelIndex(-1, 0, "ArrowDown"), -1);
  assert.equal(nextModelIndex(-1, 3, "ArrowDown"), 0);
  assert.equal(nextModelIndex(-1, 3, "ArrowUp"), 2);
  assert.equal(nextModelIndex(2, 3, "ArrowDown"), 0);
  assert.equal(nextModelIndex(0, 3, "ArrowUp"), 2);
  assert.equal(nextModelIndex(1, 3, "Home"), 0);
  assert.equal(nextModelIndex(1, 3, "End"), 2);
});
