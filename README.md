# ak-pointer

ak-pointer records what you do in a macOS window, saves it as a text file, and plays it back later. You pick the app first — an iPhone simulator, an Android emulator, VS Code, Android Studio, or any other window — and playback brings that window forward even if it is behind other windows.

Coordinates are relative to the window, so playback still lines up if the window has moved.

## Requirements

- macOS
- [Rust](https://rustup.rs/)
- Xcode command line tools

## Run

From this directory:

```bash
cargo run
```

Quit an older copy before starting a new one. A rebuild does not replace a process that is already running.

## Permissions

Grant these to the app that launches ak-pointer. When you start it from Cursor’s terminal, that app is Cursor.

1. System Settings → Privacy & Security → Accessibility
2. System Settings → Privacy & Security → Input Monitoring

Accessibility is required to record the mouse and to raise windows. Input Monitoring is required to record the keyboard. Clicks and drags still record without it, and the window says so.

## Use

1. **Choose the apps.** On the left, check every window the action may touch. A checked app is brought to the front. Clicks on anything else are ignored.
2. **Record.** Type a name and press Record. Click, drag, scroll, and type in the selected apps. Press Stop or F8. The file is saved as `actions/<name>.txt`. After saving, the summary should show the clicks you made, for example `2 clicks`, not `0 clicks`.
3. **Play.** Add saved actions to the list and press Play in order. They run from top to bottom. The first one waits 3 seconds. Stop or Esc cancels.

Edit opens a saved action. Typed text can be corrected there, and a wrong click can be deleted. The file in `actions/` is updated when you save.

## Develop

See [CONTRIBUTING.md](CONTRIBUTING.md) for the module layout and tests.
