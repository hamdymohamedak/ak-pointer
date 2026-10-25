import { expect, test } from "bun:test";
import { sizeChanged, toRelative, toScreen } from "./geometry.ts";

test("converts coordinates when the window moves or resizes", () => {
  const locked = { x: 0, y: 0, w: 400, h: 800 };
  const current = { x: 300, y: 80, w: 400, h: 800 };
  const relative = toRelative(locked, current, 420, 420);
  expect(Math.abs(relative.x - 120)).toBeLessThan(0.01);
  expect(Math.abs(relative.y - 340)).toBeLessThan(0.01);
  const screen = toScreen(400, 800, current, relative.x, relative.y);
  expect(Math.abs(screen.x - 420)).toBeLessThan(0.01);
  expect(Math.abs(screen.y - 420)).toBeLessThan(0.01);

  const resized = { ...current, w: 800, h: 1600 };
  const scaled = toRelative(locked, resized, 700, 880);
  expect(Math.abs(scaled.x - 200)).toBeLessThan(0.01);
  expect(Math.abs(scaled.y - 400)).toBeLessThan(0.01);

  const replay = { x: 10, y: 20, w: 200, h: 400 };
  const back = toScreen(locked.w, locked.h, replay, scaled.x, scaled.y);
  expect(Math.abs(back.x - 110)).toBeLessThan(0.01);
  expect(Math.abs(back.y - 220)).toBeLessThan(0.01);
  expect(sizeChanged(400, 800, replay)).toBe(true);
  expect(sizeChanged(200, 400, replay)).toBe(false);
});
