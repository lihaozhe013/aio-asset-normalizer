# UI Theme Reference

All desktop UI colors come from `src/modules/ui/theme.rs`. Feature panels must
not construct ad-hoc `Color32` values; use the tokens below so contrast,
status semantics, and the viewport background stay consistent.

## Tokens

| Token | Value | Usage |
| --- | --- | --- |
| `BG` | `#141413` | 3D viewport clear color (via `VIEWPORT_CLEAR`) |
| `PANEL` | `#1A1918` | Side, bottom, and menu panel fills |
| `CARD` | `#201F1E` | Grouped surfaces: tab strips, dialog bodies |
| `RAISED` | `#292827` | Interactive widgets at rest (buttons) |
| `BORDER` | `#3B3937` | Hairline borders, separators, selected-tab stroke |
| `TEXT` | `#EDECEA` | Default body text |
| `TEXT_MUTED` | `#9D9B97` | Hints, metadata, secondary labels |
| `ACCENT` | `#FFB066` | Selection stroke, active tab tint (with `accent_dim()`) |
| `INFO` | `#79B8E8` | Informational messages and neutral notices |
| `SUCCESS` | `#57C97B` | Successful operations, valid states |
| `WARNING` | `#E7C24F` | Recoverable problems, validation warnings |
| `ERROR` | `#E06C6C` | Failures that stop the current operation |
| `ABOUT_ICON_BACKDROP` | `#F8F0E6` | About dialog icon backdrop (brand-fixed, light) |

All text tokens keep at least 4.5:1 contrast on `PANEL`.

## Rules

1. Never write `Color32::...` literals in UI modules; import `crate::modules::ui::theme`.
2. Status messages map to exactly one token: use `SUCCESS`/`WARNING`/`ERROR`;
   neutral hints use `INFO`, passive hints use `TEXT_MUTED`.
3. Tab-like navigation uses `tabs::segmented`; do not build new
   `selectable_label` tab rows.
4. Icon-only buttons need `.on_hover_text(...)` and a minimum 28 px hit size.
5. Empty viewport states use `empty_state::render`.
6. The viewport clear color must stay aligned with `BG` through
   `VIEWPORT_CLEAR` in `src/main.rs`.

## Style application

`theme::apply(ctx)` installs the full `egui::Style` (visuals, spacing, widget
states) once from the first rendered frame in `main_panel::render_ui`, next to
font setup. Adjust global look only there; per-widget colors still come from
the token constants.

## Localization

Every user-visible string must live in `assets/locales/en-US.yaml` and
`assets/locales/zh-CN.yaml` (key parity is enforced by
`modules::i18n` tests). Shared labels use the `common.*` namespace;
page-specific strings use their page namespace.
