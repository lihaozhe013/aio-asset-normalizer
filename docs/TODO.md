# TODO -- AIO Asset Normalizer

> Completed tasks should be removed from this document. Keep only pending work.

---

## Milestone 2.0 -- Bone Visualization & Animation Playback

### Phase 6: Skeleton Visualization

- [ ] Parse skeleton hierarchy from `.glb` via `gltf` crate
- [ ] Render bone sticks (line segments) and joint spheres in 3D viewport, toggle via "Show Bones" checkbox
- [ ] Bone Tree side panel: recursive tree widget showing bone hierarchy, click to highlight in 3D

### Phase 7: Animation Player

- [ ] Parse animation clips from `.glb`
- [ ] Animation playback controls bar: play/pause, stop, loop toggle, speed slider (0.5x / 1.0x / 2.0x)
- [ ] Interpolate bone transforms per frame and render animated skeleton

### Phase 8: Blender Bridge V2

- [ ] Write `blender_scripts/normalize_v2.py`: skinned mesh bone axis correction, leaf bone preservation, animation bake
- [ ] Extend `bridge.rs` to dispatch to V2 script based on config or input type detection

---

## Backlog / Future

- [ ] BVH motion capture retargeting
- [ ] Batch export to multiple target engines (Godot preset, Unity preset, Unreal preset)
- [ ] Material texture path auto-fix (relative paths)
- [ ] Localization / i18n support
- [ ] Unit tests and CI pipeline
