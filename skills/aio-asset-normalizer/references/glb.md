# GLB processing

Call `glb_inspect` with an absolute `input` path first. Read the returned
summary, export catalog, and Skin hierarchies before choosing resource indices.
Inspect the generated output again after a successful edit.

## Current-file edits

Call `glb_edit` with a typed `job` object. Its `input` and `output` paths
must be absolute. An omitted or empty `edits` object means identity edits.
Include `selection` or `export` only when you need to restrict the exported
resources; the fields are mutually exclusive.

For texture replacement, choose mesh, primitive, and texture slot indices from
the inspection result, and provide an absolute PNG or JPEG path. Shared
materials are duplicated by default so other primitives keep their textures.
Edits apply before resource selection.

Smart Loop edits cannot be combined with root-motion removal in compact exports.
Every edited animation must remain selected. Preserve-all retains the full
document and unknown extensions; compact export fails safely when extension
references cannot be remapped.

## Batch standardization

Call `glb_batch` with absolute `input_root`, `inputs`, and `output_root`
paths. Start with `action: "preflight"`, then use `action: "export"` after
reviewing the planned files, warnings, and conflicts. Provide either a
`recipe` or an explicit `selection`, not both.

An explicit batch selection resolves the same indices independently in every
input. Use it only when those indices intentionally represent the same resources
across all assets. Character and skeleton recipes support split animation
output. Read the returned output paths instead of predicting sanitized names.
Preflight rejects source/output collisions and existing files unless overwrite
was authorized. Recursive scans exclude the output subtree. On execution failure,
report completed outputs and remaining files.
