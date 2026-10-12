# Shared core and agent skill

## Implementation boundaries

`aio_asset_normalizer` is the shared backend crate. `modules::operations`
contains front-end-neutral request contracts, defaults, errors, output planning,
and checked execution for GLB, BVH, retargeting, and Converter workflows. The
GUI and MCP server use the same computations and exporters. Operations do not
depend on UI or viewport state.

The `desktop` feature is enabled by default. The optional `mcp` feature adds
the native stdio service; a headless MCP-only build uses
`--no-default-features --features mcp`. Blender is external and required only
for conversion. The Converter profile is fixed and shared.

Loaded and generated GLBs share binary range, component alignment, sparse-index,
finite-float, and glTF-reference checks. Generated GLBs are serialized and
re-parsed before success. File commits use unique same-directory staging and
no-clobber persistence unless overwrite was requested. Source identity checks
resolve symlinks and parent components. Failures preserve previous destination
files. Multi-output execution reports completed files after a later failure.

The GUI and MCP use immutable operation inputs. Background jobs report progress
through their respective UI or MCP job interfaces. MCP file tools do not start
the GUI; session tools address the GUI explicitly.

## Oversized-file design review

The GLB coordinator once exceeded 1,000 lines. Document loading, serialization,
and atomic export were extracted into `document_io.rs`; selection, filename,
operation, and cross-domain path/error responsibilities are split into focused
modules. Existing large retarget and algorithm files retain their established
boundaries. Further growth of an oversized algorithm file should extract an
independently testable responsibility first.

## Agent skill

The provider-neutral skill lives in `skills/aio-asset-normalizer/SKILL.md`
with GLB, BVH, retarget, and Converter references. It uses MCP schemas and typed
tools, absolute paths, preflight where supported, job polling, and result
inspection. It does not implement asset algorithms or install into a personal
directory.

## Local verification

Run these checks locally when validating source changes:

```bash
cargo fmt --check
cargo test --locked --all-targets
cargo check --locked --no-default-features --features desktop --bin aio-asset-normalizer
cargo check --locked --no-default-features --features mcp --bin aio-asset-normalizer-mcp
python scripts/verify-headless.py
```

The opt-in Blender integration test requires the fixed Blender version
documented in the Converter guide. CI only builds and publishes desktop
packages from the `publish` branch.
