# Packaging

These scripts create native distribution artifacts for Windows, macOS, and
Linux. Run them from the repository root with `uv`.

All outputs default to `packaging/out`, which is ignored by Git.

## Windows

Install [Inno Setup 6](https://jrsoftware.org/isinfo.php), then run:

```bash
uv run packaging/build-windows.py
```

The script builds the release executable and creates an Inno Setup installer.
Use `--debug` to package a debug executable or `--skip-build` to package an
existing executable.

## macOS

Run the following commands on macOS:

```bash
cargo build --release
uv run packaging/build-mac-app.py
```

The script creates both an `.app` bundle and a `.dmg`. The DMG includes
`Remove Quarantine.command`, which removes the quarantine attribute from the
installed unsigned app when Gatekeeper blocks it. Use `--no-dmg` to create
only the app bundle, or `--no-script` to omit the workaround.

The macOS icon is generated with `sips` and `iconutil`. If the bundled PNG
icon cannot be used, install the Xcode command-line tools or pass another PNG
with `--icon`.

## Linux

Run the following commands on Linux:

```bash
cargo build --release
uv run packaging/build-appimage.py
```

The script uses `appimagetool` from `PATH` when available and otherwise caches
the x86_64 release under the user's cache directory. Use `--appimagetool` for
an architecture-specific tool, and `--arch aarch64` when packaging an ARM64
binary.

## CLI (headless)

```bash
uv run packaging/build-cli.py
```

The script builds the `aio-asset-normalizer-cli` executable and packages it as a
standalone archive: `.zip` on Windows, `.tar.gz` elsewhere. Each archive contains
the executable, the embedded CLI reference as `CLI.md`, and a short `README.txt`.

The archive name is `aio-asset-normalizer-cli-<version>-<platform>-<arch>`, where
`<platform>` is `win`, `macos`, or `linux` and `<arch>` is `x86-64` or `arm64`.
Use `--skip-build` to package an existing executable, `--debug` for the debug
profile, and `--platform`/`--arch` to override the detected target.

The CLI is self-contained (the reference, Blender script, and locales are
embedded), so the archive has no runtime dependencies beyond the executable. The
nightly release publishes the three archives next to the installers; the CLI
reference itself lives in [`docs/CLI.md`](../docs/CLI.md).
