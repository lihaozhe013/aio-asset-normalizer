use crate::modules::i18n::I18n;
use crate::modules::preferences::FileTreePreferences;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use three_d::egui;

const INDENT: f32 = 16.0;
const ARROW_OFFSET: f32 = 16.0;

pub struct FileTree {
    root: Option<PathBuf>,
    root_entries: Option<Vec<FileTreeEntry>>,
    selected: HashSet<PathBuf>,
    open_dirs: HashSet<PathBuf>,
    pub show_all_files: bool,
    root_changed: bool,
    /// Lowercase extensions treated as selectable files, e.g. `["glb"]` for
    /// the GLB Editor or `["fbx", "obj", "blend"]` for the FBX Converter.
    accepted_extensions: Vec<String>,
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
            open_dirs: HashSet::new(),
            show_all_files: false,
            root_changed: false,
            accepted_extensions: vec!["glb".to_owned()],
        }
    }

    /// Replace the selectable extension set and rescan the current root.
    pub fn set_accepted_extensions(&mut self, extensions: Vec<String>) {
        self.accepted_extensions = extensions;
        self.rescan();
    }

    pub fn take_root_changed(&mut self) -> bool {
        std::mem::replace(&mut self.root_changed, false)
    }

    pub fn root(&self) -> Option<&PathBuf> {
        self.root.as_ref()
    }

    pub fn clear(&mut self) {
        self.root = None;
        self.root_entries = None;
        self.selected.clear();
        self.open_dirs.clear();
        self.root_changed = true;
    }

    pub fn select_file(&mut self, path: &Path) {
        self.selected.insert(path.to_path_buf());
    }

    pub fn open_folder(&mut self, path: PathBuf) {
        self.root = Some(path.clone());
        self.selected.clear();
        self.open_dirs.clear();
        self.root_entries = Some(Self::scan_dir(
            &path,
            2,
            self.show_all_files,
            &self.accepted_extensions,
        ));
        self.root_changed = true;
    }

    /// Checked files, sorted for stable batch ordering.
    pub fn selected_files(&self) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = self.selected.iter().cloned().collect();
        files.sort();
        files
    }

    pub fn apply_prefs(&mut self, prefs: &FileTreePreferences) {
        self.show_all_files = prefs.show_all_files;
        if let Some(ref dir) = prefs.last_opened_directory {
            let path = PathBuf::from(dir);
            if path.exists() && path.is_dir() {
                self.open_folder(path);
            }
        }
    }

    pub fn to_prefs(&self) -> FileTreePreferences {
        FileTreePreferences {
            show_all_files: self.show_all_files,
            last_opened_directory: self
                .root()
                .map(|p| p.to_string_lossy().to_string()),
        }
    }

    fn rescan(&mut self) {
        if let Some(ref root) = self.root.clone() {
            let show_all = self.show_all_files;
            let accepted = self.accepted_extensions.clone();
            self.root_entries =
                Some(Self::scan_dir(root, 2, show_all, &accepted));
        }
    }

    pub fn refresh(&mut self) {
        if let Some(ref root) = self.root.clone() {
            let show_all = self.show_all_files;
            let accepted = self.accepted_extensions.clone();
            self.root_entries =
                Some(Self::scan_dir(root, 2, show_all, &accepted));
            let open: Vec<PathBuf> = self.open_dirs.iter().cloned().collect();
            for path in &open {
                let children = Self::scan_dir(path, 1, show_all, &accepted);
                if let Some(entry) = self.find_entry_mut(path) {
                    entry.children = Some(children);
                }
            }
        }
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
                    let selected = file.path.as_ref().is_some_and(|fp| {
                        Self::extension_in(fp, &self.accepted_extensions)
                    });
                    if selected {
                        if let Some(fp) = &file.path {
                            self.selected.insert(fp.clone());
                        }
                    }
                }
            }
        }
    }

    fn scan_dir(
        path: &Path,
        max_depth: usize,
        show_all_files: bool,
        accepted: &[String],
    ) -> Vec<FileTreeEntry> {
        let mut entries: Vec<FileTreeEntry> = Vec::new();

        let dir_iter = match std::fs::read_dir(path) {
            Ok(iter) => iter,
            Err(_) => return entries,
        };

        for entry in dir_iter.flatten() {
            let entry_path = entry.path();
            let is_dir = entry_path.is_dir();

            if !is_dir
                && !show_all_files
                && !Self::extension_in(&entry_path, accepted)
            {
                continue;
            }

            let name = entry_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();

            let children = if is_dir && max_depth > 1 {
                let sub = Self::scan_dir(
                    &entry_path,
                    max_depth - 1,
                    show_all_files,
                    accepted,
                );
                if sub.is_empty() {
                    None
                } else {
                    Some(sub)
                }
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

    fn extension_in(path: &Path, accepted: &[String]) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|extension| {
                accepted
                    .iter()
                    .any(|accepted| accepted.eq_ignore_ascii_case(extension))
            })
    }

    fn is_glb(path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("glb"))
            .unwrap_or(false)
    }

    pub fn select_all(&mut self) {
        if let Some(ref entries) = self.root_entries {
            let accepted = self.accepted_extensions.clone();
            Self::collect_files(entries, &mut self.selected, &accepted);
        }
    }

    pub fn deselect_all(&mut self) {
        self.selected.clear();
    }

    pub fn invert_selection(&mut self) {
        let all: HashSet<PathBuf> = if let Some(ref entries) = self.root_entries
        {
            let mut set = HashSet::new();
            Self::collect_files(entries, &mut set, &self.accepted_extensions);
            set
        } else {
            HashSet::new()
        };

        let inverted: HashSet<PathBuf> =
            all.difference(&self.selected).cloned().collect();
        self.selected = inverted;
    }

    fn collect_files(
        entries: &[FileTreeEntry],
        set: &mut HashSet<PathBuf>,
        accepted: &[String],
    ) {
        for entry in entries {
            if !entry.is_dir && Self::extension_in(&entry.path, accepted) {
                set.insert(entry.path.clone());
            }
            if let Some(ref children) = entry.children {
                Self::collect_files(children, set, accepted);
            }
        }
    }

    fn collect_visible(&self) -> Vec<FlatItem> {
        let mut result = Vec::new();
        if let Some(ref entries) = self.root_entries {
            for entry in entries {
                self.collect_visible_recursive(entry, 0, &mut result);
            }
        }
        result
    }

    fn collect_visible_recursive(
        &self,
        entry: &FileTreeEntry,
        depth: usize,
        result: &mut Vec<FlatItem>,
    ) {
        let is_open = self.open_dirs.contains(&entry.path);
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
                    self.collect_visible_recursive(child, depth + 1, result);
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
                    if let found @ Some(_) =
                        Self::find_entry_mut_recursive(children, target)
                    {
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
            Self::collect_files(entries, &mut set, &self.accepted_extensions);
            set.len()
        } else {
            0
        }
    }

    pub fn render(
        &mut self,
        ui: &mut egui::Ui,
        i18n: &I18n,
    ) -> (bool, Option<PathBuf>) {
        let mut prefs_changed = false;
        let mut preview_glb: Option<PathBuf> = None;

        if self.root.is_none() {
            self.render_open_prompt(ui, i18n);
            return (prefs_changed, None);
        }

        let root_name = self
            .root
            .as_ref()
            .and_then(|r| r.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| i18n.tr("label.default_project").to_owned());

        let mut load_requests: Vec<PathBuf> = Vec::new();
        let accepted = self.accepted_extensions.clone();

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

                let total = self.total_file_count();
                if total > 0 {
                    ui.horizontal(|ui| {
                        if ui.button(i18n.tr("button.select_all")).clicked() {
                            self.select_all();
                        }
                        if ui.button(i18n.tr("button.deselect_all")).clicked() {
                            self.deselect_all();
                        }
                        if ui
                            .button(i18n.tr("button.invert_selection"))
                            .clicked()
                        {
                            self.invert_selection();
                        }
                    });
                    ui.label(i18n.text(
                        "files.selected",
                        &[
                            ("selected", self.selected.len().to_string()),
                            ("total", total.to_string()),
                        ],
                    ));
                } else {
                    ui.label(
                        egui::RichText::new(i18n.tr("files.no_supported"))
                            .weak(),
                    );
                }

                ui.separator();

                let mut show_all = self.show_all_files;
                if ui
                    .checkbox(&mut show_all, i18n.tr("label.show_all_files"))
                    .changed()
                {
                    self.show_all_files = show_all;
                    prefs_changed = true;
                    self.rescan();
                }
                if self.show_all_files {
                    ui.label(
                        egui::RichText::new(
                            i18n.tr("label.unsupported_files_hint"),
                        )
                        .small()
                        .weak(),
                    );
                }

                let visible = self.collect_visible();

                for item in &visible {
                    if item.is_dir {
                        let header = egui::CollapsingHeader::new(&item.name)
                            .id_salt(format!("ft_dir:{}", item.path.display()))
                            .default_open(item.is_open);

                        ui.horizontal(|ui| {
                            ui.add_space(item.depth as f32 * INDENT);
                            let cr = header.show_unindented(ui, |_ui| {});
                            if cr.body_returned.is_some() {
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
                            if Self::extension_in(&item.path, &accepted) {
                                let mut checked =
                                    self.selected.contains(&item.path);
                                if ui.checkbox(&mut checked, "").changed() {
                                    if checked {
                                        self.selected.insert(item.path.clone());
                                    } else {
                                        self.selected.remove(&item.path);
                                    }
                                }
                                if Self::is_glb(&item.path) {
                                    if ui
                                        .selectable_label(false, &item.name)
                                        .clicked()
                                    {
                                        preview_glb = Some(item.path.clone());
                                    }
                                } else {
                                    ui.label(&item.name);
                                }
                            } else {
                                ui.label(
                                    egui::RichText::new(&item.name).weak(),
                                );
                            }
                        });
                    }
                }
            });

        let show_all = self.show_all_files;
        for path in &load_requests {
            let children = Self::scan_dir(path, 1, show_all, &accepted);
            if let Some(entry) = self.find_entry_mut(path) {
                entry.children = Some(children);
            }
        }

        (prefs_changed, preview_glb)
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepted_extensions_control_scanning_and_selection() {
        let dir = std::env::temp_dir().join("aio-file-tree-ext-test");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let names = [
            "rig.fbx",
            "prop.OBJ",
            "scene.blend",
            "note.txt",
            "model.glb",
        ];
        for name in names {
            std::fs::write(dir.join(name), b"x").expect("fixture file");
        }

        let mut converter_tree = FileTree::new();
        converter_tree.set_accepted_extensions(vec![
            "fbx".to_owned(),
            "obj".to_owned(),
            "blend".to_owned(),
        ]);
        converter_tree.open_folder(dir.clone());
        converter_tree.select_all();
        let selected: Vec<String> = converter_tree
            .selected_files()
            .iter()
            .map(|path| {
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(selected, vec!["prop.OBJ", "rig.fbx", "scene.blend"]);

        let mut glb_tree = FileTree::new();
        glb_tree.open_folder(dir.clone());
        glb_tree.select_all();
        let selected: Vec<String> = glb_tree
            .selected_files()
            .iter()
            .map(|path| {
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(selected, vec!["model.glb"]);

        for name in names {
            std::fs::remove_file(dir.join(name)).ok();
        }
        std::fs::remove_dir(&dir).ok();
    }
}
