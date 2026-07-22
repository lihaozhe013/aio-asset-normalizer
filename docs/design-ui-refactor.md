# UI Refactor: Three-Zone Layout (Design Doc)

**Status**: Draft  
**Date**: 2026-07-22  

---

## 1. Problem Statement

### Current State (`src/app.rs:346-395`)

All functionality is crammed into a single `SidePanel::left` + `ScrollArea` + 5 `CollapsingHeader` sections:

- Asset Import (collapsed by default)
- Conversion Config (collapsed by default)
- Bone Hierarchy (conditional)
- Animation Playback (conditional)
- Log Output (collapsed by default)

**Root cause**: The current layout groups unrelated concerns (file management, configuration, diagnostics) into one vertically-scrolling panel, forcing the user to constantly expand/collapse sections and scroll to find what they need.

### Target

A professional three-zone layout inspired by game engine editors (Unity, Unreal, Godot):

```
+-----------+-------------------+------------+
| FILE TREE |   3D VIEWPORT     | INSPECTOR  |
| (left)    |   (center)        | (right)    |
+-----------+-------------------+------------+
|              LOG OUTPUT (bottom)           |
+--------------------------------------------+
```

---

## 2. Layout Specification

### 2.1 Panel Structure (egui panels)

| Panel | egui Type | Default Size | Resizable | Min Size |
|---|---|---|---|---|
| File Tree | `SidePanel::left("file_tree")` | 250px | yes | 160px |
| 3D Viewport | `CentralPanel` (three-d canvas) | remainder | auto | 1px |
| Inspector | `SidePanel::right("inspector")` | 280px | yes | 200px |
| Log Output | `TopBottomPanel::bottom("log")` | 150px | yes | 80px |

### 2.2 Viewport Computation

Currently `compute_viewport(panel_width, ...)` only subtracts the left panel width. After refactor:

```
viewport.x      = left_panel_width * dpr
viewport.y      = 0
viewport.width  = window_width - (left + right) * dpr    (capped at min 1)
viewport.height = window_height - bottom_height * dpr    (capped at min 1)
```

`render_ui()` returns a `PanelLayout` struct instead of `f32`:

```rust
pub struct PanelLayout {
    pub left_width: f32,
    pub right_width: f32,
    pub bottom_height: f32,
}
```

---

## 3. Component Design

### 3.1 File Tree (`src/modules/ui/file_tree.rs` — new)

Replaces `file_list.rs`.

#### Data Structures

```rust
pub struct FileTree {
    root: Option<PathBuf>,
    root_entries: Option<Vec<FileTreeEntry>>,
    selected: HashSet<PathBuf>,
}

pub struct FileTreeEntry {
    name: String,
    path: PathBuf,
    is_dir: bool,
    children: Option<Vec<FileTreeEntry>>,  // None = not yet scanned
}
```

#### Lazy Loading Strategy

| Depth | Behavior |
|---|---|
| 0 (root children) | Scanned on `open_folder()` |
| 1 (grandchildren) | Also scanned on `open_folder()` (preload 2 levels) |
| 2+ (deeper) | `children = None`, scanned on first expand |

- `open_folder(path)` calls `scan_dir(path, max_depth: 2)` — recursive scan with depth limit.
- When user expands a directory whose `children == None`, it is appended to a `load_requests` queue.
- At end of frame, queued directories are scanned (`scan_dir(path, max_depth: 1)`) and their `children` is populated.
- Already-loaded children are never discarded when collapsing — subsequent expands are instant.

#### Rendering (Two-Pass)

**Pass 1 — Collect visible items** (read-only tree walk)  
Build a `Vec<FlatItem>` of currently visible nodes, respecting expand/collapse state from `egui::CollapsingState`.

```rust
struct FlatItem {
    depth: usize,
    name: String,
    path: PathBuf,
    is_dir: bool,
    is_loaded: bool,     // children have been scanned
    is_expanded: bool,   // from egui::CollapsingState
}
```

**Pass 2 — Render flat list**  
Loop over `FlatItem` list. For each:
- **Directory**: `CollapsingState::show_header()` with depth-indented arrow (`▾`/`▸`) + folder name.
- **File**: depth-indented row with `egui::Checkbox` on the left.

**Pass 3 — Process interactions**
- If a directory header was clicked to expand AND `is_loaded == false`, queue for lazy load.
- If a file checkbox value changed, update `self.selected`.
- Execute queued lazy loads after the render pass.

#### Selection UX

- Each file has a `Checkbox` on its left.
- Top of panel shows action buttons: `[全选] [取消全选] [反选]`.
- Summary label below: `已选 N / 共 M 个文件`.
- Public API: `selected_files() -> Vec<PathBuf>` for the conversion pipeline.

#### Drag & Drop

- If a folder is dropped, set it as the root and scan it.
- If files are dropped, add their parent folder as root and auto-select the dropped files.
- Retains compatibility with `handle_dropped_files(ctx)` pattern from `FileList`.

#### Menu Integration

- File > Import Folder → `open_folder()` via `rfd::pick_folder()`.
- File > Import Files → file dialog via `rfd::pick_file()`, set root to file's parent dir.
- File > Clear File List → `clear()`.

### 3.2 Inspector Panel

#### Layout (right `SidePanel`)

```
┌─ 格式转换 ──────────────────────┐
│  目标单位比例   [ 1.0     ]      │   DragValue (speed 0.1, 0.01..100.0)
│  目标朝向       [ Y-Up  ▾ ]      │   ComboBox
│  脚本版本       [ V1    ▾ ]      │   ComboBox
│                                  │
│  清理策略                         │
│    ☑ 移除未使用材质              │
│    ☑ 移除摄像机                   │
│    ☑ 移除灯光                     │
│    ☐ 移除孤立顶点                 │
│                                  │
│  (V2 only)                        │
│    ☑ 骨骼轴向校正                 │
│    ☑ 保留末端骨骼                 │
│    ☑ 烘焙动画关键帧               │
├──────────────────────────────────┤
│  ▸ 骨骼层级      (条件显示)      │   CollapsingHeader
├──────────────────────────────────┤
│  ▸ 动画播放      (条件显示)      │   CollapsingHeader
├──────────────────────────────────┤
│  [       开始转换               ]│   Always visible
└──────────────────────────────────┘
```

- `NormalizationConfig::render()` renamed to `render_inspector(ui)`.
- Renders full-width widget rows, no longer constrained by a nested collapsing section.
- Bone tree and animation controls are rendered inside `CollapsingHeader` sections within the inspector.
- "Start Conversion" button is outside any folding section, always visible at the bottom.

### 3.3 Log Viewer (`src/modules/ui/log_viewer.rs` — minor adjustments)

- Moved from a `CollapsingHeader` body into a standalone `TopBottomPanel`.
- `render(ui)` fills the full bottom panel width.
- Keep existing features: clear button, auto-scroll checkbox, `stick_to_bottom`.
- Default height 150px, resizable, min 80px.

### 3.4 Menu Bar (`src/modules/ui/menu_bar.rs` — no changes)

Stays at the top of the window, independent of the panel layout. All existing actions and keyboard shortcuts remain unchanged.

### 3.5 Font Loading (`src/modules/ui/fonts.rs` — no changes)

Font configuration currently happens inside the left panel closure (`src/app.rs:323-325`). Must be moved to the top of `render_ui()`, before any panel is created, so CJK fonts are available in all three panels.

---

## 4. Implementation Plan

### Phase 1: FileTree widget

1. Create `src/modules/ui/file_tree.rs`:
   - `FileTree::new()`, `FileTree::open_folder(path)` — sets root, scans 2 levels
   - `scan_dir(path, max_depth)` — recursive directory scan with depth cap, filters by extension
   - `collect_visible()` — tree walk respecting `CollapsingState`, returns `Vec<FlatItem>`
   - `render(ui)` — two-pass rendering with `CollapsingState`, checkboxes, selection buttons
   - `handle_dropped_files(ctx)` — drag & drop support
   - `selected_files() -> Vec<PathBuf>`, `clear()`
2. Update `src/modules/ui/mod.rs`: replace `pub mod file_list;` with `pub mod file_tree;`

### Phase 2: Inspector refactor

3. Refactor `src/modules/ui/config_panel.rs`:
   - Rename `render()` to `render_inspector(ui)`
   - Remove `CollapsingHeader` wrapper from field groups
   - Use `ui.separator()` and `RichText::heading()` for section labels
   - Adjust widget widths for ~280px inspector panel

### Phase 3: App layout rewrite

4. Refactor `src/app.rs::render_ui()`:
   - Move font config to top of method (before any panels)
   - Replace single `SidePanel::left` with three panels
   - `file_list: FileList` → `file_tree: FileTree`
   - Return `PanelLayout` instead of `f32`
   - Move bone tree and animation controls into right panel (inside `CollapsingHeader`)
   - Update `start_conversion()` to use `self.file_tree.selected_files()`
   - Update `dispatch_action()` for menu actions (ImportFiles, ImportFolder, ClearFileList)

### Phase 4: Viewport update

5. Update `src/main.rs`:
   - Replace `panel_width: f32` with `layout: PanelLayout`
   - Update `compute_viewport()` to consume all three panel dimensions
6. Remove `src/modules/ui/file_list.rs`

### Phase 5: Cleanup and verify

7. Run `cargo check` and `cargo clippy` to catch errors
8. Manual verification against the checklist below

---

## 5. File Change Summary

| File | Action | Scope |
|---|---|---|
| `src/modules/ui/file_tree.rs` | **CREATE** | New widget — full implementation |
| `src/modules/ui/mod.rs` | EDIT | `pub mod file_tree;` replaces `pub mod file_list;` |
| `src/modules/ui/file_list.rs` | **DELETE** | Replaced by file_tree |
| `src/modules/ui/config_panel.rs` | EDIT | `render()` → `render_inspector(ui)` |
| `src/modules/ui/log_viewer.rs` | EDIT | Minor sizing for bottom panel |
| `src/modules/ui/bone_tree.rs` | EDIT | Adjust indent for right panel width |
| `src/app.rs` | EDIT | Three panels, `PanelLayout`, `file_tree` field |
| `src/main.rs` | EDIT | `PanelLayout`, updated `compute_viewport` |
| `docs/design-ui-refactor.md` | THIS DOC | Design specification |

---

## 6. Verification Checklist

- [ ] `cargo check` passes with no errors
- [ ] `cargo clippy` passes with no new warnings
- [ ] Open folder → file tree populates with 2 levels preloaded
- [ ] Click expand on a depth=2 subdirectory → children load on next frame
- [ ] Click collapse on a loaded directory → hides children, expand is instant (no rescan)
- [ ] File checkbox toggles selection correctly
- [ ] Select all / Deselect all / Invert buttons work correctly
- [ ] Drag-drop a folder onto the window → sets root and scans
- [ ] Drag-drop files onto the window → sets root and selects them
- [ ] Config changes persist in inspector (scale, up axis, script version, cleanup)
- [ ] V2 options (bone axes, leaf bones, bake) appear/disappear when switching script version
- [ ] Conversion starts when files are selected and button is clicked
- [ ] Log output streams correctly in bottom panel during conversion
- [ ] 3D viewport renders in remaining space, not covered by any panel
- [ ] Window resize → panels resize within min/max, viewport recalculates correctly
- [ ] Menu bar actions (Import Folder, Clear, etc.) work with new file tree
- [ ] Keyboard shortcuts (Ctrl+O, Ctrl+Shift+O, Ctrl+R, Ctrl+G, etc.) continue to work
- [ ] Bone tree and animation controls render correctly in right-side inspector
- [ ] CJK fonts load correctly in all three panels
