use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::i18n::LanguagePreference;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPreferences {
    pub version: u32,
    #[serde(default)]
    pub language: LanguagePreference,
    #[serde(default)]
    pub view: ViewPreferences,
    #[serde(default)]
    pub file_tree: FileTreePreferences,
    #[serde(default)]
    pub log_viewer: LogViewerPreferences,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ViewPreferences {
    pub show_grid: bool,
    pub show_axes: bool,
    pub show_origin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileTreePreferences {
    pub show_all_files: bool,
    pub last_opened_directory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LogViewerPreferences {
    pub auto_scroll: bool,
}

pub fn load() -> UserPreferences {
    let template_str = include_str!("../../assets/config_template.yaml");
    let template_value: serde_yaml::Value = serde_yaml::from_str(template_str)
        .expect("Failed to parse built-in config template. The file assets/config_template.yaml may be malformed.");

    let user_path = user_config_path();
    let merged = if user_path.exists() {
        let user_str = std::fs::read_to_string(&user_path).unwrap_or_default();
        let user_value: serde_yaml::Value =
            serde_yaml::from_str(&user_str).unwrap_or(serde_yaml::Value::Null);

        let merged =
            merge_yaml_values(template_value.clone(), user_value.clone());

        if merged != user_value {
            let merged_str = serde_yaml::to_string(&merged).unwrap_or_default();
            if let Some(parent) = user_path.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            std::fs::write(&user_path, merged_str).ok();
        }

        merged
    } else {
        if let Some(parent) = user_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(
            &user_path,
            serde_yaml::to_string(&template_value).unwrap_or_default(),
        )
        .ok();
        template_value
    };

    serde_yaml::from_value(merged)
        .expect("Failed to deserialize merged config. Check field types consistency between the template and Rust struct definitions.")
}

pub fn save(prefs: &UserPreferences) {
    let user_path = user_config_path();
    if let Some(parent) = user_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let yaml_str = serde_yaml::to_string(prefs).unwrap_or_default();
    std::fs::write(&user_path, yaml_str).ok();
}

fn user_config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("aio-asset-normalizer")
        .join("config.yaml")
}

fn merge_yaml_values(
    mut template: serde_yaml::Value,
    user: serde_yaml::Value,
) -> serde_yaml::Value {
    if let (
        serde_yaml::Value::Mapping(t_map),
        serde_yaml::Value::Mapping(u_map),
    ) = (&mut template, &user)
    {
        for (key, value) in u_map {
            match t_map.get_mut(key) {
                Some(t_val) => {
                    let t_clone = t_val.clone();
                    *t_val = merge_yaml_values(t_clone, value.clone());
                }
                None => {
                    t_map.insert(key.clone(), value.clone());
                }
            }
        }
    } else if !user.is_null() {
        template = user;
    }
    template
}
