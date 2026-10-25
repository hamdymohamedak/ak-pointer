import { type Action, type InputEvent, newEvent } from "./action.ts";

const SHIFT = 0x20000;
const CAPS = 0x10000;
const CONTROL = 0x40000;
const OPTION = 0x80000;
const COMMAND = 0x100000;
const KEY_RETURN = 0x24;
const KEY_TAB = 0x30;
const KEY_BACKSPACE = 0x33;

const KEYS: Array<[number, string, string]> = [
  [0x00, "a", "A"],
  [0x01, "s", "S"],
  [0x02, "d", "D"],
  [0x03, "f", "F"],
  [0x04, "h", "H"],
  [0x05, "g", "G"],
  [0x06, "z", "Z"],
  [0x07, "x", "X"],
  [0x08, "c", "C"],
  [0x09, "v", "V"],
  [0x0b, "b", "B"],
  [0x0c, "q", "Q"],
  [0x0d, "w", "W"],
  [0x0e, "e", "E"],
  [0x0f, "r", "R"],
  [0x10, "y", "Y"],
  [0x11, "t", "T"],
  [0x12, "1", "!"],
  [0x13, "2", "@"],
  [0x14, "3", "#"],
  [0x15, "4", "$"],
  [0x16, "6", "^"],
  [0x17, "5", "%"],
  [0x18, "=", "+"],
  [0x19, "9", "("],
  [0x1a, "7", "&"],
  [0x1b, "-", "_"],
  [0x1c, "8", "*"],
  [0x1d, "0", ")"],
  [0x1e, "]", "}"],
  [0x1f, "o", "O"],
  [0x20, "u", "U"],
  [0x21, "[", "{"],
  [0x22, "i", "I"],
  [0x23, "p", "P"],
  [0x25, "l", "L"],
  [0x26, "j", "J"],
  [0x27, "'", '"'],
  [0x28, "k", "K"],
  [0x29, ";", ":"],
  [0x2a, "\\", "|"],
  [0x2b, ",", "<"],
  [0x2c, "/", "?"],
  [0x2d, "n", "N"],
  [0x2e, "m", "M"],
  [0x2f, ".", ">"],
  [0x31, " ", " "],
  [0x32, "`", "~"],
];

export interface TextRun {
  start: number;
  end: number;
  text: string;
}

function isShortcut(flags: number): boolean {
  return (flags & (CONTROL | OPTION | COMMAND)) !== 0;
}

function keyForChar(ch: string): { keycode: number; flags: number } | null {
  if (ch === "\n") return { keycode: KEY_RETURN, flags: 0 };
  if (ch === "\t") return { keycode: KEY_TAB, flags: 0 };
  for (const [keycode, lower, upper] of KEYS) {
    if (ch === lower) return { keycode, flags: 0 };
    if (ch === upper && upper !== lower) return { keycode, flags: SHIFT };
  }
  return null;
}

function charForKey(keycode: number, flags: number): string | null {
  if (keycode === KEY_RETURN) return "\n";
  if (keycode === KEY_TAB) return "\t";
  const found = KEYS.find(([code]) => code === keycode);
  if (!found) return null;
  const [, lower, upper] = found;
  const shift = (flags & SHIFT) !== 0;
  if (/[a-z]/.test(lower)) {
    const caps = (flags & CAPS) !== 0;
    return shift !== caps ? upper : lower;
  }
  return shift ? upper : lower;
}

function buttonName(button: InputEvent["button"]): string {
  if (button === "right") return "right";
  if (button === "middle") return "middle";
  return "left";
}

export function describeEvent(event: InputEvent): string {
  if (event.kind === "move") return `move  ${event.x.toFixed(0)}, ${event.y.toFixed(0)}`;
  if (event.kind === "down") {
    return `down ${buttonName(event.button)} x${event.clickCount}  ${event.x.toFixed(0)}, ${event.y.toFixed(0)}`;
  }
  if (event.kind === "up") {
    return `up ${buttonName(event.button)}  ${event.x.toFixed(0)}, ${event.y.toFixed(0)}`;
  }
  if (event.kind === "drag") {
    return `drag ${buttonName(event.button)}  ${event.x.toFixed(0)}, ${event.y.toFixed(0)}`;
  }
  if (event.kind === "scroll") {
    const unit = event.scrollUnit === "pixel" ? "pixel" : "line";
    return `scroll ${event.x.toFixed(0)}, ${event.y.toFixed(0)}  ${event.scrollX}, ${event.scrollY} ${unit}`;
  }
  const state = event.keyDown ? "down" : "up";
  return `key ${state} ${keyLabel(event)}`;
}

function keyLabel(event: InputEvent): string {
  if (event.keycode === KEY_BACKSPACE) return "backspace";
  const ch = charForKey(event.keycode, event.flags);
  if (ch === "\n") return "return";
  if (ch === "\t") return "tab";
  if (ch === " ") return "space";
  if (ch) return ch;
  return `code ${event.keycode}`;
}

export function textRuns(events: InputEvent[]): TextRun[] {
  const runs: TextRun[] = [];
  let index = 0;
  while (index < events.length) {
    if (events[index].kind !== "key" || isShortcut(events[index].flags)) {
      index += 1;
      continue;
    }
    const start = index;
    let end = index;
    const chars: string[] = [];
    let typed = false;
    while (index < events.length) {
      const event = events[index];
      if (event.kind === "move") {
        index += 1;
        continue;
      }
      if (event.kind !== "key" || isShortcut(event.flags)) break;
      end = index;
      if (event.keyDown) {
        if (event.keycode === KEY_BACKSPACE) {
          chars.pop();
          typed = true;
        } else {
          const ch = charForKey(event.keycode, event.flags);
          if (ch !== null) {
            chars.push(ch);
            typed = true;
          }
        }
      }
      index += 1;
    }
    if (typed) runs.push({ start, end, text: chars.join("") });
    if (index === start) index += 1;
  }
  return runs;
}

function encodeText(text: string, startMs: number): InputEvent[] | string {
  const events: InputEvent[] = [];
  let tMs = startMs;
  for (const ch of text) {
    const key = keyForChar(ch);
    if (!key) {
      return `This editor cannot type '${ch}'. Use the file text for that character.`;
    }
    const down = newEvent("key");
    down.tMs = tMs;
    down.keyDown = true;
    down.keycode = key.keycode;
    down.flags = key.flags;
    events.push(down);
    tMs += 25;
    const up = newEvent("key");
    up.tMs = tMs;
    up.keyDown = false;
    up.keycode = key.keycode;
    up.flags = key.flags;
    events.push(up);
    tMs += 55;
  }
  return events;
}

export function applyTextRun(
  events: InputEvent[],
  start: number,
  end: number,
  text: string,
): { ok: true; events: InputEvent[] } | { ok: false; error: string } {
  if (start > end || end >= events.length) {
    return { ok: false, error: "That typed text is no longer in this action." };
  }
  const startMs = events[start].tMs;
  const oldEndMs = events[end].tMs;
  const target = events[start].target;
  const encoded = encodeText(text, startMs);
  if (typeof encoded === "string") return { ok: false, error: encoded };
  for (const event of encoded) event.target = target;
  const newEndMs = encoded.length > 0 ? encoded[encoded.length - 1].tMs : startMs;
  const shiftBy = Math.max(0, newEndMs - oldEndMs);
  const rebuilt: InputEvent[] = events.slice(0, start);
  rebuilt.push(...encoded);
  for (const event of events.slice(start, end + 1)) {
    if (event.kind === "key") continue;
    const kept = { ...event };
    if (kept.tMs < newEndMs) kept.tMs = newEndMs;
    rebuilt.push(kept);
  }
  for (const event of events.slice(end + 1)) {
    rebuilt.push({ ...event, tMs: event.tMs + shiftBy });
  }
  return { ok: true, events: rebuilt };
}

export function syncHeader(action: Action, owner: string, title: string): Action {
  const next: Action = {
    ...action,
    windowOwner: owner,
    windowTitle: title,
    targets: action.targets.map((target) => ({ ...target })),
    events: action.events.map((event) => ({ ...event })),
  };
  if (next.targets.length === 0) {
    next.targets.push({ owner, title, w: action.windowW, h: action.windowH });
  } else {
    next.targets[0] = { ...next.targets[0], owner, title };
  }
  return next;
}
