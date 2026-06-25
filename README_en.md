<p align="center">
  <img src="assets/icons/Ucp.png" alt="UCP Clipboard app icon" width="96" height="96">
</p>

# UCP Clipboard

[简体中文](README.md)

Cross-platform desktop clipboard history app built with Dioxus 0.7. Windows is the primary tested target; macOS and Linux support is implemented on a best-effort basis and should be treated as experimental until release builds are verified on real machines.

## Features

- Captures text, image, and file clipboard entries.
- Stores history and settings locally in a SQLite database with a `.ucp` extension.
- Searches text and file entries, with filters for all, text, image, file, and favorite items.
- Supports favorite, pin, multi-select deletion, copying entries back to the clipboard, and optional quick paste.
- Saves captured images as PNG files from the item context menu.
- Includes a frameless desktop window, custom window controls, settings page, status bar feedback, system tray integration, and desktop widget mode.

## Shortcuts

- `Ctrl+Shift+V`: Default global shortcut to show and focus UCP Clipboard. This can be changed in settings by clicking the shortcut field and pressing a new key combination.
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

## Platform Notes

### Windows

- Most complete and currently tested platform.
- Uses Windows clipboard events instead of polling.
- Supports text, image, and file clipboard read/write.
- Supports quick paste through synthetic `Ctrl+V`.
- Supports launch at startup through the current user's Run registry key.
- Desktop widget mode supports always-on-top, fixed size, hidden taskbar entry, and native window opacity.

### macOS

- Experimental support.
- Supports text and image clipboard through `arboard`.
- Supports file clipboard read/write through AppKit `NSPasteboard` via `osascript` JavaScript for Automation.
- Clipboard change detection uses `NSPasteboard.changeCount`; macOS does not provide a public general clipboard event API, so this is a lightweight best-effort watcher.
- Quick paste sends `Cmd+V` through System Events and may require Accessibility permission.
- Launch at startup writes `~/Library/LaunchAgents/dev.ucp.clipboard.plist`.
- Desktop widget mode supports fixed size, always-on-top where supported by the window manager, transparent UI background, and visibility across all Spaces. The app Dock icon is kept visible so the window remains recoverable.

### Linux

- Experimental support and desktop-environment dependent.
- Supports text and image clipboard through `arboard`.
- File clipboard support uses common desktop MIME types: `x-special/gnome-copied-files` and `text/uri-list`.
- Clipboard event watching prefers `wl-paste --watch` and falls back to `clipnotify`; if neither is available, UCP falls back to polling.
- Quick paste prefers `wtype` and falls back to `xdotool`.
- Launch at startup writes `$XDG_CONFIG_HOME/autostart/dev.ucp.clipboard.desktop` or `~/.config/autostart/dev.ucp.clipboard.desktop`.
- Desktop widget mode supports fixed size, transparent UI background, visibility across workspaces, and skipping the taskbar on X11/GTK-supported environments. Wayland support depends on the compositor.
- Recommended runtime tools for best support: `wl-clipboard`, `clipnotify`, `wtype`, and `xclip` or `xdotool` depending on the session.

## System Tray

On Windows, closing the normal window hides it to the background so clipboard monitoring continues. Use the tray icon to show the window again or choose `Quit` to fully quit the app.

On macOS and Linux, the close button minimizes the window instead of hiding it completely, because tray support varies by desktop environment. If tray initialization fails, UCP shows a status message.

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

Windows:

```text
%LOCALAPPDATA%\UCP Clipboard\history.ucp
```

macOS:

```text
~/Library/Application Support/UCP Clipboard/history.ucp
```

Linux:

```text
$XDG_DATA_HOME/UCP Clipboard/history.ucp
```

If `$XDG_DATA_HOME` is unset on Linux, UCP falls back to `~/.local/share/UCP Clipboard/history.ucp`. If platform-specific environment variables are unavailable, it falls back to the current directory.

## License

This project is open source under the [MIT License](LICENSE).
