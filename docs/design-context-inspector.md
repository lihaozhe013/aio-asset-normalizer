# Context-Sensitive Inspector & GLB Viewer (Design Doc)

**Status**: Draft  
**Date**: 2026-07-24  

---

## 1. Problem Statement

### Current State

The right-side inspector (`Panel::right("inspector")`) always shows the same content regardless of what is selected in the file tree:

```
┌─ 格式转换 ──────────────────────┐
│  (config form)                  │
│  (bone hierarchy - conditional) │
│  (animation controls)           │
│  [       开始转换               ]│
└─────────────────────────────────┘
```

When no files are selected the start button is disabled, but the config form takes up space pointlessly. When a GLB result file is selected, the user sees conversion config instead of anything relevant to the GLB model they want to inspect.

### Target

The inspector panel adopts a **context-sensitive** layout driven by the file tree selection. The user sees conversion tools when working with source assets, and model inspection tools when viewing GLB results:

```
Selection contains FBX/Blend/OBJ  →  转换 (Conversion) panel
Selection is pure GLB              →  检查 (Inspection) panel
Selection is empty                 →  Empty + hint text
```

For mixed selections (FBX + GLB both checked), the conversion panel wins — conversion is the action that modifies files, and the user can uncheck GLB entries to switch to inspection mode.

---

## 2. Panel Dispatch Logic

### 2.1 Selection → Panel Mapping

```rust
enum InspectorMode {
    Empty,
    Conversion,   // at least one FBX / Blend / OBJ in selection
    Inspection,   // selection is non-empty AND all files are GLB
}
```

Derived from `self.file_tree.selected_files()` each frame:

| Condition | Mode |
|---|---|
| `selected_files().is_empty()` | `Empty` |
| any selected file has extension != "glb" | `Conversion` |
| all selected files have extension == "glb" | `Inspection` |

`selected_files()` already returns only supported extensions (`.fbx`, `.blend`, `.obj`, `.glb`), so files with those extensions define the mode.

### 2.2 Auto-load GLB on Selection

When the inspector enters `Inspection` mode and the first GLB in the selection list is **not** the currently loaded model, trigger an auto-load:

```
if mode == Inspection && first_glb_path != app.active_glb_path {
    canvas.load_glb(context, &first_glb_path);
    skeleton = Skeleton::from_glb(&first_glb_path).ok();
    animation_player = AnimationPlayer::from_glb(...).ok();
    app.active_glb_path = Some(first_glb_path);
}
```

This reuses the existing load pipeline from `reload_model_if_needed()`. The difference: the GLB was not produced by conversion — it was selected directly from the file system.

---

## 3. Panel Layouts

### 3.1 Empty Mode

```
┌────────────────────────────────────┐
│                                    │
│      选择一个文件进行检查或转换      │
│                                    │
└────────────────────────────────────┘
```

Single centered `RichText` label in a weak color. No interactive widgets.

### 3.2 Conversion Mode (unchanged from current)

```
┌─ 格式转换 ──────────────────────┐
│  (NormalizationConfig form)     │
├─────────────────────────────────┤
│  ▸ 骨骼层级     (conditional)    │
├─────────────────────────────────┤
│  ▸ 动画播放     (conditional)    │
├─────────────────────────────────┤
│  [       开始转换               ]│
└─────────────────────────────────┘
```

No functional changes to this panel. The bone hierarchy and animation controls continue to render here because they are populated from the loaded model data on `App` (not from the selection).

### 3.3 Inspection Mode — GLB Inspector

```
┌─ 模型信息 ───────────────────────┐
│  文件: run.glb                   │
│  节点数: 12 | Mesh数: 3 | 骨骼数: 28 │
├──────────────────────────────────┤
│  ▸ 场景层级                      │
│    RootNode                      │
│      ▾ Armature                  │
│          ├ Bone_Spine            │
│          ├ Bone_Arm_L            │
│          └ ...                   │
│      ▸ Body_Mesh (2,048△)       │   click → highlight in viewport
│      ▸ Weapon_Mesh (512△)       │
│        👁 ☐                       │   eye toggle → show/hide in viewport
├──────────────────────────────────┤
│  ▸ 材质列表                      │
│    ├ Material_A (PBR)            │
│    ├ Material_B (PBR)            │
│    └ Material_C (Unlit)          │
├──────────────────────────────────┤
│  ▸ 动画片段                      │
│    ├ Idle (2.5s)                 │   click → play in viewport
│    ├ Walk (1.8s)                 │
│    └ Run (1.2s)                  │
├──────────────────────────────────┤
│  ▸ 转换设置 (read-only)           │   show config that produced this GLB,
│    scale: 1.0  up: Y  ver: V1    │   if embedded in extras
└──────────────────────────────────┘
```

The inspector is a `ScrollArea` containing 4–5 `CollapsingHeader` sections. Each section is collapsed by default except "场景层级".

#### Section details

**3.3.1 模型信息 (Model Info) — always visible**

| Row | Source |
|---|---|
| File name + size | `PathBuf::metadata()` |
| Node / Mesh / Bone counts | `gltf::Document` walk |
| Data type bar: node count \| mesh count \| bone count | same walk |

**3.3.2 场景层级 (Scene Hierarchy)**

A tree built from the GLB node graph:

- `gltf::Document::nodes()` gives the flat node list with `children()` and `mesh()` indices.
- Walk from root nodes, building a tree identical to the GLB scene graph.
- Each node row shows:
  - **Folder icon** if the node has children OR a mesh OR a skin.
  - **Mesh icon** if `node.mesh().is_some()` — shows `MeshName (N△)` where N is the triangle count.
  - **Bone icon** if the node is referenced by any skin's `joints()`.
  - **Eye toggle** (checkbox) — controls mesh visibility in the 3D viewport.
  - **Click to highlight** — clicking a mesh node sets `highlighted_mesh: Option<usize>` on the canvas, which renders that mesh with an outline or emissive tint.

Data source: `gltf::Document` parsed from the GLB file path. This is the same crate already used in `Skeleton::from_glb()` and `AnimationPlayer::from_glb()`.

**3.3.3 材质列表 (Materials)**

Flat list of all materials in the GLB:

- Material name (from `gltf::Material::name()`)
- Type: PBR, Unlit, or PBR Specular-Glossiness (from `pbr_metallic_roughness()` presence)
- Base color swatch (colored square widget)
- Texture count under the material

For V1, read-only display. Actual material editing is V2+.

**3.3.4 动画片段 (Animation Clips)**

But only if the GLB contains animations. This replaces the "动画播放" section from conversion mode:

- Lists all animation clips by name + duration.
- Click a clip name → play it in the 3D viewport (calls `animation_player.set_clip(i)` and `animation_player.toggle_play()`).
- Same playback controls as the current conversion panel (play/pause/stop/loop/speed).

This should share the `render_animation_controls()` function with the conversion panel — extract it to a standalone function that both panels call.

**3.3.5 转换设置 (Conversion Settings) — optional V2**

If the GLB was produced by this tool, the `extras` field of the GLB root may contain the conversion config JSON. Display it as a read-only key-value list. If not present, omit this section.

---

## 4. Data Flow Changes

### 4.1 App Struct — New Fields

```rust
pub struct App {
    // ... existing fields ...

    pub active_glb_path: Option<PathBuf>,  // path of the GLB currently loaded in canvas
    pub highlighted_mesh: Option<usize>,    // index of the mesh to highlight in viewport
    pub mesh_visibility: Vec<bool>,         // per-mesh visibility flags, synced to canvas
}
```

### 4.2 ViewportCanvas — New Fields & Methods

```rust
pub struct ViewportCanvas {
    // ... existing fields ...
    pub highlighted_mesh: Option<usize>,
    pub mesh_visible: Vec<bool>,
}
```

New method:
```rust
pub fn set_mesh_visibility(&mut self, index: usize, visible: bool)
```

During rendering, meshes with `mesh_visible[index] == false` are skipped. `highlighted_mesh` uses a different material (emissive tint or wireframe overlay).

How? `three-d`'s `Model<PhysicalMaterial>` has a `meshes` field that can be iterated. Each `Gm<Mesh, PhysicalMaterial>` can be rendered individually via `render_partially()` instead of rendering the whole model at once. This allows skipping individual meshes.

Alternative: For each hidden mesh, set its material's alpha to 0.0. This is simpler but not a real "skip rendering" — the GPU still processes those triangles. For the number of meshes in a typical game asset (1–20), this is acceptable.

### 4.3 Inspector Render Dispatch

`main_panel::render_ui()` changes:

```rust
let mode = determine_inspector_mode(app);

match mode {
    InspectorMode::Empty => {
        render_empty_inspector(app, ui);
    }
    InspectorMode::Conversion => {
        render_conversion_inspector(app, ui);
    }
    InspectorMode::Inspection => {
        render_inspection_inspector(app, ui);
    }
}
```

Three separate render functions. `render_conversion_inspector()` is the current inspector content extracted to a function. `render_inspection_inspector()` renders the GLB inspector via a new `glb_inspector` module.

---

## 5. New Modules

### 5.1 `src/modules/ui/glb_inspector.rs`

New file containing:

```rust
/// Renders the full GLB inspection panel inside a ScrollArea.
/// Called from main_panel when InspectorMode::Inspection.
pub fn render(app: &mut App, ui: &mut egui::Ui)
```

Delegates to private section renderers:

| Function | Section |
|---|---|
| `render_model_info(ui, &glb_doc, &path)` | 模型信息 |
| `render_scene_hierarchy(ui, &glb_doc, app)` | 场景层级 |
| `render_material_list(ui, &glb_doc)` | 材质列表 |
| `render_animation_clips(ui, app)` | 动画片段 |

The GLB document is parsed once per selection change (cached in `App`), not every frame.

### 5.2 Cached GLB Document

To avoid re-parsing the GLB every frame, store a parsed document on `App`:

```rust
pub struct GlbData {
    pub path: PathBuf,
    pub doc: gltf::Document,
    pub buffers: Vec<gltf::buffer::Data>,
    pub images: Vec<gltf::image::Data>,
    pub node_count: usize,
    pub mesh_count: usize,
    pub material_count: usize,
}
```

Refreshed only when `active_glb_path` changes.

---

## 6. Implementation Plan

### Phase 1: Core Infrastructure

1. **`app.rs`** — Add new fields:
   - `active_glb_path: Option<PathBuf>`
   - `glb_data: Option<GlbData>`
   - `highlighted_mesh: Option<usize>`
   - `mesh_visibility: Vec<bool>`

2. **`app.rs`** — Add method `load_glb_for_inspection(&mut self, path: &Path, context: &Context)`:
   - Calls `canvas.load_glb(context, path)`
   - Parses `gltf::Document` into `GlbData`
   - Extracts skeleton and animation (reuse existing `Skeleton::from_glb`, `AnimationPlayer::from_glb`)
   - Sets `active_glb_path`
   - Initializes `mesh_visibility` (all `true`)

3. **`app.rs`** — Add method `ensure_glb_loaded(&mut self, context: &Context)`:
   - If `active_glb_path != first_glb_in_selection`, call `load_glb_for_inspection()`

### Phase 2: Inspector Dispatch

4. **`main_panel.rs`** — Add `InspectorMode` enum and `determine_inspector_mode(app) -> InspectorMode`.

5. **`main_panel.rs`** — Extract current inspector content into `render_conversion_inspector(app, ui)`. Add `render_empty_inspector(ui)`.

6. **`main_panel.rs`** — Dispatch based on mode:
   ```rust
   match determine_inspector_mode(app) {
       Empty => render_empty_inspector(app, ui),
       Conversion => render_conversion_inspector(app, ui),
       Inspection => {
           app.ensure_glb_loaded(context);  // but context isn't available here
           render_inspection_inspector(app, ui);
       }
   }
   ```

   **Problem**: `render_ui()` does not have access to the three-d `Context`. It only has `egui::Ui`. The `Context` is available in `main.rs` but not in `main_panel.rs`. The GLB load needs the `Context`.

   **Solution A**: Pass `&Context` through `render_ui() -> render_inspection_inspector()`.

   **Solution B**: Defer the actual `canvas.load_glb()` call to `main.rs`'s render loop. In `main_panel`, just set a `needs_load_glb: Option<PathBuf>` flag on `App`. In `main.rs`, after `render_ui()` and before the 3D render pass, check the flag and load.

   **→ Use Solution B**. It follows the existing pattern: `needs_reload` is already a flag consumed in `main.rs` after UI. Add `needs_load_inspection_glb: Option<PathBuf>` that works the same way.

7. **`main.rs`** — Before the 3D render pass, add:
   ```rust
   if let Some(ref path) = app.needs_load_inspection_glb.take() {
       app.load_glb_for_inspection(context, path);
   }
   ```

### Phase 3: GLB Inspector UI

8. **Create `src/modules/ui/glb_inspector.rs`** — Implement section renderers:

   | Section | Implementation |
   |---|---|
   | 模型信息 | `gltf::Document::nodes().count()`, walk `meshes()`, walk `skins()` → counts |
   | 场景层级 | `gltf::Document::default_scene()` → walk node tree recursively. Each row: icon + name + stats. |
   | 材质列表 | `gltf::Document::materials()` iterate, show name + type + base color |
   | 动画片段 | Read from `app.animation_player.clips` (already parsed), reuse controls |

9. **`src/modules/ui/mod.rs`** — Add `pub mod glb_inspector;`

### Phase 4: Viewport Interaction

10. **`canvas.rs`** — Add `highlighted_mesh` and `mesh_visibility` fields.

11. **`canvas.rs`** — Modify the model render block in `main.rs` to iterate individual meshes:
    ```rust
    if let Some(ref model) = app.canvas.model {
        let visible_count = app.canvas.mesh_visibility.len();
        for (i, mesh_obj) in model.meshes.iter().enumerate() {
            if i < visible_count && !app.canvas.mesh_visibility[i] {
                continue;
            }
            // render mesh_obj individually
        }
    }
    ```

12. **GLB inspector → canvas bridge**: When user clicks eye toggle in inspector, update `app.canvas.mesh_visibility[i]`. When user clicks a mesh name, set `app.canvas.highlighted_mesh = Some(i)`.

### Phase 5: Cleanup

13. Run `cargo check` and fix all errors.
14. Verify: empty selection, FBX selection, GLB selection, multi-select, GLB auto-load, mesh visibility toggle.

---

## 7. File Change Summary

| File | Action | Scope |
|---|---|---|
| `src/app.rs` | EDIT | Add fields: `active_glb_path`, `glb_data`, `needs_load_inspection_glb`; add `load_glb_for_inspection()` |
| `src/modules/ui/main_panel.rs` | EDIT | `InspectorMode` enum, dispatch to three render functions, extract `render_conversion_inspector` |
| `src/modules/ui/glb_inspector.rs` | **CREATE** | GLB inspection panel: hierarchy tree, material list, animation clips |
| `src/modules/ui/mod.rs` | EDIT | `pub mod glb_inspector;` |
| `src/modules/viewport/canvas.rs` | EDIT | `highlighted_mesh`, `mesh_visibility`, per-mesh render control |
| `src/main.rs` | EDIT | Handle `needs_load_inspection_glb` flag; per-mesh rendering in the model pass |

No files are deleted. The existing conversion panel code is extracted but not removed.

---

## 8. Risks & Open Questions

| Risk | Mitigation |
|---|---|
| GLB text rendering in tree view (large hierarchies) | Use `ScrollArea` with fixed max height per section. Collapse non-root branches by default. |
| `three-d` `Model` meshes order may not match `gltf` crate mesh order | Verify: `three-d-asset` deserializes glTF nodes in document order. Mesh indices should align. If not, match by mesh name instead of index. |
| Mesh visibility via material alpha is not a true GPU cull | For ≤50 meshes, negligible perf impact. True per-mesh skip requires `render_partially()` per mesh, which adds draw call overhead. |
| GLB with embedded textures → large memory footprint | V1 only loads the GLB once. No texture caching needed yet. |
| "编辑" scope unclear | V1 is read-only inspection. Actual GLB file editing requires a write path (`gltf` crate can write but `three-d-asset` cannot round-trip) — defer to V2. |
