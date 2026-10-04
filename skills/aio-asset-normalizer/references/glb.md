# GLB processing

Start with `glb inspect <GLB>...`. Read `results.files` for the resource catalog and
Skin hierarchies. Indices are local to each asset; exact names may be ambiguous.
Inspect the generated output again after successful processing.

## Current-file edits

Query `schema --command glb.edit`. An identity export needs only:

```json
{
  "input": "/absolute/assets/hero.glb",
  "output": "/absolute/output/hero.glb"
}
```

Run `glb edit --job <job.json> --dry-run`, then the same command without dry-run.
An omitted `edits` object and an empty object both mean identity edits.

For a selected character animation and texture replacement, adapt the IDs after
inspection, and query the schema for available texture slots:

```json
{
  "input": "/absolute/assets/hero.glb",
  "output": "/absolute/output/hero.glb",
  "edits": {
    "textures": [
      {"mesh": 0, "primitive": 0, "slot": "base-color",
       "image": "/absolute/textures/body.png", "duplicate_shared_material": true}
    ],
    "rotate_roots_degrees": [0, 90, 0],
    "scale_roots": 1,
    "translate_roots": [0, 0, 0],
    "bake_root_transform": true,
    "trim": {"animation": 0, "start": 0, "end": 1},
    "animation_rate": {"animation": 0, "rate": 1.5}
  },
  "selection": {
    "preset": "character", "scene": 0, "skin": 0,
    "nodes": [0], "primitives": {"0": [0]}, "animations": [0],
    "animation_output": "combined"
  }
}
```

Textures are PNG or JPEG. By default a shared material is duplicated so other
primitives keep their textures. Texture replacements execute in list order,
followed by trim, root rotation/scale/translation, animation rate, Smart Loop,
and resource selection. Disabling baking leaves root transforms out of export.

Use `edits.smart_loop = {"animation": 0, "transition_seconds": 0.2}` for a
transition loop. Smart Loop and root-motion removal cannot be combined for compact
exports. Every edited animation must remain selected in a compact export.

`selection` and `export` are mutually exclusive. If no explicit selection is
needed, use `export: {"preset": "character"}` or `"skeleton"`. The default
`preserve-all` retains the complete document. Unknown extensions survive
preserve-all; unsupported reference remapping makes compact exports fail safely.

## Batch standardization

```text
glb export --input-root <INPUT_DIR> --recursive --output-root <OUTPUT_DIR> --preset character --dry-run
```

Query `schema --command glb.export` for a job. It accepts `inputs`, `input_root`,
`output_root`, `overwrite`, and either `recipe` or `selection`. An explicit batch
selection resolves the supplied indices independently in every input: use it only
when those IDs intentionally mean the same resources in all assets.

`character` and `skeleton` support `animation_output: "split"`. Read planned output
paths rather than predicting sanitized animation names or duplicate suffixes.
Preflight rejects source/output collisions and existing files without overwrite.
A recursive scan excludes the output subtree; identical input/output roots are
rejected. On execution failure retain and report `completed_outputs`.
