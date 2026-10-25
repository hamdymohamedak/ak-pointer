import { describe, expect, test } from "bun:test";
import { type InputEvent, newEvent } from "./action.ts";
import { applyTextRun, textRuns } from "./edit.ts";

const SHIFT = 0x20000;
const COMMAND = 0x100000;
const KEY_BACKSPACE = 0x33;

function keyDown(tMs: number, keycode: number, flags = 0): InputEvent {
  const event = newEvent("key");
  event.tMs = tMs;
  event.keycode = keycode;
  event.flags = flags;
  event.keyDown = true;
  return event;
}

function click(tMs: number): InputEvent {
  const event = newEvent("down");
  event.tMs = tMs;
  event.x = 10;
  event.y = 20;
  return event;
}

describe("typed text", () => {
  test("backspace is applied in the text", () => {
    const events = [
      keyDown(0, 0x04),
      keyDown(80, 0x0e),
      keyDown(160, 0x25),
      keyDown(240, 0x25),
      keyDown(320, 0x1f),
      keyDown(400, KEY_BACKSPACE),
      keyDown(480, KEY_BACKSPACE),
      keyDown(560, 0x23),
    ];
    const runs = textRuns(events);
    expect(runs.length).toBe(1);
    expect(runs[0].text).toBe("help");
  });

  test("longer replacement shifts the following click", () => {
    const events = [keyDown(0, 0x00), keyDown(80, 0x0b), click(500)];
    const result = applyTextRun(events, 0, 1, "abcd");
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(textRuns(result.events)[0].text).toBe("abcd");
    const down = result.events.find((event) => event.kind === "down");
    expect(down).toBeDefined();
    expect(down!.tMs).toBeGreaterThan(500);
  });

  test("command shortcut splits plain text", () => {
    const events = [
      keyDown(0, 0x04),
      keyDown(80, 0x0e),
      keyDown(160, 0x08, COMMAND),
      keyDown(240, 0x25),
      keyDown(320, 0x1f),
    ];
    expect(textRuns(events).map((run) => run.text)).toEqual(["he", "lo"]);
  });

  test("shifted letter is uppercase", () => {
    const events = [keyDown(0, 0x00, SHIFT)];
    expect(textRuns(events)[0].text).toBe("A");
    const result = applyTextRun(events, 0, 0, "A");
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    const down = result.events.find((event) => event.keyDown);
    expect(down?.keycode).toBe(0x00);
    expect((down?.flags ?? 0) & SHIFT).toBe(SHIFT);
  });
});
