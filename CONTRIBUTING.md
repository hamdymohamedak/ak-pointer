# Contributing

ak-pointer records mouse and keyboard actions on a macOS window you choose, saves them under `actions/`, and plays them back. Playback brings that window forward, including when it is behind other windows.

## Build and test

Bun and the Xcode command line tools are required.

```bash
bun install
bun test
bun run start
```

`bun test` checks the action file format, coordinate scaling, and typed-text edits. It does not move the cursor. `bun run start` bundles the Electron app, rebuilds the native addon for Electron, and opens the window. Quit any older copy first, because a new build does not replace a process that is already running.

Recording and playback need two macOS permissions. When you start the app from Cursor’s terminal, grant them to Electron and to Cursor:

1. System Settings → Privacy & Security → Accessibility
2. System Settings → Privacy & Security → Input Monitoring

Accessibility is required for mouse recording and for raising windows. Input Monitoring is required for the keyboard. Without it, clicks and drags still record, and the window says so.

## Layout

| Path | Role |
| --- | --- |
| `src/action.ts` | The action in memory and the text format in `actions/*.txt`. |
| `src/edit.ts` | Typed-text editing. No files and no macOS calls. |
| `src/geometry.ts` | Window-relative coordinates. The same math is in the native player. |
| `src/store.ts` | Reads and writes the `actions/` directory. |
| `src/main.ts` | The Electron main process. It owns files, recording, and playback. |
| `src/preload.ts` | The bridge the window is allowed to call. |
| `src/renderer/` | The window. It does not open files or post events itself. |
| `native/ak_pointer.cc` | Window list, permissions, the event tap, and playback. |

A new event field belongs in `src/action.ts` and in the native addon’s action objects. A new macOS call belongs in `native/ak_pointer.cc`. The window should call the preload bridge instead of opening files or sending events itself.

## Action files

Files are plain text in `actions/<name>.txt`. One action remembers the window it was recorded on. If the recording touches a second selected app, a `target` line names that window and later events belong to it. Keep older files loading: `bun test` covers a single window and a second window.

## Checks

Run `bun test` before sending a change. Try a recording against the iOS Simulator or another window when the change affects capture or playback, and confirm the saved file contains the clicks you made (`2 clicks`, not `0 clicks`).
