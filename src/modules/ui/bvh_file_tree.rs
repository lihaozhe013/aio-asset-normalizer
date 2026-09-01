use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::modules::i18n::I18n;
use three_d::egui;

const INDENT: f32 = 16.0;
const ARROW_OFFSET: f32 = 16.0;

pub struct BvhFileTree {
    root: Option<PathBuf>,
    root_entries: Option<Vec<Entry>>,
    open_dirs: HashSet<PathBuf>,
    selected: Option<PathBuf>,
    pending_open: Option<PathBuf>,
}

struct Entry {
    name: String,
    path: PathBuf,
    is_dir: bool,
    children: Option<Vec<Entry>>,
}

struct FlatItem {
    depth: usize,
    name: String,
    path: PathBuf,
    is_dir: bool,
    is_open: bool,
    is_loaded: bool,
}

impl BvhFileTree {
    pub fn new() -> Self {
        Self {
            root: None,
            root_entries: None,
            open_dirs: HashSet::new(),
            selected: None,
            pending_open: None,
        }
    }

    pub fn root(&self) -> Option<&PathBuf> {
        self.root.as_ref()
    }

    pub fn clear(&mut self) {
        self.root = None;
        self.root_entries = None;
        self.open_dirs.clear();
        self.selected = None;
        self.pending_open = None;
    }

    pub fn open_folder(&mut self, path: PathBuf) {
        if !path.is_dir() {
            return;
        }
        self.root = Some(path.clone());
        self.root_entries = Some(Self::scan_dir(&path, 2));
        self.open_dirs.clear();
        self.selected = None;
        self.pending_open = None;
    }

    pub fn refresh(&mut self) {
        let Some(root) = self.root.clone() else {
            return;
        };
        self.root_entries = Some(Self::scan_dir(&root, 2));
        let open_dirs: Vec<PathBuf> = self.open_dirs.iter().cloned().collect();
        for path in open_dirs {
            let children = Self::scan_dir(&path, 1);
            if let Some(entry) = self.find_entry_mut(&path) {
                entry.children = Some(children);
            }
        }
    }

    pub fn select_file(&mut self, path: &Path) {
        if Self::is_bvh(path) {
            self.selected = Some(path.to_path_buf());
        }
    }

    pub fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|input| input.raw.dropped_files.clone());
        if dropped.is_empty() {
            return;
        }

        let Some(first_path) =
            dropped.iter().find_map(|file| file.path.clone())
        else {
            return;
        };
        if first_path.is_dir() {
            self.open_folder(first_path);
            return;
        }

        let Some(path) = dropped
            .iter()
            .filter_map(|file| file.path.as_deref())
            .find(|path| Self::is_bvh(path))
        else {
            return;
        };
        if let Some(parent) = path.parent() {
            self.open_folder(parent.to_path_buf());
        }
        self.select_file(path);
        self.pending_open = Some(path.to_path_buf());
    }

    pub fn render(
        &mut self,
        ui: &mut egui::Ui,
        i18n: &I18n,
    ) -> Option<PathBuf> {
        let mut open_path = self.pending_open.take();
        let Some(root) = self.root.clone() else {
            self.render_open_prompt(ui, i18n);
            return open_path;
        };

        let root_name = root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| i18n.tr("label.default_project").to_owned());

        let mut load_requests = Vec::new();
        egui::ScrollArea::both()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(&root_name).strong().size(14.0),
                    );
                });
                ui.horizontal(|ui| {
                    if ui.button(i18n.tr("button.switch_directory")).clicked() {
                        if let Some(folder) =
                            rfd::FileDialog::new().pick_folder()
                        {
                            self.open_folder(folder);
                        }
                    }
                    if ui.button(i18n.tr("button.refresh")).clicked() {
                        self.refresh();
                    }
                });
                ui.separator();

                let visible = self.collect_visible();
                if !visible.iter().any(|item| !item.is_dir) {
                    ui.label(
                        egui::RichText::new(i18n.tr("files.no_supported"))
                            .weak(),
                    );
                }

                for item in &visible {
                    if item.is_dir {
                        let header = egui::CollapsingHeader::new(&item.name)
                            .id_salt(format!(
                                "bvh_ft_dir:{}",
                                item.path.display()
                            ))
                            .default_open(item.is_open);
                        ui.horizontal(|ui| {
                            ui.add_space(item.depth as f32 * INDENT);
                            let response = header.show_unindented(ui, |_ui| {});
                            if response.body_returned.is_some() {
                                self.open_dirs.insert(item.path.clone());
                                if !item.is_loaded {
                                    load_requests.push(item.path.clone());
                                }
                            } else {
                                self.open_dirs.remove(&item.path);
                            }
                        });
                    } else {
                        ui.horizontal(|ui| {
                            ui.add_space(
                                item.depth as f32 * INDENT + ARROW_OFFSET,
                            );
                            let selected = self
                                .selected
                                .as_ref()
                                .is_some_and(|path| path == &item.path);
                            if ui
                                .selectable_label(selected, &item.name)
                                .clicked()
                            {
                                self.selected = Some(item.path.clone());
                                open_path = Some(item.path.clone());
                            }
                        });
                    }
                }
            });

        for path in load_requests {
            if let Some(entry) = self.find_entry_mut(&path) {
                entry.children = Some(Self::scan_dir(&path, 1));
            }
        }

        open_path
    }

    fn render_open_prompt(&mut self, ui: &mut egui::Ui, i18n: &I18n) {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            ui.label(
                egui::RichText::new(i18n.tr("label.open_folder_hint")).weak(),
            );
            ui.add_space(8.0);
            if ui.button(i18n.tr("button.open_folder")).clicked() {
                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                    self.open_folder(folder);
                }
            }
        });
    }

    fn collect_visible(&self) -> Vec<FlatItem> {
        let mut result = Vec::new();
        if let Some(entries) = &self.root_entries {
            for entry in entries {
                self.collect_visible_recursive(entry, 0, &mut result);
            }
        }
        result
    }

    fn collect_visible_recursive(
        &self,
        entry: &Entry,
        depth: usize,
        result: &mut Vec<FlatItem>,
    ) {
        let is_open = self.open_dirs.contains(&entry.path);
        result.push(FlatItem {
            depth,
            name: entry.name.clone(),
            path: entry.path.clone(),
            is_dir: entry.is_dir,
            is_open,
            is_loaded: entry.children.is_some(),
        });
        if entry.is_dir && is_open {
            if let Some(children) = &entry.children {
                for child in children {
                    self.collect_visible_recursive(child, depth + 1, result);
                }
            }
        }
    }

    fn find_entry_mut(&mut self, target: &Path) -> Option<&mut Entry> {
        self.root_entries
            .as_mut()
            .and_then(|entries| Self::find_entry_mut_recursive(entries, target))
    }

    fn find_entry_mut_recursive<'a>(
        entries: &'a mut [Entry],
        target: &Path,
    ) -> Option<&'a mut Entry> {
        for entry in entries {
            if entry.path == target {
                return Some(entry);
            }
            if let Some(children) = entry.children.as_mut() {
                if let Some(found) =
                    Self::find_entry_mut_recursive(children, target)
                {
                    return Some(found);
                }
            }
        }
        None
    }

    fn scan_dir(path: &Path, max_depth: usize) -> Vec<Entry> {
        let Ok(dir_iter) = std::fs::read_dir(path) else {
            return Vec::new();
        };
        let mut entries = Vec::new();
        for entry in dir_iter.flatten() {
            let entry_path = entry.path();
            let is_dir = entry_path.is_dir();
            if !is_dir && !Self::is_bvh(&entry_path) {
                continue;
            }
            let children = if is_dir && max_depth > 1 {
                let nested = Self::scan_dir(&entry_path, max_depth - 1);
                if nested.is_empty() {
                    None
                } else {
                    Some(nested)
                }
            } else {
                None
            };
            let name = entry_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            entries.push(Entry {
                name,
                path: entry_path,
                is_dir,
                children,
            });
        }
        entries.sort_by(|left, right| {
            right
                .is_dir
                .cmp(&left.is_dir)
                .then(left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });
        entries
    }

    fn is_bvh(path: &Path) -> bool {
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("bvh"))
    }
}

#[cfg(test)]
mod tests {
    use super::BvhFileTree;
    use std::path::Path;

    #[test]
    fn bvh_tree_accepts_only_case_insensitive_bvh_files() {
        assert!(BvhFileTree::is_bvh(Path::new("walk.BVH")));
        assert!(!BvhFileTree::is_bvh(Path::new("walk.glb")));
        assert!(!BvhFileTree::is_bvh(Path::new("walk.bvh.tmp")));
    }
}
