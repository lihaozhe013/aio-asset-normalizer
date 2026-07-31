use std::collections::HashSet;

use three_d::egui;

use crate::modules::i18n::I18n;
use crate::modules::skeleton::Skeleton;

const INDENT: f32 = 16.0;
const ARROW_OFFSET: f32 = 16.0;

const OPEN_STATE_KEY: &str = "bone_tree_open_state";
const OPEN_STATE_VERSION_KEY: &str = "bone_tree_open_state_version";

pub fn render_bone_tree(
    ui: &mut egui::Ui,
    skeleton: &mut Skeleton,
    i18n: &I18n,
) {
    let root_bones: Vec<usize> = (0..skeleton.bones.len())
        .filter(|&i| skeleton.bones[i].parent_index.is_none())
        .collect();

    if root_bones.is_empty() {
        ui.label(i18n.tr("label.no_skeleton_data"));
        return;
    }

    ui.label(i18n.text(
        "bones.count",
        &[("count", skeleton.bones.len().to_string())],
    ));

    let open_state_key = egui::Id::new(OPEN_STATE_KEY);
    let version_key = egui::Id::new(OPEN_STATE_VERSION_KEY);
    let current_version = skeleton.bones.len();

    let stored_version: Option<usize> =
        ui.ctx().memory(|m| m.data.get_temp(version_key));
    let needs_init = stored_version != Some(current_version);

    let open_state: HashSet<usize> = if needs_init {
        let initial: HashSet<usize> = (0..current_version).collect();
        ui.ctx().memory_mut(|m| {
            m.data.insert_temp(open_state_key, initial.clone());
            m.data.insert_temp(version_key, current_version);
        });
        initial
    } else {
        ui.ctx()
            .memory(|m| m.data.get_temp::<HashSet<usize>>(open_state_key))
            .unwrap_or_default()
    };

    let visible = collect_visible(skeleton, &root_bones, &open_state);
    let mut new_open_state = open_state.clone();

    let scroll_id = ui.make_persistent_id("bone_tree_scroll");

    egui::ScrollArea::both()
        .auto_shrink([false; 2])
        .id_salt("bone_tree_scroll")
        .show(ui, |ui| {
            shift_wheel_to_horizontal(ui, scroll_id);
            for item in &visible {
                let bone = &skeleton.bones[item.bone_idx];
                let bone_name = bone.name.clone();
                let has_children = !bone.children.is_empty();

                ui.horizontal(|ui| {
                    ui.add_space(item.depth as f32 * INDENT);

                    if has_children {
                        let is_open = open_state.contains(&item.bone_idx);
                        let cr = egui::CollapsingHeader::new(&bone_name)
                            .id_salt(format!("bone_{}", item.bone_idx))
                            .default_open(is_open)
                            .show_unindented(ui, |_ui| {});

                        if cr.header_response.clicked() {
                            if is_open {
                                new_open_state.remove(&item.bone_idx);
                            } else {
                                new_open_state.insert(item.bone_idx);
                            }
                        }
                    } else {
                        ui.add_space(ARROW_OFFSET);
                        let is_highlighted =
                            skeleton.highlighted_bone == Some(item.bone_idx);
                        let response =
                            ui.selectable_label(is_highlighted, &bone_name);
                        if response.clicked() {
                            skeleton.highlighted_bone = Some(item.bone_idx);
                        }
                    }
                });
            }
        });

    if new_open_state != open_state {
        ui.ctx()
            .memory_mut(|m| m.data.insert_temp(open_state_key, new_open_state));
    }
}

struct FlatBone {
    bone_idx: usize,
    depth: usize,
}

fn collect_visible(
    skeleton: &Skeleton,
    roots: &[usize],
    open_state: &HashSet<usize>,
) -> Vec<FlatBone> {
    let mut result = Vec::new();
    for &root in roots {
        collect_recursive(skeleton, root, 0, open_state, &mut result);
    }
    result
}

fn collect_recursive(
    skeleton: &Skeleton,
    bone_idx: usize,
    depth: usize,
    open_state: &HashSet<usize>,
    result: &mut Vec<FlatBone>,
) {
    result.push(FlatBone { bone_idx, depth });

    if !skeleton.bones[bone_idx].children.is_empty()
        && open_state.contains(&bone_idx)
    {
        for &child in &skeleton.bones[bone_idx].children {
            collect_recursive(skeleton, child, depth + 1, open_state, result);
        }
    }
}

/// Convert shift+vertical-wheel events into horizontal scroll on the given
/// ScrollArea id. Uses raw input events (not `smooth_scroll_delta`) so this
/// still works after a parent ScrollArea has already consumed the delta.
/// ScrollArea::show clamps the offset at draw time, so we just nudge it.
fn shift_wheel_to_horizontal(ui: &mut egui::Ui, scroll_id: egui::Id) {
    let shift_delta_y: f32 = ui.ctx().input(|i| {
        i.events
            .iter()
            .filter_map(|e| match e {
                egui::Event::MouseWheel {
                    delta, modifiers, ..
                } if modifiers.shift => Some(delta.y),
                _ => None,
            })
            .sum()
    });

    if shift_delta_y == 0.0 {
        return;
    }

    ui.ctx().data_mut(|d| {
        if let Some(mut state) =
            d.get_persisted::<egui::scroll_area::State>(scroll_id)
        {
            state.offset.x -= shift_delta_y;
            d.insert_persisted(scroll_id, state);
        }
    });
}
