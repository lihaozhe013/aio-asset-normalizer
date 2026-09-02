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
9. Open a GLB with no animations and confirm only the `Debug Log` tab is shown.
10. Open a GLB with an unsupported CUBICSPLINE or Morph Target clip and confirm
   the clip is listed as unavailable with an explanatory message.
11. Resize the bottom dock and the application window. Confirm the Canvas
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
rg "\[glb_editor\]" debug.log > glb-skeleton-debug.log
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

## Focused Debug Logging

Run the application and filter GLB editor messages into a focused log file:

```bash
cargo run
rg "\[glb_editor\]" debug.log > glb-animation-debug.log
```

Generated log files are local artifacts and must remain untracked.

## Export Overwrite Behavior

1. Open a GLB, BVH, or Mapping JSON and export it to a new output path.
2. Export again to the same output path and confirm the existing non-source
   output is replaced successfully.
3. Confirm the exported file can be opened again and no `.tmp` file remains.
4. Select the original input file as the export destination and confirm the
   application refuses to overwrite the source file.
