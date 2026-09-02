# GLB Up-Axis Presets and Export Baking

## Purpose

Some third-party `.glb` assets are authored with a world up axis other than
the glTF standard (Y-Up, -Z forward). After opening, the model appears lying
down (for example `Maria W Orc Idle.glb`, whose character stands along +Z).
This feature lets the user declare the asset's authoring up axis in one step
and controls whether the correction is baked into the exported file.

## Design decisions

- Target convention is fixed to the glTF standard (right-handed Y-Up, -Z
  forward). There is no export path to Z-Up or other bases, and
  `StandardizationProfile` still rejects automatic axis detection
  (`"use a manual rotation before export"`).
- Presets are purely manual. The application never infers an axis convention
  from names, dimensions, or heuristics.
- A preset only fills the existing manual orientation value
  (`App::orientation_euler_degrees`). Preview, retarget coordination, and
  export baking reuse the existing root-transform pipeline unchanged, so no
  new document state or persistence is introduced.

## Presets

Each correction is a single-axis `+/-90` or `180` degree rotation applied to
every scene root (Euler order is irrelevant for single-axis rotations), is a
proper rotation (determinant `+1`), and never mirrors geometry:

| Preset                 | Authored up | Correction Euler (XYZ) |
| ---------------------- | ----------- | ---------------------- |
| Y-up (glTF standard)   | `+Y`        | `[0, 0, 0]`            |
| Z-up                   | `+Z`        | `[-90, 0, 0]`          |
| -Z-up                  | `-Z`        | `[90, 0, 0]`           |
| X-up                   | `+X`        | `[0, 0, 90]`           |
| -X-up                  | `-X`        | `[0, 0, -90]`          |
| Y-down (keeps -Z fwd)  | `-Y`        | `[0, 0, 180]`          |

The Inspector shows a `Custom` state when the current Euler angles no longer
match a preset (for example after further per-axis adjustment). Selecting a
preset overwrites the orientation triple; scale and translation are not
touched.

## Export baking switch

The Orientation section contains a `Bake corrections into exported GLB`
checkbox (default: enabled).

- Enabled: existing behaviour. Orientation, scale, and translation are baked
  into the export snapshot via `RotateRoots` / `ScaleRoots` / `TranslateRoots`.
- Disabled: the export snapshot skips all root-transform baking (the already
  existing `include_root_transform = false` path used by retarget source
  snapshots). Trim, animation rate, texture replacement, and Smart LOOP
  remain unaffected. The successful-export log then appends
  `(root transform not baked)` when a correction is active, so logs stay
  truthful.

The switch is session state like the other preview controls and is not
persisted to preferences.

## Components

- `src/modules/glb/orientation_presets.rs`: `UpAxisPreset` enum, preset to
  Euler mapping, reverse lookup for the `Custom` display, and unit tests
  (up-axis mapping, determinant, round trip).
- `src/modules/ui/main_panel.rs`: ComboBox presets and bake checkbox in the
  Orientation section.
- `src/app_export.rs`: `App::bake_root_transform` wiring and
  `App::root_transform_active` for truthful export logging.
- `src/modules/glb/mod.rs`: module declaration and `UpAxisPreset` re-export
  only; no responsibilities were added to the existing file.

## Verification

Automated: `cargo test` covers preset math and the include-root-transform
export paths. Manual: open a Z-up asset such as `Maria W Orc Idle.glb`,
select the `Z-up` preset, confirm the viewport shows the model standing,
toggle baking off, export, and reopen to confirm the root nodes are unchanged
while other edits still apply.
