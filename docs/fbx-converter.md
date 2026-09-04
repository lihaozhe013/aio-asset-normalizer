# FBX Converter

## Purpose

The FBX Converter page restores the early Blender-backed batch pipeline as a
dedicated, self-contained workflow. It converts `.fbx`, `.obj`, and `.blend`
files to standardized `.glb` assets by invoking a locally installed Blender in
headless mode. The page has no 3D viewport by design: it is a file tree plus a
conversion list.

Per `AGENTS.md`, this page is the only workflow allowed to depend on an
external Blender installation. The GLB Editor, BVH Studio, build, tests, and
development flows must keep working with Blender absent.

## UI layout

- Left panel: a `FileTree` instance configured with accepted extensions
  `fbx`, `obj`, `blend`. Files are checked to queue them; the tree supports
  folder open, refresh, select/deselect/invert, drag-and-drop, and a
  "show all files" toggle (non-accepted files then appear disabled).
- Central panel: Blender discovery row (text field, Browse, Clear, detected
  path status), the Convert button, and a per-file status list with pending /
  converting / done / failed states and short error text.
- Bottom dock: the shared Debug Log tab receives all Blender output lines with
  task_id, stream=stdout|stderr, and a safe input-file label. The same records
  are retained in fbx-converter.log.
- The right inspector panel is hidden on this page.

## Conversion behavior

For each checked file, in sorted order, sequentially:

1. Compute the output path `<input_dir>/<stem>_normalized.glb`. Source files
   are never modified; re-running a batch overwrites only previous conversion
   results.
2. Stage the embedded normalization script to
   `<temp_dir>/aio-asset-normalizer/normalize_to_glb.py`. The script source
   lives at `blender_scripts/normalize_to_glb.py` and is compiled into the
   binary, so packaged builds need no extra data files.
3. Run `blender -b -P <script> -- <input> <output> <config_json>` as a
   subprocess and stream stdout/stderr to the log.
4. Require a zero exit code and an existing output file.
5. Re-parse the produced GLB through the project reader
   (`GlbDocument::load`). A file that cannot be re-read is reported as a
   failure, never as success.

All work runs on one background worker thread that reports to the UI through
an `mpsc` channel of `ConverterMessage` items, drained each frame by
`App::poll_tasks`.

Blender stdout and stderr are written to the feature log with a shared
task_id, stream=stdout or stream=stderr, and a safe input-file label. The
aggregate and feature-specific logs retain the same records.

## Fixed normalization profile

The converter exposes no options. `task::default_config_json()` sends the
historical V2 defaults to the script:

| Key | Value | Effect |
| --- | --- | --- |
| `target_scale` | 1.0 | no unit rescale |
| `up_axis` | "Y" | glTF Y-up export |
| `remove_unused_materials` | true | purge zero-user materials |
| `remove_cameras` | true | drop camera objects |
| `remove_lights` | true | drop light objects |
| `remove_loose_vertices` | false | keep loose geometry |
| `correct_bone_axes` | true | armature edit pass |
| `preserve_leaf_bones` | true | leaf bones marked deform |
| `bake_animations` | true | visual keying bake per armature |

Export uses `export_force_sampling` and `export_def_bones`, and the Blender
5.x-compatible `export_vertex_color="MATERIAL"` parameter.

## Blender discovery

`bridge::find_blender` resolves in this order:

1. The optional path override stored on the FBX Converter page
   (persisted as `converter.blender_path` in the user config). An invalid
   override falls through to detection.
2. Platform candidates: standard `Program Files\Blender Foundation\*`
   installs on Windows; `/Applications/Blender.app` (resolved through the
   bundle's `Contents/MacOS` directory) on macOS.
3. `blender` on the system `PATH`.

If nothing is found, the page shows "Blender not found", the Convert button
stays disabled, and starting a batch is refused with a log message.

## Module map

| Path | Responsibility |
| --- | --- |
| `src/modules/blender/bridge.rs` | Blender discovery, script staging, subprocess invocation, streamed output |
| `src/modules/blender/task.rs` | `ConversionTask`, `ConverterMessage`, output naming, fixed config |
| `src/app_fbx_converter.rs` | App state, batch start, worker loop, result polling |
| `src/modules/ui/fbx_converter_panel.rs` | Page controls and per-file status list |
| `blender_scripts/normalize_to_glb.py` | Embedded Blender-side normalize/export script |

## Manual verification

The automated tests cover path resolution, naming, message state
transitions, and locale key parity. Actual conversion requires a local
Blender and must be verified in the GUI on each maintained platform:

1. Run the application and switch to the FBX Converter tab.
2. Open a folder containing a `.fbx` file (optionally `.obj`/`.blend` too).
3. Check files and press Convert. Confirm: statuses advance pending ->
   converting -> done, `<stem>_normalized.glb` appears next to each source,
   the tree refreshes, and each produced GLB opens in the GLB Editor with
   expected orientation, scale, skin, and animations.
4. Failure path: point the Blender override at a non-executable value or
   run without Blender; the page must show "Blender not found", conversion
   must be refused with fbx_converter log records, and no source file may
   be modified.
5. Focused log capture:

```bash
cargo run
rg "\[fbx_converter\]" \
  "/path/to/aio-asset-normalizer/logs/fbx-converter.log" \
  > fbx-converter-debug.log
```
