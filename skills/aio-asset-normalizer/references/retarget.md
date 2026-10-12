# Retargeting with explicit Mapping v2

The `retarget` tool supports a BVH source or one animation from an animated
GLB and a Skinned GLB target. Inspect both assets before choosing animation and
Skin indices. Obtain the tool schema with `tools/list` before constructing
requests.

## Prepare and validate

Use `action: "prompt"` to generate a skeleton-mapping prompt. It contains
skeleton metadata, not raw motion frames. Preserve endpoint fingerprints, axes,
units, Skin references, and authored offsets when writing a Mapping v2 object.
Use actual node indices and hierarchy paths. Do not fabricate missing roots,
reuse target bones, or silently replace explicit mappings with name suggestions.

Map every animated source node or list it in `ignored_sources`. Resolve
semantic and hierarchy ambiguity before exporting. Ask for missing axes or
units when they affect motion transfer. For GLB sources, set
`source_skin`, `source_animation`, and `source_unit: "m"` explicitly. For
BVH, source edits support trim and root transforms; GLB textures and animation
edits are not supported for BVH sources.

Use `action: "suggest"` only for BVH name candidates. Use
`action: "validate"` with the concrete Mapping before running. Read validity,
errors, warnings, mapped count, and unmapped source nodes. Correct only issues
supported by the supplied skeletons.

## Run and verify

Set `action: "run"`, provide a new absolute `out` path, and retain the same
source settings and mapping used for validation. Set `clip_name`,
`sample_rate`, `root_motion`, `normalize_heading`, and optional
`reduce_keys` as needed. Choose the character or skeleton preset; an explicit
`selection` overrides the preset and supports one combined output clip.

Retarget replaces the target animation list with the generated clip. Source,
target, and mapping files are protected. Inspect the output GLB and report
structural validation separately from visual playback quality.
