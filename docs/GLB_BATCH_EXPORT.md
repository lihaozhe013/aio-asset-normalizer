# GLB batch export

The GLB Editor has two independent scopes:

- **Current file** previews and edits one `GlbDocument`. Its scene, node,
  primitive, animation, and Skin selections remain index-based and belong to
  that document only.
- **Batch** processes the checked files from the file tree. Batch intent is
  represented by a semantic recipe and is resolved independently for every
  source GLB.

Clicking a file name changes the current preview. Checking a file adds it to
the batch queue; those actions do not implicitly change one another. The
Inspector switches to Batch when a second file is checked, while still
allowing the user to return to the Current file scope.

## Batch recipe

The first batch implementation supports Preserve All, Character Package, and
Skeleton Animation. Character Package uses the authored default Scene's root
subtrees and all primitives. Skeleton Animation keeps the resolved Skin,
required hierarchy, and all animations. Preserve All keeps the original
resource graph and always uses Combined output.

Skin resolution is automatic only when there is zero or one Skin (zero is
valid for Character Package). A batch can instead specify an exact, authored,
case-sensitive Skin name; missing or duplicate matches are errors. Root Motion
uses the existing automatic resolver or an exact, authored, case-sensitive
node name. Node and Primitive subsets, animation trimming, animation-rate
changes, Smart LOOP, root transforms, texture replacement, and retargeting are
current-file operations and are not silently applied to a batch.

## Preflight and output safety

Preflight loads every checked source, resolves the recipe, validates the exact
export selection, builds an in-memory export estimate, and records a SHA-256
source fingerprint. Preflight does not create directories or write files. Any
validation error, output collision, output/source collision, or existing
output with overwrite disabled blocks the batch. Warnings remain exportable
and are shown per file; a Remove Root Motion request that changes zero tracks
is an explicit warning.

Outputs preserve the input tree below the selected output directory and use
`<stem>_full.glb`, `<stem>_character.glb`, or `<stem>_skeleton.glb`. Split
animation output appends `--<sanitized-animation>` and disambiguates duplicate
names. Existing outputs require an explicit overwrite setting. Each output is
written with the normal atomic replacement and GLB reparse validation.

The worker verifies all source fingerprints again before the first write. It
then processes files in sorted path order. An unexpected export or I/O failure
stops the remaining files; already completed paths are retained and reported.
Batch settings are session-only and are not written to user preferences.

## Oversized-file design review

`src/app.rs` was already larger than 1,000 lines before batch export. The
review moved GLB lifecycle methods out and keeps the coordinator at 965 lines.
Batch state, recipe resolution, task coordination, and UI live in focused
modules; `App` only owns the controller and delegates polling. GLB-specific
lifecycle methods should remain outside the coordinator when future changes
extend this workflow.
