use crate::modules::i18n::I18n;
use crate::modules::preferences::LogViewerPreferences;
use three_d::egui;

pub struct LogViewer {
    entries: Vec<String>,
    pub auto_scroll: bool,
}

impl LogViewer {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            auto_scroll: true,
        }
    }

    pub fn append(&mut self, line: &str) {
        self.entries.push(line.to_owned());
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn apply_prefs(&mut self, prefs: &LogViewerPreferences) {
        self.auto_scroll = prefs.auto_scroll;
    }

    pub fn to_prefs(&self) -> LogViewerPreferences {
        LogViewerPreferences {
            auto_scroll: self.auto_scroll,
        }
    }

    pub fn render(&mut self, ui: &mut egui::Ui, i18n: &I18n) -> bool {
        let mut prefs_changed = false;

        ui.horizontal(|ui| {
            if ui.button(i18n.tr("button.clear")).clicked() {
                self.clear();
            }
            if ui.button(i18n.tr("button.copy")).clicked() {
                let text = if self.entries.is_empty() {
                    i18n.tr("label.ready").to_owned()
                } else {
                    self.entries.join("\n")
                };
                ui.ctx().copy_text(text.clone());
                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                    clipboard.set_text(&text).ok();
                }
            }
            if ui
                .checkbox(&mut self.auto_scroll, i18n.tr("label.auto_scroll"))
                .changed()
            {
                prefs_changed = true;
            }
        });

        let text = if self.entries.is_empty() {
            i18n.tr("label.ready").to_owned()
        } else {
            self.entries.join("\n")
        };
        let mut text_ref: &str = &text;

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(self.auto_scroll)
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut text_ref)
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace),
                );
            });

        prefs_changed
    }
}
