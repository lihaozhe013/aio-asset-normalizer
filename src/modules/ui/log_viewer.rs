use std::collections::VecDeque;
use std::path::Path;

use crate::modules::i18n::I18n;
use crate::modules::logging::{LogEvent, LogLevel, LogTarget};
use crate::modules::preferences::LogViewerPreferences;
use three_d::egui;

const MAX_VISIBLE_SESSION_ENTRIES: usize = 5_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TargetFilter {
    All,
    GlbEditor,
    GlbExport,
    BvhStudio,
    Retarget,
    FbxConverter,
}

impl TargetFilter {
    fn label_key(self) -> &'static str {
        match self {
            Self::All => "log.target_all",
            Self::GlbEditor => "log.target_glb_editor",
            Self::GlbExport => "log.target_glb_export",
            Self::BvhStudio => "log.target_bvh_studio",
            Self::Retarget => "log.target_retarget",
            Self::FbxConverter => "log.target_fbx_converter",
        }
    }

    fn matches(self, target: LogTarget) -> bool {
        match self {
            Self::All => true,
            Self::GlbEditor => target == LogTarget::GlbEditor,
            Self::GlbExport => target == LogTarget::GlbExport,
            Self::BvhStudio => target == LogTarget::BvhStudio,
            Self::Retarget => matches!(
                target,
                LogTarget::Retarget
                    | LogTarget::GlbRetarget
                    | LogTarget::RetargetAgent
            ),
            Self::FbxConverter => target == LogTarget::FbxConverter,
        }
    }
}

pub struct LogViewer {
    entries: VecDeque<LogEvent>,
    pub auto_scroll: bool,
    target_filter: TargetFilter,
    minimum_level: LogLevel,
    storage_available: bool,
}

impl LogViewer {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            auto_scroll: true,
            target_filter: TargetFilter::All,
            minimum_level: LogLevel::Debug,
            storage_available: true,
        }
    }

    pub fn push(&mut self, event: LogEvent) {
        if event.target == LogTarget::App
            && event.level == LogLevel::Error
            && (event.message.starts_with("Log file ")
                || event.message.starts_with("Log writer thread "))
        {
            self.storage_available = false;
        }
        if self.entries.len() >= MAX_VISIBLE_SESSION_ENTRIES {
            self.entries.pop_front();
        }
        self.entries.push_back(event);
    }

    fn clear(&mut self) {
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

    pub fn render(
        &mut self,
        ui: &mut egui::Ui,
        i18n: &I18n,
        log_dir: &Path,
    ) -> bool {
        let mut prefs_changed = false;

        ui.horizontal(|ui| {
            if ui.button(i18n.tr("button.clear")).clicked() {
                self.clear();
            }
            if ui.button(i18n.tr("button.copy")).clicked() {
                let text = self.visible_lines().join("\n");
                let text = if text.is_empty() {
                    i18n.tr("label.ready").to_owned()
                } else {
                    text
                };
                ui.ctx().copy_text(text.clone());
                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                    clipboard.set_text(&text).ok();
                }
            }
            if ui.button(i18n.tr("log.open_directory")).clicked() {
                open::that(log_dir).ok();
            }
            if ui
                .checkbox(&mut self.auto_scroll, i18n.tr("label.auto_scroll"))
                .changed()
            {
                prefs_changed = true;
            }
        });
        if !self.storage_available {
            ui.colored_label(
                egui::Color32::YELLOW,
                i18n.tr("log.storage_unavailable"),
            );
        }

        ui.horizontal(|ui| {
            ui.label(i18n.tr("log.target"));
            egui::ComboBox::from_id_salt("debug_log_target")
                .selected_text(i18n.tr(self.target_filter.label_key()))
                .show_ui(ui, |ui| {
                    for filter in [
                        TargetFilter::All,
                        TargetFilter::GlbEditor,
                        TargetFilter::GlbExport,
                        TargetFilter::BvhStudio,
                        TargetFilter::Retarget,
                        TargetFilter::FbxConverter,
                    ] {
                        ui.selectable_value(
                            &mut self.target_filter,
                            filter,
                            i18n.tr(filter.label_key()),
                        );
                    }
                });
            ui.label(i18n.tr("log.minimum_level"));
            egui::ComboBox::from_id_salt("debug_log_level")
                .selected_text(level_label(self.minimum_level, i18n))
                .show_ui(ui, |ui| {
                    for level in [
                        LogLevel::Debug,
                        LogLevel::Info,
                        LogLevel::Warn,
                        LogLevel::Error,
                    ] {
                        ui.selectable_value(
                            &mut self.minimum_level,
                            level,
                            level_label(level, i18n),
                        );
                    }
                });
        });

        let text = self.visible_lines().join("\n");
        let text = if text.is_empty() {
            i18n.tr("label.ready").to_owned()
        } else {
            text
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

    fn visible_lines(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter(|event| {
                self.target_filter.matches(event.target)
                    && event.level >= self.minimum_level
            })
            .map(LogEvent::format_line)
            .collect()
    }
}

fn level_label(level: LogLevel, i18n: &I18n) -> &str {
    match level {
        LogLevel::Debug => i18n.tr("log.level_debug"),
        LogLevel::Info => i18n.tr("log.level_info"),
        LogLevel::Warn => i18n.tr("log.level_warn"),
        LogLevel::Error => i18n.tr("log.level_error"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(target: LogTarget, level: LogLevel, message: &str) -> LogEvent {
        LogEvent {
            timestamp: std::time::UNIX_EPOCH,
            target,
            level,
            task_id: None,
            stream: None,
            fields: Vec::new(),
            message: message.to_owned(),
        }
    }

    #[test]
    fn session_history_is_bounded() {
        let mut viewer = LogViewer::new();
        for index in 0..=MAX_VISIBLE_SESSION_ENTRIES {
            viewer.push(event(
                LogTarget::App,
                LogLevel::Info,
                &format!("event-{index}"),
            ));
        }

        assert_eq!(viewer.entries.len(), MAX_VISIBLE_SESSION_ENTRIES);
        assert_eq!(
            viewer.entries.front().map(|entry| entry.message.as_str()),
            Some("event-1")
        );
    }

    #[test]
    fn filters_apply_target_and_minimum_level() {
        let mut viewer = LogViewer::new();
        viewer.push(event(LogTarget::GlbEditor, LogLevel::Info, "editor"));
        viewer.push(event(LogTarget::GlbExport, LogLevel::Info, "export-info"));
        viewer.push(event(LogTarget::GlbExport, LogLevel::Error, "export"));
        viewer.target_filter = TargetFilter::GlbExport;
        viewer.minimum_level = LogLevel::Warn;

        let lines = viewer.visible_lines();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("export"));
    }

    #[test]
    fn clear_only_removes_session_view() {
        let mut viewer = LogViewer::new();
        viewer.push(event(LogTarget::App, LogLevel::Info, "event"));
        viewer.clear();
        assert!(viewer.entries.is_empty());
    }
}
