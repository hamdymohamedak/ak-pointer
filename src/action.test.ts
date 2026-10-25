import { describe, expect, test } from "bun:test";
import { formatAction, newAction, newEvent, parseActionText, sanitizeActionName } from "./action.ts";

describe("action files", () => {
  test("sanitizes names", () => {
    expect(sanitizeActionName("  login flow  ")).toBe("login_flow");
    expect(sanitizeActionName("../etc/passwd").includes("/")).toBe(false);
    expect(sanitizeActionName("open app")).toBe("open_app");
    expect(sanitizeActionName("...")).toBe("");
  });

  test("round-trips an action file", () => {
    const action = newAction();
    action.windowOwner = "qemu-system-aarch64";
    action.windowTitle = "Pixel 7";
    action.windowW = 411.4;
    action.windowH = 823.6;
    const move = newEvent("move");
    move.x = 120.2;
    move.y = 340.6;
    const down = newEvent("down");
    down.tMs = 180;
    down.x = 120;
    down.y = 340;
    const up = { ...down, tMs: 250, kind: "up" as const };
    const drag = newEvent("drag");
    drag.tMs = 300;
    drag.button = "right";
    drag.x = 10;
    drag.y = 20;
    const scroll = newEvent("scroll");
    scroll.tMs = 900;
    scroll.x = 200;
    scroll.y = 500;
    scroll.scrollY = -3;
    scroll.scrollUnit = "pixel";
    const key = newEvent("key");
    key.tMs = 980;
    key.keycode = 36;
    action.events = [move, down, up, drag, scroll, key];
    const loaded = parseActionText(formatAction(action));
    expect(loaded).not.toBeNull();
    if (!loaded) return;
    expect(loaded.windowOwner).toBe("qemu-system-aarch64");
    expect(loaded.windowTitle).toBe("Pixel 7");
    expect(loaded.windowW).toBe(411);
    expect(loaded.windowH).toBe(824);
    expect(loaded.events.length).toBe(6);
    expect(loaded.events[0].kind).toBe("move");
    expect(loaded.events[0].x).toBe(120);
    expect(loaded.events[0].y).toBe(341);
    expect(loaded.events[1].kind).toBe("down");
    expect(loaded.events[1].clickCount).toBe(1);
    expect(loaded.events[3].kind).toBe("drag");
    expect(loaded.events[3].button).toBe("right");
    expect(loaded.events[4].scrollY).toBe(-3);
    expect(loaded.events[4].scrollUnit).toBe("pixel");
    expect(loaded.events[5].keyDown).toBe(true);
    expect(loaded.events[5].keycode).toBe(36);
    expect(parseActionText("not an action")).toBeNull();
  });

  test("round-trips a second window", () => {
    const action = newAction();
    action.targets = [
      { owner: "Simulator", title: "iPhone 16", w: 402, h: 874 },
      { owner: "Android Studio", title: "Pixel 7", w: 400, h: 800 },
    ];
    const first = newEvent("down");
    first.x = 10;
    first.y = 20;
    const second = newEvent("down");
    second.tMs = 400;
    second.x = 30;
    second.y = 40;
    second.target = 1;
    action.events = [first, second];
    const loaded = parseActionText(formatAction(action));
    expect(loaded).not.toBeNull();
    if (!loaded) return;
    expect(loaded.windowOwner).toBe("Simulator");
    expect(loaded.events[0].target).toBe(0);
    expect(loaded.events[1].target).toBe(1);
    expect(loaded.targets[1].owner).toBe("Android Studio");
    expect(loaded.targets[1].title).toBe("Pixel 7");
  });
});
