# Config System: User Preferences Persistence (Design Doc)

**Status**: Implemented
**Date**: 2026-07-23

---

## 1. Motivation

All user-configurable state lives in memory and is lost on exit. A persistent config layer solves this transparently — no user-facing friction, no setup required.

## 2. Config Schema

### 2.1 YAML Structure (assets/config_template.yaml)

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

### 2.2 Rust Data Structures (src/modules/preferences.rs)

```rust
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
```

Each sub-struct derives `Default` as a safety net, but the template YAML is the single source of truth for default values.

### 2.3 Field Mapping

| Config Path | Runtime State | Default |
|---|---|---|
| `version` | schema version marker | 1 |
| `view.show_grid` | `canvas.show_grid` | true |
| `view.show_axes` | `canvas.show_axes` | true |
| `view.show_origin` | `canvas.show_origin` | true |
| `view.show_bones` | `canvas.show_bones` | true |
| `file_tree.show_all_files` | `file_tree.show_all_files` | false |
| `file_tree.last_opened_directory` | `file_tree.root` | null |
| `log_viewer.auto_scroll` | `log.auto_scroll` | true |
| `conversion.target_scale` | `config.target_scale` | 1.0 |
| `conversion.up_axis` | `config.up_axis` | `"Y"` |
| `conversion.script_version` | `config.script_version` | `"V1"` |
| `conversion.remove_unused_materials` | `config.remove_unused_materials` | true |
| `conversion.remove_cameras` | `config.remove_cameras` | true |
| `conversion.remove_lights` | `config.remove_lights` | true |
| `conversion.remove_loose_vertices` | `config.remove_loose_vertices` | false |
| `conversion.correct_bone_axes` | `config.correct_bone_axes` | true |
| `conversion.preserve_leaf_bones` | `config.preserve_leaf_bones` | true |
| `conversion.bake_animations` | `config.bake_animations` | true |

---

## 3. Architecture

### 3.1 Data Flow

```
STARTUP
=======
main.rs
  │
  └─ preferences::load()
       │
       ├─ include_str!("assets/config_template.yaml")   ← embedded at compile time
       ├─ read ~/.config/aio-asset-normalizer/config.yaml
       ├─ recursive merge (user overrides template)
       ├─ auto-migrate: write back if user config was missing keys
       └─ return UserPreferences

  └─ App::new(context, viewport, &prefs)
       │
       ├─ canvas.apply_view_prefs(&prefs.view)         ← ViewportCanvas method
       ├─ file_tree.apply_prefs(&prefs.file_tree)       ← FileTree method
       ├─ log.apply_prefs(&prefs.log_viewer)             ← LogViewer method
       └─ NormalizationConfig::from(&prefs.conversion)  ← From trait impl

  └─ render_loop { ... }


RUNTIME SAVE (dirty-flag mechanism)
====================================
Any state change sets app.needs_save = true:
  - dispatch_action arms (ToggleGrid, ImportFolder, ...)
  - file_tree render checkbox (show_all_files)
  - config_panel render (any drag/checkbox/combo)
  - log_viewer render checkbox (auto_scroll)
  - bone panel checkbox (show_bones)

At end of each render_ui call:
  if needs_save && !quit_requested:
      preferences::save(&self.collect_preferences())
      needs_save = false


EXIT
====
Ctrl+Q / File→Quit:
  └─ dispatch_action(Quit)
       ├─ preferences::save(&self.collect_preferences())
       └─ quit_requested = true

X button (window close):
  └─ three-d CloseRequested → ControlFlow::Exit
     └─ Final RedrawRequested callback fires our render loop
        └─ render_ui checks needs_save → saves if dirty
     └─ Loop exits
```

### 3.2 Delegation Pattern

app.rs does NOT perform field-by-field mapping of preferences. Each sub-component owns its own conversion logic:

| Component | Load (prefs → state) | Save (state → prefs) |
|---|---|---|
| `ViewportCanvas` | `canvas.apply_view_prefs(&prefs.view)` | `canvas.to_view_prefs()` |
| `FileTree` | `file_tree.apply_prefs(&prefs.file_tree)` | `file_tree.to_prefs()` |
| `LogViewer` | `log.apply_prefs(&prefs.log_viewer)` | `log.to_prefs()` |
| `NormalizationConfig` | `NormalizationConfig::from(&prefs.conversion)` | `ConversionPreferences::from(&config)` |

`collect_preferences()` in app.rs is reduced to 6 lines:

```rust
pub fn collect_preferences(&self) -> UserPreferences {
    UserPreferences {
        version: 1,
        view: self.canvas.to_view_prefs(),
        file_tree: self.file_tree.to_prefs(),
        log_viewer: self.log.to_prefs(),
        conversion: (&self.config).into(),
    }
}
```

### 3.3 Module Interface

`src/modules/preferences.rs` exposes two public functions:

- `pub fn load() -> UserPreferences` — load from disk, merge with template, auto-migrate
- `pub fn save(prefs: &UserPreferences)` — serialize and write to disk

No state, no struct wrapping. Pure functions operating on the config file.

### 3.4 Merge Strategy

Recursive merge on `serde_yaml::Value` (not on deserialized structs):

```
1. Deserialize template YAML  →  serde_yaml::Value  (full defaults)
2. Deserialize user YAML      →  serde_yaml::Value  (may miss keys)
3. For each key in user:
     If key exists in template and both values are Mapping → recurse
     Otherwise → override template[key] with user[key]
   Keys only in template remain as template defaults
4. If merged != user_value → write merged back (migrate in-place)
5. Deserialize merged Value → UserPreferences
```

The `version` field enables future schema-breaking migrations by branching before deserialization.

### 3.5 Save Strategy — Dirty Flag

Not real-time (user requirement). Not on Drop (no I/O in destructors). Two save points:

1. **render_ui polish phase**: After UI panels render each frame, if `needs_save` is set and app is not already quitting, save immediately and clear the flag.
2. **Quit action**: Save synchronously then set `quit_requested = true`.

The dirty flag covers the X-button close case: three-d fires one final `RedrawRequested` callback after `CloseRequested`, so `render_ui` runs and saves before the loop exits.

### 3.6 Config Path

Uses the `dirs` crate for cross-platform home directory:

```
Linux:   $HOME/.config/aio-asset-normalizer/config.yaml
macOS:   $HOME/.config/aio-asset-normalizer/config.yaml
Windows: %USERPROFILE%\.config\aio-asset-normalizer\config.yaml
```

Directory created recursively on first `load()` or `save()`.

### 3.7 Template File

`assets/config_template.yaml` — embedded at compile time via `include_str!`. Always available, never written back. Single source of truth for all config keys and default values.

---

## 4. File Manifest

### New Files

| File | Purpose |
|---|---|
| `assets/config_template.yaml` | Default config template, embedded in binary |
| `src/modules/preferences.rs` | Structs, `load()`, `save()`, `merge_yaml_values()` |

### Modified Files

| File | Changes |
|---|---|
| `Cargo.toml` | Added `serde_yaml`, `dirs` dependencies |
| `Cargo.lock` | Lockfile update from new deps |
| `src/modules/mod.rs` | `pub mod preferences` |
| `src/main.rs` | `preferences::load()` before `App::new()` |
| `src/app.rs` | Accepts `&UserPreferences` in `new()`; `collect_preferences()` delegates to sub-components; `needs_save` dirty flag; save-on-render-ui; save-on-quit |
| `src/modules/ui/config_panel.rs` | `impl From<&ConversionPreferences> for NormalizationConfig`; `impl From<&NormalizationConfig> for ConversionPreferences` |
| `src/modules/viewport/canvas.rs` | `apply_view_prefs()`, `to_view_prefs()` |
| `src/modules/ui/file_tree.rs` | `apply_prefs()`, `to_prefs()`, `root()` getter, `show_all_files` pub |
| `src/modules/ui/log_viewer.rs` | `apply_prefs()`, `to_prefs()`, `auto_scroll` pub, `render()` returns `bool` |
| `src/modules/ui/main_panel.rs` | Captures render return values, sets `needs_save` |

---

## 5. Enums / String Serialization

`NormalizationConfig` uses Rust enums (`UpAxis`, `ScriptVersion`); the YAML stores plain strings. Conversion uses `From` trait impls in `config_panel.rs`:

| Enum | YAML value | Fallback |
|---|---|---|
| `UpAxis::YUp` | `"Y"` | default (unknown → YUp) |
| `UpAxis::ZUp` | `"Z"` | |
| `ScriptVersion::V1` | `"V1"` | default (unknown → V1) |
| `ScriptVersion::V2` | `"V2"` | |

---

## 6. Future Extensibility

- **New config keys**: Add key to template + field to struct. Existing user configs auto-migrate via merge.
- **Deprecating keys**: Remove from template, stop reading in apply methods. Old YAML keys ignored silently.
- **Breaking schema**: Bump `version` to 2. Branch in `load()` before deserialization.
- **New config sections**: Add sub-struct + YAML section. Follow same delegation pattern.
