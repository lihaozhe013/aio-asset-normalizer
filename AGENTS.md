# Repository Engineering Policy

These rules are mandatory for every change in this repository. Keep this file
focused on durable engineering policy. Feature plans, release notes, historical
investigations, and manual test cases belong in dedicated documents under
`docs/`.

## 1. Language

- All source-code comments, doc comments, commit messages, and newly created or
  updated documentation MUST be written in English.
- Chinese text MUST NOT be added to comments or engineering documentation. When
  touching an existing non-English comment or documentation section, translate
  the affected text to English as part of the same change.
- Localized user-facing strings are exempt from this rule. Keep localization
  content separate from engineering documentation whenever practical.
- Names and prose MUST be clear enough to explain intent. Do not add comments
  that merely restate the code.

## 2. Project Scope and Product Boundaries

- This repository is a cross-platform desktop tool for editing, previewing, and
  standardizing glTF 2.0 Binary assets (`.glb`) for independent game
  developers and creators.
- The supported asset format is `.glb`. Do not add FBX, OBJ, Blend, or other
  general-purpose format conversion paths unless the product scope is
  explicitly changed.
- The GLB Editor and BVH Studio core MUST NOT depend on Blender, the Blender
  API, a Blender installation, or an external conversion process; they MUST
  remain fully functional when Blender is absent.
- The FBX Converter workflow is the single sanctioned exception: it MAY invoke
  an external Blender installation as a headless subprocess to convert
  FBX/OBJ/Blend inputs to GLB. It MUST degrade to a clear, actionable error
  when Blender is unavailable, MUST NOT block other pages, and MUST NOT become
  a dependency for building, testing, or developing the rest of the
  application.
- BVH functionality belongs to an independent workflow. It MUST remain
  generic: no fixed company model, fixed skeleton size, fixed rest pose,
  device protocol, IMU mapping, or hardcoded proprietary asset dependency.
- BVH retargeting MUST use a versioned Mapping contract. Name matching may
  produce suggestions, but it MUST NOT silently replace explicit mappings.
- Prefer preserving source GLB resources and unknown extensions. Operations
  that cannot be validated safely MUST fail with a useful error instead of
  producing a potentially corrupted asset.

## 3. Architecture and Dependency Direction

The application is intentionally split into layers. Keep dependencies flowing
from UI and rendering toward application state and domain services, never in
the opposite direction.

- `src/main.rs` owns only process startup, the window, and the render loop.
- `src/app.rs` owns top-level state, page routing, task polling, and coordination
  between the editor, BVH workflow, and viewport.
- UI modules own egui presentation and user intent. UI code MUST NOT parse GLB
  binary data, write Accessors, or invoke worker implementation details.
- Viewport modules own three-d objects, camera controls, helpers, and preview
  snapshots. Viewport rendering MUST NOT depend directly on egui widget state.
- GLB domain modules own document indexing, edits, Accessor/resource updates,
  validation, and atomic export.
- BVH domain modules own parsing, rest-pose data, forward kinematics, trimming,
  Mapping validation, retargeting, and animation export.
- Background workers receive immutable job inputs and communicate with the UI
  through `std::sync::mpsc` or an equivalent message boundary.
- `three-d-asset` is a preview/loading aid; it is not the source of truth for
  GLB write-back. Preserve the editable JSON/BIN document separately.
- Keep public APIs small and predictable. Avoid global mutable state, circular
  module dependencies, and convenience modules that become dumping grounds.

## 4. Cross-Platform Requirements

- New functionality MUST support every maintained platform unless the task
  explicitly narrows its scope.
- A Windows-only toolchain, PowerShell script, batch file, registry operation,
  or Win32 command MUST NOT be the sole implementation of a build, test,
  development, or maintenance workflow.
- Prefer portable Rust code and established cross-platform crates. Isolate
  unavoidable platform-specific behavior behind explicit `cfg` boundaries and
  provide equivalent behavior for other maintained platforms.
- For repository automation that cannot reasonably be implemented in Rust,
  prefer a Python script using the standard library. Invoke Python tooling with
  `uv run`. Do not create parallel shell, PowerShell, and batch implementations
  when one portable script can serve all platforms.
- Platform-specific packaging scripts are allowed only inside the relevant
  packaging workflow. They MUST NOT become prerequisites for normal development
  on other platforms.
- Do not introduce environment variables for routine configuration when a
  command-line option, configuration file, or stable application default is
  sufficient. Any required environment variable MUST be documented and kept to
  the narrowest possible scope.
- Use `std::path::Path` and `PathBuf` for filesystem paths. Do not hardcode path
  separators, drive letters, home directories, or platform-specific executable
  suffixes in shared code.
- Use cross-platform file dialogs, window APIs, image loading, and atomic file
  replacement. Do not make a GUI acceptance path depend on one operating
  system.

## 5. Logging and Debugging

- Application logs MUST be written to `debug.log` by default. Running the
  application MUST NOT require stdout or stderr redirection to capture logs.
- Normal application logging MUST NOT write to the terminal. Startup must remain
  resilient if the log file cannot be created.
- `RUST_LOG` may be used as an optional log-level override, but the application
  MUST provide a useful default without it.
- Never log passwords, tokens, private keys, credentials, local secrets, or
  complete user-provided paths when they may contain sensitive information.
- Logs added for a feature or investigation MUST use a stable prefix such as
  `[glb_editor]` or `[bvh_studio]` so they can be filtered reliably.
- When handing off a debugging workflow, provide a ready-to-run command that
  exercises the relevant flow and filters `debug.log` into a focused log file.
  For example:

  ```bash
  cargo run
  rg "\[bvh_studio\]" debug.log > bvh-studio-debug.log
  ```

- Generated `*.log` files MUST remain untracked and MUST NOT be included in
  commits or release archives.

## 6. GLB and BVH Data Safety

- Treat GLB input as untrusted data. Validate headers, chunk lengths, JSON
  references, Accessor ranges, component types, counts, and finite numeric
  values before use.
- Keep GLB JSON and BIN alignment compliant with the glTF 2.0 specification.
- Re-parse every generated GLB through the project reader before reporting
  success. For animation or Skin edits, sample representative frames and
  validate world transforms, hierarchy, Skin references, and Mesh bounds.
- Do not silently discard `extras`, unknown extensions, materials, textures,
  animations, or scene objects that are outside the requested edit.
- Detect unsupported compressed geometry and return a node/resource-specific
  error when an operation would require decoding it.
- BVH parsers MUST return structured errors with useful context. Do not use
  `panic!`, `unwrap`, or `expect` for malformed user input or file content.
- Mapping validation MUST reject missing roots, duplicate target nodes, invalid
  Skin references, and ambiguous mappings before retargeting begins.
- Use explicit coordinate-system and unit metadata. Never infer a destructive
  conversion solely from a filename or an arbitrary model dimension.

## 7. Code Organization and File Size

- Preserve the existing structure and formatting unless a refactor is part of
  the requested change.
- Every source file over 1,000 lines MUST trigger an explicit design review
  before more responsibilities are added. Evaluate cohesion, dependency
  direction, state ownership, and whether behavior can move to focused modules.
- Do not allow a file to cross the 1,000-line threshold without recording the
  assessment in the change summary or commit body.
- When modifying an existing file that already exceeds 1,000 lines, avoid
  increasing its scope. If the affected behavior has a clear boundary, split it
  during the change. If an immediate split would make the change riskier, state
  the reason and identify the intended module boundary.
- New modules MUST have one clear responsibility. Keep entry points, `mod.rs`
  files, and application coordinators thin.
- Prefer `cargo fmt`-standard Rust, explicit imports in submodules, and
  `Result`-based error propagation with `thiserror`/`anyhow` when appropriate.
- Do not add emojis or unnecessary comments to source, documentation, or commit
  messages.

## 8. Required Validation

- Before every commit, run `cargo fmt --all`. This is mandatory even when the
  change appears not to affect formatting.
- After formatting, run the most relevant automated checks. `cargo test` is the
  minimum default for Rust behavior changes; use `cargo check --all-targets`
  when a full test run is not applicable.
- For GLB or BVH changes, add or update focused unit tests and run the relevant
  round-trip and validation fixtures.
- Do not report a check as successful unless it was actually run. Clearly state
  any check that could not be completed and why.
- GUI behavior that cannot be validated reliably in the agent environment MUST
  be handed off with concise, platform-neutral manual verification steps.
- Do not make Windows-only manual verification the canonical acceptance path for
  cross-platform behavior.
- Before declaring work complete, check `git diff --check`, inspect the final
  diff, and confirm generated artifacts are not included.

## 9. Documentation and Task Tracking

- `README.md` contains the product overview, supported scope, and developer
  entry points. Keep detailed design decisions in `docs/`.
- Feature plans, migration notes, release notes, historical investigations,
  and manual test procedures belong in dedicated English documents under
  `docs/`.
- Do not recreate removed legacy documents or maintain a stale checklist of
  completed tasks. When a task is complete, remove its pending entry from the
  relevant planning document.
- Documentation MUST describe actual behavior. Clearly label planned behavior,
  unsupported input, experimental features, and platform-specific limitations.

## 10. Commits

- Every commit MUST use a complete Conventional Commits message:
  `<type>(optional-scope): imperative summary`.
- Use the narrowest accurate type, such as `feat`, `fix`, `refactor`, `docs`,
  `test`, `build`, `ci`, or `chore`. Vague subjects such as `update files` or
  `misc fixes` are forbidden.
- Non-trivial commits MUST include a body explaining the motivation, behavior
  change, and important compatibility or validation details.
- Breaking changes MUST use `!` in the header or a `BREAKING CHANGE:` footer.
- Do not commit, amend, push, or create a pull request unless the user
  explicitly requests it.

## 11. Safety and Repository Hygiene

- Inspect `git status` before editing. Preserve unrelated user changes and do
  not rewrite them.
- Never commit generated logs, credentials, private keys, build artifacts,
  local profile databases, or user asset outputs.
- Destructive or irreversible commands require explicit user approval. Confirm
  exact targets before deleting or overwriting files.
- Use `rg` for text search, `fd` for file discovery, and `uv run` for Python
  commands. Prefer `apply_patch` for source and documentation edits.
- Do not create or switch Git branches unless the user explicitly requests it.
- Keep changes focused. Do not mix unrelated cleanup, refactoring, and feature
  work in one commit.
