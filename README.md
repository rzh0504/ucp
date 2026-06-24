# UCP Clipboard

Cross-platform desktop clipboard history app built with Dioxus 0.7. Windows is the primary tested target; macOS and Linux support is implemented on a best-effort basis and should be treated as experimental until release builds are verified on real machines.

基于 Dioxus 0.7 的跨平台桌面剪贴板历史应用。Windows 是主要验证目标；macOS 和 Linux 已尽量补齐支持，但在真实设备和发布包验证完成前仍属于实验支持。

## Features

- Captures text, image, and file clipboard entries.
- Stores history and settings locally in a SQLite database with a `.ucp` extension.
- Searches text and file entries, with filters for all, text, image, file, and favorite items.
- Supports favorite, pin, multi-select deletion, copying entries back to the clipboard, and optional quick paste.
- Saves captured images as PNG files from the item context menu.
- Includes a frameless desktop window, custom window controls, settings page, status bar feedback, system tray integration, and desktop widget mode.

## 功能

- 捕获文本、图片和文件剪贴板记录。
- 使用本地 SQLite 数据库存储历史和设置，数据库扩展名为 `.ucp`。
- 支持搜索文本和文件记录，并按全部、文本、图片、文件、收藏筛选。
- 支持收藏、置顶、多选删除、复制回剪贴板和可选快捷粘贴。
- 可从条目菜单将捕获的图片保存为 PNG 文件。
- 包含无边框桌面窗口、自定义窗口控制、设置页、状态栏反馈、系统托盘集成和桌面小组件模式。

## Shortcuts

- `Ctrl+Shift+V`: Default global shortcut to show and focus UCP Clipboard. This can be changed in settings.
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

In-app keyboard shortcuts can be disabled in settings. If the global shortcut is invalid or already used by another app, UCP shows a status message and keeps running.

## 快捷键

- `Ctrl+Shift+V`：默认全局显示窗口快捷键，可在设置中修改。
- `Ctrl+F`：聚焦搜索。
- `Ctrl+,`：切换设置页。
- `Ctrl+1` 到 `Ctrl+5`：切换筛选。
- `ArrowUp` / `ArrowDown`：在历史列表中移动。
- `Shift+ArrowUp` / `Shift+ArrowDown`：扩展选择。
- `Ctrl+A`：选择当前可见记录。
- `Enter`：复制当前聚焦记录。
- `Delete` / `Backspace`：删除当前聚焦或已选择记录。
- `F`：切换当前聚焦记录的收藏状态。
- `P`：切换当前聚焦记录的置顶状态。
- `Escape`：清除选择、清除搜索或关闭设置页。

应用内快捷键可在设置中关闭。全局快捷键格式无效或被其他应用占用时，UCP 会在状态栏提示并继续运行。

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

## 平台备注

### Windows

- 当前支持最完整、验证最多的平台。
- 使用 Windows 剪贴板事件监听，不依赖轮询。
- 支持文本、图片和文件剪贴板读写。
- 通过模拟 `Ctrl+V` 支持快捷粘贴。
- 通过当前用户的 Run 注册表项支持开机启动。
- 桌面小组件模式支持置顶、固定尺寸、隐藏任务栏入口和原生窗口透明度。

### macOS

- 实验支持。
- 文本和图片剪贴板通过 `arboard` 支持。
- 文件剪贴板通过 `osascript` JavaScript for Automation 调用 AppKit `NSPasteboard` 读写。
- 剪贴板变更检测使用 `NSPasteboard.changeCount`；macOS 没有公开的通用剪贴板事件 API，因此这是轻量的尽力监听。
- 快捷粘贴通过 System Events 发送 `Cmd+V`，可能需要授予辅助功能权限。
- 开机启动会写入 `~/Library/LaunchAgents/dev.ucp.clipboard.plist`。
- 桌面小组件模式支持固定尺寸、窗口管理器支持范围内的置顶、透明 UI 背景和跨 Space 显示。Dock 图标会保留，避免窗口不可恢复。

### Linux

- 实验支持，实际效果依赖桌面环境。
- 文本和图片剪贴板通过 `arboard` 支持。
- 文件剪贴板使用常见桌面 MIME 类型：`x-special/gnome-copied-files` 和 `text/uri-list`。
- 剪贴板事件监听优先使用 `wl-paste --watch`，回退到 `clipnotify`；两者都不可用时会回退到轮询。
- 快捷粘贴优先使用 `wtype`，回退到 `xdotool`。
- 开机启动会写入 `$XDG_CONFIG_HOME/autostart/dev.ucp.clipboard.desktop` 或 `~/.config/autostart/dev.ucp.clipboard.desktop`。
- 桌面小组件模式支持固定尺寸、透明 UI 背景、跨工作区显示，以及在 X11/GTK 支持的环境中跳过任务栏。Wayland 下取决于具体 compositor。
- 为获得最佳运行效果，建议安装：`wl-clipboard`、`clipnotify`、`wtype`，以及根据会话选择 `xclip` 或 `xdotool`。

## System Tray

On Windows, closing the normal window hides it to the background so clipboard monitoring continues. Use the tray icon to show the window again or choose `退出` / `Quit` to fully quit the app.

On macOS and Linux, the close button minimizes the window instead of hiding it completely, because tray support varies by desktop environment. If tray initialization fails, UCP shows a status message.

## 系统托盘

在 Windows 上，关闭普通窗口会将应用隐藏到后台，剪贴板监听会继续运行。可通过托盘图标重新显示窗口，或选择 `退出` / `Quit` 完全退出应用。

在 macOS 和 Linux 上，由于托盘支持受桌面环境影响，关闭按钮会最小化窗口而不是完全隐藏。如果托盘初始化失败，UCP 会在状态栏提示。

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

## 数据存储

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

如果 Linux 未设置 `$XDG_DATA_HOME`，UCP 会回退到 `~/.local/share/UCP Clipboard/history.ucp`。如果平台相关环境变量不可用，则回退到当前目录。
