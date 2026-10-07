use super::repository::{parse_remote, Repository};
use crate::settings::{Settings, Source};
use serde_json::Value;
use std::path::Path;
use tauri::AppHandle;

pub(in crate::github) async fn load(
    app: AppHandle,
    path: String,
    repo: &Repository,
) -> Result<Source, String> {
    let repo = repo.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let settings = crate::settings::load_settings(app)?;
        for_path(&settings, Path::new(&path), &repo)
    })
    .await
    .map_err(|_| "Cannot resolve the stored GitHub source".to_string())?
}

pub(in crate::github) fn for_path(
    settings: &Settings,
    path: &Path,
    repo: &Repository,
) -> Result<Source, String> {
    if !settings.sources.iter().any(|source| {
        matches!(source.kind.as_str(), "github" | "ghe")
            && source.host.eq_ignore_ascii_case(&repo.host)
            || source.kind == "manual"
    }) {
        return Err(format!("Add a source for {}", repo.host));
    }
    let mut matching = None;
    let sets = settings
        .workspace
        .get("sets")
        .and_then(Value::as_array)
        .ok_or("No registered repository sets")?;
    for item in sets
        .iter()
        .filter_map(|set| set.get("items").and_then(Value::as_array))
        .flatten()
    {
        let Some(id) = item.get("id").and_then(Value::as_str) else {
            continue;
        };
        let Some(url) = item.get("url").and_then(Value::as_str) else {
            continue;
        };
        if crate::compare::registered_clone_destination(settings, id, path, url).is_err() {
            continue;
        }
        let source = for_item(settings, item, repo)?;
        if matching
            .as_ref()
            .is_some_and(|previous: &Source| previous.id != source.id)
        {
            return Err(
                "Repository path belongs to several GitHub sources; use separate checkout folders"
                    .into(),
            );
        }
        matching = Some(source);
    }
    matching.ok_or("Repository path is not registered; add it to a set with a GitHub source".into())
}

fn for_item(settings: &Settings, item: &Value, repo: &Repository) -> Result<Source, String> {
    let url = item
        .get("url")
        .and_then(Value::as_str)
        .ok_or("Registered repository has no URL")?;
    let source_id = item
        .get("repoId")
        .and_then(Value::as_str)
        .and_then(|value| value.split_once(':'))
        .map(|(id, _)| id)
        .ok_or("Registered repository has no source identity")?;
    let source = settings
        .sources
        .iter()
        .find(|source| source.id == source_id)
        .ok_or("Registered GitHub source is missing")?;
    crate::settings::valid_id(&source.id)?;
    if !matches!(source.kind.as_str(), "github" | "ghe" | "manual") {
        return Err("Pull requests require a GitHub source".into());
    }
    let source = if source.kind == "manual" || source.host.eq_ignore_ascii_case(&repo.host) {
        source
    } else {
        settings
            .sources
            .iter()
            .find(|source| {
                matches!(source.kind.as_str(), "github" | "ghe")
                    && source.host.eq_ignore_ascii_case(&repo.host)
            })
            .ok_or_else(|| format!("Add a source for {}", repo.host))?
    };
    let registered = parse_remote(url, &repo.host).map_err(|error| error.to_string())?;
    if !repo.same_repo(&registered) {
        return Err("GitHub remote does not match the registered repository".into());
    }
    crate::settings::valid_id(&source.id)?;
    Ok(source.clone())
}
