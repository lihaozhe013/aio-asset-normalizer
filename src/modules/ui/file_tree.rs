use std::collections::HashSet;
use std::path::{Path, PathBuf};
use three_d::egui;

const INDENT: f32 = 16.0;
const ARROW_OFFSET: f32 = 16.0;

pub struct FileTree {
    root: Option<PathBuf>,
    root_entries: Option<Vec<FileTreeEntry>>,
    selected: HashSet<PathBuf>,
}

pub struct FileTreeEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub children: Option<Vec<FileTreeEntry>>,
}

struct FlatItem {
    depth: usize,
    name: String,
    path: PathBuf,
    is_dir: bool,
    is_open: bool,
    is_loaded: bool,
}

impl FileTree {
    pub fn new() -> Self {
        Self {
            root: None,
            root_entries: None,
            selected: HashSet::new(),
        }
    }

    pub fn selected_files(&self) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = self.selected.iter().cloned().collect();
        files.sort();
        files
    }

    pub fn clear(&mut self) {
        self.root = None;
        self.root_entries = None;
        self.selected.clear();
    }

    pub fn select_file(&mut self, path: &Path) {
        self.selected.insert(path.to_path_buf());
    }

    pub fn open_folder(&mut self, path: PathBuf) {
        self.root = Some(path.clone());
        self.selected.clear();
        self.root_entries = Some(Self::scan_dir(&path, 2));
    }

    pub fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if dropped.is_empty() {
            return;
        }

        let first = &dropped[0];
        if let Some(path) = &first.path {
            if path.is_dir() {
                self.open_folder(path.clone());
                return;
            }
            if let Some(parent) = path.parent().map(|p| p.to_path_buf()) {
                self.open_folder(parent);
                for file in &dropped {
                    if let Some(fp) = &file.path {
                        if Self::is_supported(fp) {
                            self.selected.insert(fp.clone());
                        }
                    }
                }
            }
        }
    }

    fn scan_dir(path: &Path, max_depth: usize) -> Vec<FileTreeEntry> {
        let mut entries: Vec<FileTreeEntry> = Vec::new();

        let dir_iter = match std::fs::read_dir(path) {
            Ok(iter) => iter,
            Err(_) => return entries,
        };

        for entry in dir_iter.flatten() {
            let entry_path = entry.path();
            let is_dir = entry_path.is_dir();

            if !is_dir && !Self::is_supported(&entry_path) {
                continue;
            }

            let name = entry_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();

            let children = if is_dir && max_depth > 1 {
                let sub = Self::scan_dir(&entry_path, max_depth - 1);
                if sub.is_empty() {
                    None
                } else {
                    Some(sub)
                }
            } else if is_dir {
                None
            } else {
                None
            };

            entries.push(FileTreeEntry {
                name,
                path: entry_path,
                is_dir,
                children,
            });
        }

        entries.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });

        entries
    }

    fn is_supported(path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| matches!(e.to_lowercase().as_str(), "fbx" | "blend" | "obj" | "glb"))
            .unwrap_or(false)
    }

    pub fn select_all(&mut self) {
        if let Some(ref entries) = self.root_entries {
            Self::collect_files(entries, &mut self.selected);
        }
    }

    pub fn deselect_all(&mut self) {
        self.selected.clear();
    }

    pub fn invert_selection(&mut self) {
        let all: HashSet<PathBuf> = if let Some(ref entries) = self.root_entries {
            let mut set = HashSet::new();
            Self::collect_files(entries, &mut set);
            set
        } else {
            HashSet::new()
        };

        let inverted: HashSet<PathBuf> = all.difference(&self.selected).cloned().collect();
        self.selected = inverted;
    }

    fn collect_files(entries: &[FileTreeEntry], set: &mut HashSet<PathBuf>) {
        for entry in entries {
            if !entry.is_dir {
                set.insert(entry.path.clone());
            }
            if let Some(ref children) = entry.children {
                Self::collect_files(children, set);
            }
        }
    }

    fn collect_visible(&self, ctx: &egui::Context) -> Vec<FlatItem> {
        let mut result = Vec::new();
        if let Some(ref entries) = self.root_entries {
            for entry in entries {
                self.collect_visible_recursive(entry, 0, ctx, &mut result);
            }
        }
        result
    }

    fn collect_visible_recursive(
        &self,
        entry: &FileTreeEntry,
        depth: usize,
        ctx: &egui::Context,
        result: &mut Vec<FlatItem>,
    ) {
        let id_source = format!("ft_dir:{}", entry.path.display());
        let id = egui::Id::new(&id_source);
        let is_open = egui::collapsing_header::CollapsingState::load(ctx, id)
            .map(|s| s.is_open())
            .unwrap_or(false);
        let is_loaded = entry.children.is_some();

        result.push(FlatItem {
            depth,
            name: entry.name.clone(),
            path: entry.path.clone(),
            is_dir: entry.is_dir,
            is_open,
            is_loaded,
        });

        if entry.is_dir && is_open && is_loaded {
            if let Some(ref children) = entry.children {
                for child in children {
                    self.collect_visible_recursive(child, depth + 1, ctx, result);
                }
            }
        }
    }

    fn find_entry_mut(&mut self, target: &Path) -> Option<&mut FileTreeEntry> {
        self.root_entries
            .as_mut()
            .and_then(|entries| Self::find_entry_mut_recursive(entries, target))
    }

    fn find_entry_mut_recursive<'a>(
        entries: &'a mut [FileTreeEntry],
        target: &Path,
    ) -> Option<&'a mut FileTreeEntry> {
        for entry in entries {
            if entry.path == target {
                return Some(entry);
            }
            if entry.is_dir {
                if let Some(ref mut children) = entry.children {
                    if let found @ Some(_) = Self::find_entry_mut_recursive(children, target) {
                        return found;
                    }
                }
            }
        }
        None
    }

    fn total_file_count(&self) -> usize {
        if let Some(ref entries) = self.root_entries {
            let mut set = HashSet::new();
            Self::collect_files(entries, &mut set);
            set.len()
        } else {
            0
        }
    }

    pub fn render(&mut self, ui: &mut egui::Ui) {
        if self.root.is_none() {
            self.render_open_prompt(ui);
            return;
        }

        let root_name = self
            .root
            .as_ref()
            .and_then(|r| r.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "项目".to_owned());

        ui.horizontal(|ui| {
            ui.heading(&root_name);
            if ui.button("切换目录").clicked() {
                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                    self.open_folder(folder);
                }
            }
        });

        ui.separator();

        let total = self.total_file_count();
        if total > 0 {
            ui.horizontal(|ui| {
                if ui.button("全选").clicked() {
                    self.select_all();
                }
                if ui.button("取消全选").clicked() {
                    self.deselect_all();
                }
                if ui.button("反选").clicked() {
                    self.invert_selection();
                }
            });
            ui.label(format!(
                "已选 {} / 共 {} 个文件",
                self.selected.len(),
                total
            ));
        } else {
            ui.label(egui::RichText::new("未发现支持的文件").weak());
        }

        ui.separator();

        let visible = self.collect_visible(ui.ctx());
        let mut load_requests: Vec<PathBuf> = Vec::new();

        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                for item in &visible {
                    if item.is_dir {
                        let header = egui::CollapsingHeader::new(&item.name)
                            .id_salt(format!("ft_dir:{}", item.path.display()))
                            .default_open(item.is_open);

                        ui.horizontal(|ui| {
                            ui.add_space(item.depth as f32 * INDENT);
                            let cr = header.show_unindented(ui, |_ui| {});
                            if cr.body_returned.is_some() && !item.is_loaded {
                                load_requests.push(item.path.clone());
                            }
                        });
                    } else {
                        ui.horizontal(|ui| {
                            ui.add_space(item.depth as f32 * INDENT + ARROW_OFFSET);
                            let mut checked = self.selected.contains(&item.path);
                            if ui.checkbox(&mut checked, &item.name).changed() {
                                if checked {
                                    self.selected.insert(item.path.clone());
                                } else {
                                    self.selected.remove(&item.path);
                                }
                            }
                        });
                    }
                }
            });

        for path in &load_requests {
            if let Some(entry) = self.find_entry_mut(path) {
                let children = Self::scan_dir(path, 1);
                entry.children = Some(children);
            }
        }
    }

    fn render_open_prompt(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            ui.label(egui::RichText::new("打开文件夹以浏览资产").weak());
            ui.add_space(8.0);
            if ui.button("打开文件夹").clicked() {
                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                    self.open_folder(folder);
                }
            }
        });
    }
}
