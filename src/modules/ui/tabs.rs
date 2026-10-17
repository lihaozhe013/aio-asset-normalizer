//! Shared segmented tab control.
//!
//! Used by the page header, the bottom dock, and inspector tab rows so all
//! tab-like navigation in the app looks and behaves identically.

use crate::modules::ui::theme;
use three_d::egui::{self, CornerRadius, Frame, Margin, RichText, Stroke};

/// A single entry of a segmented control: icon glyph plus localized label.
pub struct TabEntry {
    /// Phosphor icon glyph, e.g. `egui_phosphor::regular::CUBE`.
    pub icon: &'static str,
    pub label: String,
}

/// Draw a segmented control and return the index of a newly clicked entry.
///
/// The selected entry is accent-tinted; hover feedback comes from the
/// theme's widget visuals. The control never fires for the entry that is
/// already selected.
pub fn segmented(ui: &mut egui::Ui, entries: &[TabEntry], selected: usize) -> Option<usize> {
    let mut clicked = None;
    Frame::NONE
        .fill(theme::CARD)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::same(3))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.horizontal(|ui| {
                for (index, entry) in entries.iter().enumerate() {
                    let is_selected = index == selected;
                    let text = if is_selected {
                        RichText::new(format!("{} {}", entry.icon, entry.label)).strong()
                    } else {
                        RichText::new(format!("{} {}", entry.icon, entry.label)).weak()
                    };
                    let button = egui::Button::new(text)
                        .corner_radius(CornerRadius::same(4))
                        .fill(if is_selected {
                            theme::accent_dim()
                        } else {
                            egui::Color32::TRANSPARENT
                        })
                        .stroke(if is_selected {
                            Stroke::new(1.0, theme::BORDER)
                        } else {
                            Stroke::NONE
                        });
                    if ui.add(button).clicked() && !is_selected {
                        clicked = Some(index);
                    }
                }
            });
        });
    clicked
}
