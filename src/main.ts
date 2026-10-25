import { app, BrowserWindow, ipcMain } from "electron";
import { createRequire } from "node:module";
import path from "node:path";
import type { Action } from "./action.ts";
import { sanitizeActionName, tally } from "./action.ts";
import { ActionStore } from "./store.ts";

interface NativeStatus {
  ok: boolean;
  status: string;
}

interface NativeActionResult {
  status: string;
  action?: Action;
  message?: string;
}

interface NativeAddon {
  listApps(): Array<{ owner: string; title: string; w: number; h: number; visible: boolean }>;
  keyboardReady(): boolean;
  startRecording(apps: Array<{ owner: string; title: string }>): NativeStatus;
  stopRecording(): void;
  pollRecording(): NativeActionResult | null;
  startPlayback(action: Action, countdown: number): NativeStatus;
  stopPlayback(): void;
  pollPlayback(): { status: string; message: string } | null;
}

const require = createRequire(__filename);
const root = app.getAppPath();
const native = require(path.join(root, "build", "Release", "ak_pointer.node")) as NativeAddon;

const store = new ActionStore(path.join(root, "actions"));

let recording = false;
let playing = false;
let message = "Choose the app, record an action, then play the list in order.";
let pendingName = "";
let playQueue: string[] = [];
let playIndex = 0;
let playCancel = false;

function snapshot(catalog = false) {
  return {
    busy: recording || playing,
    message,
    catalog: catalog ? store.catalog() : undefined,
  };
}

function permissionMessage(): string {
  return "Accessibility is off. System Settings → Privacy & Security → Accessibility → enable Electron. If you started ak-pointer from Cursor, enable Cursor too.";
}

function chosenLabel(apps: Array<{ owner: string; title: string }>): string {
  return apps
    .map((appChoice) => (appChoice.title ? `${appChoice.owner} — ${appChoice.title}` : appChoice.owner))
    .join(", ");
}

function beginPlayback(index: number, countdown: number): string {
  const name = playQueue[index];
  const action = store.load(name);
  if (!action) {
    playing = false;
    playQueue = [];
    message = `${name} could not be read.`;
    return message;
  }
  const started = native.startPlayback(action, countdown);
  if (!started.ok) {
    playing = false;
    playQueue = [];
    message = started.status === "no-accessibility" ? permissionMessage() : "Playback could not start.";
    return message;
  }
  playing = true;
  const owner = action.windowTitle ? `${action.windowOwner} — ${action.windowTitle}` : action.windowOwner;
  message = `Playing ${index + 1} of ${playQueue.length}: ${name}. Bringing ${owner} to the front.`;
  return message;
}

function finishPlayback(result: { status: string; message: string }) {
  if (playCancel || result.status === "cancelled") {
    playing = false;
    playQueue = [];
    message = "Stopped.";
    return;
  }
  if (result.status !== "done") {
    playing = false;
    playQueue = [];
    message = result.message || "Playback failed.";
    console.log(message);
    return;
  }
  playIndex += 1;
  if (playIndex >= playQueue.length) {
    playing = false;
    playQueue = [];
    message = "Finished the list.";
    return;
  }
  beginPlayback(playIndex, 0);
}

ipcMain.handle("list-apps", () => ({
  apps: native.listApps(),
  keyboard: native.keyboardReady(),
}));

ipcMain.handle("catalog", () => store.catalog());

ipcMain.handle(
  "start-recording",
  (
    _event,
    payload: { name?: string; apps?: Array<{ owner: string; title: string }>; replace?: boolean },
  ) => {
    if (recording || playing) {
      return { ...snapshot(), ok: false, message: "Wait for the current run to finish." };
    }
    const apps = payload.apps ?? [];
    if (apps.length === 0) {
      message = "Choose at least one app on the left. For example the iPhone simulator.";
      return { ...snapshot(), ok: false };
    }
    const name = sanitizeActionName(payload.name ?? "");
    if (!name) {
      message = "Enter a name for the action.";
      return { ...snapshot(), ok: false, sanitized: "" };
    }
    if (store.exists(name) && !payload.replace) {
      message = `${name} already exists. Press Record again to replace it.`;
      return { ...snapshot(), ok: false, needReplace: true, sanitized: name };
    }
    const started = native.startRecording(apps);
    if (!started.ok) {
      message =
        started.status === "no-accessibility"
          ? permissionMessage()
          : started.status === "busy"
            ? "Wait for the current run to finish."
            : "The selected app has no window. Open it, then record again.";
      return { ...snapshot(), ok: false, sanitized: name };
    }
    recording = true;
    pendingName = name;
    message = `Recording on ${chosenLabel(apps)}. Clicks, drags, moves, and keys are tracked. Other apps are ignored. Press Stop or F8.`;
    if (!native.keyboardReady()) message += " Keyboard needs Input Monitoring for Electron.";
    return { ...snapshot(), ok: true, started: true, sanitized: name };
  },
);

ipcMain.handle("stop", () => {
  if (recording) native.stopRecording();
  if (playing) {
    playCancel = true;
    native.stopPlayback();
  }
  message = "Stopping…";
  return snapshot();
});

ipcMain.handle("poll", () => {
  if (recording) {
    const result = native.pollRecording();
    if (result) {
      recording = false;
      if (result.status === "ok" && result.action) {
        const file = store.pathFor(pendingName);
        if (store.save(pendingName, result.action)) {
          message = `Saved ${file}. ${tally(result.action)}`;
        } else {
          message = `Could not save ${file}.`;
        }
        console.log(message);
      } else if (result.status === "empty") {
        message =
          "Nothing was recorded inside the selected window, so it was not saved. Click the simulator screen, then press Stop.";
        console.log(message);
      } else if (result.status === "no-window") {
        message = "The selected app has no window. Open it, then record again.";
      } else {
        message = permissionMessage();
      }
      return snapshot(true);
    }
  }
  if (playing) {
    const result = native.pollPlayback();
    if (result) finishPlayback(result);
  }
  return snapshot(false);
});

ipcMain.handle("play", (_event, names: string[]) => {
  if (recording || playing) {
    return { ...snapshot(), ok: false, message: "Wait for the current run to finish." };
  }
  if (!names.length) {
    message = "Add an action to the list first.";
    return { ...snapshot(), ok: false };
  }
  playQueue = names;
  playIndex = 0;
  playCancel = false;
  beginPlayback(0, 3);
  return { ...snapshot(), ok: playing };
});

ipcMain.handle("load", (_event, name: string) => {
  const action = store.load(name);
  if (!action) return { ok: false, message: `${name} could not be read.` };
  return { ok: true, name, path: store.pathFor(name), action };
});

ipcMain.handle("save", (_event, name: string, action: Action) => {
  const clean = sanitizeActionName(name);
  if (!clean) return { ok: false, message: "Enter a name for the action.", ...snapshot() };
  if (!store.save(clean, action)) {
    return { ok: false, message: "Could not save the action file.", ...snapshot() };
  }
  message = "Saved the action file.";
  return { ok: true, message, ...snapshot(true) };
});

ipcMain.handle("save-raw", (_event, name: string, text: string) => {
  const clean = sanitizeActionName(name);
  const action = store.saveText(clean, text);
  if (!action) {
    return {
      ok: false,
      message: "Could not read that file. Check the lines and try again.",
      ...snapshot(),
    };
  }
  message = "Saved the action file.";
  return { ok: true, message, action, ...snapshot(true) };
});

ipcMain.handle("delete", (_event, name: string) => {
  if (!store.delete(name)) return { ok: false, message: `Could not delete ${name}.`, ...snapshot() };
  message = `Deleted ${name}.`;
  return { ok: true, message, ...snapshot(true) };
});

function createWindow(): void {
  const window = new BrowserWindow({
    width: 1080,
    height: 700,
    minWidth: 860,
    minHeight: 540,
    title: "ak-pointer",
    backgroundColor: "#1b1b1b",
    webPreferences: {
      preload: path.join(root, "dist", "preload.js"),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
  });
  window.webContents.on("did-fail-load", (_event, code, description, url) => {
    console.error(`Failed to load ${url}: ${description} (${code})`);
  });
  window.webContents.on("did-finish-load", () => {
    console.log("ak-pointer window ready");
  });
  void window.loadFile(path.join(root, "dist", "index.html"));
}

app.whenReady().then(() => {
  createWindow();
  app.on("activate", () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
  });
});

app.on("window-all-closed", () => {
  app.quit();
});

app.on("before-quit", () => {
  try {
    native.stopRecording();
    native.stopPlayback();
  } catch {
    // The addon is already gone during shutdown.
  }
});
