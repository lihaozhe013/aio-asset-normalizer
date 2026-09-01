# BVH Skeleton Visualization

BVH Studio and GLB retarget previews share the `SkeletonVisual` renderer. The
default display is an instanced, Blender-style octahedral bone with separate
joint and End Site markers. Stick and Lines modes are available for topology
debugging. Source BVH bones use orange and target Skin bones use cyan.
The GLB retarget preview exposes the same display mode, visibility, width, and
fit controls while the ordinary GLB editor view remains unchanged.

Display dimensions are calibrated from the authored Rest Pose. Sampling a
different motion frame therefore changes transforms only; it does not change
bone thickness, joint size, or the camera framing. Target Skin previews include
the ancestors needed to keep a continuous hierarchy, including helper nodes
that are not direct Skin joints.

BVH units remain explicit. The inspector reports raw and converted spans and
offers `Use m`, `Use cm`, and `Use mm` shortcuts. A warning is shown when the
selected unit produces an unusually small or large preview; no unit is inferred
from a filename or model dimensions.

The camera fit derives its distance and clipping planes from the current bounds
and viewport aspect ratio. Axes and the ground grid are regenerated at the
same skeleton scale. BVH Studio hides the origin helper by default; it can be
enabled from the View menu when needed.

## Manual verification

1. Run `cargo run` and open a BVH in BVH Studio.
2. With the default unit, check the raw/converted span diagnostic. Choose
   `Use m` when the file contains metre-scale offsets.
3. Confirm that Octahedral bones have visible thickness and that joints and End
   Sites are colored orange. Switch to Stick and Lines to inspect topology.
4. Load a target GLB Skin and confirm that the cyan hierarchy follows the
   character while playback, frame stepping, and speed changes keep its size
   stable.
5. Use `Fit source`, `Fit target`, or `Fit all` after changing the unit or
   selecting a different Skin.

For focused runtime logs, use:

```bash
cargo run
rg "\[(bvh_studio|retarget|glb_retarget)\]" debug.log > bvh-studio-debug.log
```
