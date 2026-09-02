# Generic BVH and GLB Retargeting

The retargeting workflow accepts a BVH or an animated GLB source and a
Skinned GLB target. Both paths use the versioned Mapping v2 contract in
`src/modules/retarget.rs`. A mapping identifies every node by its display
name, complete hierarchy path, and node index. GLB endpoints also identify the
selected Skin. Skeleton fingerprints are authoritative; a file fingerprint
change is reported as a warning when the skeleton fingerprint still matches.

Mapping v1 files remain readable for BVH Studio. They are converted to the
internal v2 representation only when names are unique, and new exports use v2.
Name matching is advisory and never replaces an explicit mapping. Animated
source nodes must be mapped or listed in `ignored_sources`.

## Agent handoff

BVH Studio and the GLB Editor can export a deterministic provider-neutral
Markdown prompt. Prompt construction and atomic writing run in the same
background task queue as retarget exports. The prompt contains source and target identities, selected
Skins, node paths, hierarchy, local and world rest transforms, parent
distances, animation channel metadata, and name-match candidates. BVH prompts
include hierarchy offsets and channel order but not motion frames. Asset text is
quoted as untrusted data and the prompt asks an external agent to return one
Mapping v2 JSON object only. The application performs the final schema,
identity, hierarchy, Skin, duplicate, and finite-number checks after import.

## Retargeting math

The source rest pose is authored BVH `OFFSET` plus zero rotations, never the
first motion frame. For each mapped bone the world-space delta is

```
source_pose_world * inverse(source_rest_world)
```

After coordinate-basis and unit conversion, the delta is applied to the target
calibrated rest world rotation. Target local rotations are reconstructed in
hierarchy order using the already-retargeted dynamic parent world rotation.
Root Motion is optional, has an explicit scale, and is rebased to the selected
clip's first frame. Initial heading normalization is optional.
When the GLB Editor has pending root orientation, scale, or translation
controls, those controls are applied to sampled source poses so animated TRS
channels are not shadowed by a static root matrix.

GLB sources use the currently selected animation and accept only STEP or LINEAR
TRS channels. CUBICSPLINE, Morph Target channels, and clips with fewer than two
distinct samples are rejected. Generated retarget packages replace the target
animation list with one new clip while preserving meshes, Skins, inverse bind
matrices, materials, textures, extras, and unknown extensions. Ordinary GLB
editor exports are unaffected.

## Design review

The shared domain implementation currently lives in `src/modules/retarget.rs`
because Mapping v2 validation, rest-pose math, BVH/GLB adapters, and prompt
serialization must share the same resolved node references and error contract.
This file is intentionally above the repository's 1,000-line review threshold.
The stable boundaries are `SkeletonDescriptor`/Mapping validation,
`retarget_frames`/coordinate math, and prompt generation; future work should
move those boundaries into `retarget_mapping.rs`, `retarget_core.rs`, and
`retarget_prompt.rs` before adding new retargeting formats. BVH and application
coordination stay in their existing modules, while GLB document writes remain
owned by `src/modules/glb`.

The application coordinator remains below the review threshold after the BVH
workflow was split into `src/app_bvh.rs`. GLB retargeting operations live in
`src/app_retarget.rs`, prompt workers in `src/app_retarget_prompt.rs`, and the
coordinator owns page routing, task polling, reload ordering, and shared state.
Further UI or worker behavior should extend the focused module instead of
adding another responsibility to the coordinator.

The existing GLB document and runtime modules are now also above 1,000 lines
because they retain the raw JSON/BIN writer and CPU animation sampler as their
respective sources of truth. Skin indexing/inverse-bind validation belongs to
the document module, while animation curve validation and sampling belong to
the runtime module. Future GLB resource editors should move those boundaries
into dedicated files before adding another large feature there.

## Manual smoke test

1. Open BVH Studio, import a `.bvh`, choose a target `.glb`, and select its
   Skin. Confirm the orange source skeleton and cyan target Skin overlay.
2. Import or export Mapping v2 and verify that a duplicate name is rejected
   unless its path and index identify exactly one node.
3. Export the Agent prompt, obtain a Mapping v2 from an external agent, import
   it, and inspect the validation report before exporting.
4. Toggle Root Motion and Initial Heading normalization and scrub the preview.
5. In the GLB Editor, choose a source animation and a target GLB, import the
   same mapping, preview the retargeted animation, then export to a new path.
6. Re-open the exported GLB and verify one animation, valid Skin references,
   unchanged mesh bounds, and preserved extensions/extras.

For filtered diagnostics, run:

```bash
cargo run
rg "\[(retarget|retarget_agent|bvh_studio|glb_retarget)\]" debug.log > retarget-debug.log
```
