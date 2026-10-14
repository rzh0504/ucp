# GPUI Kit 0.7.3 migration

## Scope

- Replace direct GPUI, platform, component and asset dependencies with the
  `gpui-kit` facade pinned to `=0.7.3`. Commit `Cargo.lock` with the compatible
  `gpui-pre` 0.3.9 snapshot family.
- Initialize Kit and the existing bundled UCP palettes before opening windows.
- Use `gpui_kit::open_window` from inside the async context's `update`. Its builder
  returns application content; Kit provides the Base Root and component plugins.
- Remove `Root::bordered(false)` and manual dialog/notification layer rendering.
  Kit's presentation plugin owns window chrome. Keep `TitleBar::window_options`,
  the native HWND integration, close-to-tray interception and activation loop.
- Enable Kit's `test-support` only through dev-dependencies.

This migration does not change the application version, SQLite schema, data
directory, history/search algorithms, or clipboard behavior. It does not restore
selectable expanded text or enable the optional `gpui-fast` backend.

Upstream references:

- [0.7.0 window/overlay migration](https://github.com/longbridge/gpui-kit/releases/tag/v0.7.0)
- [0.7.1 input and control behavior changes](https://github.com/longbridge/gpui-kit/releases/tag/v0.7.1)
- [0.7.3 release; replaces withdrawn 0.7.2](https://github.com/longbridge/gpui-kit/releases/tag/v0.7.3)

## Automated regression checks

Run from the repository root:

```powershell
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release --locked
```

The UI probes in `src/app/tests.rs` use production `init_ui`, Kit's real window
entry point, bundled palettes, styled Input and window extension operations.
They intentionally do not construct `ClipboardApp`, open the user's database,
start a native clipboard listener, or post OS notifications. They verify:

- The returned content belongs to a Base Root.
- Dialogs and notifications render automatically, without duplicate identities
  or leakage to a second window.
- Mixed Chinese/Latin search input survives alert confirmation and cancellation,
  confirmation runs once, and focus returns to the input.
- Both UCP palettes render and preserve their configured background colors.
- Custom icons and the default Kit icon fallback remain bundled.

The existing model/storage tests remain in the suite. These probes are not
end-to-end tests of the full clipboard application.

### Validation record

On Windows x86_64 MSVC, the pre-upgrade baseline passed 37 tests and Clippy.
After migration, all 42 tests, Clippy, formatting checks and the locked release
build passed. The native smoke checklist below remains unverified; automated
results must not be interpreted as a visual or cross-platform sign-off.

## Native smoke-test checklist

Run `cargo run` and repeat on a release build before publishing. Use disposable
clipboard contents for destructive tests. These checks require a native desktop;
headless tests cannot validate them.

- [ ] Start normally; verify title bar, initial size, search, icons and status bar.
- [ ] Capture/copy text, image and file entries; expand/collapse and scroll.
- [ ] Type Chinese using an IME; check candidate location, selection and Enter.
- [ ] Open row context menus; verify the action targets the clicked row only once.
- [ ] Open clear confirmation; cancel, then confirm using disposable history.
- [ ] Switch light/dark themes and open menus/dialogs in both.
- [ ] Navigate search, filters, settings and buttons using keyboard focus.
- [ ] On Windows: close to tray, restore from tray/hotkey, toggle always-on-top.
- [ ] On Windows: start with `--silent-startup`, restore, then quit from the tray.
- [ ] Trigger an error while inactive; check in-app/OS notification routing.
- [ ] Resize and check 100%/150%/200% scaling for clipping and pointer hit areas.
- [ ] On Linux: verify X11/Wayland client decorations, resizing, maximizing and
      tiled-window menu placement.
- [ ] On macOS: verify native title-bar controls and application activation.

Persistence ordering, clear/capture races, canonical IDs, row measurement caches,
Unicode search semantics and other performance/UX review findings are separate
follow-up work, not fixes claimed by this dependency migration.
