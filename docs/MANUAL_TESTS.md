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
6. Change the speed, drag the timeline, pause, and use both frame-step
   buttons. Confirm every action updates the preview without marking the GLB
   document dirty.
7. Switch to the `Debug Log` tab and confirm log controls are visible while
   animation playback state is preserved. Switch back to `Animation`.
8. Open a GLB with no animations and confirm only the `Debug Log` tab is shown.
9. Open a GLB with an unsupported CUBICSPLINE or Morph Target clip and confirm
   the clip is listed as unavailable with an explanatory message.
10. Resize the bottom dock and the application window. Confirm the Canvas
    boundary moves with the dock and never renders underneath it.

## GLB Transform Preview

1. Open a static `.glb` and change a Manual Orientation X, Y, or Z angle.
   Confirm the model updates immediately while the floor grid and axes remain
   fixed.
2. Change Root Transform scale and translation. Confirm both values update the
   model immediately without changing the GLB document until Apply is pressed.
3. Press Reset Preview and confirm the model returns to the committed document
   state while the current camera view is preserved.
4. Press Apply for rotation, scale, or translation individually. Confirm only
   the applied input resets to its neutral value, other pending preview inputs
   remain visible, and the camera does not reset.
5. Repeat the checks while playing an animation and while scrubbing the
   timeline. Confirm the preview transform remains applied to every pose.

## Focused Debug Logging

Run the application and filter GLB editor messages into a focused log file:

```bash
cargo run
rg "\[glb_editor\]" debug.log > glb-animation-debug.log
```

Generated log files are local artifacts and must remain untracked.
