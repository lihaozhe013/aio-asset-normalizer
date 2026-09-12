//! Cross-file GLB export recipes.
//!
//! A `GlbExportSelection` contains indices that are meaningful only inside one
//! document.  This module keeps batch intent semantic until a concrete
//! document is available, then resolves it into the existing selection type.

use serde_json::Value;

use super::{
    AnimationOutputMode, GlbDocument, GlbError, GlbExportPreset,
    GlbExportSelection,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatchNameSelector {
    AutomaticUnique,
    Exact(String),
}

impl Default for BatchNameSelector {
    fn default() -> Self {
        Self::AutomaticUnique
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlbBatchRecipe {
    pub preset: GlbExportPreset,
    pub skin: BatchNameSelector,
    pub root_motion_node: BatchNameSelector,
    pub animation_output: AnimationOutputMode,
    pub remove_root_motion: bool,
}

impl Default for GlbBatchRecipe {
    fn default() -> Self {
        Self {
            preset: GlbExportPreset::PreserveAll,
            skin: BatchNameSelector::AutomaticUnique,
            root_motion_node: BatchNameSelector::AutomaticUnique,
            animation_output: AnimationOutputMode::Combined,
            remove_root_motion: false,
        }
    }
}

impl GlbBatchRecipe {
    /// Resolve semantic batch intent into indices belonging to `document`.
    pub fn resolve(
        &self,
        document: &GlbDocument,
    ) -> Result<GlbExportSelection, GlbError> {
        let catalog = document.export_catalog()?;
        let defaults = document.default_export_selection()?;
        let compact = self.preset != GlbExportPreset::PreserveAll;

        let skin_index = if compact {
            resolve_skin(document, &catalog, self.preset, &self.skin)?
        } else {
            None
        };

        if self.preset == GlbExportPreset::SkeletonAnimation
            && catalog.animations.is_empty()
        {
            return Err(GlbError::Invalid(
                "Skeleton Animation requires at least one animation".to_owned(),
            ));
        }

        let root_motion_node_override = if compact && self.remove_root_motion {
            match &self.root_motion_node {
                BatchNameSelector::AutomaticUnique => None,
                BatchNameSelector::Exact(name) => {
                    Some(resolve_node(document, name)?)
                }
            }
        } else {
            None
        };

        let selected_nodes = if self.preset == GlbExportPreset::CharacterPackage
        {
            catalog
                .scenes
                .get(defaults.scene_index)
                .map(|scene| scene.roots.iter().copied().collect())
                .unwrap_or_default()
        } else {
            defaults.selected_nodes
        };

        let selected_animations = if compact {
            catalog
                .animations
                .iter()
                .map(|animation| animation.index)
                .collect()
        } else {
            defaults.selected_animations
        };

        Ok(GlbExportSelection {
            preset: self.preset,
            scene_index: defaults.scene_index,
            skin_index,
            selected_nodes,
            selected_primitives: Default::default(),
            selected_animations,
            animation_output: if compact {
                self.animation_output
            } else {
                AnimationOutputMode::Combined
            },
            remove_root_motion: compact && self.remove_root_motion,
            root_motion_node_override,
        })
    }
}

fn resolve_skin(
    document: &GlbDocument,
    catalog: &super::GlbExportCatalog,
    preset: GlbExportPreset,
    selector: &BatchNameSelector,
) -> Result<Option<usize>, GlbError> {
    match selector {
        BatchNameSelector::AutomaticUnique => {
            if catalog.skins.len() > 1 {
                return Err(GlbError::Invalid(format!(
                    "Batch recipe requires a unique Skin; found {}",
                    catalog.skins.len()
                )));
            }
            return if catalog.skins.is_empty() {
                if preset == GlbExportPreset::SkeletonAnimation {
                    Err(GlbError::Invalid(
                        "Skeleton Animation requires a Skin".to_owned(),
                    ))
                } else {
                    Ok(None)
                }
            } else {
                Ok(Some(catalog.skins[0].index))
            };
        }
        BatchNameSelector::Exact(name) => {
            let matches = authored_indices(document, "skins", name, "Skin")?;
            if matches.len() != 1 {
                return Err(GlbError::Invalid(format!(
                    "Batch recipe requires exactly one Skin named {name:?}; found {}",
                    matches.len()
                )));
            }
            return Ok(Some(matches[0]));
        }
    }
}

fn resolve_node(document: &GlbDocument, name: &str) -> Result<usize, GlbError> {
    let matches = authored_indices(document, "nodes", name, "node")?;
    if matches.len() != 1 {
        return Err(GlbError::Invalid(format!(
            "Batch recipe requires exactly one node named {name:?}; found {}",
            matches.len()
        )));
    }
    Ok(matches[0])
}

fn authored_indices(
    document: &GlbDocument,
    array_name: &str,
    expected_name: &str,
    label: &str,
) -> Result<Vec<usize>, GlbError> {
    let values = document
        .json
        .get(array_name)
        .and_then(Value::as_array)
        .ok_or_else(|| {
            GlbError::Invalid(format!(
                "GLB has no {label} resources for exact-name matching"
            ))
        })?;
    Ok(values
        .iter()
        .enumerate()
        .filter_map(|(index, value)| {
            value
                .get("name")
                .and_then(Value::as_str)
                .filter(|name| *name == expected_name)
                .map(|_| index)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn document(
        skins: &[&str],
        nodes: &[&str],
        animations: usize,
    ) -> GlbDocument {
        let json = json!({
            "asset": {"version": "2.0"},
            "scene": 0,
            "scenes": [{"name": "Main", "nodes": [0]}],
            "nodes": nodes.iter().enumerate().map(|(index, name)| {
                let children = if index == 0 && nodes.len() > 1 {
                    vec![1]
                } else {
                    Vec::<usize>::new()
                };
                json!({"name": name, "children": children})
            }).collect::<Vec<_>>(),
            "skins": skins.iter().map(|name| json!({"name": name, "joints": [1]})).collect::<Vec<_>>(),
            "animations": (0..animations).map(|index| json!({
                "name": format!("Anim {index}"),
                "samplers": [],
                "channels": []
            })).collect::<Vec<_>>(),
            "buffers": [{"byteLength": 0}]
        });
        GlbDocument {
            source_path: None,
            json,
            bin: None,
            dirty: false,
        }
    }

    #[test]
    fn automatic_skin_requires_unique_skin_for_compact_exports() {
        let document = document(&["A", "B"], &["Root", "Joint"], 1);
        let recipe = GlbBatchRecipe {
            preset: GlbExportPreset::SkeletonAnimation,
            ..Default::default()
        };
        let error = recipe.resolve(&document).unwrap_err().to_string();
        assert!(error.contains("unique Skin"));
    }

    #[test]
    fn exact_name_matching_uses_authored_names() {
        let document = document(&["Armature"], &["Root", "Pelvis"], 2);
        let recipe = GlbBatchRecipe {
            preset: GlbExportPreset::SkeletonAnimation,
            skin: BatchNameSelector::Exact("Armature".to_owned()),
            root_motion_node: BatchNameSelector::Exact("Root".to_owned()),
            remove_root_motion: true,
            ..Default::default()
        };
        let selection = recipe.resolve(&document).unwrap();
        assert_eq!(selection.skin_index, Some(0));
        assert_eq!(selection.root_motion_node_override, Some(0));
        assert_eq!(selection.selected_animations, [0, 1].into_iter().collect());
    }

    #[test]
    fn preserve_all_ignores_compact_recipe_options() {
        let document = document(&["Armature"], &["Root", "Pelvis"], 2);
        let recipe = GlbBatchRecipe {
            preset: GlbExportPreset::PreserveAll,
            skin: BatchNameSelector::Exact("Armature".to_owned()),
            root_motion_node: BatchNameSelector::Exact("Root".to_owned()),
            animation_output: super::AnimationOutputMode::Split,
            remove_root_motion: true,
        };
        let selection = recipe.resolve(&document).unwrap();
        assert_eq!(selection.preset, GlbExportPreset::PreserveAll);
        assert_eq!(selection.skin_index, None);
        assert_eq!(
            selection.animation_output,
            super::AnimationOutputMode::Combined
        );
        assert!(!selection.remove_root_motion);
        assert_eq!(selection.root_motion_node_override, None);
    }

    #[test]
    fn character_package_uses_default_roots_and_all_animations() {
        let document = document(&["Armature"], &["Root", "Pelvis"], 2);
        let recipe = GlbBatchRecipe {
            preset: GlbExportPreset::CharacterPackage,
            ..Default::default()
        };
        let selection = recipe.resolve(&document).unwrap();
        assert_eq!(selection.selected_nodes, [0].into_iter().collect());
        assert_eq!(selection.selected_animations, [0, 1].into_iter().collect());
        assert_eq!(selection.skin_index, Some(0));
        assert!(selection.selected_primitives.is_empty());
    }

    #[test]
    fn exact_name_matching_rejects_missing_and_duplicate_names() {
        let missing = document(&["Armature"], &["Root", "Pelvis"], 1);
        let missing_recipe = GlbBatchRecipe {
            preset: GlbExportPreset::SkeletonAnimation,
            skin: BatchNameSelector::Exact("Missing".to_owned()),
            ..Default::default()
        };
        assert!(missing_recipe.resolve(&missing).is_err());

        let duplicate =
            document(&["Armature", "Armature"], &["Root", "Pelvis"], 1);
        let duplicate_recipe = GlbBatchRecipe {
            preset: GlbExportPreset::SkeletonAnimation,
            skin: BatchNameSelector::Exact("Armature".to_owned()),
            ..Default::default()
        };
        let error = duplicate_recipe
            .resolve(&duplicate)
            .unwrap_err()
            .to_string();
        assert!(error.contains("exactly one Skin"));

        let duplicate_node =
            document(&["Armature"], &["Root", "Root", "Joint"], 1);
        let duplicate_node_recipe = GlbBatchRecipe {
            preset: GlbExportPreset::SkeletonAnimation,
            root_motion_node: BatchNameSelector::Exact("Root".to_owned()),
            remove_root_motion: true,
            ..Default::default()
        };
        let error = duplicate_node_recipe
            .resolve(&duplicate_node)
            .unwrap_err()
            .to_string();
        assert!(error.contains("exactly one node"));
    }

    #[test]
    fn skeleton_animation_requires_at_least_one_skin_and_animation() {
        let zero_animation_document =
            document(&["Armature"], &["Root", "Joint"], 0);
        let recipe = GlbBatchRecipe {
            preset: GlbExportPreset::SkeletonAnimation,
            ..Default::default()
        };
        let selection = recipe
            .resolve(&zero_animation_document)
            .unwrap_err()
            .to_string();
        assert!(selection.contains("animation"));

        let no_skin_document = document(&[], &["Root"], 1);
        let selection =
            recipe.resolve(&no_skin_document).unwrap_err().to_string();
        assert!(selection.contains("Skin"));
    }
}
