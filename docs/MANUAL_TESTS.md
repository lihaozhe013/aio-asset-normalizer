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
   Confirm the Root Motion Node control offers `Automatic` and the selected
   animation's translation-channel nodes. Export and sample the output at
   multiple times; the resolved root's local translation must equal its first
   keyframe while other channels remain unchanged.
9. Choose a manual Root Motion Node and repeat the export. Confirm the source
   GLB and the current viewport remain unchanged, the export report contains
   the number of rewritten channels, and Split output applies the setting to
   each selected animation independently.
10. Confirm Remove Root Motion is disabled and cleared by `Preserve All`.
    Select an animation without a translation channel on the resolved node and
    confirm export succeeds with a warning and zero modified channels. Try a
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
