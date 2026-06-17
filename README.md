# UCP Clipboard

Windows-first desktop clipboard history app built with Dioxus 0.7.

## Features

- Captures text, image, and file clipboard entries on Windows.
- Stores history and settings locally in SQLite.
- Searches text and file entries, with filters for all, text, image, file, and favorite items.
- Supports favorite, pin, multi-select deletion, copying entries back to the clipboard, and optional quick paste.
- Saves captured images as PNG files from the item context menu.
- Keeps running in the background when the window is closed, with a system tray menu for showing the window or exiting.
- Includes a frameless desktop window, custom window controls, settings page, and status bar feedback.

## Shortcuts

- `Ctrl+Shift+V`: Global shortcut to show and focus UCP Clipboard.
- `Ctrl+F`: Focus search.
- `Ctrl+,`: Toggle settings.
- `Ctrl+1` to `Ctrl+5`: Switch filters.
- `ArrowUp` / `ArrowDown`: Move through the history list.
- `Shift+ArrowUp` / `Shift+ArrowDown`: Extend selection.
- `Ctrl+A`: Select visible entries.
- `Enter`: Copy the focused entry.
- `Delete` / `Backspace`: Delete focused or selected entries.
- `F`: Toggle favorite on the focused entry.
- `P`: Toggle pin on the focused entry.
- `Escape`: Clear selection, clear search, or close settings.

In-app keyboard shortcuts can be disabled in settings. The global show-window shortcut is registered by the desktop shell integration.

## System Tray

Closing the window hides it to the background so clipboard monitoring continues. Use the tray icon to show the window again or choose `退出` to fully quit the app.

## Development

Install the Dioxus CLI if it is not available:

```powershell
cargo install dioxus-cli
```

Run debug builds with hot reload enabled:

```powershell
dx serve --platform desktop
```

`dx serve` enables RSX hot reload and Subsecond hot patching in debug mode. Use `cargo run` only when hot reload is not needed.

Run tests and lint checks:

```powershell
cargo test
cargo clippy --all-targets -- -D warnings
```

## Data Storage

On Windows, UCP stores its SQLite database under:

```text
%LOCALAPPDATA%\UCP Clipboard\history.sqlite3
```

If `%LOCALAPPDATA%` is unavailable, it falls back to `%APPDATA%`, then the current directory.

## Scope

- Current platform target: Windows desktop.
- Current clipboard support: text, image, and file read/write through a platform adapter.
- Compatibility boundary: future macOS/mobile work should extend `src/platform` without reshaping UI or history state.
