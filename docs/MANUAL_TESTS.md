# Manual Verification

## GLB Animation Playback

1. Start the application with `cargo run`.
2. Open a conventional animated `.glb` containing node or Skinned Mesh
   animation.
3. Confirm the bottom dock opens on the `Animation` tab, the `GLB Animation`
   timeline is visible below the viewport, and the first playable clip is
   selected while the model is paused at `0.0s`.
4. Press Play in the Animation tab and verify that the model moves in the
   Canvas and loops at the clip duration.
5. Toggle Loop off, play to the end, and confirm playback stops on the final
   pose.
6. Change the animation rate in the right Inspector, press Play, and confirm
   the preview uses the new rate without marking the GLB document dirty. Drag
   the timeline, pause, and use both frame-step buttons.
7. Export with a pending rate, reopen the export, and confirm the selected
   animation duration is shorter at `2.0x` or longer at `0.5x`, while its poses
   remain unchanged. Export again with a different pending rate and confirm the
   current document and preview controls remain available for further edits.
8. Switch to the `Debug Log` tab and confirm log controls are visible while
   animation playback state is preserved. Switch back to `Animation`.
9. Open a GLB with no animations and confirm the Inspector reports
   `Animations: 0`, the authored Rest Pose is visible, and only the `Debug Log`
   tab is shown.
10. Open a static Skinned GLB with a root-node scale (for example,
    `Pete.glb`). Confirm the authored Rest Pose is visible at the expected
    size, Skeleton Display and Fit work, and changing Root Transform does not
    modify the source GLB.
11. Open a GLB with an unsupported CUBICSPLINE or Morph Target clip and confirm
   the clip is listed as unavailable with an explanatory message.
12. Resize the bottom dock and the application window. Confirm the Canvas
    boundary moves with the dock and never renders underneath it.

## Meshless GLB Skeleton Playback

1. Open a GLB with no Meshes, a first-scene node hierarchy, and a supported
   node animation. Confirm the Inspector reports skeleton-only preview and the
   skeleton is visible without a synthetic Mesh.
2. Confirm that the first playable animation is selected and starts playing;
   pause, toggle Loop, change the animation rate, drag the timeline, and use
   both frame-step buttons.
3. Confirm that a GLB with a non-empty Skin displays its Skin joints together
   with the ancestors needed for a continuous hierarchy. A GLB without a Skin
   displays the first scene's node hierarchy instead.
4. Open a meshless GLB with no animations and confirm its Rest Pose remains
   visible. Open one with an unsupported animation and confirm the animation
   is marked unavailable while the Rest Pose remains visible.
5. Use the Skeleton Display controls to switch Octahedral, Stick, and Lines
   modes, toggle joints and End Sites, adjust bone width, hide/show the
   skeleton, and use Fit skeleton. Confirm the camera frames the skeleton.
6. Change root orientation, scale, and translation. Confirm the skeleton
   follows the preview transform and the source GLB remains unchanged until an
   explicit export.
7. Switch between GLB Editor, BVH Studio, and the FBX Converter. Confirm that
   no stale GLB skeleton appears in another page and that existing BVH source
   and target overlays still work.
8. Open a conventional skinned Mesh GLB and confirm the GLB skeleton overlay
   is also visible in the GLB Editor, follows playback, and does not replace or
   alter the rendered Mesh.

For focused meshless GLB logs, use:

```bash
cargo run
rg "\[glb_editor\]" "/path/to/aio-asset-normalizer/logs/glb-editor.log" \
  > glb-skeleton-debug.log
```

Generated log files are local artifacts and must remain untracked.

## GLB Batch Export

Use at least two GLB files with Skeletons and animations, plus one file with a
different Skin layout or an ambiguous root hierarchy.

1. Click a file name without checking it. Confirm it becomes the current
   preview and the Current file Inspector remains available.
2. Check two or more files. Confirm the Inspector switches to Batch and the
   checked count is separate from the current preview highlight.
3. Select Skeleton Animation, Combined output, Automatic Skin, enable Remove
   Root Motion with Automatic Root Motion Node, choose an empty output folder,
   and run Preflight. Confirm every source is analyzed independently and no
   output is written during preflight.
4. Confirm a valid batch shows per-file estimates and warnings. A file with no
   removable root translation must show a warning and zero modified channels,
   not silently disappear.
5. Confirm an ambiguous multiple-Skin file blocks the whole batch. Select an
   exact authored Skin name and rerun preflight; duplicate or missing names
   must remain errors.
6. Confirm a valid preflight enables Export Batch. Reopen every output and
   verify the selected Skin hierarchy and all animations are present, render
   resources are removed, and Root Motion is frozen when a track was modified.
7. Verify nested input directories are preserved below the output directory,
   names use the preset suffix, and Split output creates one sanitized file per
   animation.
8. Create an existing output, leave overwrite disabled, and confirm preflight
   blocks the batch. Enable overwrite, rerun preflight, and confirm only the
   output is replaced; no input GLB can be overwritten.
9. Change the checked files, recipe, output directory, or overwrite setting
   after preflight. Confirm Export Batch becomes disabled until preflight is
   run again.
10. Modify a source file after preflight and before export. Confirm the worker
    detects the SHA-256 mismatch before writing any output.
11. Force an output I/O failure during export. Confirm completed outputs remain
    intact, later files are marked skipped, and the result identifies the
    failed input and completed paths.
12. Switch back to Current file after a batch run. Confirm the viewport,
    single-file selection, and source document are unchanged by batch export.

## Inspector and Canvas Input Boundaries

1. Open a GLB in the GLB Editor and resize the left resource tree, right
   Inspector, and bottom dock. Confirm the Canvas occupies only the remaining
   central rectangle and never renders underneath any panel.
2. Drag a numeric value in the Inspector, including orientation, scale,
   translation, and animation rate. Scroll over the Inspector as well. Confirm
   the Inspector value changes while the camera does not rotate, pan, or zoom.
3. Drag in the central Canvas area with the left mouse button and confirm the
   camera rotates. Use the middle mouse button to pan and the mouse wheel to
   zoom.
4. Start a Canvas drag, move the pointer into the Inspector, release the mouse,
   then drag in the Inspector again. Confirm the camera stops and does not
   remain stuck in a rotating or panning state.

## GLB Transform Preview

1. Open a static `.glb` and change a Manual Orientation X, Y, or Z angle.
   Confirm the model updates immediately while the floor grid and axes remain
   fixed.
2. Change Root Transform scale and translation. Confirm both values update the
   model immediately without changing the committed GLB document.
3. Export with pending orientation, scale, and translation values. Reopen the
   export and confirm all three settings are present, while the source document
   remains unchanged in the editor.
4. Reset rotation, scale, or translation individually. Confirm only that
   component returns to its neutral value, the other pending preview inputs
   remain visible, and the camera does not reset.
5. Repeat the checks while playing an animation and while scrubbing the
   timeline. Confirm the preview transform remains applied to every pose.

6. Adjust animation rate together with root transforms, export, and reopen the
   file. Confirm the exported animation timing and root transform match the
   preview. Use Reset rate and confirm it does not reset the root transform.

## Structured Debug Logging

1. Open the Debug Log tab and confirm the Target selector offers All, GLB
   Editor, GLB Export, BVH Studio, Retarget, and FBX Converter.
2. Add or trigger records from two features. Select one target and confirm
   unrelated records are hidden; raise the minimum level and confirm lower
   level records are hidden.
3. Press Copy and confirm only the visible filtered records are copied. Press
   Clear and confirm the UI view is empty while the files in the application
   log directory remain intact.
4. Press Open Log Directory and confirm the platform application data log
   directory opens.
5. Run an export or conversion and use the same task_id to locate its start,
   output, completion, or failure records in debug.log and the feature log.

## Focused Debug Logging

Run the application and filter GLB editor messages into a focused log file:

```bash
cargo run
rg "\[glb_editor\]" "/path/to/aio-asset-normalizer/logs/glb-editor.log" \
  > glb-animation-debug.log
```

Generated log files are local artifacts and must remain untracked.

## Export Overwrite Behavior

1. Open a GLB, BVH, or Mapping JSON and export it to a new output path.
2. Export again to the same output path and confirm the existing non-source
   output is replaced successfully.
3. Confirm the exported file can be opened again and no `.tmp` file remains.
4. Select the original input file as the export destination and confirm the
   application refuses to overwrite the source file.

## GLB Resource Selection and Package Export

Use a mixed GLB containing several model parts, one or more Skins, multiple
animations, a camera, a punctual light, and some unreferenced resources.

1. Open the file in GLB Editor and expand `Export Selection`. Confirm the
   default `Preserve All` preset is available and the source resource counts
   are shown.
2. Choose `Character Package`, select one model node, and confirm its node
   subtree and Mesh primitive checkboxes are available. Uncheck one Primitive
   and confirm the selected Mesh is still represented by its remaining
   Primitive.
3. Select one Scene, one Skin, and one animation. Export with `Combined`,
   reopen the output, and confirm it contains one Scene, the selected model,
   the Skin joints and inverse bind matrices, and the selected animation.
4. Clear the animation checkboxes and export again. Confirm the output is a
   model-plus-skeleton GLB with no animation array and that the source file is
   unchanged.
5. Choose `Skeleton Animation`, select a Skin and one animation, and export.
   Confirm the output has a node hierarchy, Skin, inverse bind matrices, and
   animation, but no Mesh, Material, Texture, Image, Camera, or punctual-light
   resource.
6. Choose multiple animations and `Split`. Confirm one output is written per
   selected animation, names are derived from sanitized animation names, and
   duplicate names receive numeric suffixes.
7. Confirm the Debug Log reports source/output counts and BIN sizes. Compare
   the output file sizes and confirm unused BIN ranges are not retained.
8. Enable `Remove Root Motion` for a selected Character Package animation.
   Confirm the mode control defaults to `Horizontal X/Z (Recommended)`, and
   that the Root Motion Node control offers `Automatic` and the selected
   animation's translation-channel nodes. Export and sample the output at
   multiple times; the resolved root's local X/Z must equal its first keyframe,
   the Y values must match the source at every sampled time, and other channels
   must remain unchanged.
9. Choose a manual Root Motion Node and repeat the export. Confirm the source
   GLB and the current viewport remain unchanged, the export report contains
   the number of rewritten channels, and Split output applies the selected
   mode to each selected animation independently. Select `All X/Y/Z` and
   confirm all three components remain at the first keyframe.
10. Confirm Remove Root Motion is disabled and cleared by `Preserve All`.
    Select an animation without a translation channel on the resolved node and
    confirm export succeeds with a warning and zero modified channels. Try a
    clip with Y-only root motion in `Horizontal X/Z` mode and confirm export
    succeeds without a zero-modification warning. Try a
    CUBICSPLINE root translation accessor, a sparse accessor, and an
    interleaved accessor; confirm each produces a validation error without
    changing the source document. Enable Smart LOOP together with Remove Root
    Motion and confirm the compact export is rejected.
11. Repeat the package and Skeleton Animation exports from BVH Studio and
   GLB-to-GLB retargeting. Confirm the target GLB's selection is independent
   from the source GLB's selection.
12. Try a selection with no model node, multiple incompatible Skins, an
   external buffer, an external image URI, an unsupported extension, and a
   Skeleton Animation Morph Target channel. Confirm the UI reports a clear
   validation error and does not write a partial output.

For focused package-export logs, use:

```bash
cargo run
rg "\[glb_export\]" "/path/to/aio-asset-normalizer/logs/glb-export.log" \
  > glb-export-debug.log
```

Generated log files are local artifacts and must remain untracked.

## CLI

`cargo test --test cli` covers the command contract end to end. For a manual
pass, build the CLI and drive it against a known GLB, BVH, and target GLB:

```bash
cargo build --bin aio-asset-normalizer-cli
BIN=target/debug/aio-asset-normalizer-cli

$BIN docs --raw | head           # embedded reference
$BIN glb inspect asset.glb       # summary, catalog, Skin hierarchies (JSON)
$BIN glb export asset.glb --output-root out --preset skeleton --dry-run
$BIN glb export asset.glb --output-root out --preset skeleton
$BIN glb export asset.glb --output-root out --preset skeleton   # exits 3
$BIN bvh inspect motion.bvh
$BIN retarget prompt --source motion.bvh --target target.glb --out prompt.md
$BIN retarget validate --source motion.bvh --target target.glb --mapping mapping.json
$BIN retarget run --source motion.bvh --target target.glb --mapping mapping.json \
  --out retargeted.glb --preset character
```

1. Confirm every command prints one JSON envelope on stdout with
   `schema_version: 1`, and that progress and diagnostics appear only on stderr.
2. Confirm exit codes: `0` success, `2` for an unknown flag, `3` for an invalid
   or blocked operation, `4` for an unreadable input, and `5` when Blender is
   missing for `fbx convert`.
3. Confirm a re-run without `--overwrite` is blocked and that `--overwrite`
   replaces the output atomically with no `.tmp` file left behind.
4. Confirm the classified error code in `error.code` matches the process exit
   code.
5. On Windows, run the CLI from `cmd.exe` and confirm stdout and stderr are
   visible (the CLI is a console application and does not use the GUI
   subsystem).
