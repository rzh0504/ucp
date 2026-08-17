# UCP Agent Guide

UCP is a native clipboard history application built with Rust, GPUI, and GPUI Component.

## Project Structure

- `src/app.rs` and `src/app/`: application state, views, and clipboard monitor.
- `src/model.rs`: clipboard entries, filters, settings, and history behavior.
- `src/storage.rs`: SQLite persistence and image preview cache.
- `src/platform/`: platform clipboard, startup, and single-instance integrations.
- `src/services/`: UI-independent clipboard operations.

## Development

```powershell
cargo run
cargo test
cargo clippy --all-targets -- -D warnings
```

Initialize GPUI Component with `gpui_component::init(cx)` before creating views, and wrap each window's root entity in `gpui_component::Root`.

The history list is virtualized. Keep row height stable and avoid synchronous clipboard, image encoding, or SQLite work on the GPUI thread.

Preserve the existing SQLite schema and data directory so upgrades retain user history and settings.
