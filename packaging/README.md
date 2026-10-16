# Packaging

These scripts create native distribution artifacts for Windows, macOS, and
Linux. Run them from the repository root with `uv`.

All outputs default to `packaging/out`, which is ignored by Git.

## Application icon

Every shipped icon uses the opaque white-background artwork so the dark logo
stays visible on dark window borders, docks, and taskbars. `assets/icon` holds:

- `aio-asset-normalizer.png`: white-background master, the source for every
  generated artifact.
- `aio-asset-normalizer-512.png`: 512 px white-background variant used by the
  runtime window icon and the AppImage desktop entry and `.DirIcon`.
- `aio-asset-normalizer.ico`: Windows icon with the same white background,
  embedded into the executable by `build.rs` and used by the Inno Setup
  installer.
- `aio-asset-normalizer-transparent.png`: original transparent artwork, kept
  for reference only. Do not ship it. When the artwork changes, flatten it onto
  the `#FAF8F4` background to refresh the master, then resize that master to
  512 px and export the `.ico` at 16, 32, 48, 64, 128, and 256 px.

## Windows

Install [Inno Setup 6](https://jrsoftware.org/isinfo.php), then run:

```bash
uv run packaging/build-windows.py
```

The script builds the release GUI and MCP executables and creates an Inno Setup installer. Both are installed together so the MCP service can find and launch the GUI from the stable installation directory.
Use `--debug` to package a debug executable or `--skip-build` to package an
existing executable.

## macOS

Run the following commands on macOS:

```bash
cargo build --locked --release --features mcp --bin aio-asset-normalizer --bin aio-asset-normalizer-mcp
uv run packaging/build-mac-app.py
```

The script creates both an `.app` bundle and a `.dmg`. The GUI and MCP executables are siblings in `Contents/MacOS`. The DMG includes
`Remove Quarantine.command`, which removes the quarantine attribute from the
installed unsigned app when Gatekeeper blocks it. Use `--no-dmg` to create
only the app bundle, or `--no-script` to omit the workaround.

The macOS icon is generated with `sips` and `iconutil`. If the bundled PNG
icon cannot be used, install the Xcode command-line tools or pass another PNG
with `--icon`.

## Linux

Run the following commands on Linux:

```bash
cargo build --locked --release --features mcp --bin aio-asset-normalizer --bin aio-asset-normalizer-mcp
uv run packaging/build-appimage.py
```

The script uses `appimagetool` from `PATH` when available and otherwise caches
the x86_64 release under the user's cache directory. Use `--appimagetool` for
an architecture-specific tool, and `--arch aarch64` when packaging an ARM64
binary. The AppImage contains both binaries. Configure MCP clients to launch the stable `.AppImage` path with the `--mcp` argument; `AppRun` dispatches to the bundled service without exposing its temporary mount path.

## MCP service

The native MCP server uses stdio and has no Python or Node.js runtime dependency.
Install the desktop package once, then copy the platform-specific client
configuration from [`docs/MCP.md`](../docs/MCP.md). File tools do not start the
GUI. Call `session_open` when visual review is needed; it connects to a running
window or launches the installed GUI.

## Make targets

From the repository root, `make build` builds the release GUI and MCP server.
`make dev` builds and launches the debug pair. `make build-win`,
`make build-mac`, and `make build-appimage` build and package the matching
desktop distribution.
