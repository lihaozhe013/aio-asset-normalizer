# AIO Asset Normalizer Rebuild Plan

## 1. Product Positioning and Assessment

The project is being repositioned from a Blender-dependent multi-format converter into a pure-Rust GLB editor and motion standardization tool for indie game developers and independent creators.

This direction is feasible and gives the product a clearer, more sustainable boundary than continuing to expand an FBX/Blend/OBJ conversion pipeline. The existing `three-d` Canvas, egui window, orbit camera, axes, ground grid, and skeleton visualization can be retained. GLB document write-back, skinning transforms, and BVH retargeting must be rebuilt as independent core services.

Key conclusions:

- Removing Blender is appropriate, but the work is more than deleting the Blender bridge; the core product must become a validated GLB document editor.
- The BVH parser, trimming, animation writing, and validation code in `zl_mocap` are valuable migration sources, but the current BVH-to-GLB exporter is still tied to `robot-y.glb`, a fixed hierarchy, and name-based matching. It cannot be copied unchanged.
- Generic BVH reliability comes from an explicit Mapping contract and Rest Pose delta retargeting, not from silently guessing every skeleton.
- The initial target-model contract is a conventional Skinned GLB. Rigid company-specific Mesh parts, N-Pose assumptions, fixed skeleton sizes, IMU, serial protocols, and anthropometric logic do not enter the generic tool.

## 2. Goals and Non-Goals

### Goals

- Support `.glb` only. FBX, OBJ, Blend, and other input formats are out of scope.
- Remove all runtime dependence on Blender, the Blender API, and external conversion processes.
- Implement GLB loading, editing, animation processing, resource replacement, and export in Rust.
- Provide independent GLB Editor and BVH Studio pages.
- Support arbitrary target naming and hierarchy when the target GLB satisfies the input contract and has a valid Mapping.
- Keep expensive computation and export off the UI thread and report progress and readable errors.

### Non-Goals

- Restore general FBX/OBJ/Blend conversion.
- Keep Blender as an optional backend.
- Implement the company motion-capture application's serial, IMU, hardware calibration, foot locking, or anthropometric systems.
- Silently accept every malformed or unsupported GLB; unsafe edits must fail explicitly.
- Implement skeleton replacement in the initial release; reserve the data and UI extension points only.
- Implement geometric boolean clipping. In the initial release, “trimming” means animation and BVH time-range trimming.

## 3. User Experience and Page Design

### 3.1 GLB Editor

The main page contains four regions:

1. Left resource tree: the current GLB, Scene, Node, Mesh, Primitive, Skin, materials, and animation clips.
2. Center 3D Canvas: the model, grid, axes, skeleton, and selected-object highlighting.
3. Right Inspector: transforms, materials, textures, Skin, animation clips, and export settings.
4. Shared bottom dock: switchable Animation and Debug Log tabs. The Animation
   tab contains clip selection, playback, pause, frame stepping, speed, and
   trim controls.

Initial interactions:

- Support `.glb` file selection and drag-and-drop.
- Use XYZ `±90°` shortcuts and precise Euler input for orientation instead of requiring a complex 3D gizmo.
- Preview node transforms live, but perform complete baking during export.
- Display animation trim in seconds. If a boundary falls between keyframes, interpolate a boundary key and rebase the clip to time zero.
- If a material is shared by multiple Primitives, duplicate it for the selected Primitive by default; let the user explicitly choose to edit the shared material.
- Support Base Color, Normal, Metallic-Roughness, Occlusion, and Emissive texture slots initially.
- Accept PNG/JPEG texture inputs initially and embed them in the GLB.
- Never overwrite the source in place; save through “Save As” and atomic replacement.

### 3.2 BVH Studio

The BVH page is an independent workflow and must not share mutable editing state with the GLB Editor. Its inputs are:

- a BVH file;
- a target GLB;
- a Mapping JSON file.

The workflow provides:

- BVH hierarchy and channel inspection;
- playback, pause, frame stepping, speed control, and viewport skeleton display;
- inclusive start/end frame trimming and BVH export;
- automatic name-match suggestions and manual Mapping editing;
- mapped, unmapped, duplicate-target, and coverage reporting;
- Character Package and Animation Clip export;
- optional Root Motion, first-frame heading normalization, key reduction, and current trim-range export.

## 4. GLB Standardization Contract

### 4.1 Default Output

- glTF 2.0 Binary GLB;
- right-handed coordinates;
- Y-Up;
- model forward direction `-Z`;
- meter-based output by default;
- bounding box centered on the XZ plane;
- lowest point placed at `Y=0`;
- identity scene-root transform.

Grounding, centering, and unit scaling can be disabled or adjusted. Units must not be silently inferred from a filename or arbitrary model dimensions; they must come from an explicit user option or input setting.

### 4.2 Complete Orientation Baking

Orientation and scale must not be represented only by adding a wrapper parent node. The export pipeline must update all affected data together:

- Scene/Node TRS;
- static Mesh POSITION, NORMAL, TANGENT, and Morph Target data;
- Skin joints and inverse bind matrices;
- animation translation, rotation, and scale channels;
- Accessor min/max values;
- preview and export coordinate calculations.

For Skinned models, compare world matrices and Mesh bounds at the first, representative, and last frames to verify that baking preserves the visible result.

### 4.3 Resource Boundaries

- Preserve unaffected JSON, BIN, `extras`, and unknown extensions.
- Detect geometry extensions such as `KHR_draco_mesh_compression` and `EXT_meshopt_compression` when they cannot be safely decoded and rewritten.
- If an operation requires modifying compressed geometry that the implementation cannot decode, return an error containing the extension name and node name.
- Never produce a file that appears successful while being damaged in an engine or viewer.

## 5. Core Data Model and Interfaces

### 5.1 GLB Document Model

Create a document core independent of UI and rendering:

```rust
pub struct GlbDocument {
    pub source_path: PathBuf,
    pub json: serde_json::Value,
    pub binary: Vec<u8>,
    pub index: DocumentIndex,
}

pub enum EditOperation {
    BakeOrientation(OrientationEdit),
    Normalize(NormalizationOptions),
    TrimAnimation(AnimationTrim),
    ReplaceTexture(TextureReplacement),
}

pub struct ExportOptions {
    pub center_xz: bool,
    pub ground_y: bool,
    pub unit_scale: f32,
    pub preserve_unknown_extensions: bool,
}
```

Requirements:

- Keep raw JSON/BIN as the write-back source of truth so unknown extensions are not lost during round trips.
- Make `DocumentIndex` provide fast indexes for node parents/children, Scene roots, Mesh/Primitive relationships, materials, images, Skins, animations, and Accessors.
- Use `three-d-asset` only to build preview objects, never as the GLB write-back source.
- Validate each edit operation independently and apply operations in a fixed order before export.
- Export in this order: load and validate, apply edits, update resource indexes, encode GLB, reparse, sample and validate, then atomically save.

### 5.2 Application State

Split the current monolithic `App` into:

- `EditorSession`: current GLB, edit operations, selection, animation player, and dirty state;
- `BvhSession`: BVH, target character, Mapping, playback state, trim range, and export request;
- `ViewportState`: Canvas, camera, helpers, skeleton display, and the current preview snapshot;
- `TaskState`: background jobs, progress, logs, and errors.

UI code must not access the GLB encoder, Accessor parsing details, or BVH file writer. Workers receive immutable snapshots and return progress and results through `mpsc`.

## 6. BVH Mapping and Retargeting Algorithm

### 6.1 Mapping JSON

The initial format is versioned JSON:

```json
{
  "schema_version": 1,
  "source": {
    "up_axis": "Y",
    "forward_axis": "-Z",
    "unit": "cm",
    "root": "Hips"
  },
  "target": {
    "skin": "Armature",
    "root": "pelvis"
  },
  "bones": [
    {
      "source_joint": "Hips",
      "target_node": "pelvis",
      "rotation_offset_xyzw": [0.0, 0.0, 0.0, 1.0]
    }
  ]
}
```

Constraints:

- `schema_version` must be present and supported;
- source joint names must be unique;
- every target node must belong to the selected Skin's joints;
- one target bone cannot be assigned to multiple source bones;
- both a source root joint and target root node are required;
- unmapped source joints leave the target in its Rest Pose;
- automatic matching only creates suggestions and never exports directly.

### 6.2 Rest Pose Delta Retargeting

For every mapped joint:

1. Parse BVH offsets and channels to derive source Rest Pose local and world transforms.
2. Read target GLB nodes and Skin data to derive target Rest Pose local and world transforms.
3. For each frame, compute the source world-rotation delta relative to the source Rest Pose.
4. Convert the delta from the source coordinate basis to the target coordinate basis.
5. Apply the optional `rotation_offset_xyzw` from the Mapping.
6. Apply the converted delta to the target Rest Pose world rotation.
7. Convert the result back into target-local TRS and write animation channels.

Non-root joints retarget rotation by default. The root may additionally retarget translation. Root translation uses the source `unit_scale`, with an optional source-to-target skeleton-height scale. Different bone lengths do not directly change target bone lengths.

### 6.3 Target Model Contract

The initial release supports arbitrary naming and hierarchy for conventional Skinned GLBs, provided that they satisfy all of the following:

- a valid Skin exists;
- the Skin contains joints;
- Mesh Primitives contain `JOINTS_0` and `WEIGHTS_0`;
- the inverse bind matrix count matches the joint count;
- the target skeleton hierarchy can be reconstructed from node relationships;
- the target Mesh, Accessor, and animation data can be reparsed by `gltf`.

Missing Skins, invalid weights, fixed company rigid-part structures, and unverifiable compressed geometry must be rejected explicitly.

## 7. BVH Outputs

### Character Package

- Copy the user-selected target GLB;
- retain Meshes, Skin, materials, textures, and images;
- append one user-named animation clip;
- leave existing animations unchanged unless the user explicitly chooses to replace a same-named clip;
- validate Skin data, animation targets, and representative keyframes.

### Animation Clip

- retain target skeleton nodes, the Skin contract, inverse bind matrices, and animation;
- remove Meshes, materials, textures, and images;
- allow animation channels to target only skeleton nodes or the scene root;
- target later engine import or animation merging, without promising standalone rendering.

## 8. Migration Boundaries

### Reusable

- the current window and render loop;
- egui and `three-d` integration;
- Orbit Camera, grid, axes, lighting, and the base Canvas;
- skeleton tree and skeleton visualization;
- the current animation player's interpolation approach;
- `zl_mocap` GLB JSON/BIN parsing, Accessor appending, alignment, encoding, and atomic writing;
- `zl_mocap` animation channel generation, first-frame heading normalization, key reduction, trimming, and validation tests.

### Must Be Rewritten

- the `zl_mocap` export entry point bound to `robot-y.glb`;
- the name-only BVH player mapping logic;
- company-specific rigid-part-to-single-influence Skin export;
- fixed skeleton counts, N-Pose, finger-angle, and bone-precision logic;
- `panic!`/`expect`-based error handling in the current BVH parser;
- target Skin selection, Mapping parsing, and Rest Pose delta retargeting;
- page state and legacy conversion settings.

### Remove or Retire

- the Blender bridge and Blender task modules;
- `blender_scripts`;
- Blender path preferences and legacy conversion settings;
- the “Start Conversion” action, conversion log, and FBX/Blend/OBJ filters;
- code that depends on `robot-y.glb`, fixed mappings, or company assets.

## 9. Staged Implementation Plan

### Stage One: GLB Editor

1. Rebuild Cargo dependencies and module boundaries and remove the Blender runtime path.
2. Implement `GlbDocument`, GLB parser/encoder, `DocumentIndex`, and structural validation.
3. Connect the existing Canvas to the new Editor Session.
4. Implement node selection, Inspector, orientation preview, Euler input, and orientation shortcuts.
5. Implement orientation and unit baking for static and Skinned Meshes.
6. Implement animation clip playback, trimming, and boundary interpolation.
7. Implement complete PBR-slot texture replacement and shared-material duplication.
8. Implement standardized export, atomic saving, and reparse validation.
9. Remove the legacy conversion UI, Blender bridge, scripts, and settings.

Stage-one acceptance: conventional static GLBs, Skinned GLBs, and animated GLBs can be loaded, previewed, edited, and saved as new files. Reopening an output must preserve model appearance, skeleton behavior, and animation.

### Stage Two: BVH Studio

1. Migrate and rewrite the BVH parser so all input failures return line-aware `Result` values.
2. Extract BVH Rest Pose, FK, timeline, and trimming cores.
3. Implement the Mapping schema, save/load support, automatic suggestions, and validation reports.
4. Implement target Skin selection and generic Rest Pose delta retargeting.
5. Implement BVH preview and skeleton overlays.
6. Implement Character Package and Animation Clip export.
7. Reuse the stage-one GLB writer, Accessor helpers, and validation framework.

Stage-two acceptance: at least two conventional Skinned GLBs with different naming, proportions, and Rest Poses can play the same BVH through separate Mappings and produce both valid output types.

### Stage Three: Quality and Release

- complete cross-platform file dialogs, error messages, and recovery flows;
- add performance baselines and progress reporting for large files;
- establish test fixtures, CI, and optional Khronos glTF Validator checks;
- update release notes, the user guide, and Mapping examples;
- evaluate Draco/Meshopt, WebP/KTX2, and multi-Skin support.

## 10. Testing and Acceptance

### GLB Documents

- GLB headers, JSON/BIN alignment, and empty BIN chunks;
- preservation of unknown extensions, `extras`, materials, and image resources;
- static Meshes, multiple Primitives, Skinned Meshes, Morph Targets, and non-uniform scale;
- node parent/child relationships, Scene roots, Skin joints, and inverse bind matrices;
- Accessor component types, counts, byte ranges, and min/max values;
- readable errors for corrupted input and unsupported compressed extensions.

### Editing Operations

- XYZ orientation, scale, grounding, and XZ centering;
- complete baking for static and Skinned models;
- animation clip endpoints, interpolation across keyframes, one-frame clips, and empty clips;
- all five PBR texture slots and shared-material duplication;
- reparse outputs and compare world matrices and AABBs at the first, middle, and last frames.

### BVH

- varied whitespace, line endings, channel orders, and special joint names;
- invalid hierarchies, missing frames, zero-frame files, invalid Frame Time, and trim boundaries;
- case-insensitive and alias suggestions, duplicate targets, missing roots, and non-Skin targets;
- Y-Up/Z-Up, unit scaling, Root Motion, first-frame heading, and unmapped helper bones;
- T-Pose/A-Pose/N-Pose, different bone lengths, and different target hierarchies;
- resource policies and animation-target validation for both output types.

### Engineering Checks

```bash
cargo fmt --check
cargo check
cargo test
```

The project must build without warnings, all unit tests must pass, and generated GLBs must be readable again through `gltf::Gltf`. CI may additionally run the Khronos glTF Validator.

## 11. Constraints and Assumptions

- Keep the project name `aio-asset-normalizer` for now.
- “Any model” means a conventional Skinned GLB that satisfies the glTF 2.0 input contract.
- The initial release processes one selected Skin; synchronized multi-Skin animation is deferred.
- The initial release accepts PNG/JPEG texture inputs; KTX2, WebP, and compressed geometry are future extensions.
- The initial Mapping schema is JSON; do not maintain a parallel YAML schema yet.
- Source files are not overwritten by default; all outputs are saved to a user-selected path.
- Reused `zl_mocap` code must have appropriate authorization and comply with its applicable license.
- When a task is completed, remove its pending entry from the relevant `docs/` planning document instead of leaving a completed checkbox.

## 12. Implementation Checkpoint

The current implementation slice is now in place:

- the executable no longer compiles or invokes the Blender bridge;
- the GLB Editor has pure-Rust GLB validation, scene inspection, root
  transforms, interpolated animation trimming with time rebasing, PNG/JPEG PBR
  texture replacement, shared-material duplication, runtime node and Skinned
  Mesh animation playback with CPU skinning, and atomic reparse-validated
  export;
- BVH Studio has generic hierarchy parsing, frame trimming, versioned Mapping
  JSON loading and saving, mapping validation reports, reviewed name-match
  suggestions, Rest Pose delta retargeting, frame-stepped skeleton playback,
  optional redundant-key reduction, and Character Package / Animation Clip
  GLB export;
- the existing egui and three-d Canvas infrastructure remains the preview
  foundation;
- the bottom dock switches between GLB animation playback and Debug Log while
  reserving the selected panel's area from the 3D Canvas;
- complete mesh/Skinned standardization baking, optional root-motion and
  heading controls, broader fixture validation, and optional compressed-
  geometry support remain planned;
- CUBICSPLINE, Morph Target playback, GPU skinning, and skeleton replacement
  remain intentionally deferred.
