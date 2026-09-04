use std::collections::BTreeSet;

use crate::app::App;
use crate::modules::glb::{
    AnimationOutputMode, GlbDocument, GlbExportCatalog, GlbExportPreset,
    GlbExportSelection,
};
use crate::modules::i18n::I18n;

pub fn render(app: &mut App, ui: &mut three_d::egui::Ui) {
    use three_d::egui::*;

    let Some(document) = app.glb.as_ref() else {
        return;
    };
    let catalog = match document.export_catalog() {
        Ok(catalog) => catalog,
        Err(error) => {
            ui.colored_label(
                three_d::egui::Color32::YELLOW,
                format!("{}: {error}", app.i18n.tr("glb.export_selection")),
            );
            return;
        }
    };
    let mut selection = app.glb_export_selection.clone();
    render_selection_controls(
        ui,
        &app.i18n,
        document,
        &catalog,
        &mut selection,
        true,
        true,
        true,
    );
    let (validation, summary, bin_size, fresh_estimate) = {
        let Some(document) = app.glb.as_ref() else {
            return;
        };
        let mut validation = document.validate_export_selection(&selection);
        if selection.preset != GlbExportPreset::PreserveAll
            && selection.remove_root_motion
            && app.smart_loop_enabled
        {
            validation.errors.push(
                app.i18n
                    .tr("glb.export_root_motion_smart_loop_error")
                    .to_owned(),
            );
        }
        let summary = document.summary();
        let bin_size = document.binary_size();
        let needs_estimate = app
            .glb_export_estimate
            .as_ref()
            .is_none_or(|(cached, _)| cached != &selection);
        let fresh_estimate = needs_estimate.then(|| {
            document
                .preview_export_selection(&selection)
                .map_err(|error| error.to_string())
        });
        (validation, summary, bin_size, fresh_estimate)
    };
    if let Some(estimate) = fresh_estimate {
        app.glb_export_estimate = Some((selection.clone(), estimate));
    }
    render_validation(ui, &app.i18n, &validation);
    ui.label(format!(
        "{}: {} scenes, {} nodes, {} meshes, {} materials, {} skins, {} animations, {} images, {} bytes BIN",
        app.i18n.tr("glb.export_source"),
        summary.scenes,
        summary.nodes,
        summary.meshes,
        summary.materials,
        summary.skins,
        summary.animations,
        summary.images,
        bin_size,
    ));
    if validation.is_valid() {
        if let Some((_, estimate)) = app.glb_export_estimate.as_ref() {
            match estimate {
                Ok(report) => {
                    ui.label(format!(
                        "{}: {} scenes, {} nodes, {} meshes, {} materials, {} skins, {} animations, {} images, {} bytes BIN / {} bytes GLB",
                        app.i18n.tr("glb.export_estimate"),
                        report.output.scenes,
                        report.output.nodes,
                        report.output.meshes,
                        report.output.materials,
                        report.output.skins,
                        report.output.animations,
                        report.output.images,
                        report.output_bin_bytes,
                        report.output_glb_bytes,
                    ));
                }
                Err(error) => {
                    ui.colored_label(
                        Color32::YELLOW,
                        format!(
                            "{}: {error}",
                            app.i18n.tr("glb.export_estimate")
                        ),
                    );
                }
            }
        }
        ui.colored_label(
            Color32::LIGHT_BLUE,
            app.i18n.tr("glb.export_estimate_hint"),
        );
    }
    app.glb_export_selection = selection;
    if app.task_busy {
        ui.label(app.i18n.tr("glb.export_busy"));
    } else if ui.button(app.i18n.tr("glb.export_button")).clicked() {
        app.export_glb();
    }
}

pub fn render_selection_controls(
    ui: &mut three_d::egui::Ui,
    i18n: &I18n,
    document: &GlbDocument,
    catalog: &GlbExportCatalog,
    selection: &mut GlbExportSelection,
    show_skin: bool,
    show_animations: bool,
    show_root_motion: bool,
) {
    use three_d::egui::*;

    let title = i18n.tr("glb.export_selection").to_owned();
    ui.collapsing(title, |ui| {
        let previous_preset = selection.preset;
        ComboBox::from_label(i18n.tr("glb.export_preset"))
            .selected_text(preset_label(i18n, selection.preset))
            .show_ui(ui, |ui| {
                for preset in [
                    GlbExportPreset::PreserveAll,
                    GlbExportPreset::CharacterPackage,
                    GlbExportPreset::SkeletonAnimation,
                ] {
                    ui.selectable_value(
                        &mut selection.preset,
                        preset,
                        preset_label(i18n, preset),
                    );
                }
            });
        if previous_preset != selection.preset {
            if selection.preset == GlbExportPreset::PreserveAll {
                selection.animation_output = AnimationOutputMode::Combined;
                selection.remove_root_motion = false;
                selection.root_motion_node_override = None;
            }
            if selection.preset != GlbExportPreset::PreserveAll
                && selection.selected_nodes.is_empty()
            {
                selection.selected_nodes = catalog
                    .scenes
                    .get(selection.scene_index)
                    .map(|scene| scene.roots.iter().copied().collect())
                    .unwrap_or_default();
            }
            if selection.preset == GlbExportPreset::SkeletonAnimation
                && selection.skin_index.is_none()
            {
                selection.skin_index =
                    catalog.skins.first().map(|skin| skin.index);
            }
        }

        if catalog.scenes.is_empty() {
            ui.colored_label(Color32::YELLOW, i18n.tr("glb.export_no_scenes"));
        } else {
            let previous_scene = selection.scene_index;
            selection.scene_index = selection
                .scene_index
                .min(catalog.scenes.len().saturating_sub(1));
            ComboBox::from_label(i18n.tr("glb.export_scene"))
                .selected_text(scene_label(catalog, selection.scene_index))
                .show_ui(ui, |ui| {
                    for scene in &catalog.scenes {
                        ui.selectable_value(
                            &mut selection.scene_index,
                            scene.index,
                            scene_label_from_scene(scene),
                        );
                    }
                });
            if previous_scene != selection.scene_index {
                selection.selected_nodes = catalog
                    .scenes
                    .get(selection.scene_index)
                    .map(|scene| scene.roots.iter().copied().collect())
                    .unwrap_or_default();
                selection.selected_primitives.clear();
            }
        }

        if show_skin {
            if catalog.skins.is_empty() {
                ui.colored_label(
                    Color32::YELLOW,
                    i18n.tr("glb.export_no_skins"),
                );
            } else {
                selection.skin_index = selection
                    .skin_index
                    .filter(|index| *index < catalog.skins.len());
                ComboBox::from_label(i18n.tr("glb.export_skin"))
                    .selected_text(skin_label(catalog, selection.skin_index))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut selection.skin_index,
                            None,
                            i18n.tr("glb.export_no_skin"),
                        );
                        for skin in &catalog.skins {
                            ui.selectable_value(
                                &mut selection.skin_index,
                                Some(skin.index),
                                format!(
                                    "{} ({} joints)",
                                    skin.name, skin.joint_count
                                ),
                            );
                        }
                    });
            }
        }

        if selection.preset == GlbExportPreset::CharacterPackage {
            render_node_selection(ui, i18n, catalog, selection);
        } else if selection.preset == GlbExportPreset::SkeletonAnimation {
            ui.label(i18n.tr("glb.export_skeleton_hint"));
        }

        if show_animations {
            render_animation_selection(ui, i18n, catalog, selection);
        }
        if show_root_motion {
            render_root_motion_controls(ui, i18n, document, catalog, selection);
        }
    });
}

fn render_node_selection(
    ui: &mut three_d::egui::Ui,
    i18n: &I18n,
    catalog: &GlbExportCatalog,
    selection: &mut GlbExportSelection,
) {
    use three_d::egui::*;

    ui.label(i18n.tr("glb.export_nodes"));
    ui.horizontal(|ui| {
        if ui.button(i18n.tr("glb.export_select_all")).clicked() {
            selection.selected_nodes = catalog
                .scenes
                .get(selection.scene_index)
                .map(|scene| scene.roots.iter().copied().collect())
                .unwrap_or_default();
        }
        if ui.button(i18n.tr("glb.export_select_none")).clicked() {
            selection.selected_nodes.clear();
        }
    });
    let roots = catalog
        .scenes
        .get(selection.scene_index)
        .map(|scene| scene.roots.clone())
        .unwrap_or_default();
    let has_roots = !roots.is_empty();
    let mut rendered_meshes = BTreeSet::new();
    for root in roots {
        render_node(
            ui,
            catalog,
            selection,
            root,
            false,
            0,
            &mut rendered_meshes,
        );
    }
    if !has_roots {
        ui.colored_label(Color32::YELLOW, i18n.tr("glb.export_no_nodes"));
    }
}

fn render_node(
    ui: &mut three_d::egui::Ui,
    catalog: &GlbExportCatalog,
    selection: &mut GlbExportSelection,
    node_index: usize,
    selected_ancestor: bool,
    depth: usize,
    rendered_meshes: &mut BTreeSet<usize>,
) {
    use three_d::egui::*;

    let Some(node) = catalog.nodes.get(node_index) else {
        return;
    };
    let explicitly_selected = selection.selected_nodes.contains(&node_index);
    let mut checked = explicitly_selected || selected_ancestor;
    ui.horizontal(|ui| {
        ui.add_space(depth as f32 * 12.0);
        let label = format!(
            "{} [{}]{}",
            node.name,
            node.index,
            node.mesh
                .map(|mesh| format!(" · Mesh {mesh}"))
                .unwrap_or_default()
        );
        let changed = ui
            .add_enabled(!selected_ancestor, Checkbox::new(&mut checked, label))
            .changed();
        if changed && !selected_ancestor {
            if checked {
                selection.selected_nodes.insert(node_index);
            } else {
                selection.selected_nodes.remove(&node_index);
            }
        }
    });

    if let Some(mesh_index) = node.mesh {
        if rendered_meshes.insert(mesh_index) {
            render_mesh_primitives(
                ui,
                catalog,
                selection,
                mesh_index,
                depth + 1,
            );
        }
    }
    let child_selected = selected_ancestor || explicitly_selected;
    for child in &node.children {
        render_node(
            ui,
            catalog,
            selection,
            *child,
            child_selected,
            depth + 1,
            rendered_meshes,
        );
    }
}

fn render_mesh_primitives(
    ui: &mut three_d::egui::Ui,
    catalog: &GlbExportCatalog,
    selection: &mut GlbExportSelection,
    mesh_index: usize,
    depth: usize,
) {
    let Some(mesh) = catalog.meshes.get(mesh_index) else {
        return;
    };
    ui.indent(format!("mesh-{mesh_index}"), |ui| {
        ui.label(format!("Mesh {}: {}", mesh.index, mesh.name));
        let all_primitives = mesh
            .primitives
            .iter()
            .map(|primitive| primitive.index)
            .collect::<BTreeSet<_>>();
        for primitive in &mesh.primitives {
            let mut checked = selection
                .selected_primitives
                .get(&mesh_index)
                .map(|values| values.contains(&primitive.index))
                .unwrap_or(true);
            let label = format!(
                "{}Primitive {}{}",
                " ".repeat(depth),
                primitive.index,
                primitive
                    .material
                    .map(|material| format!(" · Material {material}"))
                    .unwrap_or_default()
            );
            if ui.checkbox(&mut checked, label).changed() {
                let values = selection
                    .selected_primitives
                    .entry(mesh_index)
                    .or_insert_with(|| all_primitives.clone());
                if checked {
                    values.insert(primitive.index);
                } else {
                    values.remove(&primitive.index);
                }
                if values.len() == all_primitives.len() {
                    selection.selected_primitives.remove(&mesh_index);
                }
            }
        }
    });
}

fn render_animation_selection(
    ui: &mut three_d::egui::Ui,
    i18n: &I18n,
    catalog: &GlbExportCatalog,
    selection: &mut GlbExportSelection,
) {
    use three_d::egui::*;

    ui.separator();
    ui.label(i18n.tr("glb.export_animations"));
    let enabled = selection.preset != GlbExportPreset::PreserveAll;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(enabled, Button::new(i18n.tr("glb.export_select_all")))
            .clicked()
        {
            selection.selected_animations = catalog
                .animations
                .iter()
                .map(|animation| animation.index)
                .collect();
        }
        if ui
            .add_enabled(
                enabled,
                Button::new(i18n.tr("glb.export_select_none")),
            )
            .clicked()
        {
            selection.selected_animations.clear();
        }
    });
    if catalog.animations.is_empty() {
        ui.colored_label(
            Color32::LIGHT_BLUE,
            i18n.tr("glb.export_no_animations"),
        );
    } else {
        for animation in &catalog.animations {
            let mut checked =
                selection.selected_animations.contains(&animation.index);
            if ui
                .add_enabled(
                    enabled,
                    Checkbox::new(
                        &mut checked,
                        format!("{} [{}]", animation.name, animation.index),
                    ),
                )
                .changed()
            {
                if checked {
                    selection.selected_animations.insert(animation.index);
                } else {
                    selection.selected_animations.remove(&animation.index);
                }
            }
        }
    }
    ComboBox::from_label(i18n.tr("glb.export_animation_output"))
        .selected_text(match selection.animation_output {
            AnimationOutputMode::Combined => i18n.tr("glb.export_combined"),
            AnimationOutputMode::Split => i18n.tr("glb.export_split"),
        })
        .show_ui(ui, |ui| {
            let combined =
                selection.animation_output == AnimationOutputMode::Combined;
            if ui
                .add_enabled(
                    enabled,
                    Button::selectable(
                        combined,
                        i18n.tr("glb.export_combined"),
                    ),
                )
                .clicked()
            {
                selection.animation_output = AnimationOutputMode::Combined;
            }
            let split =
                selection.animation_output == AnimationOutputMode::Split;
            if ui
                .add_enabled(
                    enabled && !catalog.animations.is_empty(),
                    Button::selectable(split, i18n.tr("glb.export_split")),
                )
                .clicked()
            {
                selection.animation_output = AnimationOutputMode::Split;
            }
        });
    if !enabled {
        ui.label(i18n.tr("glb.export_preserve_hint"));
    }
}

fn render_root_motion_controls(
    ui: &mut three_d::egui::Ui,
    i18n: &I18n,
    document: &GlbDocument,
    catalog: &GlbExportCatalog,
    selection: &mut GlbExportSelection,
) {
    use three_d::egui::*;

    if selection.preset == GlbExportPreset::PreserveAll {
        selection.remove_root_motion = false;
        selection.root_motion_node_override = None;
    }
    let enabled = selection.preset != GlbExportPreset::PreserveAll
        && !selection.selected_animations.is_empty();
    ui.separator();
    ui.add_enabled(
        enabled,
        Checkbox::new(
            &mut selection.remove_root_motion,
            i18n.tr("glb.export_remove_root_motion"),
        ),
    );
    if !enabled || !selection.remove_root_motion {
        if selection.preset == GlbExportPreset::PreserveAll {
            ui.label(i18n.tr("glb.export_root_motion_preserve_hint"));
        } else if selection.selected_animations.is_empty() {
            ui.label(i18n.tr("glb.export_root_motion_animation_hint"));
        }
        return;
    }

    let info = document.root_motion_info(selection);
    let mut candidates = info
        .as_ref()
        .map(|info| info.candidates.clone())
        .unwrap_or_else(|_| {
            catalog.nodes.iter().map(|node| node.index).collect()
        });
    if let Some(override_node) = selection.root_motion_node_override {
        candidates.push(override_node);
    }
    candidates.sort_unstable();
    candidates.dedup();

    let automatic_text = info
        .as_ref()
        .ok()
        .and_then(|info| info.resolved_node)
        .and_then(|node| catalog.nodes.get(node))
        .map(|node| {
            format!(
                "{} ({})",
                i18n.tr("glb.export_root_motion_auto"),
                node_label(node)
            )
        })
        .unwrap_or_else(|| i18n.tr("glb.export_root_motion_auto").to_owned());
    let selected_text = selection
        .root_motion_node_override
        .and_then(|node| catalog.nodes.get(node).map(node_label))
        .unwrap_or(automatic_text);
    ComboBox::from_label(i18n.tr("glb.export_root_motion_node"))
        .selected_text(selected_text)
        .show_ui(ui, |ui| {
            ui.selectable_value(
                &mut selection.root_motion_node_override,
                None,
                i18n.tr("glb.export_root_motion_auto"),
            );
            for node_index in &candidates {
                let label = catalog
                    .nodes
                    .get(*node_index)
                    .map(node_label)
                    .unwrap_or_else(|| format!("Node {node_index}"));
                ui.selectable_value(
                    &mut selection.root_motion_node_override,
                    Some(*node_index),
                    label,
                );
            }
        });
    ui.label(i18n.tr("glb.export_root_motion_hint"));
    if let Err(error) = info {
        ui.colored_label(
            Color32::YELLOW,
            format!("{}: {error}", i18n.tr("glb.export_root_motion_node")),
        );
    }
}

fn node_label(node: &crate::modules::glb::GlbExportNode) -> String {
    format!("{} [{}]", node.name, node.index)
}

fn render_validation(
    ui: &mut three_d::egui::Ui,
    i18n: &I18n,
    validation: &crate::modules::glb::GlbExportValidation,
) {
    use three_d::egui::*;

    if validation.is_valid() {
        ui.colored_label(Color32::LIGHT_GREEN, i18n.tr("glb.export_valid"));
    } else {
        ui.colored_label(Color32::YELLOW, i18n.tr("glb.export_invalid"));
        for error in validation.errors.iter().take(6) {
            ui.label(error);
        }
    }
    for warning in validation.warnings.iter().take(4) {
        ui.colored_label(Color32::YELLOW, format!("Warning: {warning}"));
    }
}

fn preset_label(i18n: &I18n, preset: GlbExportPreset) -> String {
    i18n.tr(match preset {
        GlbExportPreset::PreserveAll => "glb.export_preset_all",
        GlbExportPreset::CharacterPackage => "glb.export_preset_character",
        GlbExportPreset::SkeletonAnimation => "glb.export_preset_skeleton",
    })
    .to_owned()
}

fn scene_label(catalog: &GlbExportCatalog, index: usize) -> String {
    catalog
        .scenes
        .get(index)
        .map(scene_label_from_scene)
        .unwrap_or_else(|| format!("Scene {index}"))
}

fn scene_label_from_scene(
    scene: &crate::modules::glb::GlbExportScene,
) -> String {
    format!("{} [{}]", scene.name, scene.index)
}

fn skin_label(catalog: &GlbExportCatalog, index: Option<usize>) -> String {
    index
        .and_then(|index| catalog.skins.get(index))
        .map(|skin| format!("{} [{}]", skin.name, skin.index))
        .unwrap_or_else(|| "None".to_owned())
}
