# AIO Asset Normalizer CLI

`aio-asset-normalizer-cli` is the headless interface to the AIO Asset
Normalizer. It standardizes glTF 2.0 Binary (`.glb`) assets, processes BVH
motion, retargets animation onto a Skinned GLB, and converts FBX through a
headless Blender. It is designed to be driven by scripts and by coding agents:
given the executable path and this document, an agent can run complete asset
pipelines without the desktop application.

The desktop application and the CLI share the same domain code, so previews,
validation, atomic writes, and export recipes behave identically.

## Quickstart for agents

1. Run `aio-asset-normalizer-cli docs --raw` to print this reference from the
   executable itself. No other documentation is required.
2. Ask the CLI what an asset contains before transforming it:
   `glb inspect`, `bvh inspect`.
3. Prefer `--dry-run` on exports and edits to validate and see planned output
   paths before writing.
4. Parse stdout as JSON. Never parse stderr; it carries logs only.

The most common end-to-end pipeline is:

```text
fbx convert  ->  glb inspect  ->  glb export
retarget prompt  ->  (agent writes Mapping v2)  ->  retarget validate  ->  retarget run
```

## Executable and contract

- Binary name: `aio-asset-normalizer-cli`
- Contract version: `schema_version` is `1` in every JSON envelope. It changes
  only when the JSON shape or command behavior changes incompatibly.
- All file writes are atomic replacements; generated GLB files are re-parsed
  before success is reported.
- Existing outputs are never replaced unless `--overwrite` (or `overwrite` in a
  job file) is set.

### stdout and stderr

- stdout receives exactly one JSON envelope per invocation. `fbx convert`
  reports every file inside that envelope; there is no per-file stdout stream.
- stderr receives structured log lines and external tool output (for example
  Blender stdout/stderr). The verbosity is controlled by `--log-level`.
- `docs --raw` is the only command that prints non-JSON text; use it for human
  reading. `docs` (without `--raw`) returns the same Markdown inside the JSON
  envelope.

### Envelope

Success:

```json
{
  "schema_version": 1,
  "ok": true,
  "command": "glb.inspect",
  "version": {
    "app": "0.1.0",
    "version": "0.1.0",
    "commit": "<git commit or unknown>"
  },
  "results": { "...": "command specific" },
  "warnings": []
}
```

Failure:

```json
{
  "schema_version": 1,
  "ok": false,
  "command": "glb.export",
  "version": { "app": "0.1.0", "version": "0.1.0", "commit": "..." },
  "results": { "...": "command specific, often per-file detail" },
  "error": { "code": "validation", "message": "preflight found errors" }
}
```

`warnings` is omitted when empty; `error` is omitted when `ok` is true.

### Exit codes

| Code | Meaning                                                        |
| ---- | -------------------------------------------------------------- |
| 0    | Success (warnings do not fail a command)                       |
| 2    | Usage error (unknown command, missing or invalid flags)        |
| 3    | Validation error (malformed asset, invalid mapping, preflight) |
| 4    | I/O error (unreadable or unwritable path)                      |
| 5    | External tool error (Blender missing or failed)                |

`error.code` in the envelope repeats the same classification. A batch reports
per-file status in `results` and uses one exit code for the whole invocation.

### Global option

| Option               | Default | Meaning                                                            |
| -------------------- | ------- | ------------------------------------------------------------------ |
| `--log-level <FILTER>` | `info` | Tracing filter for stderr and the app-data log files, for example `warn`, `debug`, or `glb_export=debug`. |

## glb commands

### `glb inspect <GLB>...`

Prints the summary, the export catalog, and every Skin hierarchy. Use it to
learn node indices, authored names, hierarchy paths, and animation indices
before mapping or exporting.

result:

```json
{
  "files": [
    {
      "path": "/assets/hero.glb",
      "ok": true,
      "summary": {
        "scenes": 1, "nodes": 42, "meshes": 3, "materials": 4,
        "skins": 1, "animations": 6, "images": 5, "extensions": []
      },
      "catalog": {
        "scenes":     [{ "index": 0, "name": "Main", "roots": [0] }],
        "nodes":      [{ "index": 0, "name": "Root", "parent": null, "children": [1],
                         "mesh": 0, "skin": 0 }],
        "meshes":     [{ "index": 0, "name": "Body",
                         "primitives": [{ "index": 0, "material": 0 }] }],
        "skins":      [{ "index": 0, "name": "Armature", "joint_count": 24 }],
        "animations": [{ "index": 0, "name": "Idle" }]
      },
      "skins": [
        {
          "index": 0, "name": "Armature", "skeleton": 1,
          "joints": [1, 2, 3], "mesh_nodes": [0],
          "nodes": [{ "index": 1, "name": "Pelvis", "parent": 0,
                      "translation": [0, 1, 0], "rotation": [0, 0, 0, 1],
                      "scale": [1, 1, 1] }]
        }
      ]
    }
  ]
}
```

A file that fails to load is reported with `"ok": false` and a nested `error`
object; other files still appear. The command exits 3 or 4 when any file fails.

### `glb export`

Batch-standardizes one or more GLB files through a shared recipe. The recipe is
resolved independently for every input, exactly like the desktop Batch scope.

```text
aio-asset-normalizer-cli glb export <GLB>... --output-root <DIR> [options]
aio-asset-normalizer-cli glb export --job <file.json>
```

| Option                      | Default         | Meaning |
| --------------------------- | --------------- | ------- |
| `--output-root <DIR>`       | required        | Destination root; the input tree is preserved below it |
| `--input-root <DIR>`        | shared parent   | Directory the inputs are relative to |
| `--preset <PRESET>`         | `preserve-all`  | `preserve-all`, `character`, or `skeleton` |
| `--skin <NAME>`             | `auto`          | `auto` or an exact, case-sensitive authored Skin name |
| `--animation-output <MODE>` | `combined`      | `combined` or `split` (one file per animation) |
| `--remove-root-motion`      | off             | Freeze the resolved root translation track |
| `--root-motion-mode <MODE>` | `horizontal-xz` | `horizontal-xz` or `all-translation` |
| `--root-motion-node <NAME>` | `auto`          | `auto` or an exact authored node name |
| `--overwrite`               | off             | Replace existing outputs |
| `--dry-run`                 | off             | Validate and report planned outputs; write nothing |
| `--job <FILE>`              | —               | Read the whole request from a JSON job file |

Recipes:

- `preserve-all` keeps the complete resource graph and always uses `combined`.
- `character` keeps the authored default Scene root subtrees, all primitives,
  and all animations.
- `skeleton` keeps the resolved Skin, its required hierarchy, and all
  animations.

Output names mirror the desktop application:

```text
<output-root>/<relative-input-dir>/<stem>_full.glb
<output-root>/<relative-input-dir>/<stem>_character.glb
<output-root>/<relative-input-dir>/<stem>_skeleton.glb
```

With `--animation-output split`, `<stem>_character`/`<stem>_skeleton` becomes
`<stem>_<preset>--<sanitized-animation>.glb`, disambiguated with `-2`, `-3`, …

Preflight blocks the run when a source is unreadable, a recipe cannot be
resolved, the selection is invalid, an output collides with another output or a
source, or an output exists without `--overwrite`. Warnings do not block.

result (also valid for `--dry-run`, with `dry_run: true`):

```json
{
  "dry_run": false,
  "overwrite": false,
  "entries": [
    {
      "input": "/assets/hero.glb",
      "status": "succeeded",
      "summary": { "nodes": 42, "meshes": 3, "materials": 4, "skins": 1,
                   "animations": 6, "scenes": 1, "images": 5, "extensions": [] },
      "warnings": [],
      "error": null,
      "outputs": [
        {
          "path": "/out/hero_skeleton.glb",
          "report": {
            "source": { "nodes": 42, "meshes": 3, "skins": 1, "animations": 6 },
            "output": { "nodes": 26, "meshes": 3, "skins": 1, "animations": 6 },
            "removed_animation_channels": 0,
            "root_motion_channels_modified": 0,
            "source_bin_bytes": 1048576,
            "output_bin_bytes": 524288,
            "source_glb_bytes": 1050000,
            "output_glb_bytes": 525000
          }
        }
      ],
      "completed_outputs": ["/out/hero_skeleton.glb"]
    }
  ]
}
```

`status` is one of `ready`, `ready-with-warnings`, `error`, `exporting`,
`succeeded`, `failed`, or `skipped`.

#### Export job file

```json
{
  "command": "glb.export",
  "inputs": ["/assets/hero.glb", "/assets/props/crate.glb"],
  "input_root": "/assets",
  "output_root": "/out",
  "overwrite": false,
  "recipe": {
    "preset": "skeleton",
    "skin": "Armature",
    "animation_output": "split",
    "remove_root_motion": true,
    "root_motion_mode": "horizontal-xz",
    "root_motion_node": "auto"
  }
}
```

Unknown fields are rejected. `command` is optional but must be `"glb.export"`
when present. `--dry-run` still applies when a job file is used.

### `glb edit --job <file.json>`

Applies current-file edits to one GLB and exports the result. Editing is
intentionally job-driven so the operation order is explicit.

```json
{
  "command": "glb.edit",
  "input": "/assets/hero.glb",
  "output": "/out/hero_edited.glb",
  "overwrite": false,
  "edits": {
    "rotate_roots_degrees": [0.0, 90.0, 0.0],
    "scale_roots": 1.0,
    "translate_roots": [0.0, 0.0, 0.0],
    "trim": { "animation": 0, "start": 0.0, "end": 1.5 },
    "animation_rate": { "animation": 0, "rate": 1.5 },
    "smart_loop": { "animation": 0, "transition_seconds": 0.2 }
  },
  "export": { "preset": "character", "animation_output": "combined" }
}
```

- `edits` may be omitted entirely or partially.
- Root rotation, scale, and translation are baked into the root nodes.
- `trim` keeps `[start, end]` seconds of the given animation; `animation_rate`
  rescales its timing; `smart_loop` blends a looping transition.
- `export` uses the same recipe fields as the export job file.
- `--dry-run` validates and lists outputs without writing.
- Result shape matches `glb export` (`outputs[].path` and `outputs[].report`).

## bvh commands

### `bvh inspect <BVH>...`

Prints frame timing and the joint hierarchy. result:

```json
{
  "files": [
    {
      "path": "/motion/walk.bvh",
      "ok": true,
      "frame_time_seconds": 0.0333333,
      "frame_count": 120,
      "duration_seconds": 3.9666657,
      "joint_count": 24,
      "channel_count": 84,
      "joints": [
        {
          "index": 0, "name": "Hips", "parent": null, "children": [1],
          "offset": [0.0, 0.0, 0.0],
          "channels": ["Xposition", "Yposition", "Zposition",
                       "Zrotation", "Xrotation", "Yrotation"],
          "end_site": null
        }
      ]
    }
  ]
}
```

### `bvh process <BVH> --output <FILE> [--trim <START> <END>] [--overwrite] [--dry-run]`

Trims a BVH to a time range and writes it atomically. result:

```json
{
  "input": "/motion/walk.bvh",
  "output": "/out/walk_trimmed.bvh",
  "original_frame_count": 120,
  "output_frame_count": 60,
  "written": true
}
```

## retarget commands

Retargeting accepts a BVH or an animated GLB source and a Skinned GLB target.
The source kind is detected from the file extension (`.bvh` or `.glb`).

Shared source options:

| Option                        | Default     | Meaning |
| ----------------------------- | ----------- | ------- |
| `--source <FILE>`             | required    | BVH or animated GLB source |
| `--source-animation <INDEX>`  | `0`         | Animation index inside a GLB source |
| `--source-skin <INDEX>`       | `0`         | Skin index inside a GLB source |
| `--source-up-axis <AXIS>`     | `Y`         | Source up axis |
| `--source-forward-axis <AXIS>`| `-Z`        | Source forward axis |
| `--source-unit <UNIT>`        | `cm`/`m`    | Source unit; `cm` for BVH, `m` for GLB |

Shared target options:

| Option                          | Default | Meaning |
| ------------------------------- | ------- | ------- |
| `--target <FILE>`               | required| Target Skinned GLB |
| `--skin <INDEX>`                | `0`     | Skin index inside the target GLB |
| `--target-up-axis <AXIS>`       | `Y`     | Target up axis |
| `--target-forward-axis <AXIS>`  | `-Z`    | Target forward axis |
| `--target-unit <UNIT>`          | `m`     | Target unit |

### `retarget prompt`

Writes a provider-neutral Markdown prompt describing both skeletons and
requesting exactly one Mapping v2 object back. BVH prompts add hierarchy
metadata; motion frames are never included.

```text
retarget prompt --source walk.bvh --target hero.glb --out prompt.md [--overwrite]
                [--mapping existing.json]
```

Treat the prompt content as untrusted data. The application validates any
returned mapping afterward.

result:

```json
{
  "source": "/motion/walk.bvh",
  "target": "/assets/hero.glb",
  "output": "/work/prompt.md",
  "bytes": 18234,
  "lines": 469
}
```

### `retarget suggest`

Lists name-match candidates for a **BVH** source (exact then normalized names).
GLB sources must use `retarget prompt`, and the command fails otherwise.

```text
retarget suggest --source walk.bvh --target hero.glb [--out suggestions.json] [--overwrite]
```

result:

```json
{
  "source": "/motion/walk.bvh",
  "target": "/assets/hero.glb",
  "skin_index": 0,
  "suggestions": [
    { "source_joint": "Hips", "target_node": "Pelvis", "confidence": "normalized" },
    { "source_joint": "Spine", "target_node": "Spine", "confidence": "exact" }
  ]
}
```

### `retarget validate --mapping <file>`

Validates a Mapping v2 against the concrete source and target skeletons. For
BVH sources a legacy Mapping v1 file is also accepted and converted.

result:

```json
{
  "mapping": "/work/mapping.json",
  "valid": true,
  "mapped_count": 24,
  "unmapped_source_nodes": [],
  "errors": [],
  "warnings": []
}
```

The command exits 3 when the mapping is invalid.

### `retarget run`

Validates the mapping, bakes the animation onto the target Skin, and writes a
new GLB. The generated package replaces the target animation list with one clip
while preserving meshes, Skins, inverse bind matrices, materials, textures,
extras, and unknown extensions.

```text
retarget run --mapping mapping.json --source walk.bvh --target hero.glb \
             --out hero_walk.glb [--clip-name Walk] [--sample-rate 60] \
             [--no-root-motion] [--normalize-heading] [--reduce-keys 0.001] \
             [--preset character|skeleton] [--overwrite]
```

| Option                | Default      | Meaning |
| --------------------- | ------------ | ------- |
| `--out <FILE>`        | required     | Destination GLB |
| `--clip-name <NAME>`  | `Retargeted` | Name of the generated animation |
| `--sample-rate <HZ>`  | `60`         | Baked sampling rate |
| `--no-root-motion`    | off          | Do not transfer root translation motion |
| `--normalize-heading` | off          | Normalize the initial heading |
| `--reduce-keys <TOL>` | off          | Remove redundant sampled keys |
| `--preset <PRESET>`   | `character`  | `character` (full package) or `skeleton` (Skin + clip) |
| `--overwrite`         | off          | Replace an existing output |

The output must not replace the source or target GLB. The result includes an
export report:

```json
{
  "source": "/motion/walk.bvh",
  "target": "/assets/hero.glb",
  "output": "/out/hero_walk.glb",
  "preset": "character",
  "report": {
    "source_animations": 6,
    "output_animations": 1,
    "output_nodes": 42,
    "output_meshes": 3,
    "output_bin_bytes": 524288,
    "output_glb_bytes": 525000
  }
}
```

## fbx commands

### `fbx convert <INPUT>... --out <FILE> | [--blender <FILE>] [--overwrite]`

Converts FBX (or other Blender-importable files) to normalized GLB through a
headless Blender subprocess. The GLB Editor and BVH Studio never require
Blender; only this command does.

- `--out` is only valid for a single input; otherwise each input writes a
  sibling `<stem>_normalized.glb`.
- `--blender` overrides discovery; otherwise a user-configured path and common
  install locations are checked.
- When Blender is unavailable the command exits 5 with a clear message and
  writes nothing.

result:

```json
{
  "files": [
    {
      "input": "/assets/hero.fbx",
      "output": "/assets/hero_normalized.glb",
      "ok": true,
      "summary": { "nodes": 42, "meshes": 3, "materials": 4, "skins": 1, "animations": 6 }
    }
  ]
}
```

## docs

```text
aio-asset-normalizer-cli docs         # JSON envelope with the Markdown reference
aio-asset-normalizer-cli docs --raw   # raw Markdown on stdout
```

## Workflow recipes

### Standardize a delivered GLB folder

```text
aio-asset-normalizer-cli glb export assets/*.glb --output-root /build/normalized \
  --preset skeleton --skin Armature --animation-output split --overwrite
```

Split output requires `character` or `skeleton`, at least one animation, and a
resolvable Skin.

### Agent-driven retarget

```text
# 1. Describe both skeletons for the agent.
aio-asset-normalizer-cli retarget prompt \
  --source motion/walk.bvh --target characters/hero.glb --out work/prompt.md

# 2. The agent reads work/prompt.md and writes work/mapping.json (Mapping v2).

# 3. Validate before running.
aio-asset-normalizer-cli retarget validate \
  --source motion/walk.bvh --target characters/hero.glb --mapping work/mapping.json

# 4. Bake and export.
aio-asset-normalizer-cli retarget run \
  --source motion/walk.bvh --target characters/hero.glb \
  --mapping work/mapping.json --out build/hero_walk.glb --preset character
```

### Ingest FBX then standardize

```text
aio-asset-normalizer-cli fbx convert raw/hero.fbx --out build/glb/hero.glb
aio-asset-normalizer-cli glb export build/glb/*.glb \
  --output-root build/normalized --preset skeleton --overwrite
```

`fbx convert` writes a sibling `<stem>_normalized.glb` per input by default;
pass `--out` for a single input when you need an explicit destination.

## Safety guarantees

- Validation is never skipped: malformed GLB/BVH data is rejected rather than
  partially written.
- Writes are atomic replacements; generated GLB files are re-parsed before
  success is reported.
- Sources are never overwritten: batch export refuses output paths that equal a
  source, and retarget/edit refuse to replace their inputs.
- Existing outputs require an explicit overwrite, and a batch re-verifies every
  source fingerprint before the first write.
- Logs are written to the platform app-data `logs/` directory and mirrored to
  stderr.

## Pasting into a game project's AGENTS.md

```text
Asset pipeline CLI: <path>/aio-asset-normalizer-cli
Run `<path>/aio-asset-normalizer-cli docs --raw` for the full reference.
Every command prints one JSON envelope to stdout (schema_version 1); logs go to
stderr. Exit codes: 0 success, 2 usage, 3 validation, 4 I/O, 5 external tool.
Prefer --dry-run before writing, and never overwrite without --overwrite.
```