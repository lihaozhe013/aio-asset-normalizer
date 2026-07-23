# Config System: User Preferences Persistence (Design Doc)

**Status**: Draft
**Date**: 2026-07-23

---

## 1. Motivation

All user-configurable state currently lives in memory and is lost on exit. On every launch the user must re-toggle view helpers, re-enable auto-scroll, re-configure conversion parameters, and re-open their working directory. A persistent config layer solves this with zero user-facing friction.

---

## 2. Config Schema

### 2.1 YAML Structure

```yaml
version: 1
view:
  show_grid: true
  show_axes: true
  show_origin: true
  show_bones: true
file_tree:
  show_all_files: false
  last_opened_directory: null
log_viewer:
  auto_scroll: true
conversion:
  target_scale: 1.0
  up_axis: "Y"
  script_version: "V1"
  remove_unused_materials: true
  remove_cameras: true
  remove_lights: true
  remove_loose_vertices: false
  correct_bone_axes: true
  preserve_leaf_bones: true
  bake_animations: true
```

### 2.2 Field Mapping

| Config Path | App State | Type | Default |
|---|---|---|---|
| `version` | (schema version marker) | u32 | 1 |
| `view.show_grid` | `canvas.show_grid` | bool | true |
| `view.show_axes` | `canvas.show_axes` | bool | true |
| `view.show_origin` | `canvas.show_origin` | bool | true |
| `view.show_bones` | `canvas.show_bones` | bool | true |
| `file_tree.show_all_files` | `file_tree.show_all_files` | bool | false |
| `file_tree.last_opened_directory` | `file_tree.root` | Option\<String\> | null |
| `log_viewer.auto_scroll` | `log.auto_scroll` | bool | true |
| `conversion.target_scale` | `config.target_scale` | f32 | 1.0 |
| `conversion.up_axis` | `config.up_axis` | String (`"Y"`\|`"Z"`) | `"Y"` |
| `conversion.script_version` | `config.script_version` | String (`"V1"`\|`"V2"`) | `"V1"` |
| `conversion.remove_unused_materials` | `config.remove_unused_materials` | bool | true |
| `conversion.remove_cameras` | `config.remove_cameras` | bool | true |
| `conversion.remove_lights` | `config.remove_lights` | bool | true |
| `conversion.remove_loose_vertices` | `config.remove_loose_vertices` | bool | false |
| `conversion.correct_bone_axes` | `config.correct_bone_axes` | bool | true |
| `conversion.preserve_leaf_bones` | `config.preserve_leaf_bones` | bool | true |
| `conversion.bake_animations` | `config.bake_animations` | bool | true |

### 2.3 Rust Data Structures

```rust
// src/modules/preferences.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPreferences {
    pub version: u32,
    #[serde(default)]
    pub view: ViewPreferences,
    #[serde(default)]
    pub file_tree: FileTreePreferences,
    #[serde(default)]
    pub log_viewer: LogViewerPreferences,
    #[serde(default)]
    pub conversion: ConversionPreferences,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewPreferences {
    pub show_grid: bool,
    pub show_axes: bool,
    pub show_origin: bool,
    pub show_bones: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTreePreferences {
    pub show_all_files: bool,
    pub last_opened_directory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogViewerPreferences {
    pub auto_scroll: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversionPreferences {
    pub target_scale: f32,
    pub up_axis: String,
    pub script_version: String,
    pub remove_unused_materials: bool,
    pub remove_cameras: bool,
    pub remove_lights: bool,
    pub remove_loose_vertices: bool,
    pub correct_bone_axes: bool,
    pub preserve_leaf_bones: bool,
    pub bake_animations: bool,
}
```

**Default implementations**: Each field-level default is defined in `assets/config_template.yaml`. At runtime, the template YAML is deserialized to produce a fully-populated `UserPreferences` with `serde` field defaults as a safety net.

---

## 3. Architecture

### 3.1 Layering

```
main.rs startup sequence:

  1. preferences::load()
       │
       ├─ include_str!("assets/config_template.yaml")   ← template (compiled in)
       ├─ Read ~/.config/aio-asset-normalizer/config.yaml  ← user overrides
       ├─ Merge: user values overlay template defaults
       └─ Return UserPreferences
       
  2. App::new(context, viewport, &preferences)
       │
       ├─ Apply prefs.view.*        → canvas.show_*
       ├─ Apply prefs.file_tree.*   → file_tree (show_all_files, open_folder)
       ├─ Apply prefs.log_viewer.*  → log.auto_scroll
       └─ Apply prefs.conversion.*  → config (NormalizationConfig fields)
       
  3. render_loop { ... }

main.rs shutdown path (quit_requested == true):

  4. app.collect_preferences()       ← gather current state
  5. preferences::save(&prefs)       ← write to disk
```

### 3.2 Module Interface

`src/modules/preferences.rs` exposes exactly two public functions:

```rust
/// Load user preferences from disk, merging with built-in template defaults.
/// Creates the config directory and file on first run.
/// Auto-migrates: if user's config is missing keys present in template,
/// writes the merged result back.
pub fn load() -> UserPreferences;

/// Serialize current preferences and write to the user config path.
/// Called once on graceful exit.
pub fn save(prefs: &UserPreferences);
```

### 3.3 Merge Strategy (Version Iteration Compatibility)

The `load()` function implements a recursive merge:

```
1. Deserialize template YAML  →  serde_yaml::Value  (full defaults)
2. Deserialize user YAML      →  serde_yaml::Value  (partial, may miss new keys)
3. Recursive merge:
   - For each key in user:
     - If key exists in template AND both values are Mapping:
         recurse, merging user subtree into template subtree
     - Otherwise:
         override template[key] = user[key]  (scalar or list)
   - Keys in template but NOT in user remain as template defaults
4. If merged ≠ user_value:
     Write merged YAML back to user config path  (migrate in-place)
5. Deserialize merged Value → UserPreferences
```

**Example migration**: v0.2 adds `view.show_normals` to template. User's v0.1 config is missing it. The merge produces a config with `show_normals: true` from template plus all the user's existing overrides. The merged result is written back so the user file is always complete.

**Schema versions**: The `version` field in the YAML acts as a schema version marker. If in the future a breaking format change is needed, the loader can branch on `version` before deserialization.

### 3.4 Save Strategy

- **When**: Only on normal exit (`quit_requested == true`), immediately before the render loop returns `FrameOutput { exit: true }`.
- **Not real-time**: No writes on individual checkbox toggles or field edits.
- **Crash tolerance**: Explicitly out of scope per project requirements. Crashed sessions lose changes made in that session.

### 3.5 Config Path

```
Linux:   $HOME/.config/aio-asset-normalizer/config.yaml
macOS:   $HOME/.config/aio-asset-normalizer/config.yaml
Windows: %USERPROFILE%\.config\aio-asset-normalizer\config.yaml
```

Uses the `dirs` crate for cross-platform home directory resolution (not yet a dependency, to be added). The config directory is created recursively if it does not exist on first `load()` or `save()`.

### 3.6 Template File

Location: `assets/config_template.yaml`

It is embedded into the binary at compile time via `include_str!` so it is always available regardless of the working directory or whether the user has deleted the config directory. This is the single source of truth for every config key and its default value.

The file is **not** written back to `assets/` at any point. It is read-only at build time.

---

## 4. Implementation Plan

### 4.1 New Dependencies

Add to `Cargo.toml`:

```toml
serde_yaml = "0.9"
dirs = "5"
```

### 4.2 New Files

| File | Purpose |
|---|---|
| `assets/config_template.yaml` | Default config template (embedded via `include_str!`) |
| `src/modules/preferences.rs` | `UserPreferences` struct, `load()`, `save()`, merge logic |

### 4.3 Modified Files

| File | Changes |
|---|---|
| `src/modules/mod.rs` | Add `pub mod preferences;` |
| `src/main.rs` | (a) Call `preferences::load()` before `App::new()`; (b) Call `preferences::save()` before returning exit |
| `src/app.rs` | (a) `App::new()` accepts `&UserPreferences` and initializes sub-components; (b) Add `pub fn collect_preferences(&self) -> UserPreferences`; (c) Adjust `dispatch_action(Quit)` to trigger save |
| `src/modules/ui/log_viewer.rs` | Make `auto_scroll` field `pub` |
| `src/modules/ui/file_tree.rs` | Make `show_all_files` field `pub` |

### 4.4 app.rs API Changes

```rust
// Before:
pub fn new(context: &Context, viewport: Viewport) -> Self

// After:
pub fn new(context: &Context, viewport: Viewport, prefs: &UserPreferences) -> Self
```

`App::new()` responsibilities extended:

1. Initialize `ViewportCanvas` with default `show_* = true`, then override from `prefs.view.*`
2. Initialize `FileTree` with defaults, then set `show_all_files` and optionally `open_folder()` from `prefs.file_tree`
3. Initialize `LogViewer` with defaults, then set `auto_scroll` from `prefs.log_viewer`
4. Initialize `NormalizationConfig` with defaults, then override all 9 fields from `prefs.conversion`

```rust
/// Gather current runtime state into a UserPreferences for saving.
/// Called only on graceful exit.
pub fn collect_preferences(&self) -> UserPreferences {
    UserPreferences {
        version: 1,
        view: ViewPreferences {
            show_grid: self.canvas.show_grid,
            show_axes: self.canvas.show_axes,
            show_origin: self.canvas.show_origin,
            show_bones: self.canvas.show_bones,
        },
        file_tree: FileTreePreferences {
            show_all_files: self.file_tree.show_all_files,
            last_opened_directory: self.file_tree.root().map(|p| p.to_string_lossy().to_string()),
        },
        log_viewer: LogViewerPreferences {
            auto_scroll: self.log.auto_scroll,
        },
        conversion: ConversionPreferences {
            target_scale: self.config.target_scale,
            up_axis: match self.config.up_axis { YUp => "Y", ZUp => "Z" }.to_string(),
            script_version: match self.config.script_version { V1 => "V1", V2 => "V2" }.to_string(),
            remove_unused_materials: self.config.remove_unused_materials,
            remove_cameras: self.config.remove_cameras,
            remove_lights: self.config.remove_lights,
            remove_loose_vertices: self.config.remove_loose_vertices,
            correct_bone_axes: self.config.correct_bone_axes,
            preserve_leaf_bones: self.config.preserve_leaf_bones,
            bake_animations: self.config.bake_animations,
        },
    }
}
```

`FileTree` needs a new getter:

```rust
// In impl FileTree
pub fn root(&self) -> Option<&PathBuf> {
    self.root.as_ref()
}
```

### 4.5 main.rs Changes

```rust
fn main() {
    // --- Layer 1: Load config before app starts ---
    let prefs = modules::preferences::load();

    let window = Window::new(WindowSettings { /* ... */ }).expect("...");
    let context = window.gl();
    let mut gui = three_d::GUI::new(&context);
    let mut app = App::new(&context, window.viewport(), &prefs);

    window.render_loop(move |mut frame_input| {
        // ... existing UI + render code unchanged ...

        // --- Save before returning exit ---
        if app.quit_requested() {
            app.collect_preferences_and_save();  // or inline the save call
        }

        FrameOutput {
            exit: app.quit_requested(),
            ..Default::default()
        }
    });
}
```

Since the render loop closure takes ownership of `app`, the save must happen inside the closure. Two options:

**Option A — Save inside the closure on the exit frame** (recommended):
```rust
let exit = app.quit_requested();
if exit {
    modules::preferences::save(&app.collect_preferences());
}
FrameOutput { exit, ..Default::default() }
```

**Option B — App owns the save**:
```rust
// In dispatch_action:
MenuAction::Quit => {
    self.quit_requested = true;
    self.save_preferences();  // internal method
}
```

Option B is simpler and keeps `main.rs` minimal. However, it couples `app.rs` to the preferences module. Since `app.rs` already calls `preferences::save()`, this coupling is acceptable and architecturally clean (App is the state owner, it decides when to persist).

**Final choice**: Option B for minimal `main.rs` changes.

### 4.6 Drop Behavior

No `Drop` implementation on `App`. Save is explicit in the Quit action handler. This makes the save timing deterministic and avoids side-effect surprises from `Drop` ordering.

---

## 5. Up Axis / Script Version Mapping

`NormalizationConfig` uses Rust enums; the YAML stores strings. Conversion logic:

```rust
// preferences → NormalizationConfig (on load)
let up_axis = match prefs.conversion.up_axis.as_str() {
    "Z" => UpAxis::ZUp,
    _   => UpAxis::YUp,
};
let script_version = match prefs.conversion.script_version.as_str() {
    "V2" => ScriptVersion::V2,
    _    => ScriptVersion::V1,
};

// NormalizationConfig → preferences (on save)
let up_axis = match self.config.up_axis {
    UpAxis::YUp => "Y",
    UpAxis::ZUp => "Z",
};
let script_version = match self.config.script_version {
    ScriptVersion::V1 => "V1",
    ScriptVersion::V2 => "V2",
};
```

Unknown string values silently fall back to the default variant (`YUp` / `V1`).

---

## 6. Future Extensibility

The merge-based architecture supports these additions without migration scripts:

- **New config keys**: Add the key to `assets/config_template.yaml` and the corresponding field to the Rust struct with a `#[serde(default)]` attribute. Existing user configs get the template default automatically.
- **Deprecating keys**: Remove the key from the template and stop reading the field in `App::new()`. Old user configs will still have the key but it is ignored (serde skips unknown fields by default).
- **Breaking schema changes**: Bump `version` to 2 in the template. In `load()`, branch on `version` before deserializing, apply a conversion function, and save as version 2.
- **Additional config sections**: Add a new top-level struct and YAML section. Follow the same pattern — template defines defaults, merge fills gaps.

---

## 7. Verification Checklist

After implementation, verify:

- [ ] First launch (no config file): template defaults written to `~/.config/aio-asset-normalizer/config.yaml`
- [ ] Subsequent launch: user-preferred values restored correctly
- [ ] View toggles persist: toggle Grid/Axes/Origin/Bones off, restart, verify state
- [ ] Show all files persists: enable, restart, verify
- [ ] Auto scroll persists: disable, restart, verify
- [ ] Conversion settings persist: change scale/axis/cleanup options, restart, verify
- [ ] Last opened directory persists: open a folder, restart, file tree shows that folder
- [ ] Null last_opened_directory: first launch, no folder opened, no crash
- [ ] Version migration: manually edit config.yaml to remove a key (e.g. `show_bones`), restart, verify key is added back with template default, rest of config unchanged
- [ ] Non-existent config directory: delete `~/.config/aio-asset-normalizer/`, restart, directory recreated automatically
- [ ] Invalid YAML in user config: corrupt the file, restart, app loads template defaults, does not crash
