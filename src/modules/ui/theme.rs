//! Central design tokens for the desktop UI.
//!
//! Every color used by a UI module must come from this file; ad-hoc
//! `Color32` literals in feature panels are not allowed. The palette is a
//! neutral near-black base with a single light-orange accent so status
//! colors stay unambiguous, and all text colors keep at least 4.5:1
//! contrast on the panel background.

use three_d::egui::{self, Color32, Context, CornerRadius, Stroke, Vec2};

/// Window background behind the 3D viewport.
pub const BG: Color32 = Color32::from_rgb(0x0B, 0x0B, 0x0E);
/// Fill of side, bottom, and menu panels.
pub const PANEL: Color32 = Color32::from_rgb(0x12, 0x12, 0x16);
/// Fill of grouped surfaces such as tab strips and dialogs.
pub const CARD: Color32 = Color32::from_rgb(0x17, 0x17, 0x1C);
/// Fill of raised interactive widgets at rest.
pub const RAISED: Color32 = Color32::from_rgb(0x1F, 0x1F, 0x26);
/// Hairline borders and separators.
pub const BORDER: Color32 = Color32::from_rgb(0x2E, 0x2E, 0x38);
/// Default body text.
pub const TEXT: Color32 = Color32::from_rgb(0xEC, 0xEC, 0xEF);
/// Secondary text such as hints and metadata.
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x9B, 0x9B, 0xA4);
/// The single accent color: selection, active tabs, primary actions.
pub const ACCENT: Color32 = Color32::from_rgb(0xFF, 0xB0, 0x66);

/// Translucent accent used as the selected-state fill.
pub fn accent_dim() -> Color32 {
    Color32::from_rgba_unmultiplied(0xFF, 0xB0, 0x66, 0x3C)
}
/// Informational messages.
pub const INFO: Color32 = Color32::from_rgb(0x79, 0xB8, 0xE8);
/// Successful operations.
pub const SUCCESS: Color32 = Color32::from_rgb(0x57, 0xC9, 0x7B);
/// Recoverable problems and validation warnings. Leans yellow to stay
/// distinguishable from the orange accent.
pub const WARNING: Color32 = Color32::from_rgb(0xE7, 0xC2, 0x4F);
/// Failures that stop the current operation.
pub const ERROR: Color32 = Color32::from_rgb(0xE0, 0x6C, 0x6C);

/// Deep background for text fields and slider rails.
const FIELD: Color32 = Color32::from_rgb(0x08, 0x08, 0x0B);
/// Subtle row hover highlight.
const FAINT: Color32 = Color32::from_rgb(0x10, 0x10, 0x14);
/// Widget fill on hover.
const RAISED_HOVER: Color32 = Color32::from_rgb(0x28, 0x28, 0x32);

/// Warm backdrop behind the app icon in the About dialog. The icon artwork
/// is designed for a light surface, so this stays brand-specific instead of
/// following the dark theme.
pub const ABOUT_ICON_BACKDROP: Color32 = Color32::from_rgb(0xF8, 0xF0, 0xE6);
/// Hairline around the About dialog icon backdrop.
pub const ABOUT_ICON_BACKDROP_STROKE: Color32 =
    Color32::from_rgb(0xE0, 0xC6, 0xB0);

/// Normalized RGBA for the 3D viewport clear color, matching [`BG`].
pub const VIEWPORT_CLEAR: [f32; 4] = [0.0431, 0.0431, 0.0549, 1.0];

/// Install the application style on the egui context.
///
/// Called once from the first rendered frame, mirroring font setup.
pub fn apply(ctx: &Context) {
    let mut style = egui::Style::default();
    apply_to_style(&mut style);
    ctx.set_global_style(style);
}

fn apply_to_style(style: &mut egui::Style) {
    let visuals = &mut style.visuals;
    visuals.dark_mode = true;
    visuals.panel_fill = PANEL;
    visuals.window_fill = CARD;
    visuals.window_stroke = Stroke::new(1.0, BORDER);
    visuals.window_corner_radius = CornerRadius::same(8);
    visuals.menu_corner_radius = CornerRadius::same(6);
    visuals.selection.bg_fill = accent_dim();
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    visuals.hyperlink_color = INFO;
    visuals.warn_fg_color = WARNING;
    visuals.error_fg_color = ERROR;
    visuals.extreme_bg_color = FIELD;
    visuals.faint_bg_color = FAINT;

    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    for state in [
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        state.weak_bg_fill = RAISED;
        state.corner_radius = CornerRadius::same(5);
    }
    visuals.widgets.hovered.weak_bg_fill = RAISED_HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.active.weak_bg_fill = accent_dim();
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, TEXT);

    let spacing = &mut style.spacing;
    spacing.item_spacing = Vec2::new(8.0, 6.0);
    spacing.button_padding = Vec2::new(10.0, 4.0);
}
