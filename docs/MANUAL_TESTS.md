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

## Focused Debug Logging

Run the application and filter GLB editor messages into a focused log file:

```bash
cargo run
rg "\[glb_editor\]" debug.log > glb-animation-debug.log
```

Generated log files are local artifacts and must remain untracked.
