# UCP Clipboard

Windows-first desktop clipboard history app built with Dioxus 0.7.

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

## Scope

- Current platform target: Windows desktop.
- Current clipboard support: text read/write through a platform adapter.
- Compatibility boundary: future macOS/mobile work should extend `src/platform` without reshaping UI or history state.
