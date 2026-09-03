use std::path::Path;

use crate::app::App;
use crate::modules::bvh::{self, BvhDocument};
use crate::modules::glb::{GlbDocument, GlbExportPreset};
use crate::modules::retarget;
use crate::modules::ui::menu_bar::Page;
use crate::modules::viewport::skeleton_visual::SkeletonPose;

impl App {
    pub(crate) fn set_bvh_unit_from_ui(&mut self, unit: &str) {
        if !matches!(unit, "m" | "cm" | "mm") {
            return;
        }
        self.bvh_unit = unit.to_owned();
        if let Some(mapping) = self.retarget_mapping.as_mut() {
            if mapping.source.kind == retarget::SourceKind::Bvh {
                mapping.source.unit = self.bvh_unit.clone();
            }
        }
        if let Some(mapping) = self.mapping.as_mut() {
            mapping.source.unit = self.bvh_unit.clone();
        }
        self.refresh_v2_retarget_mapping();
        self.needs_bvh_skeleton_reload = true;
        self.bvh_camera_focus_pending = true;
        self.needs_bvh_target_reload = true;
    }

    pub(crate) fn load_bvh_target(&mut self, path: &Path) {
        match GlbDocument::load(path) {
            Ok(document) => {
                let mut export_selection =
                    document.default_export_selection().unwrap_or_default();
                export_selection.preset = GlbExportPreset::CharacterPackage;
                self.log.append(&format!(
                    "[bvh_studio] Loaded target GLB {}",
                    path.display()
                ));
                self.bvh_target_glb = Some(document);
                self.bvh_target_path = Some(path.to_path_buf());
                self.bvh_export_selection = export_selection;
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
                self.bvh_camera_focus_pending = true;
                self.canvas.show_origin = false;
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

    pub(crate) fn bvh_preview_positions(
        &self,
        document: &BvhDocument,
        frame: usize,
    ) -> Result<Vec<[f32; 3]>, String> {
        Ok(self.bvh_preview_pose(document, frame)?.positions)
    }

    pub(crate) fn bvh_preview_pose(
        &self,
        document: &BvhDocument,
        frame: usize,
    ) -> Result<SkeletonPose, String> {
        let convert = |positions: Vec<[f32; 3]>| {
            positions
                .into_iter()
                .map(|position| {
                    retarget::convert_bvh_position_to_glb(
                        position,
                        &self.bvh_up_axis,
                        &self.bvh_forward_axis,
                        &self.bvh_unit,
                    )
                    .map_err(|error| error.to_string())
                })
                .collect::<Result<Vec<_>, _>>()
        };
        let rest_transforms = document
            .rest_transforms_for_retarget()
            .map_err(|error| error.to_string())?;
        let frame_index = frame.min(document.frames.len().saturating_sub(1));
        let frame_transforms = document
            .frames
            .get(frame_index)
            .ok_or_else(|| "BVH has no motion frames".to_owned())
            .and_then(|frame| {
                document
                    .frame_transforms_for_retarget(frame)
                    .map_err(|error| error.to_string())
            })?;
        let mut rest_positions = convert(
            rest_transforms
                .iter()
                .map(|transform| transform.0)
                .collect(),
        )?;
        let mut positions = convert(
            frame_transforms
                .iter()
                .map(|transform| transform.0)
                .collect(),
        )?;
        let root_origin = if let Some(first_frame) = document.frames.first() {
            let transforms = document
                .frame_transforms_for_retarget(first_frame)
                .map_err(|error| error.to_string())?;
            if let Some(transform) = transforms.first() {
                Some(
                    retarget::convert_bvh_position_to_glb(
                        transform.0,
                        &self.bvh_up_axis,
                        &self.bvh_forward_axis,
                        &self.bvh_unit,
                    )
                    .map_err(|error| error.to_string())?,
                )
            } else {
                rest_positions.first().copied()
            }
        } else {
            rest_positions.first().copied()
        };
        if let Some(root_origin) = root_origin {
            rebase_points(&mut positions, root_origin);
            rebase_points(&mut rest_positions, root_origin);
        }
        let world_rotations = frame_transforms
            .iter()
            .map(|transform| transform.1)
            .collect::<Vec<_>>();
        let mut end_sites = vec![None; document.joints.len()];
        for (index, joint) in document.joints.iter().enumerate() {
            let Some(offset) = joint.end_site else {
                continue;
            };
            let Some(transform) = frame_transforms.get(index) else {
                continue;
            };
            let endpoint =
                add_point(transform.0, rotate_point(transform.1, offset));
            let mut endpoint = retarget::convert_bvh_position_to_glb(
                endpoint,
                &self.bvh_up_axis,
                &self.bvh_forward_axis,
                &self.bvh_unit,
            )
            .map_err(|error| error.to_string())?;
            if let Some(root_origin) = root_origin {
                endpoint[0] -= root_origin[0];
                endpoint[1] -= root_origin[1];
                endpoint[2] -= root_origin[2];
            }
            end_sites[index] = Some(endpoint);
        }
        let parents = document
            .joints
            .iter()
            .map(|joint| joint.parent)
            .collect::<Vec<_>>();
        Ok(SkeletonPose::with_rest_positions(
            positions,
            world_rotations,
            parents,
            end_sites,
            rest_positions,
        ))
    }

    pub(crate) fn bvh_span_diagnostic(
        &self,
        document: &BvhDocument,
    ) -> Result<(f32, f32), String> {
        let rest = document
            .rest_transforms_for_retarget()
            .map_err(|error| error.to_string())?;
        let raw = rest.iter().map(|transform| transform.0).collect::<Vec<_>>();
        let converted = raw
            .iter()
            .copied()
            .map(|position| {
                retarget::convert_bvh_position_to_glb(
                    position,
                    &self.bvh_up_axis,
                    &self.bvh_forward_axis,
                    &self.bvh_unit,
                )
                .map_err(|error| error.to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok((span(&raw), span(&converted)))
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

fn span(points: &[[f32; 3]]) -> f32 {
    let mut minimum = [f32::INFINITY; 3];
    let mut maximum = [f32::NEG_INFINITY; 3];
    for point in points {
        if point.iter().any(|value| !value.is_finite()) {
            continue;
        }
        for axis in 0..3 {
            minimum[axis] = minimum[axis].min(point[axis]);
            maximum[axis] = maximum[axis].max(point[axis]);
        }
    }
    if minimum.iter().any(|value| !value.is_finite()) {
        return 0.0;
    }
    ((maximum[0] - minimum[0]).powi(2)
        + (maximum[1] - minimum[1]).powi(2)
        + (maximum[2] - minimum[2]).powi(2))
    .sqrt()
}

fn rebase_points(points: &mut [[f32; 3]], origin: [f32; 3]) {
    for point in points {
        point[0] -= origin[0];
        point[1] -= origin[1];
        point[2] -= origin[2];
    }
}

fn add_point(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn rotate_point(rotation: [f32; 4], value: [f32; 3]) -> [f32; 3] {
    let q = rotation;
    let qv = [q[0], q[1], q[2]];
    let cross = |a: [f32; 3], b: [f32; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let twice_cross = cross(qv, value).map(|component| component * 2.0);
    [
        value[0] + twice_cross[0] * q[3] + cross(qv, twice_cross)[0],
        value[1] + twice_cross[1] * q[3] + cross(qv, twice_cross)[1],
        value[2] + twice_cross[2] * q[3] + cross(qv, twice_cross)[2],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_rebases_the_first_frame_root_without_changing_scale() {
        let mut points = vec![[4.0, 1.0, -2.0], [4.0, 2.0, -2.0]];
        rebase_points(&mut points, [4.0, 1.0, -2.0]);
        assert_eq!(points, [[0.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
    }
}
