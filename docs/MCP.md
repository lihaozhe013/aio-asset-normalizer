# MCP Integration

AIO Asset Normalizer includes a native, local MCP server for AI clients. The
server uses stdio and the same Rust operation services as the GUI and CLI. It
does not require Python or Node.js.

## Install and configure

Install the GUI package once. The installer places the GUI and MCP executable
in the same stable directory. Add one server entry to the MCP client's
configuration and replace the example path with the installed path.

Windows:

```json
{
  "mcpServers": {
    "aio-asset-normalizer": {
      "command": "C:\\Program Files\\AIO Asset Normalizer\\aio-asset-normalizer-mcp.exe",
      "args": []
    }
  }
}
```

macOS, after moving the app into `/Applications`:

```json
{
  "mcpServers": {
    "aio-asset-normalizer": {
      "command": "/Applications/AIO Asset Normalizer.app/Contents/MacOS/aio-asset-normalizer-mcp",
      "args": []
    }
  }
}
```

Linux AppImage:

```json
{
  "mcpServers": {
    "aio-asset-normalizer": {
      "command": "/absolute/path/to/AIO-Asset-Normalizer.AppImage",
      "args": ["--mcp"]
    }
  }
}
```

The AppImage launcher dispatches `--mcp` to its bundled MCP executable. Keep
the configured path pointed at the AppImage file; its internal mount directory
is temporary. MCP configuration paths should be absolute and must refer to the
installed package rather than a Cargo `target` directory for normal use.

### Configure an AI client from the GUI

Open **Help → Connect AI** or use the **Connect AI** button at the right side of
the page navigation row. The dialog detects the MCP server shipped with the
running application, shows the resolved command and arguments, and generates
one setup prompt for the AI client currently in use. Select **Copy setup prompt**
and paste the result into that client's agent. The agent is asked to preserve
existing MCP settings, avoid replacing a conflicting server entry without
asking, reconnect the server, verify the required tools, and call the read-only
`session_list` tool.

To inspect app-side GUI session activity or stop an active GUI task, expand
**MCP diagnostics** in the dialog. This status only describes the application's
GUI bridge and does not confirm that an AI client has configured or connected
the MCP server.

Switch the dialog to **One-time use** when you want an agent to use MCP for the
current task without registering a persistent server. This prompt asks the
agent to launch the detected command directly as a temporary stdio child
process, negotiate MCP, inspect tool schemas, perform the requested task, and
close the process when finished. The agent can use an MCP client SDK already
available in its environment or a temporary Python 3 standard-library client;
the prompt does not ask it to install dependencies or edit client/project
configuration. The process must stay alive for related calls such as job
polling, then be closed. The agent must say if its execution sandbox cannot
launch or manage a local child process and must not silently switch to
persistent configuration.

The dialog reports **Copied** only when the system clipboard accepts the full
prompt. If clipboard access fails, select and copy the prompt from its preview.
The dialog does not confirm that the AI client has loaded the server; the agent
reports connection status after it verifies the tools. Some clients require a
restart before the server appears.

The launch data is resolved from the running installation each time the dialog
is opened or refreshed. Windows and macOS use the MCP executable beside the GUI;
Linux binary installations use the sibling MCP executable. Linux AppImages use
the outer `.AppImage` path with `--mcp`, never the temporary mount directory.
If the matching MCP executable or a stable AppImage path cannot be found, the
dialog explains the issue and disables copying until the path can be resolved.
Development builds use the MCP binary beside the GUI in the build output; move
or rebuild both binaries together before configuring an agent.

## Connection self-check

Restart or reconnect the MCP client after saving its configuration. Confirm
that initialization succeeds and `tools/list` contains `glb_inspect`,
`session_list`, and `session_capture`. Call `session_list`; it should return a
structured `sessions` array and must not open a window. Then call `session_open`
with `{}`. If no compatible GUI is running, this explicit call starts the GUI
from the same installation. Call `session_get_state` with the returned
`session_id`, then call `session_capture` with `scope: "viewport"`. The capture
result contains image content with MIME type `image/png` and structured frame,
revision, camera, animation, and preview metadata. The GUI's session panel shows
whether the bridge is listening or connected.

For a file-only smoke test, call `glb_inspect` with an absolute path to a known
GLB. The response includes a structured summary and catalog. To check structured
errors, use a missing absolute input path and confirm that the tool response
contains `isError: true` and an error object. MCP protocol messages use stdout;
diagnostics go through the app logging pipeline.

## Tool groups

| Tools | Behavior |
| --- | --- |
| `glb_inspect`, `glb_edit` | Inspect a disk GLB or start a typed edit job that writes an explicit output. |
| `bvh_inspect`, `bvh_process` | Inspect a disk BVH or start a trim and validation job. |
| `retarget` | Start a prompt, mapping suggestion, Mapping v2 validation, or disk retarget job. Suggestions never modify an explicit mapping. |
| `glb_batch` | Start preflight or export for a typed GLB batch. Job results include per-file outcomes and completed outputs. |
| `fbx_convert` | Start a conversion job for one FBX, OBJ, or Blend file through the fixed Blender profile. Blender is required only for this tool. |
| `session_list`, `session_open`, `session_get_state` | Discover, explicitly connect to, or start the GUI and read its state. |
| `session_glb`, `session_bvh`, `session_retarget`, `session_batch`, `session_converter` | Drive the corresponding page against the GUI's in-memory state, including edits, previews, and exports. |
| `session_view`, `session_capture` | Set the page, camera, overlays, GLB animation or BVH frame; capture a fresh viewport or window PNG. |
| `session_checkpoint` | Create, restore, or delete the single in-memory checkpoint for a GUI session. |
| `job_get`, `job_cancel` | Query progress and outcomes, or request cooperative cancellation. Include `session_id` for GUI jobs and omit it for file jobs. |

Every operation has a generated typed input schema. Paths passed to MCP file and
session tools must be absolute. File tools operate on disk and never start the
GUI. Session tools operate on a specific GUI session. A session export includes
its current pending edits. Session mutations use an `expected_revision`; read
the current value with `session_get_state` and submit that value with each
mutation. A stale revision returns `state_conflict` and does not apply the
request.

For example, open a GLB in the GUI session with this `session_glb` argument
shape (substitute the session identifier and file path):

```json
{
  "session_id": "session-id-from-session_open",
  "command": {
    "operation": "open_glb",
    "arguments": {
      "path": "/absolute/path/model.glb",
      "discard_changes": false,
      "expected_revision": 0
    }
  }
}
```

`session_view` accepts a typed `set_view` command. Camera `target` is a world
position; `radius` is positive, and `theta` and `phi` are radians. Use
`animation_index` and `animation_time` for GLB playback or `bvh_frame` for BVH
positioning. Set `playing` to pause or resume the active page. Every explicit
view change requires the current `expected_revision`.

Mutating file tools return a `job_id` immediately. Poll `job_get` with that ID
and omit `session_id`; GUI jobs require both the `session_id` and `job_id`. Wait
until status is `succeeded`, `failed`, or `cancelled`. Cancellation is
cooperative; outputs already committed atomically remain available and the job
result reports completed outputs. A cancelled or timed-out request is never
replayed automatically.

## Session lifecycle and safety

The GUI registers a random session ID and authentication token in the current
user's application data directory. The local bridge binds only to `127.0.0.1`,
uses length-prefixed JSON frames, checks the protocol and application version,
and rejects requests with an invalid token. The session manifest is removed
when the GUI exits. On Unix, the session directory and manifest use owner-only
permissions.

`session_open` without an ID reuses the only compatible session, starts the
installed GUI when none is available, or reports all candidates when multiple
sessions exist. Other session calls never start a window. Disconnecting the MCP
client leaves the GUI and its session jobs running. GUI shutdown requests
cooperative cancellation and waits for an active atomic output boundary before
exit; running tasks do not survive a GUI process restart.

A checkpoint stores the current in-memory documents, mappings, edit settings,
selections, and view state. Restoring pauses playback and rebuilds the preview.
It does not restore jobs or undo files that have already been written. The
bridge does not expose arbitrary shell commands, scripts, or general mouse and
keyboard control. It does not provide video streaming or remote access. A
cross-process lock serializes asset computations across installed GUI and MCP
processes for the current user; a conflicting computation returns `busy`.

## Troubleshooting

- If the client cannot start the server, verify its configured executable path
  and that the installed MCP and GUI binaries came from the same package.
- If `session_open` reports a version or protocol mismatch, update both binaries
  from one package and restart the GUI.
- If the tool reports `multiple_sessions`, call `session_list` and pass one
  returned `session_id` to `session_open`.
- If a session call times out, call `session_list` and `session_get_state`.
  Mutating requests are not retried automatically because the GUI may have
  completed them before the connection failed.
- If capture reports that the viewport is unavailable, switch to a page with a
  rendered viewport, restore a visible window, and request another capture.
- If conversion reports that Blender is unavailable, install Blender or set its
  explicit path in the Converter configuration. Other pages and file tools do
  not require Blender.
- If an output path is rejected, use an absolute path and verify that it does
  not replace a selected source file. Existing outputs require explicit
  overwrite authorization in the request.
