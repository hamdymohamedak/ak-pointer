export type EventKind = "move" | "down" | "up" | "drag" | "scroll" | "key";
export type MouseButton = "left" | "right" | "middle";
export type ScrollUnit = "line" | "pixel";

export interface InputEvent {
  tMs: number;
  kind: EventKind;
  button: MouseButton;
  clickCount: number;
  x: number;
  y: number;
  scrollX: number;
  scrollY: number;
  scrollUnit: ScrollUnit;
  keyDown: boolean;
  keycode: number;
  flags: number;
  target: number;
}

export interface TargetWindow {
  owner: string;
  title: string;
  w: number;
  h: number;
}

export interface Action {
  version: number;
  windowOwner: string;
  windowTitle: string;
  windowW: number;
  windowH: number;
  targets: TargetWindow[];
  events: InputEvent[];
}

export function newEvent(kind: EventKind): InputEvent {
  return {
    tMs: 0,
    kind,
    button: "left",
    clickCount: 1,
    x: 0,
    y: 0,
    scrollX: 0,
    scrollY: 0,
    scrollUnit: "line",
    keyDown: true,
    keycode: 0,
    flags: 0,
    target: 0,
  };
}

export function newAction(): Action {
  return {
    version: 1,
    windowOwner: "",
    windowTitle: "",
    windowW: 0,
    windowH: 0,
    targets: [],
    events: [],
  };
}

export function sanitizeActionName(name: string): string {
  const bytes: number[] = [];
  for (const byte of new TextEncoder().encode(name)) {
    if (byte < 0x20 || byte === 0x7f || byte === 0x2f || byte === 0x5c || byte === 0x3a) {
      continue;
    }
    bytes.push(byte === 0x20 || byte === 0x09 ? 0x5f : byte);
  }
  while (bytes.length > 0 && (bytes[bytes.length - 1] === 0x2e || bytes[bytes.length - 1] === 0x5f)) {
    bytes.pop();
  }
  while (bytes.length > 0 && (bytes[0] === 0x2e || bytes[0] === 0x5f)) {
    bytes.shift();
  }
  let out = new TextDecoder().decode(new Uint8Array(bytes));
  if (out === "." || out === "..") return "";
  if (out.length > 120) {
    out = out.slice(0, 120);
    while (out.length > 0 && (out.charCodeAt(out.length - 1) & 0xc0) === 0x80) {
      out = out.slice(0, -1);
    }
  }
  return out;
}

function oneLine(value: string): string {
  return [...value].filter((ch) => ch !== "\n" && ch !== "\r").join("");
}

function field(value: string): string {
  return oneLine(value).replaceAll("\t", " ");
}

function rounded(value: number): number {
  return Math.sign(value) * Math.round(Math.abs(value));
}

function buttonName(button: MouseButton): string {
  if (button === "right") return "right";
  if (button === "middle") return "middle";
  return "left";
}

function parseButton(name: string): MouseButton | null {
  if (name === "left" || name === "right" || name === "middle") return name;
  return null;
}

function primaryTarget(action: Action): TargetWindow {
  return (
    action.targets[0] ?? {
      owner: action.windowOwner,
      title: action.windowTitle,
      w: action.windowW,
      h: action.windowH,
    }
  );
}

export function formatAction(action: Action): string {
  const primary = primaryTarget(action);
  const owner = oneLine(primary.owner);
  const title = oneLine(primary.title);
  let body = `version ${action.version}\n`;
  body += "window_owner";
  if (owner.length > 0) body += ` ${owner}`;
  body += "\nwindow_title";
  if (title.length > 0) body += ` ${title}`;
  body += `\nwindow_w ${rounded(primary.w)}\nwindow_h ${rounded(primary.h)}\n\n`;
  let currentTarget = 0;
  for (const event of action.events) {
    if (event.target !== currentTarget) {
      const target = action.targets[event.target];
      if (target) {
        body += `target\t${field(target.owner)}\t${field(target.title)}\t${rounded(target.w)}\t${rounded(target.h)}\n`;
        currentTarget = event.target;
      }
    }
    body += `${event.tMs} `;
    if (event.kind === "move") {
      body += `MOVE ${rounded(event.x)} ${rounded(event.y)}`;
    } else if (event.kind === "down" || event.kind === "up") {
      const name = event.kind === "down" ? "DOWN" : "UP";
      body += `${name} ${buttonName(event.button)} ${event.clickCount} ${rounded(event.x)} ${rounded(event.y)}`;
    } else if (event.kind === "drag") {
      body += `DRAG ${buttonName(event.button)} ${rounded(event.x)} ${rounded(event.y)}`;
    } else if (event.kind === "scroll") {
      const unit = event.scrollUnit === "pixel" ? "pixel" : "line";
      body += `SCROLL ${rounded(event.x)} ${rounded(event.y)} ${event.scrollX} ${event.scrollY} ${unit}`;
    } else {
      const state = event.keyDown ? "down" : "up";
      body += `KEY ${state} ${event.keycode} ${event.flags}`;
    }
    body += "\n";
  }
  return body;
}

function headerParts(line: string): [string, string] {
  const index = line.indexOf(" ");
  if (index === -1) return [line, ""];
  return [line.slice(0, index), line.slice(index + 1)];
}

function parseEvent(line: string): InputEvent | null {
  const parts = line.trim().split(/\s+/);
  if (parts.length < 2) return null;
  const tMs = Number(parts[0]);
  if (!Number.isFinite(tMs)) return null;
  const kind = parts[1];
  const event = newEvent("move");
  event.tMs = tMs;
  if (kind === "MOVE") {
    if (parts.length < 4) return null;
    event.kind = "move";
    event.x = Number(parts[2]);
    event.y = Number(parts[3]);
    if (!Number.isFinite(event.x) || !Number.isFinite(event.y)) return null;
    return event;
  }
  if (kind === "DOWN" || kind === "UP" || kind === "DRAG") {
    const button = parseButton(parts[2] ?? "");
    if (!button) return null;
    event.button = button;
    event.kind = kind === "DOWN" ? "down" : kind === "UP" ? "up" : "drag";
    if (event.kind === "drag") {
      if (parts.length < 5) return null;
      event.x = Number(parts[3]);
      event.y = Number(parts[4]);
    } else {
      if (parts.length < 6) return null;
      event.clickCount = Number(parts[3]);
      event.x = Number(parts[4]);
      event.y = Number(parts[5]);
      if (!Number.isFinite(event.clickCount)) return null;
    }
    if (!Number.isFinite(event.x) || !Number.isFinite(event.y)) return null;
    return event;
  }
  if (kind === "SCROLL") {
    if (parts.length < 6) return null;
    event.kind = "scroll";
    event.x = Number(parts[2]);
    event.y = Number(parts[3]);
    event.scrollX = Number(parts[4]);
    event.scrollY = Number(parts[5]);
    if (![event.x, event.y, event.scrollX, event.scrollY].every(Number.isFinite)) return null;
    if (parts[6] !== undefined) {
      if (parts[6] === "pixel") event.scrollUnit = "pixel";
      else if (parts[6] === "line") event.scrollUnit = "line";
      else return null;
    }
    return event;
  }
  if (kind === "KEY") {
    const state = parts[2];
    if (state !== "down" && state !== "up") return null;
    const keycode = Number(parts[3]);
    const flags = Number(parts[4]);
    if (!Number.isFinite(keycode) || !Number.isFinite(flags)) return null;
    event.kind = "key";
    event.keyDown = state === "down";
    event.keycode = keycode;
    event.flags = flags;
    return event;
  }
  return null;
}

function parseTargetLine(line: string): TargetWindow | null {
  const parts = line.split("\t");
  if (parts[0] !== "target" || parts.length < 5) return null;
  const owner = parts[1] ?? "";
  const title = parts[2] ?? "";
  const w = Number(parts[3]);
  const h = Number(parts[4]);
  if (owner.length === 0 || !Number.isFinite(w) || !Number.isFinite(h) || w < 1 || h < 1) {
    return null;
  }
  return { owner, title, w, h };
}

export function parseActionText(text: string): Action | null {
  const action = newAction();
  let sawVersion = false;
  let header = true;
  let currentTarget = 0;
  for (let line of text.split("\n")) {
    if (line.endsWith("\r")) line = line.slice(0, -1);
    if (header) {
      if (line.length === 0) {
        header = false;
        continue;
      }
      const [key, value] = headerParts(line);
      if (key === "version") {
        const version = Number(value);
        if (!Number.isFinite(version)) return null;
        action.version = version;
        sawVersion = true;
        continue;
      }
      if (key === "window_owner") {
        action.windowOwner = value;
        continue;
      }
      if (key === "window_title") {
        action.windowTitle = value;
        continue;
      }
      if (key === "window_w") {
        const width = Number(value);
        if (!Number.isFinite(width)) return null;
        action.windowW = width;
        continue;
      }
      if (key === "window_h") {
        const height = Number(value);
        if (!Number.isFinite(height)) return null;
        action.windowH = height;
        continue;
      }
      header = false;
    }
    if (line.length === 0 || line.startsWith("#")) continue;
    if (line.startsWith("target\t") || line === "target") {
      const target = parseTargetLine(line);
      if (!target) return null;
      if (action.targets.length === 0) {
        action.targets.push({
          owner: action.windowOwner,
          title: action.windowTitle,
          w: action.windowW,
          h: action.windowH,
        });
      }
      action.targets.push(target);
      currentTarget = action.targets.length - 1;
      continue;
    }
    const event = parseEvent(line);
    if (!event) return null;
    event.target = currentTarget;
    action.events.push(event);
  }
  if (action.targets.length === 0) {
    action.targets.push({
      owner: action.windowOwner,
      title: action.windowTitle,
      w: action.windowW,
      h: action.windowH,
    });
  }
  if (
    !sawVersion ||
    action.version !== 1 ||
    action.windowOwner.length === 0 ||
    action.windowW < 1 ||
    action.windowH < 1
  ) {
    return null;
  }
  return action;
}

export function tally(action: Action): string {
  let clicks = 0;
  let drags = 0;
  let moves = 0;
  let keys = 0;
  let scrolls = 0;
  for (const event of action.events) {
    if (event.kind === "down") clicks += 1;
    else if (event.kind === "drag") drags += 1;
    else if (event.kind === "move") moves += 1;
    else if (event.kind === "key" && event.keyDown) keys += 1;
    else if (event.kind === "scroll") scrolls += 1;
  }
  return `${clicks} clicks · ${drags} drags · ${moves} moves · ${keys} keys · ${scrolls} scrolls`;
}

export function windowLabel(action: Action): string {
  let label = action.windowOwner;
  if (action.windowTitle.length > 0) {
    if (label.length > 0) label += " / ";
    label += action.windowTitle;
  }
  return label;
}
