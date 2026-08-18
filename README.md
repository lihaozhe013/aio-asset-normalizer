# AIO Asset Normalizer

A pure-Rust 3D asset standardization tool for indie game developers and independent creators.

The project is undergoing a complete redesign. It is moving away from a Blender-dependent multi-format converter and becoming a desktop tool focused exclusively on `.glb` assets. The new product will provide:

- GLB editing, preview, and standardized export;
- animation clip playback, timeline controls, trimming, and export;
- Mesh material and PBR texture replacement;
- BVH playback, trimming, generic skeleton mapping, and GLB animation export;
- reusable Mapping files for different motion-capture systems and character models.

## Design Goals

- Support glTF 2.0 Binary (`.glb`) only. FBX, OBJ, Blend, and other source formats are out of scope.
- Require no Blender runtime, Blender API, or external conversion process.
- Implement GLB loading, editing, animation processing, and export in Rust.
- Reuse the existing `egui` + `three-d` window, 3D Canvas, orbit camera, axes, grid, and skeleton visualization foundations.
- Use a shared bottom dock with switchable Animation and Debug Log tabs so the Canvas always owns its reserved viewport area.
- Keep UI, GLB document processing, BVH algorithms, and rendering decoupled; run expensive work through background tasks and message passing.
- Never overwrite source files by default. All exports use temporary files and atomic replacement.

## Product Pages

### GLB Editor

The main page edits existing GLB files instead of converting between formats.

- Load, inspect, and preview scenes, nodes, Meshes, materials, Skins, skeletons, and animations.
- Play standard GLB node and Skinned Mesh animations with pause, looping, speed, seeking, and frame stepping.
- Support `STEP` and `LINEAR` animation sampling; unsupported CUBICSPLINE and Morph Target clips are reported explicitly.
- Adjust model orientation with XYZ `±90°` shortcuts and precise Euler input.
- Configure animation trim by start and end time with a live preview; trim is
  applied to the export copy only when enabled.
- Apply Smart LOOP processing to close small capture drift with a configurable
  0.01-2.00 second transition with millisecond-level adjustment. Significant Root Motion is rejected instead of
  being silently converted to an in-place clip. Smart LOOP currently accepts
  LINEAR translation, rotation, and scale channels on a single Skin.
- Replace Base Color, Normal, Metallic-Roughness, Occlusion, and Emissive textures.
- Reserve extension points for future skeleton and Mesh replacement.
- Export game-ready GLBs with consistent coordinates, units, grounding, and facing.

### BVH Studio

BVH processing is an independent page that takes a BVH file, a target GLB, and a Mapping file:

- Play and inspect BVH motion frame by frame;
- trim and save BVH files;
- retarget BVH motion to any target Skinned GLB that satisfies the input contract;
- export a Character Package containing a character and animation;
- export an Animation Clip containing only the skeleton and animation;
- use explicit Mapping files to support different motion-capture systems and character naming conventions.

## Default Standardization Contract

The default export contract is listed below. Grounding, centering, and unit scaling can be disabled or adjusted in the export options:

- right-handed coordinates;
- Y-Up;
- model forward direction `-Z`;
- meter-based output by default;
- bounding box centered on the XZ plane;
- lowest point placed at `Y=0`;
- identity scene-root transform;
- orientation, scale, Skin, inverse bind matrices, and root animation baked together.

For compressed geometry such as Draco or Meshopt that cannot be safely decoded and rewritten, the tool must report the unsupported extension explicitly instead of silently producing a corrupted file.

## BVH Mapping

The Mapping file is the single source of truth for BVH retargeting. Automatic name matching only produces suggestions; the user must confirm them before export. The initial format is versioned JSON:

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

The initial target-model contract requires a conventional glTF Skin with valid `JOINTS_0`, `WEIGHTS_0`, and inverse bind matrices. Fixed company models, fixed skeleton sizes, fixed N-Pose assumptions, serial protocols, and IMU logic from the company motion-capture application do not belong in this generic tool.

## Technology Stack

| Layer | Technology |
| --- | --- |
| GUI | `egui` through `three-d` |
| 3D viewport | `three-d` / `wgpu` |
| GLB loading and validation | `gltf` |
| GLB document editing | Preserve raw JSON + BIN; use `gltf-json` when appropriate |
| Image processing | Rust `image` ecosystem |
| Background tasks | `std::sync::mpsc` + worker threads |
| File dialogs | `rfd` |

The GLB read/write layer preserves the original JSON and BIN data and changes only affected resources wherever possible. This helps retain unknown extensions, `extras`, and the original resource layout. The `gltf` crate and GLB container APIs provide the foundation for loading, reparsing, and writing files.

## Rebuild Stages

1. **GLB Editor core**: document model, orientation/scale baking, animation trimming, PBR texture replacement, and standardized export.
2. **BVH Studio**: robust BVH parsing, generic FK, Mapping editor, Rest Pose delta retargeting, Character Package export, and Animation Clip export.
3. **Quality and release**: complete tests, diagnostics, documentation, and cross-platform packaging.

The complete design, interfaces, migration boundaries, acceptance criteria, and test plan are documented in [`docs/REBUILD_PLAN.md`](docs/REBUILD_PLAN.md).

## Current Status

The application builds without the Blender bridge or legacy format scripts. The GLB Editor has a pure-Rust document layer with GLB validation, scene indexing, root transforms, interpolated animation trimming, runtime playback for node and Skinned Mesh animations, CPU skinning, PNG/JPEG PBR texture replacement, shared-material duplication, and atomic reparse-validated export. BVH Studio has generic hierarchy parsing, frame trimming, Mapping JSON validation and saving, reviewed name-match suggestions, Rest Pose delta retargeting, frame-stepped skeleton playback, optional redundant-key reduction, and Character Package / Animation Clip GLB export.

The next engineering slice is complete standardization baking for mesh and Skinned data, followed by richer fixture coverage and optional compressed-geometry support. Root-motion and heading controls remain optional BVH enhancements, while GPU skinning, Morph Target playback, CUBICSPLINE sampling, and skeleton replacement are intentionally reserved for later releases.

## Development Verification

```bash
cargo fmt --check
cargo check
cargo test
```

The project is licensed under the MIT License.
