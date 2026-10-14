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

The Phase 1 dependency commit does not change the application version, SQLite
schema, data directory, history/search algorithms, or clipboard behavior.
Subsequent optimization commits are recorded below. None restore selectable
expanded text or enable the optional `gpui-fast` backend.

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
- [ ] Copy while startup is loading or retrying; verify the current clipboard is
      captured after initialization/listener setup succeeds.
- [ ] Clear while capturing new and identical content, including a full history
      containing favorites/pins; restart and verify the retained records.
- [ ] Stress long/repetitive text, huge trailing-newline suffixes, rapid search
      changes, expansion and narrow-window scrolling.

## Optimization phases

### Phase 1 — dependency migration (completed)

Commit `4859e1a` contains the isolated migration and its five headless UI probes.
The native checklist above remains unverified.

### Phase 2 — persistence consistency (completed)

- UI actions synchronously enqueue work on one FIFO storage worker, then await
  replies without blocking GPUI. Favorite and promotion commands update only
  their own fields, rather than replaying stale full-entry snapshots.
- Clear/delete apply the same ID snapshot to storage and memory. New entries
  captured after that snapshot survive; destructive actions wait for loading.
- Deduplication preserves the submitted ID and protected/image metadata.
  Initial loading persists capacity pruning while retaining the pre-pruning ID
  high-water mark for the in-memory allocator. Schema and data directory remain
  unchanged.
- Settings writes use the same queue. Persistence failures use the notification
  path and leave a visible unsaved-data warning; unrelated successful writes do
  not silently dismiss a previous failure.
- Removed the unused full-table clear path.

Validation: `cargo test` passed all 50 tests, including eight new storage/worker
regressions; `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check`
passed on Windows x86_64 MSVC. Native clipboard/visual checks remain unverified.

### Phase 3 — responsive initialization, search and rendering (completed)

- Render a loading view without synchronously opening/migrating SQLite. Load
  settings/history, prune, sort and construct history on the background executor;
  install the complete storage session atomically. Failed startup cannot start
  capturing with an empty allocator, and offers retry. Settings stay disabled
  until ready.
- Construct native listeners off GPUI. A bounded-one nonblocking channel
  coalesces notifications: this intentionally captures the latest clipboard
  state, not every intermediate state. Sample the current clipboard once after
  successful startup/retry. Listener failures remain visible and offer retry.
- Use Unicode default case folding consistently for query and content, followed
  by streaming KMP matching. Greek sigma and multi-character expansions such as
  `ß`/`SS` match consistently. Canonical/compatibility normalization is
  intentionally not performed.
- Share immutable entries with `Arc` so background search snapshots do not clone
  clipboard text. Generation checks reject obsolete results; cancellation checks
  run between records and every 1024 folded characters inside large records.
  Pending searches cannot copy stale results with Enter.
- Cache virtual-list row sizes until data/layout changes. Bound collapsed text
  previews to 8 KiB at UTF-8 boundaries, before any suffix trimming. Shape bounded
  previews with GPUI using font and resolved Kit content width, including window
  decoration insets. Cache exact character counts across filter/search changes.
  Only explicitly expanded text retains a shared full string; expanded content
  always supports scrolling, and its expand/collapse control does not shrink.
- Review follow-ups harden Phase 2: pending deletion IDs participate in neither
  capture deduplication nor capacity eviction; tombstones are applied by the FIFO
  worker, not ahead of previously submitted saves. Regression cases cover
  same-content recapture, full protected histories, and failed deletion after an
  earlier save.

Validation on Windows x86_64 MSVC: all 71 tests, formatting, strict Clippy and
the locked release build pass. Tests include deterministic linear-comparison
bounds, cancellation, Unicode matching, cache reuse/pruning, actual GPUI text
wrapping and real SQLite failure/ordering cases. No FPS, latency, memory or
startup-time benchmark is claimed; the native checklist remains unverified.

Remaining boundaries: exact character count still performs one full scan on
first display of an uncached entry; explicit expansion still lays out full text.
Cache rebuilds on data/font/width changes are not constant-time. Graceful draining
of queued writes before the application's forced process exit remains follow-up
work; the worker draining its queue on drop is not a process-exit durability
guarantee. System-theme following and other UX review items are not included here.
