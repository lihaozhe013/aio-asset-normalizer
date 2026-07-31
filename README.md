# AIO Asset Normalizer

A cross-platform desktop application for batch-normalizing 3D assets and exporting them as `.glb` files.

The application combines a Rust/egui control panel with a three-d preview viewport. Asset conversion is performed by Blender in background processes, keeping the UI responsive while supporting both static meshes and animated, skinned assets.

## Features

- Import individual assets or folders, and select multiple files for batch processing.
- Support `.fbx`, `.blend`, `.obj`, `.gltf`, and `.glb` input files.
- Normalize target scale and up-axis (`Y-Up` or `Z-Up`).
- Remove unused materials, cameras, lights, and loose mesh geometry.
- Choose between two Blender processing scripts:
  - **V1** for static mesh and material normalization.
  - **V2** for skinned meshes, leaf-bone preservation, and animation baking.
- Export normalized assets as `.glb` files.
- Preview exported GLB files with a 3D orbit camera, coordinate axes, and a ground grid.
- Inspect skeletons, highlight bones, and preview embedded animation clips.
- View Blender output and conversion progress in the application log.
- English and Simplified Chinese UI translations.

## Requirements

- Rust and Cargo
- Blender installed and available on the system

If Blender is not detected automatically, set its executable path in **Edit > Preferences**, or set the `BLENDER_PATH` environment variable.

## Build and run

```bash
cargo run
```

To create a release build:

```bash
cargo build --release
```

## Basic workflow

1. Open an asset folder or drag supported files into the file tree.
2. Select the files to process.
3. Configure scale, target orientation, cleanup options, and the processing script version.
4. Start the conversion.
5. Review the generated `_normalized.glb` files and inspect them in the preview viewport.

The bundled Blender scripts can also be run directly:

```bash
blender -b -P blender_scripts/normalize_v1.py -- <input> <output.glb> <config.json>
blender -b -P blender_scripts/normalize_v2.py -- <input> <output.glb> <config.json>
```

## Architecture

```text
Rust application
├── egui UI             File management, configuration, preferences, and logs
├── three-d viewport    GLB preview, camera, axes, grid, skeletons, and animation
└── Blender bridge      Background process execution and task messages

Blender scripts         Import, normalize, and export assets as GLB
```

The UI communicates with background conversion tasks through message channels. Blender-specific process handling remains in the Blender bridge, while viewport rendering is kept independent of UI panels.

## Project structure

```text
src/
├── app.rs
├── main.rs
└── modules/
    ├── blender/          Blender process bridge and task definitions
    ├── ui/               egui panels and file management
    ├── viewport/         three-d rendering and camera controls
    ├── animation.rs      GLB animation parsing and playback
    ├── skeleton.rs       Skeleton parsing and bone data
    └── preferences.rs    Persistent application preferences
blender_scripts/
├── normalize_v1.py       Static asset normalization
└── normalize_v2.py       Skinned asset and animation normalization
```

## License

This project is licensed under the MIT License.
