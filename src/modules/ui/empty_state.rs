//! Centered empty-state hint for the 3D viewport.
//!
//! Rendered inside the central panel when the active page has no content.
//! Only the small hint block intercepts pointer input, so camera orbiting
//! outside it keeps working.

use crate::modules::ui::theme;
use three_d::egui::{self, RichText};

/// Draw a centered icon, title, and hint inside the remaining space.
///
/// `action_label` renders an optional button; returns `true` when clicked.
pub fn render(
    ui: &mut egui::Ui,
    icon: &str,
    title: &str,
    hint: &str,
    action_label: Option<&str>,
) -> bool {
    let mut clicked = false;
    ui.centered_and_justified(|ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(24.0);
            ui.label(RichText::new(icon).size(42.0).color(theme::TEXT_MUTED));
            ui.add_space(4.0);
            ui.label(RichText::new(title).strong().size(16.0));
            ui.label(RichText::new(hint).small().color(theme::TEXT_MUTED));
            if let Some(label) = action_label {
                ui.add_space(8.0);
                if ui.button(label).clicked() {
                    clicked = true;
                }
            }
        });
    });
    clicked
}
