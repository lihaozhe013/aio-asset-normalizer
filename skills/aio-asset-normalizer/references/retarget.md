# Retargeting with explicit Mapping v2

Retarget supports a BVH source or one animation from an animated GLB, and a Skinned
GLB target. Inspect both assets before choosing source animation and Skin indices.
Read the selected executable's `docs --raw` for the Mapping v2 contract.

## Prepare the same source throughout

Query `schema --command retarget.run`. Prompt, validate, and run share the following
job fields. Omit `command` when reusing source settings between those commands:

```json
{
  "source": "/absolute/motion/walk.bvh",
  "target": "/absolute/assets/hero.glb",
  "source_up_axis": "Y", "source_forward_axis": "-Z", "source_unit": "cm",
  "target_up_axis": "Y", "target_forward_axis": "-Z", "target_unit": "m",
  "skin": 0,
  "out": "/absolute/work/prompt.md"
}
```

The coordinates above are examples: confirm them for the actual motion source.
For GLB, set `source_skin`, `source_animation`, and `source_unit: "m"` explicitly.
`source_edits` supports GLB textures, trim, animation rate, Smart Loop, and root
transforms. Root transforms are transferred through retarget options without
baking animated source nodes. For BVH, source edits support trim (animation 0) and
root transforms only. Use identical source settings for prompt, validate, and run.

## Author and validate

1. Run `retarget prompt --job <prompt-job.json>` and read `results.prompt` or the
   generated Markdown file. It contains skeleton metadata, never raw motion frames.
2. Write one explicit Mapping v2 JSON object. Preserve the supplied endpoint
   fingerprints, axes, units, Skin references, and authored offsets. Use actual
   node indices and hierarchy paths. Do not fabricate missing roots, reuse target
   bones, or silently substitute name-match suggestions for explicit mappings.
3. Map every animated source node or list it in `ignored_sources`. Resolve semantic
   and hierarchy ambiguity before exporting; ask for clarification when metadata
   cannot resolve it. The agent may author an unambiguous mapping as part of the
   requested asset-processing task without requiring a separate approval ritual.
4. Set `mapping` in the job and run `retarget validate --job <job.json>`. Read
   `valid`, errors, warnings, mapped count, and unmapped source nodes. Correct only
   issues supported by the concrete skeletons, then validate again. If correction
   requires guessing bone roles or metadata, stop and request that information.

`retarget suggest` supplies BVH name candidates only. Legacy BVH mappings remain
accepted through the core's explicit compatibility conversion; new mappings use v2.

## Bake and verify

Change `out` to a new `.glb` destination, retain the same source settings and
mapping, and set `clip_name`, `sample_rate` (default 60), `root_motion`,
`normalize_heading`, and optional `reduce_keys` as needed.

Run `retarget run --job <job.json> --dry-run`, then without dry-run. Dry-run performs
retarget computation and output serialization/validation without asset writes.
Use `preset: "character"` or `"skeleton"`; an explicit `selection` overrides the
preset and supports one combined output clip.

Retarget replaces the target animation list with the generated clip. Character
resource selection otherwise follows the shared exporter and its extension safety
checks. Source, target, and mapping files are protected. Inspect the result and
report structural validation separately from any visual playback assessment.
