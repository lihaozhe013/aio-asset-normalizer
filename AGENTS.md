# AGENTS.md -- AI Agent Rules & Project Conventions

## Project Identity

**aio-asset-normalizer** is a lightweight, cross-platform desktop tool for batch-processing and normalizing 3D assets (FBX, Blend, OBJ) into game-engine-ready `.glb` files. It uses a dual-pane layout: a 2D control panel (egui) on the left and a 3D preview viewport (three-d) on the right. Heavy mesh transforms are offloaded to Blender CLI running headlessly in a background worker thread.

## Architecture: Separation of Concerns

This project is deliberately decoupled. Every agent action must respect these boundaries:

| Layer | Crate / Module | Responsibility |
|---|---|---|
| Entry & UI | `src/main.rs`, `src/app.rs`, `src/modules/ui/` | egui panels, file lists, config forms, log viewer |
| 3D Viewport | `src/modules/viewport/` | three-d rendering, orbit camera, axes/grid helpers |
| Blender Bridge | `src/modules/blender/` | `std::process::Command` calls, `mpsc` task channels, Python script dispatch |
| Asset I/O | `three-d-asset`, `gltf` crates | `.glb` loading / saving |

- **Never** let UI code reach into Blender subprocess details.
- **Never** let viewport rendering depend on egui panel state (pass data through the top-level app state instead).
- **Prefer** message passing (`std::sync::mpsc`) over shared mutable state for background tasks.
- Keep `main.rs` minimal; routing and state ownership belongs in `src/app.rs`.

## Tool Preferences

- **Prefer `rg` / `fd`** -- Search files and content with `rg` (ripgrep) and `fd` (fd-find) first. They are far faster than `grep` / `find`. Fall back to `grep` / `find` only when `rg` / `fd` are unavailable.
- **Prefer dedicated file tools** -- Use the Read, Write, Edit, Glob, and Grep tools rather than shell `cat`/`echo`/`sed`/`awk` for file operations.
- Run **`cargo check`** (or `cargo clippy`) after every non-trivial change to catch errors early.

## Commit Rules

- **Commit only on request** -- Never `git commit`, `git push`, or create a PR unless explicitly asked.
- Before committing, inspect `git status` and `git diff --stat`. Stage only the intended files; never stage generated artifacts (`target/`, `*.pdb`) or secrets.
- Write concise, descriptive commit messages in English that match the repo's existing style.
- **Use Conventional Commits** -- format commit messages as `type: description`, e.g. `feat:`, `fix:`, `docs:`, `refactor:`, `chore:`.

## Code Conventions

- **No emojis** in source files, comments, documentation, or commit messages.
- **No unnecessary comments** -- code should be self-documenting. Add comments only when intent is genuinely non-obvious.
- **Rust idioms**: use `cargo fmt`-standard formatting. Match the surrounding code's import style (`use three_d::*` currently used at crate root; prefer explicit imports in sub-modules). Use `anyhow` / `thiserror` for error propagation once the project adopts them.
- **Naming**: modules and types follow the planned directory structure (`modules::ui::file_list`, `modules::viewport::canvas`, `modules::blender::bridge`). Keep public API surface small and predictable.
- **Privacy**: never hardcode paths, tokens, or credentials. Accept them via config or environment variables.
- **File size** -- No single file should exceed ~500 lines. When a function, struct impl, or UI component grows large, extract it into a dedicated submodule file. Entry-point files (`main.rs`, `app.rs`) and top-level module files (`mod.rs`) must stay lean: expose only the public API and defer all implementation details to child modules. These files call functions; they do not contain inline heavy logic.

## Project Layout (Planned)

```
src/
  main.rs                  Entry point -- minimal, delegates to app.rs
  app.rs                   Top-level egui state machine & layout dispatch
  modules/
    ui/
      file_list.rs         File import & batch list panel
      config_panel.rs      Scale / axis / cleanup configuration
      log_viewer.rs        Background task stdout/stderr viewer
    viewport/
      canvas.rs            three-d render loop & viewport wrapper
      camera.rs            Orbit camera controller
      helpers.rs           Coordinate axes & ground grid builders
    blender/
      bridge.rs            Blender CLI invocation & lifecycle
      task.rs              mpsc task definitions & progress reporting
blender_scripts/
  normalize_v1.py          V1: static mesh / material normalization
  normalize_v2.py          V2: bone & animation bake (future)
```

When adding new functionality, place it in the appropriate module above rather than growing `main.rs`.

## Verification

- After any code change, run `cargo check` (and `cargo test` once tests exist).
- If `cargo check` emits warnings, fix them unless they are deliberately suppressed with a clear reason.
- Before declaring a task done, confirm the project still compiles cleanly.

## Task Tracking

- The project to-do list lives in `docs/TODO.md`.
- When a task is completed, remove its checkbox line from `docs/TODO.md`. Do not mark it as done; delete it.
