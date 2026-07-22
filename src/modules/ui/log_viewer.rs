use three_d::egui;

pub struct LogViewer {
    entries: Vec<String>,
    auto_scroll: bool,
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

    pub fn render(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("清空").clicked() {
                self.clear();
            }
            ui.checkbox(&mut self.auto_scroll, "自动滚动");
        });

        let text = if self.entries.is_empty() {
            "就绪，等待任务...".to_owned()
        } else {
            self.entries.join("\n")
        };

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(self.auto_scroll)
            .show(ui, |ui| {
                let label_text = egui::RichText::new(&text).monospace().weak();
                ui.label(label_text);
            });
    }
}
