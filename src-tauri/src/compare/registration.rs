use super::{CompareRef, Context, Endpoint, Problem, SetRoots};
use crate::{git, paths};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn text(value: &serde_json::Value, key: &str) -> Result<String, Problem> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| Problem::new("invalidContext", "Saved repository context is incomplete"))
}

fn js_space(character: char) -> bool {
    matches!(character, '\u{0009}'..='\u{000d}' | ' ' | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
}

fn prepared_template(template: &str) -> String {
    let template = template.trim_matches(js_space);
    let mut template = if template.is_empty() {
        "{org}\\{folder}"
    } else {
        template
    }
    .to_string();
    if !template.contains("{folder}") && !template.contains("{repo}") {
        template.push_str("\\{folder}");
    }
    template
}

fn expanded_tokens(template: &str, values: &BTreeMap<&str, String>) -> String {
    let mut expanded = String::new();
    let mut remainder = template;
    while !remainder.is_empty() {
        if let Some(token) = remainder.strip_prefix('{') {
            let length = token
                .bytes()
                .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
                .count();
            if length > 0 && token.as_bytes().get(length) == Some(&b'}') {
                let original = &remainder[..length + 2];
                expanded.push_str(
                    values
                        .get(&token[..length])
                        .map(String::as_str)
                        .unwrap_or(original),
                );
                remainder = &remainder[length + 2..];
                continue;
            }
        }
        let character = remainder.chars().next().unwrap();
        expanded.push(character);
        remainder = &remainder[character.len_utf8()..];
    }
    expanded
}

pub(super) fn template_segments_with_policy(template: &str, values: &BTreeMap<&str, String>, windows: bool) -> Vec<String> {
    let template = prepared_template(template);
    if !windows {
        return template.split(['/', '\\'])
            .flat_map(|part| expanded_tokens(part.trim_matches(js_space), values).split('/').map(str::to_string).collect::<Vec<_>>())
            .filter(|part| !part.is_empty() && part != "." && part != "..")
            .collect();
    }
    let expanded = expanded_tokens(&template, values);
    expanded
        .split(['/', '\\'])
        .map(|part| {
            part.chars()
                .filter(|character| *character > '\u{001f}' && !":*?\"<>|".contains(*character))
                .collect::<String>()
                .trim_matches(js_space)
                .trim_end_matches(['.', ' '])
                .to_string()
        })
        .filter(|part| !part.is_empty() && part != "." && part != "..")
        .collect()
}

pub(super) fn template_segments(template: &str, values: &BTreeMap<&str, String>) -> Vec<String> {
    template_segments_with_policy(template, values, cfg!(windows))
}

pub(super) fn flat_value(value: &str, windows: bool) -> String {
    let mut output = String::new();
    let mut separator = false;
    for character in value.chars() {
        if character == '/' || (windows && character == '\\') {
            if !separator { output.push('-'); }
            separator = true;
        } else {
            output.push(character);
            separator = false;
        }
    }
    output
}

#[cfg(test)]
pub(super) fn layout_destination(root: &str, segments: &[String], windows: bool) -> String {
    let separator = if windows { "\\" } else { "/" };
    let root = root.trim_end_matches(|character| character == '/' || (windows && character == '\\'));
    format!("{root}{separator}{}", segments.join(separator))
}

pub(super) fn bind(settings: &crate::settings::Settings, endpoint: Endpoint) -> Result<Context, Problem> {
    let workspace = &settings.workspace;
    let sets = workspace
        .get("sets")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| Problem::new("invalidContext", "No registered sets"))?;
    let matches: Vec<_> = sets
        .iter()
        .filter(|set| set.get("id").and_then(serde_json::Value::as_str) == Some(&endpoint.set_id))
        .collect();
    if matches.len() != 1 {
        return Err(Problem::new(
            "invalidContext",
            "Unknown or duplicate set identity",
        ));
    }
    let set = matches[0];
    let items = set
        .get("items")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| Problem::new("invalidContext", "No registered items"))?;
    let matches: Vec<_> = items
        .iter()
        .filter(|item| {
            item.get("id").and_then(serde_json::Value::as_str) == Some(&endpoint.item_id)
        })
        .collect();
    if matches.len() != 1 {
        return Err(Problem::new(
            "invalidContext",
            "Unknown or duplicate item identity",
        ));
    }
    let item = matches[0];
    let workspace_root = PathBuf::from(text(workspace, "root")?);
    crate::platform::native_root(
        workspace_root
            .to_str()
            .ok_or_else(|| Problem::new("unsafePath", "Unsupported root encoding"))?,
    )
    .map_err(|error| Problem::new("unsafePath", &error))?;
    if let Some(path) = item.get("path").and_then(serde_json::Value::as_str).filter(|value| !value.is_empty()) {
        return Ok(Context { endpoint, root: PathBuf::from(path), workspace_root });
    }
    let folder = item
        .get("folder")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or(text(item, "name")?);
    let segments = if workspace.get("layout").and_then(serde_json::Value::as_str) == Some("custom")
    {
        let flat = |value: &str| flat_value(value, cfg!(windows));
        let source_id = text(item, "repoId")?
            .split(':')
            .next()
            .unwrap_or("")
            .to_string();
        let source = settings
            .sources
            .iter()
            .find(|source| source.id == source_id)
            .map(|source| flat(&source.name))
            .unwrap_or_default();
        let reference = item
            .get("ref")
            .ok_or_else(|| Problem::new("invalidContext", "Missing checkout ref"))?;
        let name = text(reference, "name")?;
        let values = BTreeMap::from([
            ("folder", folder.clone()),
            ("repo", text(item, "name")?),
            ("org", text(item, "org")?),
            ("set", flat(&text(set, "name")?)),
            ("source", source),
            (
                "ref",
                flat(
                    if reference.get("type").and_then(serde_json::Value::as_str) == Some("commit") {
                        name.get(..8).unwrap_or(&name)
                    } else {
                        &name
                    },
                ),
            ),
        ]);
        let template = workspace
            .get("pathTemplate")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("{org}\\{folder}");
        template_segments(template, &values)
    } else {
        vec![folder]
    };
    if segments.is_empty() {
        return Err(Problem::new("unsafePath", "Empty repository destination"));
    }
    let relative = segments.join("/");
    paths::relative(&relative).map_err(|error| Problem::new("unsafePath", &error))?;
    let root = workspace_root.join(segments.iter().collect::<PathBuf>());
    Ok(Context {
        endpoint,
        root,
        workspace_root,
    })
}

pub(super) fn confined_destination(workspace: &Path, destination: &Path) -> Result<(), String> {
    git::valid_path(destination.to_str().ok_or("Unsupported destination encoding")?, false)?;
    let workspace = std::fs::canonicalize(workspace).map_err(|_| "Workspace root unavailable")?;
    let mut ancestor = destination;
    while !ancestor.exists() {
        ancestor = ancestor.parent().ok_or("Destination is outside registered root")?;
    }
    let ancestor = std::fs::canonicalize(ancestor).map_err(|_| "Destination ancestor unavailable")?;
    if !ancestor.starts_with(&workspace) {
        return Err("Destination is outside registered root".into());
    }
    Ok(())
}

pub(super) fn registered_clone_destination(settings: &crate::settings::Settings, id: &str, destination: &Path, url: &str) -> Result<(), String> {
    let sets = settings.workspace.get("sets").and_then(serde_json::Value::as_array).ok_or("No registered sets")?;
    for set in sets {
        let Some(set_id) = set.get("id").and_then(serde_json::Value::as_str) else { continue; };
        let Some(items) = set.get("items").and_then(serde_json::Value::as_array) else { continue; };
        for item in items {
            if item.get("id").and_then(serde_json::Value::as_str) != Some(id) || item.get("url").and_then(serde_json::Value::as_str) != Some(url) { continue; }
            let context = bind(settings, Endpoint { set_id: set_id.into(), item_id: id.into(), reference: CompareRef::WorkingTree })
                .map_err(|problem| problem.message)?;
            confined_destination(&context.workspace_root, destination)?;
            if crate::platform::same_destination(&context.root, destination)? { return Ok(()); }
        }
    }
    Err("Clone destination does not match a registered repository item".into())
}

pub(super) fn set_roots(settings: &crate::settings::Settings, set_id: &str) -> Result<SetRoots, String> {
    let sets = settings.workspace.get("sets").and_then(serde_json::Value::as_array).ok_or("No registered sets")?;
    let mut own = Vec::new();
    let mut others = std::collections::HashSet::new();
    let mut found = false;
    for set in sets {
        let Some(id) = set.get("id").and_then(serde_json::Value::as_str) else { continue; };
        let Some(items) = set.get("items").and_then(serde_json::Value::as_array) else { continue; };
        found |= id == set_id;
        for item in items {
            let Some(item_id) = item.get("id").and_then(serde_json::Value::as_str) else { continue; };
            let endpoint = Endpoint { set_id: id.into(), item_id: item_id.into(), reference: CompareRef::WorkingTree };
            let Ok(context) = bind(settings, endpoint) else { continue; };
            if context.root == context.workspace_root { continue; }
            let fixed = item.get("path").and_then(serde_json::Value::as_str).is_some_and(|value| !value.is_empty());
            if id == set_id {
                if !fixed { own.push((item_id.to_string(), context.root)); }
            } else { others.insert(crate::platform::destination_key(&context.root)?);
            }
        }
    }
    if !found { return Err("This set has not been saved yet".into()); }
    Ok((own, others))
}

#[cfg(test)]
mod fixed_folder_tests {
    use super::*;

    #[test]
    fn a_saved_open_with_set_protects_the_folder_and_is_never_trashed() {
        let root = std::env::temp_dir().join(format!("skein-fixed-{}", std::process::id()));
        let shared = root.join("o").join("alpha");
        std::fs::create_dir_all(&shared).unwrap();
        let item = |id: &str| serde_json::json!({ "id": id, "name": "alpha", "org": "o", "url": "u", "repoId": "s:1", "ref": { "type": "branch", "name": "main" } });
        let mut fixed = item("f1");
        fixed["path"] = serde_json::json!(shared.to_string_lossy());
        let settings = crate::settings::Settings {
            sources: vec![],
            workspace: serde_json::json!({
                "root": root.to_string_lossy(), "layout": "custom", "pathTemplate": "{org}\\{folder}",
                "sets": [{ "id": "one", "name": "One", "items": [item("i1")] }, { "id": "open", "name": "Open", "items": [fixed] }],
            }),
        };
        let (own, others) = set_roots(&settings, "one").unwrap();
        assert_eq!(own.len(), 1);
        assert!(others.contains(&crate::platform::destination_key(&shared).unwrap()), "the open-with set uses this folder");
        let (own, _) = set_roots(&settings, "open").unwrap();
        assert!(own.is_empty(), "folders opened in place are never trashed");
        let _ = std::fs::remove_dir_all(&root);
    }
}
