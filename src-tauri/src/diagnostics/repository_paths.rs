use crate::settings::Settings;
use std::path::{Path, PathBuf};

#[derive(Hash)]
pub(super) struct Candidate {
    pub(super) set_index: usize,
    pub(super) path: PathBuf,
}

pub(super) fn candidates(settings: &Settings) -> Vec<Candidate> {
    registered_candidates(settings)
        .into_iter()
        .filter(|candidate| {
            let git_entry = candidate.path.join(".git");
            candidate.path.is_dir() && (git_entry.is_dir() || git_entry.is_file())
        })
        .collect()
}

pub(super) fn registered_candidates(settings: &Settings) -> Vec<Candidate> {
    let Some(sets) = settings
        .workspace
        .get("sets")
        .and_then(serde_json::Value::as_array)
    else {
        return Vec::new();
    };
    let root = settings
        .workspace
        .get("root")
        .and_then(serde_json::Value::as_str)
        .map(PathBuf::from);
    let mut result = Vec::new();
    for (set_index, set) in sets.iter().enumerate() {
        let Some(items) = set.get("items").and_then(serde_json::Value::as_array) else {
            continue;
        };
        for item in items {
            let Some(path) = item_destination(settings, set, item, root.as_deref()) else {
                continue;
            };
            result.push(Candidate { set_index, path });
        }
    }
    result
}

fn item_destination(
    settings: &Settings,
    set: &serde_json::Value,
    item: &serde_json::Value,
    root: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(path) = item
        .get("path")
        .and_then(serde_json::Value::as_str)
        .filter(|path| !path.is_empty())
    {
        let path = PathBuf::from(path);
        return path.is_absolute().then_some(path);
    }
    let root = root?;
    if !root.is_absolute() {
        return None;
    }
    let name = item.get("name").and_then(serde_json::Value::as_str)?;
    let folder = item
        .get("folder")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(name);
    let segments = if settings
        .workspace
        .get("layout")
        .and_then(serde_json::Value::as_str)
        == Some("custom")
    {
        custom_segments(settings, set, item, folder)
    } else {
        vec![folder.to_string()]
    };
    if segments.is_empty() || segments.iter().any(|part| part == "." || part == "..") {
        return None;
    }
    let path = segments
        .iter()
        .fold(root.to_path_buf(), |path, segment| path.join(segment));
    path.starts_with(root).then_some(path)
}

fn custom_segments(
    settings: &Settings,
    set: &serde_json::Value,
    item: &serde_json::Value,
    folder: &str,
) -> Vec<String> {
    let ref_value = item.get("ref");
    let ref_name = ref_value
        .and_then(|value| value.get("name"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let ref_type = ref_value
        .and_then(|value| value.get("type"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let ref_name = if ref_type == "commit" {
        ref_name.get(..8).unwrap_or(ref_name)
    } else {
        ref_name
    };
    let source_id = item
        .get("repoId")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("");
    let set_name = flatten(
        set.get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(""),
    );
    let source_name = flatten(
        settings
            .sources
            .iter()
            .find(|source| source.id == source_id)
            .map_or("", |source| source.name.as_str()),
    );
    let flattened_ref = flatten(ref_name);
    let values = [
        ("folder", folder),
        (
            "repo",
            item.get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(""),
        ),
        (
            "org",
            item.get("org")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(""),
        ),
        ("set", set_name.as_str()),
        ("source", source_name.as_str()),
        ("ref", flattened_ref.as_str()),
    ];
    let default = "{org}\\{folder}";
    let mut template = settings
        .workspace
        .get("pathTemplate")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(default)
        .trim()
        .to_string();
    if template.is_empty() {
        template = default.to_string();
    }
    if !template.contains("{folder}") && !template.contains("{repo}") {
        template.push_str("\\{folder}");
    }
    let expanded = values.iter().fold(template, |text, (key, value)| {
        text.replace(&format!("{{{key}}}"), value)
    });
    expanded
        .split(['/', '\\'])
        .map(sanitize_segment)
        .filter(|segment| !segment.is_empty() && segment != "." && segment != "..")
        .collect()
}

fn flatten(value: &str) -> String {
    value.replace(['/', '\\'], "-")
}

fn sanitize_segment(segment: &str) -> String {
    if cfg!(windows) {
        segment
            .chars()
            .filter(|character| !character.is_control() && !":*?\"<>|".contains(*character))
            .collect::<String>()
            .trim()
            .trim_end_matches(['.', ' '])
            .to_string()
    } else {
        segment.trim().to_string()
    }
}
