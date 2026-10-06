use crate::commit::{idle_check, quick, run};
use crate::git::{valid_ref, valid_root, OutputPolicy};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[cfg(test)]
mod tests;

const MAX_MESSAGE: usize = 64 * 1024;
const LIST_FORMAT: &str =
    "%(refname:strip=2)%1f%(objecttype)%1f%(objectname)%1f%(*objectname)%1f%(contents:subject)";

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TagInfo {
    name: String,
    commit: String,
    object: String,
    annotated: bool,
    subject: Option<String>,
}

#[derive(Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct CreateTagRequest {
    name: String,
    message: Option<String>,
    target: Option<String>,
    #[serde(default)]
    move_existing: bool,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CreatedTag {
    name: String,
    commit: String,
    object: String,
    annotated: bool,
    previous_object: Option<String>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DeletedTag {
    name: String,
    object: String,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PushedTag {
    remote: String,
    name: String,
    forced: bool,
}

fn parse_tags(text: &str) -> Vec<TagInfo> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split('\u{1f}');
            let (name, kind, object, peeled, subject) = (
                fields.next()?,
                fields.next()?,
                fields.next()?,
                fields.next()?,
                fields.next()?,
            );
            let annotated = kind == "tag";
            let commit = if annotated { peeled } else { object };
            Some(TagInfo {
                name: name.to_string(),
                commit: commit.to_string(),
                object: object.to_string(),
                annotated,
                subject: annotated.then(|| subject.to_string()),
            })
        })
        .collect()
}

fn valid_object_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

async fn validate_name(path: &str, name: &str) -> Result<(), String> {
    valid_ref(name).map_err(|_| "Invalid tag name".to_string())?;
    if name.eq_ignore_ascii_case("head") {
        return Err("Invalid tag name".into());
    }
    let full = format!("refs/tags/{name}");
    let checked = quick(
        path,
        &["check-ref-format", &full],
        &format!("Tag name: {path}"),
        &[0, 1],
    )
    .await?;
    if checked.code != Some(0) {
        return Err("Invalid tag name".into());
    }
    Ok(())
}

async fn validate_remote(path: &str, remote: &str) -> Result<(), String> {
    valid_ref(remote).map_err(|_| "Invalid remote name".to_string())?;
    let remotes = quick(path, &["remote"], &format!("Remotes: {path}"), &[0]).await?;
    if !String::from_utf8_lossy(&remotes.stdout)
        .lines()
        .any(|line| line.trim() == remote)
    {
        return Err(format!("There is no remote named {remote}"));
    }
    Ok(())
}

async fn tag_object(path: &str, name: &str) -> Result<Option<String>, String> {
    let full = format!("refs/tags/{name}");
    let found = quick(
        path,
        &["rev-parse", "--verify", "--quiet", &full],
        &format!("Tag probe: {path}"),
        &[0, 1],
    )
    .await?;
    Ok((found.code == Some(0)).then(|| String::from_utf8_lossy(&found.stdout).trim().to_string()))
}

async fn resolve_commit(path: &str, target: Option<&str>) -> Result<String, String> {
    let target = target
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("HEAD");
    if target.starts_with('-') || target.chars().any(|character| character.is_control()) {
        return Err("Invalid tag target".into());
    }
    let spec = format!("{target}^{{commit}}");
    let found = quick(
        path,
        &["rev-parse", "--verify", "--quiet", &spec],
        &format!("Tag target: {path}"),
        &[0, 1],
    )
    .await?;
    if found.code != Some(0) {
        return Err(found.safe(&format!("Tag target {target} was not found")));
    }
    Ok(String::from_utf8_lossy(&found.stdout).trim().to_string())
}

async fn signs_tags(path: &str) -> Result<bool, String> {
    let setting = quick(
        path,
        &["config", "--type=bool", "--get", "tag.gpgSign"],
        &format!("Tag signing: {path}"),
        &[0, 1],
    )
    .await?;
    Ok(String::from_utf8_lossy(&setting.stdout).trim() == "true")
}

fn clean_message(message: Option<String>) -> Result<Option<String>, String> {
    let Some(message) = message
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    if message.len() > MAX_MESSAGE || message.contains('\0') {
        return Err("Invalid tag message".into());
    }
    Ok(Some(message))
}

#[tauri::command]
pub async fn list_tags(path: String) -> Result<Vec<TagInfo>, String> {
    valid_root(&path)?;
    let format = format!("--format={LIST_FORMAT}");
    let listed = run(
        &path,
        &["for-each-ref", "--sort=refname", &format, "refs/tags"],
        &format!("Tags: {path}"),
        &[0],
        OutputPolicy::Metadata,
        None,
        Duration::from_secs(45),
    )
    .await?;
    Ok(parse_tags(&String::from_utf8_lossy(&listed.stdout)))
}

#[tauri::command]
pub async fn create_tag(path: String, request: CreateTagRequest) -> Result<CreatedTag, String> {
    valid_root(&path)?;
    idle_check()?;
    validate_name(&path, &request.name).await?;
    let message = clean_message(request.message)?;
    if message.is_none() && signs_tags(&path).await? {
        return Err("This repository signs tags; enter a tag message".into());
    }
    let commit = resolve_commit(&path, request.target.as_deref()).await?;
    let previous = tag_object(&path, &request.name).await?;
    if previous.is_some() && !request.move_existing {
        return Err(format!("A tag named {} already exists", request.name));
    }
    let mut args = vec!["tag"];
    if previous.is_some() {
        args.push("--force");
    }
    if let Some(message) = &message {
        args.extend(["-a", "--cleanup=whitespace", "-m", message]);
    }
    args.extend([request.name.as_str(), commit.as_str()]);
    run(
        &path,
        &args,
        &format!("New tag: {path}"),
        &[0],
        OutputPolicy::Text,
        None,
        Duration::from_secs(60),
    )
    .await?;
    let object = tag_object(&path, &request.name)
        .await?
        .ok_or("The tag was not created")?;
    Ok(CreatedTag {
        name: request.name,
        commit,
        object,
        annotated: message.is_some(),
        previous_object: previous,
    })
}

/// Pushes exactly one tag ref. `lease` is the expected remote object id ("" for absent); when set, the push may move the remote tag.
#[tauri::command]
pub async fn push_tag(
    path: String,
    remote: String,
    name: String,
    lease: Option<String>,
) -> Result<PushedTag, String> {
    valid_root(&path)?;
    idle_check()?;
    validate_name(&path, &name).await?;
    validate_remote(&path, &remote).await?;
    if tag_object(&path, &name).await?.is_none() {
        return Err(format!("There is no local tag named {name}"));
    }
    let reference = format!("refs/tags/{name}");
    let forced = lease.is_some();
    let mut args = vec!["push".to_string()];
    let spec = match &lease {
        Some(expected) => {
            if !expected.is_empty() && !valid_object_id(expected) {
                return Err("Invalid expected tag object".into());
            }
            args.push(format!("--force-with-lease={reference}:{expected}"));
            reference.clone()
        }
        None => reference,
    };
    args.extend([remote.clone(), spec]);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    run(
        &path,
        &args,
        &format!("Push tag: {path}"),
        &[0],
        OutputPolicy::Text,
        None,
        Duration::from_secs(180),
    )
    .await?;
    Ok(PushedTag {
        remote,
        name,
        forced,
    })
}

/// Deletes a local tag only; remotes are never touched.
#[tauri::command]
pub async fn delete_tag(path: String, name: String) -> Result<DeletedTag, String> {
    valid_root(&path)?;
    idle_check()?;
    validate_name(&path, &name).await?;
    let Some(object) = tag_object(&path, &name).await? else {
        return Err(format!("There is no local tag named {name}"));
    };
    run(
        &path,
        &["tag", "--delete", &name],
        &format!("Delete tag: {path}"),
        &[0],
        OutputPolicy::Text,
        None,
        Duration::from_secs(45),
    )
    .await?;
    Ok(DeletedTag { name, object })
}

#[tauri::command]
pub async fn delete_remote_tag(
    path: String,
    remote: String,
    name: String,
) -> Result<PushedTag, String> {
    valid_root(&path)?;
    idle_check()?;
    validate_name(&path, &name).await?;
    validate_remote(&path, &remote).await?;
    let reference = format!("refs/tags/{name}");
    run(
        &path,
        &["push", &remote, "--delete", &reference],
        &format!("Delete remote tag: {path}"),
        &[0],
        OutputPolicy::Text,
        None,
        Duration::from_secs(180),
    )
    .await?;
    Ok(PushedTag {
        remote,
        name,
        forced: false,
    })
}
