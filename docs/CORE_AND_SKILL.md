# Shared core and agent skill

## Implementation boundaries

`aio_asset_normalizer` remains the single backend crate. `modules::operations`
contains front-end-neutral request contracts, shared defaults and errors, edited
snapshots, output planning, and checked execution for GLB, BVH, retarget, and
Converter. These APIs do not depend on clap, egui, or viewport state. The GUI
constructs immutable snapshots and the CLI loads equivalent file snapshots.
Both call the same computations and exporter. Features select front ends only;
they do not alter asset behavior.

The `desktop` and `cli` features are both enabled by default. CLI-only builds
exclude desktop dependencies, verified by `scripts/verify-headless.py`. Blender
is external and optional except during conversion. The Converter profile is
fixed and shared. GUI output overwrite is explicit and disabled initially.

Loaded and generated GLBs share binary range, component alignment, sparse-index,
and finite-float checks in `document_validation.rs`, alongside glTF reference
validation. Generated GLBs are serialized and re-parsed before success. File commits use
unique same-directory staging and no-clobber persistence unless overwrite was
requested. Source identity checks resolve symlinks and parent components.
Failures preserve previous destination files. Multi-output GLB execution records
completed files on subsequent failure; it does not roll back successful outputs.

## Oversized-file design review

The existing GLB coordinator (`src/modules/glb/mod.rs`) exceeded 1,000 lines.
Document loading, serialization, and atomic export were extracted into
`document_io.rs`, reducing the coordinator rather than adding new workflows to
it. New orchestration is split by domain under `operations/`; request schemas
and cross-domain path/error utilities are separate responsibilities. Existing
large retarget/domain test files retain their established algorithm boundaries;
this change does not add a second retarget implementation. Application and
retarget coordinators remain below 1,000 lines after this change. The generated
Cargo lockfile changes were reviewed for the schema/staging additions and the
headless dependency boundary. Changes in
`glb/export_selection.rs` only derive report serialization; changes in the
2,000-line `retarget.rs` preserve existing algorithm boundaries and replace
swallowed fingerprint/serialization errors with propagation. Both were reviewed
without adding new orchestration responsibilities. GLB request contracts now
live in `glb/request_spec.rs`, shared file/error utilities in `operation_support`,
and filename handling in `glb/filename.rs` so domain submission does not call
back into operation coordinators. Further growth
of existing oversized algorithm files should extract an independently testable
responsibility first.

## Compatibility and agent delivery

Envelope v1 and existing command spellings remain supported. New request fields
are additive. Missing edits now correctly use identity scale 1. JSON usage
errors have exit code 2; unreadable files and fingerprints produce I/O errors.
`--job` cannot be combined with explicit task flags. Relative job paths continue
to resolve from the process working directory. Explicit GLB selection and recipe
are mutually exclusive. Smart Loop plus root-motion removal is rejected by the
shared validator for compact exports.

The standard skill lives in `skills/aio-asset-normalizer/SKILL.md` with four
workflow references. It discovers the executable, queries capabilities/schema,
inspects assets, prepares jobs, performs dry-run, executes, and verifies results.
It does not implement algorithms or install into a personal directory. CLI
archives carry the skill from the same source checkout as the executable.

## Verification

```bash
cargo test --locked --all-targets
cargo test --locked --no-default-features --features cli --all-targets
cargo check --locked --no-default-features --features desktop --bin aio-asset-normalizer
uv run scripts/verify-headless.py
cargo test --locked --no-default-features --features cli --test converter -- --include-ignored
```

The last command requires Blender 5.1.2; normal tests do not. CI runs headless
builds and tests plus desktop checks on Windows, macOS, and Linux. A separate
Linux job pins Blender 5.1.2 for real conversion comparison.

`tests/agent_workflows.rs` invokes the actual CLI and the same pure request
adapters used by the GUI. It compares resolved requests, GLB bytes/reports, BVH
trim bytes, BVH and edited-GLB retarget outputs, and shared validation errors.
It also covers capability/schema discovery, job conflicts, unreadable files,
source protection, invalid mapping, unknown extension preservation or safe
rejection, and recursive output exclusion. Additional regressions cover malformed binary
ranges/non-finite values and preservation of completed batch outputs on I/O failure. `tests/converter.rs` covers failed
validation, commit races, concurrent staging, batch collisions, dependency-only
dry-run, and opt-in real Blender output equivalence. Existing domain tests cover
malformed assets and edit/export resource invariants.

Byte equivalence is required within the same platform/runtime and with complete
matching parameters. Cross-platform floating-point/Blender version differences
are not treated as interchangeable runtime environments. Dry-run retarget
computes and validates output in memory; Converter dry-run checks dependencies,
inputs and paths only and never invokes Blender.

## Local acceptance results

On macOS arm64, the default-feature suite passed 164 tests, and the CLI-only
suite passed 134 tests. The opt-in Blender 5.1.2 suite passed all six Converter
tests, including byte comparison and conversion followed by inspect/dry-run/export.
Desktop-only compilation, skill-creator format validation, capability/schema
queries, and the headless dependency check passed. A CLI tar archive was extracted
and its executable/reference/skill resources verified; ZIP resource layout was
also checked. Windows and Linux execution is configured in `verify.yml` and was
not run locally. The official Blender download was not accessible from this local
network; the pinned Linux download is verified when that CI job runs.
