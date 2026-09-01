use std::path::Path;

use crate::app::App;
use crate::modules::bvh::{self, BvhDocument};
use crate::modules::glb::GlbDocument;
use crate::modules::retarget;
use crate::modules::ui::menu_bar::Page;

impl App {
    pub(crate) fn load_bvh_target(&mut self, path: &Path) {
        match GlbDocument::load(path) {
            Ok(document) => {
                self.log.append(&format!(
                    "[bvh_studio] Loaded target GLB {}",
                    path.display()
                ));
                self.bvh_target_glb = Some(document);
                self.bvh_target_path = Some(path.to_path_buf());
                self.retarget_target_skin_index = 0;
                self.needs_bvh_target_reload = true;
                self.refresh_retarget_plan();
                self.refresh_v2_retarget_mapping();
            }
            Err(error) => self.log.append(&format!("[bvh_studio] {error}")),
        }
    }

    pub(crate) fn import_bvh(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("BVH", &["bvh"])
            .pick_file()
        else {
            return;
        };
        self.load_bvh_path(&path);
    }

    pub(crate) fn load_bvh_path(&mut self, path: &Path) {
        match BvhDocument::load(path) {
            Ok(document) => {
                self.bvh_trim_end =
                    document.duration().max(document.frame_time);
                self.log.append(&format!(
                    "[bvh_studio] Loaded {} ({} joints, {} frames)",
                    path.display(),
                    document.joints.len(),
                    document.frames.len()
                ));
                self.bvh = Some(document);
                self.bvh_path = Some(path.to_path_buf());
                self.bvh_frame = 0;
                self.bvh_playing = false;
                self.needs_bvh_skeleton_reload = true;
                self.page = Page::BvhStudio;
                self.refresh_retarget_plan();
                self.refresh_v2_retarget_mapping();
                self.needs_bvh_target_reload = true;
                if let Some(parent) = path.parent() {
                    if self
                        .bvh_file_tree
                        .root()
                        .is_none_or(|root| root != parent)
                    {
                        self.bvh_file_tree.open_folder(parent.to_path_buf());
                    }
                }
                self.bvh_file_tree.select_file(path);
            }
            Err(error) => self.log.append(&format!("[bvh_studio] {error}")),
        }
    }

    pub(crate) fn import_mapping(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Mapping JSON", &["json"])
            .pick_file()
        else {
            return;
        };
        if let Ok(mapping) = retarget::load_mapping(&path) {
            self.log.append(&format!(
                "[retarget] Loaded Mapping v2 {}",
                path.display()
            ));
            self.retarget_mapping = Some(mapping);
            self.retarget_mapping_path = Some(path.clone());
            self.mapping = None;
            self.mapping_report = None;
            self.retarget_plan = None;
            self.mapping_suggestions.clear();
            self.mapping_path = Some(path);
            if self.retarget_mapping.as_ref().is_some_and(|mapping| {
                mapping.source.kind == retarget::SourceKind::Bvh
            }) {
                self.page = Page::BvhStudio;
            }
            self.refresh_v2_retarget_mapping();
            self.refresh_glb_retarget_mapping();
            return;
        }
        match bvh::load_mapping(&path) {
            Ok(mapping) => {
                self.log.append(&format!(
                    "[bvh_studio] Loaded mapping {}",
                    path.display()
                ));
                self.retarget_plan = self
                    .bvh
                    .as_ref()
                    .and_then(|document| document.plan_retarget(&mapping).ok());
                self.mapping = Some(mapping);
                self.retarget_mapping = None;
                self.retarget_mapping_path = None;
                self.retarget_validation = None;
                if let Some(mapping) = self.mapping.as_ref() {
                    self.bvh_up_axis = mapping.source.up_axis.clone();
                    self.bvh_forward_axis = mapping.source.forward_axis.clone();
                    self.bvh_unit = mapping.source.unit.clone();
                }
                self.mapping_path = Some(path);
                self.page = Page::BvhStudio;
                self.refresh_retarget_plan();
                self.refresh_v2_retarget_mapping();
                self.refresh_glb_retarget_mapping();
            }
            Err(error) => self.log.append(&format!("[bvh_studio] {error}")),
        }
    }

    pub(crate) fn refresh_retarget_plan(&mut self) {
        self.mapping_report = None;
        self.mapping_suggestions.clear();
        let Some(document) = self.bvh.as_ref() else {
            self.retarget_plan = None;
            return;
        };
        let Some(mapping) = self.mapping.as_ref() else {
            self.retarget_plan = None;
            return;
        };
        let Some(target) = self.bvh_target_glb.as_ref() else {
            self.retarget_plan = None;
            return;
        };
        let Ok(skin) = target.skin_data_at(self.retarget_target_skin_index)
        else {
            self.retarget_plan = None;
            self.log.append(
                "[bvh_studio] Target GLB does not expose a usable Skin",
            );
            return;
        };
        let report = document.mapping_report(mapping, &skin);
        self.mapping_suggestions = document.suggest_mapping(&skin);
        self.retarget_plan = if report.is_valid() {
            document.plan_retarget(mapping).ok()
        } else {
            None
        };
        self.mapping_report = Some(report);
    }
}
