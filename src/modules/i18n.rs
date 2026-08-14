use std::collections::HashMap;

use serde::{Deserialize, Serialize};

const EN_US: &str = include_str!("../../assets/locales/en-US.yaml");
const ZH_CN: &str = include_str!("../../assets/locales/zh-CN.yaml");

#[derive(
    Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq,
)]
pub enum LanguagePreference {
    #[serde(rename = "auto")]
    #[default]
    Auto,
    #[serde(rename = "en-US")]
    English,
    #[serde(rename = "zh-CN")]
    Chinese,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    English,
    Chinese,
}

impl LanguagePreference {
    pub fn resolve(self) -> Language {
        match self {
            Self::English => Language::English,
            Self::Chinese => Language::Chinese,
            Self::Auto => detect_system_language(),
        }
    }

    pub fn label(self, i18n: &I18n) -> &str {
        match self {
            Self::Auto => i18n.tr("preferences.language_system_default"),
            Self::English => i18n.tr("preferences.language_english"),
            Self::Chinese => i18n.tr("preferences.language_chinese"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct I18n {
    preference: LanguagePreference,
    language: Language,
    messages: HashMap<String, String>,
}

impl I18n {
    pub fn new(preference: LanguagePreference) -> Self {
        let language = preference.resolve();
        Self {
            preference,
            language,
            messages: load_messages(language),
        }
    }

    pub fn preference(&self) -> LanguagePreference {
        self.preference
    }

    pub fn set_preference(&mut self, preference: LanguagePreference) {
        if self.preference == preference {
            return;
        }
        self.preference = preference;
        self.language = preference.resolve();
        self.messages = load_messages(self.language);
    }

    pub fn tr<'a>(&'a self, key: &'a str) -> &'a str {
        self.messages
            .get(key)
            .map(String::as_str)
            .unwrap_or_else(|| fallback_message(key))
    }

    pub fn text(&self, key: &str, values: &[(&str, String)]) -> String {
        let mut message = self.tr(key).to_owned();
        for (name, value) in values {
            message = message.replace(&format!("{{{name}}}"), value);
        }
        message
    }
}

fn load_messages(language: Language) -> HashMap<String, String> {
    let yaml = match language {
        Language::English => EN_US,
        Language::Chinese => ZH_CN,
    };
    let value: serde_yaml::Value = serde_yaml::from_str(yaml)
        .expect("Built-in locale resource is malformed.");
    let mut messages = HashMap::new();
    flatten_messages(String::new(), &value, &mut messages);
    messages
}

fn flatten_messages(
    prefix: String,
    value: &serde_yaml::Value,
    messages: &mut HashMap<String, String>,
) {
    match value {
        serde_yaml::Value::Mapping(mapping) => {
            for (key, value) in mapping {
                let Some(key) = key.as_str() else {
                    continue;
                };
                let full_key = if prefix.is_empty() {
                    key.to_owned()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten_messages(full_key, value, messages);
            }
        }
        serde_yaml::Value::String(message) => {
            messages.insert(prefix, message.clone());
        }
        _ => {}
    }
}

fn fallback_message(key: &str) -> &str {
    key
}

fn detect_system_language() -> Language {
    let locale = sys_locale::get_locale()
        .or_else(|| {
            ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"]
                .iter()
                .filter_map(|name| std::env::var(name).ok())
                .next()
        })
        .unwrap_or_default()
        .to_ascii_lowercase();

    if locale.starts_with("zh") {
        Language::Chinese
    } else {
        Language::English
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_resources_have_the_same_keys() {
        let english = load_messages(Language::English);
        let chinese = load_messages(Language::Chinese);
        assert_eq!(english.len(), chinese.len());
        assert!(english.keys().all(|key| chinese.contains_key(key)));
    }

    #[test]
    fn interpolation_replaces_named_values() {
        let i18n = I18n::new(LanguagePreference::English);
        let text = i18n.text(
            "files.selected",
            &[("selected", "2".to_owned()), ("total", "5".to_owned())],
        );
        assert_eq!(text, "2 / 5 files");
    }
}
