use crate::app::App;
use crate::modules::{preferences, ui::main_panel};

impl App {
    pub fn collect_preferences(&self) -> preferences::UserPreferences {
        preferences::UserPreferences {
            version: 1,
            language: self.i18n.preference(),
            view: self.canvas.to_view_prefs(),
            file_tree: self.file_tree.to_prefs(),
            log_viewer: self.log.to_prefs(),
        }
    }

    pub fn render_ui(
        &mut self,
        ui: &mut three_d::egui::Ui,
        window_width: u32,
    ) -> three_d::egui::Rect {
        let rect = main_panel::render_ui(self, ui, window_width);
        if self.needs_save && !self.quit_requested() {
            self.needs_save = false;
            preferences::save(&self.collect_preferences());
        }
        rect
    }

    pub(crate) fn glb_animation_entries(
        &self,
    ) -> Vec<(String, f32, bool, String)> {
        self.canvas
            .animation_clips()
            .iter()
            .map(|clip| {
                (
                    clip.name.clone(),
                    clip.duration,
                    clip.is_playable(),
                    clip.unsupported.join(", "),
                )
            })
            .collect()
    }

    pub(crate) fn glb_animation_duration(&self) -> f32 {
        self.canvas
            .animation_clips()
            .get(self.glb_animation_index)
            .map(|clip| clip.duration)
            .unwrap_or(0.0)
    }

    pub(crate) fn first_playable_glb_animation(&self) -> Option<usize> {
        self.canvas
            .animation_clips()
            .iter()
            .position(|clip| clip.is_playable())
    }

    pub(crate) fn select_glb_animation(&mut self, index: usize) {
        if self
            .canvas
            .animation_clips()
            .get(index)
            .is_none_or(|clip| !clip.is_playable())
        {
            self.glb_animation_playing = false;
            return;
        }
        self.glb_animation_index = index;
        self.refresh_glb_retarget_mapping();
        self.glb_animation_time = 0.0;
        self.glb_animation_accumulator = 0.0;
        self.glb_animation_playing = false;
        if let Err(error) = self.update_glb_animation_preview() {
            self.log.append(&format!(
                "[glb_editor] Animation selection failed: {error}"
            ));
        }
    }

    pub(crate) fn set_glb_animation_time(&mut self, time: f32) {
        let duration = self.glb_animation_duration();
        self.glb_animation_time = time.clamp(0.0, duration.max(0.0));
        self.glb_animation_accumulator = 0.0;
        if let Err(error) = self.update_glb_animation_preview() {
            self.log.append(&format!(
                "[glb_editor] Animation seek failed: {error}"
            ));
        }
    }

    pub(crate) fn step_glb_animation(&mut self, direction: f32) {
        let duration = self.glb_animation_duration();
        if duration <= 0.0 {
            return;
        }
        let time = self.glb_animation_time + direction * (1.0 / 30.0);
        self.glb_animation_time = if self.glb_animation_loop {
            time.rem_euclid(duration)
        } else {
            time.clamp(0.0, duration)
        };
        self.glb_animation_playing = false;
        self.glb_animation_accumulator = 0.0;
        if let Err(error) = self.update_glb_animation_preview() {
            self.log.append(&format!(
                "[glb_editor] Animation step failed: {error}"
            ));
        }
    }

    pub(crate) fn update_glb_animation_preview(
        &mut self,
    ) -> Result<(), String> {
        if self
            .canvas
            .animation_clips()
            .get(self.glb_animation_index)
            .is_none()
        {
            return Ok(());
        }
        self.canvas.update_glb_animation(
            self.glb_animation_index,
            self.glb_animation_time,
        )
    }

    pub(crate) fn reset_glb_animation_state(&mut self) {
        self.glb_animation_index = 0;
        self.glb_animation_time = 0.0;
        self.glb_animation_playing = false;
        self.glb_animation_accumulator = 0.0;
    }

    pub(crate) fn reset_glb_animation_rate(&mut self) {
        self.glb_animation_rate = 1.0;
    }
}
