use std::path::PathBuf;
use three_d::egui;

pub struct FileList {
    files: Vec<PathBuf>,
    path_input: String,
}

impl FileList {
    pub fn new() -> Self {
        Self {
            files: Vec::new(),
            path_input: String::new(),
        }
    }

    pub fn files(&self) -> &[PathBuf] {
        &self.files
    }

    pub fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for file in dropped.iter() {
            if let Some(path) = &file.path {
                if self.is_supported(path) && !self.files.contains(path) {
                    self.files.push(path.clone());
                }
            }
        }
    }

    pub fn render(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let browse = ui.button("浏览...").clicked();
            ui.add(
                egui::TextEdit::singleline(&mut self.path_input)
                    .hint_text("文件路径或拖拽文件到此处"),
            );
            let add_clicked = ui.button("添加").clicked();
            if browse {
                if let Some(path) = Self::pick_files() {
                    self.add_path(path);
                }
            }
            if add_clicked && !self.path_input.is_empty() {
                let path = PathBuf::from(self.path_input.trim());
                self.add_path(path);
                self.path_input.clear();
            }
        });

        if !self.files.is_empty() {
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("清空列表").clicked() {
                    self.files.clear();
                }
                ui.label(format!("共 {} 个文件", self.files.len()));
            });

            egui::ScrollArea::vertical()
                .max_height(200.0)
                .show(ui, |ui| {
                    let mut remove_idx: Option<usize> = None;
                    for (i, path) in self.files.iter().enumerate() {
                        ui.horizontal(|ui| {
                            if ui.button("✕").clicked() {
                                remove_idx = Some(i);
                            }
                            ui.label(
                                path.file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_else(|| path.to_string_lossy().to_string()),
                            );
                        });
                    }
                    if let Some(i) = remove_idx {
                        self.files.remove(i);
                    }
                });
        }

        self.render_drop_hint(ui);
    }

    fn add_path(&mut self, path: PathBuf) {
        if self.is_supported(&path) && !self.files.contains(&path) {
            self.files.push(path);
        }
    }

    fn is_supported(&self, path: &PathBuf) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| matches!(e.to_lowercase().as_str(), "fbx" | "blend" | "obj" | "glb"))
            .unwrap_or(false)
    }

    fn pick_files() -> Option<PathBuf> {
        rfd::FileDialog::new()
            .add_filter("3D 模型", &["fbx", "blend", "obj", "glb"])
            .pick_file()
    }

    fn render_drop_hint(&self, ui: &mut egui::Ui) {
        if self.files.is_empty() {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("拖拽 FBX / Blend / OBJ 文件到此处").weak());
            });
            ui.add_space(10.0);
        }
    }
}
