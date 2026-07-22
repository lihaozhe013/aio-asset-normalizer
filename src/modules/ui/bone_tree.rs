use three_d::egui;

use crate::modules::skeleton::Skeleton;

pub fn render_bone_tree(ui: &mut egui::Ui, skeleton: &mut Skeleton) {
    let root_bones: Vec<usize> = (0..skeleton.bones.len())
        .filter(|&i| skeleton.bones[i].parent_index.is_none())
        .collect();

    if root_bones.is_empty() {
        ui.label("No skeleton data");
        return;
    }

    ui.label(format!("{} bones", skeleton.bones.len()));

    egui::ScrollArea::vertical()
        .max_height(180.0)
        .auto_shrink([false; 2])
        .id_salt("bone_tree_scroll")
        .show(ui, |ui| {
            for &root_idx in &root_bones {
                render_bone_node(ui, skeleton, root_idx, 0);
            }
        });
}

fn render_bone_node(ui: &mut egui::Ui, skeleton: &mut Skeleton, bone_idx: usize, depth: usize) {
    let children: Vec<usize> = skeleton.bones[bone_idx].children.clone();
    let bone_name = skeleton.bones[bone_idx].name.clone();
    let has_children = !children.is_empty();

    if has_children {
        let id = ui.make_persistent_id(format!("bone_{}", bone_idx));

        let header = egui::collapsing_header::CollapsingState::load_with_default_open(
            ui.ctx(),
            id,
            true,
        );

        header
            .show_header(ui, |ui| {
                ui.add_space(depth as f32 * 16.0);
                let is_highlighted = skeleton.highlighted_bone == Some(bone_idx);
                let response = ui.selectable_label(is_highlighted, &bone_name);
                if response.clicked() {
                    skeleton.highlighted_bone = Some(bone_idx);
                }
            })
            .body(|body_ui| {
                for child in children {
                    render_bone_node(body_ui, skeleton, child, depth + 1);
                }
            });
    } else {
        ui.horizontal(|ui| {
            ui.add_space(depth as f32 * 16.0 + 20.0);
            let is_highlighted = skeleton.highlighted_bone == Some(bone_idx);
            let response = ui.selectable_label(is_highlighted, &bone_name);
            if response.clicked() {
                skeleton.highlighted_bone = Some(bone_idx);
            }
        });
    }
}
