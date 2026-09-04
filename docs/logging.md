# Application Logging

The application uses one structured logging pipeline for the UI and all
background workers. Log records are plain text so they can be read directly or
filtered with `rg`:

```text
2026-09-03T12:30:00.000Z [INFO] [glb_editor] Loaded GLB
```

## Log directory

Logs are written under the platform application data directory:

- macOS: `~/Library/Application Support/aio-asset-normalizer/logs/`
- Linux: `${XDG_DATA_HOME:-~/.local/share}/aio-asset-normalizer/logs/`
- Windows: `%LOCALAPPDATA%/aio-asset-normalizer/logs/`

The application creates the directory on demand. A legacy `debug.log` in the
working directory is not removed or migrated.

## Log files

| File | Contents |
| --- | --- |
| `debug.log` | All application targets |
| `glb-editor.log` | GLB loading, preview, animation, and texture operations |
| `glb-export.log` | GLB export validation and background export tasks |
| `bvh-studio.log` | BVH loading, preview, mapping, and BVH export |
| `retarget.log` | Generic retargeting, GLB retargeting, and Agent prompt tasks |
| `fbx-converter.log` | Blender discovery, task state, and Blender output |

The category files are subsets of `debug.log`. Retargeting aliases
`retarget`, `glb_retarget`, and `retarget_agent` are intentionally combined in
`retarget.log`.

Each file is buffered and rotated at 10 MiB. The active file plus up to three
backups (`.1`, `.2`, `.3`) are retained. A file-system error does not terminate
the application; the Debug Log panel reports that the affected file is
unavailable.

## Levels and task correlation

The default level is `info`. Set `RUST_LOG` before launching the application to
override it, for example:

```bash
RUST_LOG=debug cargo run
```

Background export and conversion workflows assign a numeric `task_id`. The
same ID appears on task-start, Blender output, progress, completion, and
failure records. Use it to follow one operation across the aggregate and
feature-specific logs:

```bash
rg 'task_id=42' '/path/to/aio-asset-normalizer/logs'
```

File names are logged with a short stable hash, such as
`character.glb#1a2b3c4d`; complete user directories are not written to logs.
Blender stdout and stderr records additionally include `stream=stdout` or
`stream=stderr`.

## Collecting a feature log

Use the feature-specific file directly and write any filtered copy outside the
application log directory:

```bash
rg '\[glb_editor\]' '/path/to/aio-asset-normalizer/logs/glb-editor.log' \
  > glb-editor-debug.log
rg '\[glb_export\]' '/path/to/aio-asset-normalizer/logs/glb-export.log' \
  > glb-export-debug.log
rg '\[bvh_studio\]|\[retarget\]|\[glb_retarget\]' \
  '/path/to/aio-asset-normalizer/logs/bvh-studio.log' \
  > bvh-studio-debug.log
rg '\[fbx_converter\]' \
  '/path/to/aio-asset-normalizer/logs/fbx-converter.log' \
  > fbx-converter-debug.log
```

Replace `/path/to/aio-asset-normalizer/logs` with the platform path above.
Generated `*.log` files remain local artifacts and must not be committed or
included in release archives.

