# Repository Policy

## Project

- Cross-platform desktop tool for editing, previewing, and standardizing glTF 2.0
  Binary assets (`.glb`) for indie game developers.
- Only `.glb` is supported. No FBX/OBJ/Blend conversion paths except the FBX
  Converter workflow.
- The GLB Editor and BVH Studio MUST work without Blender. The FBX Converter alone
  MAY invoke a headless Blender subprocess; it MUST error clearly when Blender is
  unavailable and MUST NOT block other pages or affect building/testing.
- BVH functionality is an independent, generic workflow: no fixed company model,
  skeleton size, rest pose, device protocol, or IMU mapping. Retargeting uses a
  versioned Mapping contract; name matching may suggest but must never silently
  replace explicit mappings.
- Preserve source GLB resources and unknown extensions. Fail safely with a useful
  error instead of emitting a potentially corrupted asset.

## Architecture

Dependencies flow from UI/rendering toward application state and domain services,
never the reverse.

- `src/main.rs`: process startup, window, render loop only.
- `src/app.rs`: top-level state, page routing, task polling.
- UI modules: egui only; MUST NOT parse GLB binary data or write Accessors.
- Viewport modules: three-d objects, camera, helpers; MUST NOT depend on egui
  widget state.
- GLB domain: document indexing, edits, Accessor/resource updates, validation,
  atomic export.
- BVH domain: parsing, rest pose, forward kinematics, trimming, Mapping
  validation, retargeting, animation export.
- Workers take immutable jobs and talk to the UI via `std::sync::mpsc`.
- `three-d-asset` is preview-only; the editable JSON/BIN document is the source
  of truth for write-back.
- Keep public APIs small. No global mutable state, circular module deps, or
  dumping-ground modules.

## Constraints

- All comments, doc comments, commit messages, and docs in English; no Chinese or
  emoji in code/docs. Comments must explain intent, not restate code.
- Support every maintained platform. Prefer portable Rust crates; isolate
  platform-specific behavior behind `cfg`. For automation, prefer a portable
  python stdlib (or Rust) script over parallel shell/PowerShell/batch files.
  No Windows-only workflows; no environment variables when CLI args or config
  suffice; use `Path`/`PathBuf`, no hardcoded separators or home paths.
- Logging only through `tracing` pipeline in `src/modules/logging.rs` with stable
  targets (`glb_editor`, `bvh_studio`, ...). No `println!`/`dbg!`. Logs live in
  the platform app-data `logs/` dir. Attach `task_id` to background records; use
  `logging::safe_path_label`; never log secrets or `*.log` into commits.
- Treat GLB as untrusted: validate headers, chunk lengths, JSON references,
  Accessor ranges/components/counts, finite values. Keep glTF 2.0 alignment;
  re-parse every generated GLB before reporting success. Do not silently drop
  `extras`, unknown extensions, materials, textures, animations, or scene objects.
- BVH parsers return structured errors; no `panic!`/`unwrap`/`expect` on
  malformed input. Mapping validation rejects missing roots, duplicate targets,
  invalid Skin refs, and ambiguity. Use explicit coordinate/unit metadata; never
  infer a destructive conversion from a filename.
- Files over 1,000 lines trigger an explicit design review before growth; record
  the assessment in the change summary or commit. Don't further inflate an
  oversized file; split when the change boundary is clear. New modules have one
  responsibility; entry points and coordinators stay thin.
- Feature plans, release notes, and test cases belong in `docs/`, not here.