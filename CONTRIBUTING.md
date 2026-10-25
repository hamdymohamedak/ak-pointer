# Contributing

This is a macOS app that records mouse and keyboard actions on a window you choose, saves them under `actions/`, and plays them back. Playback brings that window forward, including when it is behind other windows.

## Build and test

Rust and the Xcode command line tools are required.

```bash
cargo test
cargo run
```

`cargo run` opens the window. Quit any older copy first, because a new build does not replace a process that is already running.

Recording and playback need two macOS permissions for the app that launches this program (usually Cursor, when you start it from the terminal):

1. System Settings → Privacy & Security → Accessibility
2. System Settings → Privacy & Security → Input Monitoring

Accessibility is required for mouse recording and for raising windows. Input Monitoring is required for the keyboard. Without it, clicks and drags still record, and the window says so.

## Layout

The binary in `src/main.rs` only starts the UI. Everything else lives in the library, split by what it is allowed to know:

| Module | Role |
| --- | --- |
| `model` | The action in memory: events, target windows, and typed-text editing. No files and no macOS calls. |
| `store` | `ActionStore` reads and writes the `actions/` directory. `store/codec.rs` is the text format. |
| `platform` | Window lists, raising a window, coordinate conversion, and permissions. `platform/ffi.rs` is the CoreGraphics binding. |
| `engine` | `record` watches the mouse and keyboard. `play` posts those events back. |
| `ui` | The egui window. `ui/app.rs` is the shell. `ui/editor.rs` edits one saved action. |

A change should stay in the lowest module that can express it. A new event field belongs in `model` and `store/codec.rs`. A new macOS call belongs in `platform`. The UI should call `ActionStore`, `engine`, and `platform` instead of opening files or sending events itself.

## Action files

Files are plain text in `actions/<name>.txt`. One action remembers the window it was recorded on. If the recording touches a second selected app, a `target` line names that window and later events belong to it. Keep older files loading: `store` tests cover a single window and a second window.

## Checks

Run `cargo test` before sending a change. The tests cover the file format, coordinate scaling, and typed-text edits. They do not move the real cursor. Try a recording against the iOS Simulator or another window when the change affects capture or playback, and confirm the saved file contains the clicks you made (`2 clicks`, not `0 clicks`).
