# GLB export selection

The GLB Editor exposes an Export Selection panel for producing smaller runtime
assets without changing the source document. Selection state belongs to the
current document session and is reset when a different document is loaded.

## Presets

- **Preserve All** keeps the existing GLB resource graph and remains the
  default for ordinary GLB export.
- **Character Package** writes one selected Scene containing selected node
  subtrees, selected Mesh primitives, one Skin, and selected animations. A
  Character Package with no selected animations is a model-plus-skeleton GLB.
- **Skeleton Animation** writes a meshless GLB containing the selected Skin,
  its joints and required ancestors, animation target nodes, inverse bind
  matrices, and selected animations. A GLB animation package still needs a
  node hierarchy; it is not an animations-array-only file.

Node selection is subtree-based. Primitive selection is the smallest mesh
selection unit. Mesh primitive choices are global for a shared Mesh, so one
Mesh instance cannot use a different primitive subset from another instance in
the same export.

Only one Scene and one Skin are written. Skin joints, the optional skeleton
root, animation targets, and their ancestors are retained automatically.
Unselected render resources, cameras, punctual lights, and unreachable BIN
data are removed. JSON resources are reindexed and selected bufferViews are
packed on four-byte boundaries.

## Animation output

Combined output stores all selected animations in one compact GLB. Split output
creates one GLB per selected animation using a sanitized animation name and a
numeric suffix when names collide. Split output is available for compact
presets; Preserve All always produces one complete document.

Existing orientation, root transform, trim, animation-rate, and Smart LOOP
settings are applied to an export clone before resource pruning. Background
export reports resource counts, BIN sizes, and serialized GLB sizes, and a
failure after one or more files have been written reports the completed paths
separately.

## Safety boundaries

Every compact export is serialized and re-read through the project GLB reader,
`gltf::Gltf`, and the skeleton/animation runtime before it is written. The
source file is never overwritten by the export flow. External buffers and
external image URIs are rejected for compact output because their references
cannot be safely repacked in a self-contained GLB. Unknown extensions that may
contain index references are rejected instead of being copied into a damaged
file. Common texture, material, Draco, and material-variant extensions are
preserved when their references can be remapped.

Skeleton Animation rejects Morph Target (`weights`) channels because their
meaning depends on a mesh. CUBICSPLINE and other resources that are not edited
by the application remain available to the GLB parser, but are not expanded
into the editor's preview controls.

BVH Studio uses Character Package semantics for retargeted model exports and
Skeleton Animation semantics for Animation Clip exports. GLB-to-GLB retargeting
uses the target GLB's independent selection state. FBX Converter keeps its
existing Blender-backed conversion path and is outside this compaction flow.

The resource graph compiler lives in a dedicated GLB export module. Its scope
is intentionally limited to cataloging selections, validating references, and
compacting reachable GLTF resources; UI presentation and background task
coordination remain in their existing layers. A line-count review is recorded
here because this cohesive compiler is larger than 1,000 lines: splitting it
further would separate tightly coupled dependency collection, remapping, and
BIN packing without reducing the resource-graph responsibility. Future
features must add new focused modules rather than expanding this compiler with
UI, preview, or task-coordination responsibilities.
