# TODO -- AIO Asset Normalizer

> Completed tasks should be removed from this document. Keep only pending work.

---

## Milestone 1.0 -- MVP Core Pipeline & 3D Preview

### Phase 2: UI Panels (2D Control Panel)

- [ ] **File List Panel** (`file_list.rs`): system file dialog for picking FBX/Blend/OBJ files, display selected files in a scrollable list, remove/clear items
- [ ] **Drag & Drop** support: accept dropped file/directory paths into the file list
- [ ] **Config Panel** (`config_panel.rs`): target unit scale (float input), up-axis dropdown (Y-Up / Z-Up), cleanup checkboxes (remove unused materials, cameras, lights, loose vertices)
- [ ] **Log Viewer** (`log_viewer.rs`): scrollable text area showing stdout/stderr from Blender subprocess, auto-scroll to bottom, clear button

### Phase 3: Orbit Camera & 3D Viewport Enhancements

- [ ] OrbitCamera: mouse right-drag to rotate, middle-drag to pan, scroll to zoom, with sensible min/max distance limits
- [ ] Wire viewport camera into the egui+three-d split layout properly (render scissor region right of panel)
- [ ] Add `.glb` model loading into viewport (three-d-asset gltf importer)
- [ ] Auto-load converted `.glb` into preview after Blender task finishes

### Phase 4: Blender Bridge V1

- [ ] Write `blender_scripts/normalize_v1.py`: apply transforms, force Y-Up/Z-Forward, convert to `.glb`, support JSON config via CLI args
- [ ] Locate Blender executable at runtime (env var `BLENDER_PATH` or `PATH`)
- [ ] Implement `bridge.rs`: spawn `blender -b -P normalize_v1.py -- <input> <output> <config.json>`, capture stdout/stderr via mpsc channel
- [ ] Implement `task.rs`: define `ConversionTask` struct (input path, output path, config), mpsc sender/receiver for progress + completion
- [ ] Wire conversion tasks to UI: trigger from a "Convert All" button in file list, feed output to log viewer, load result .glb on completion

### Phase 5: Polish & Release

- [ ] Add `build.rs` to embed `blender_scripts/` into binary (or load from adjacent directory)
- [ ] Error handling for missing Blender, invalid files, conversion failures (display in log viewer)
- [ ] End-to-end test: build release binary, run with sample FBX/OBJ, verify .glb output + 3D preview

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
