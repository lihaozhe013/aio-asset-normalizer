use std::path::Path;

use crate::app::App;
use crate::modules::glb::{PrimitiveTarget, StandardizationProfile};
use crate::modules::logging::safe_path_label;
use crate::reload::{merge_glb_reload_kind, GlbReloadKind};

impl App {
    pub fn preview_glb(&mut self, path: &Path) {
        self.page = crate::modules::ui::menu_bar::Page::GlbEditor;
        self.glb_retarget_preview_active = false;
        self.pending_glb_retarget_runtime = None;
        self.canvas.clear_glb_skeleton();
        self.canvas.clear_bvh_skeleton();
        self.canvas.clear_target_skeleton();
        self.glb_path = Some(path.to_path_buf());
        self.pending_animation_selection = None;
        self.reset_root_preview();
        self.reset_glb_animation_rate();
        self.smart_loop_enabled = false;
        self.smart_loop_transition = 0.15;
        self.trim_enabled = false;
        self.trim_animation = 0;
        self.trim_start = 0.0;
        self.trim_end = 1.0;
        self.request_glb_reload(GlbReloadKind::OpenModel);
        self.pending_auto_play = true;
    }

    pub(crate) fn request_glb_reload(&mut self, requested: GlbReloadKind) {
        self.reload_request =
            Some(merge_glb_reload_kind(self.reload_request, requested));
    }

    pub(crate) fn standardize(&mut self) {
        let Some(document) = self.glb.as_mut() else {
            tracing::warn!(
                target: "glb_editor",
                "Open a GLB before standardizing"
            );
            return;
        };
        match document.standardize(&StandardizationProfile::default()) {
            Ok(()) => tracing::info!(
                target: "glb_editor",
                "GLB matches the default contract"
            ),
            Err(error) => tracing::error!(
                target: "glb_editor",
                error = %error,
                "GLB standardization failed"
            ),
        }
    }

    pub(crate) fn trim_setting_changed(&mut self) {
        self.pending_animation_selection = Some(self.glb_animation_index);
        self.glb_export_estimate = None;
        self.request_glb_reload(GlbReloadKind::EditedModel);
    }

    pub(crate) fn smart_loop_setting_changed(&mut self) {
        self.pending_animation_selection = Some(self.glb_animation_index);
        self.glb_export_estimate = None;
        self.request_glb_reload(GlbReloadKind::EditedModel);
    }

    pub(crate) fn replace_glb_texture(&mut self) {
        let Some(document) = self.glb.as_mut() else {
            tracing::warn!(
                target: "glb_editor",
                "Open a GLB before replacing a texture"
            );
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .add_filter("PNG or JPEG", &["png", "jpg", "jpeg"])
            .pick_file()
        else {
            return;
        };
        match document.replace_texture(
            PrimitiveTarget {
                mesh: self.texture_mesh,
                primitive: self.texture_primitive,
            },
            self.texture_slot,
            &path,
            self.texture_duplicate_shared,
        ) {
            Ok(()) => {
                tracing::info!(
                    target: "glb_editor",
                    slot = %self.texture_slot.label(),
                    input = %safe_path_label(&path),
                    "Replaced texture"
                );
                self.request_glb_reload(GlbReloadKind::EditedModel);
            }
            Err(error) => tracing::error!(
                target: "glb_editor",
                error = %error,
                "Texture replacement failed"
            ),
        }
    }

    pub(crate) fn import_glb(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("GLB", &["glb"])
            .pick_file()
        else {
            return;
        };
        if self.page == crate::modules::ui::menu_bar::Page::BvhStudio {
            self.load_bvh_target(&path);
            return;
        }
        if let Some(parent) = path.parent() {
            self.file_tree.open_folder(parent.to_path_buf());
            self.file_tree.select_file(&path);
        }
        self.preview_glb(&path);
    }
}
