//! Portable and stable export filename components.
pub fn clean_filename_component(value: &str) -> String {
    let cleaned = value
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
                )
            {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();
    let trimmed =
        cleaned.trim_matches(|character| character == ' ' || character == '.');
    if trimmed.is_empty() {
        "animation".to_owned()
    } else {
        trimmed.to_owned()
    }
}
