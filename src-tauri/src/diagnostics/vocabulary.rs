use super::{document, scale, timings};
use std::collections::HashSet;
use std::sync::OnceLock;

pub(super) fn contains(token: &str) -> bool {
    static VOCABULARY: OnceLock<HashSet<String>> = OnceLock::new();
    VOCABULARY
        .get_or_init(|| {
            let mut vocabulary = HashSet::new();
            collect_schema(&document::schema(), &mut vocabulary);
            for text in timings::PHASES
                .iter()
                .chain(timings::OPERATIONS)
                .chain(document::OS_FAMILIES)
                .chain(document::DRIVE_TYPES)
                .chain([&scale::REPO_PREFIX])
            {
                insert(text, &mut vocabulary);
            }
            vocabulary
        })
        .iter()
        .any(|text| contains_word(text, token))
}

fn collect_schema(value: &serde_json::Value, vocabulary: &mut HashSet<String>) {
    match value {
        serde_json::Value::String(text) => insert(text, vocabulary),
        serde_json::Value::Array(values) => {
            for value in values {
                collect_schema(value, vocabulary);
            }
        }
        serde_json::Value::Object(values) => {
            for (key, value) in values {
                insert(key, vocabulary);
                collect_schema(value, vocabulary);
            }
        }
        _ => {}
    }
}

fn insert(text: &str, vocabulary: &mut HashSet<String>) {
    vocabulary.insert(text.to_lowercase());
    vocabulary.extend(
        text.split(|character| !is_word_character(character))
            .filter(|word| !word.is_empty())
            .map(str::to_lowercase),
    );
}

pub(super) fn is_word_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

pub(super) fn contains_word(text: &str, token: &str) -> bool {
    text.match_indices(token).any(|(start, _)| {
        let before = text[..start].chars().next_back();
        let after = text[start + token.len()..].chars().next();
        !before.is_some_and(is_word_character) && !after.is_some_and(is_word_character)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::leak::{serialize_checked, Denylist, REFUSAL};

    #[test]
    fn derives_exempt_tokens_from_serialized_fields_and_static_values() {
        let mut words = HashSet::new();
        collect_schema(&document::schema(), &mut words);
        for text in timings::PHASES
            .iter()
            .chain(timings::OPERATIONS)
            .chain(document::OS_FAMILIES)
            .chain(document::DRIVE_TYPES)
        {
            insert(text, &mut words);
            if let Some((_, suffix)) = text.split_once(['.', '-']) {
                insert(suffix, &mut words);
            }
        }
        let mut denylist = Denylist::default();
        for word in &words {
            denylist.add(&word.to_uppercase());
        }
        assert!(serialize_checked(&words, &denylist).is_ok());
        denylist.add("ClientCorp");
        assert_eq!(serialize_checked(&["ClientCorp"], &denylist), Err(REFUSAL));
    }
}
