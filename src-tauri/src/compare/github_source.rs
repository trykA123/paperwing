use super::{read_root, resolve, CompareRef, Context, Job, Problem};
use crate::github::compare::Request;
use crate::settings::Settings;
use serde::Deserialize;

#[derive(Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Preference {
    Local,
    Github,
}

fn reference(reference: &CompareRef) -> Result<Option<String>, Problem> {
    let value = match reference {
        CompareRef::Branch { name } => format!("heads/{name}"),
        CompareRef::Tag { name } => format!("tags/{name}"),
        CompareRef::RemoteBranch { name } => {
            let branch = name.strip_prefix("origin/").ok_or_else(|| {
                Problem::new(
                    "invalidRef",
                    "GitHub comparison requires the registered origin branch",
                )
            })?;
            format!("heads/{branch}")
        }
        CompareRef::Commit { sha } if super::hex(sha) => sha.clone(),
        CompareRef::Commit { .. } => {
            return Err(Problem::new("invalidRef", "Invalid commit object ID"))
        }
        CompareRef::Head | CompareRef::WorkingTree => return Ok(None),
    };
    crate::git::valid_ref(&value).map_err(|message| Problem::new("invalidRef", &message))?;
    Ok(Some(value))
}

fn endpoint(settings: &Settings, context: &Context, reference: String) -> Result<Request, Problem> {
    let item = settings.workspace["sets"]
        .as_array()
        .and_then(|sets| {
            sets.iter()
                .find(|set| set["id"].as_str() == Some(&context.endpoint.set_id))
        })
        .and_then(|set| set["items"].as_array())
        .and_then(|items| {
            items
                .iter()
                .find(|item| item["id"].as_str() == Some(&context.endpoint.item_id))
        })
        .ok_or_else(|| Problem::new("invalidContext", "Registered repository item is missing"))?;
    let source_id = item["repoId"]
        .as_str()
        .and_then(|id| id.split_once(':'))
        .map(|(id, _)| id)
        .ok_or_else(|| Problem::new("invalidContext", "Registered repository source is missing"))?;
    let matches: Vec<_> = settings
        .sources
        .iter()
        .filter(|source| source.id == source_id)
        .collect();
    let [source] = matches.as_slice() else {
        return Err(Problem::new(
            "invalidContext",
            "Unknown or duplicate repository source",
        ));
    };
    if !matches!(source.kind.as_str(), "github" | "ghe" | "manual") {
        return Err(Problem::new(
            "githubUnavailable",
            "Comparison requires a GitHub source",
        ));
    }
    let url = item["url"]
        .as_str()
        .ok_or_else(|| Problem::new("invalidContext", "Registered repository URL is missing"))?;
    Request::from_url((*source).clone(), url, [reference.clone(), reference])
        .map_err(super::remote::problem)
}

pub(super) fn bind(
    settings: &Settings,
    contexts: &[Context; 2],
) -> Result<Option<Request>, Problem> {
    let Some(base) = reference(&contexts[0].endpoint.reference)? else {
        return Ok(None);
    };
    let Some(head) = reference(&contexts[1].endpoint.reference)? else {
        return Ok(None);
    };
    let mut left = endpoint(settings, &contexts[0], base)?;
    let right = endpoint(settings, &contexts[1], head)?;
    if !left.repository.same_repo(&right.repository) {
        return Ok(None);
    }
    if left.source.id != right.source.id {
        return Ok(None);
    }
    left.head = right.head;
    Ok(Some(left))
}

pub(super) async fn choose(
    settings: &Settings,
    contexts: &[Context; 2],
    preference: Option<Preference>,
    job: &Job,
) -> Result<Option<Request>, Problem> {
    if preference == Some(Preference::Local) {
        return Ok(None);
    }
    let roots = contexts.each_ref().map(|context| context.root.clone());
    let missing_clone =
        tauri::async_runtime::spawn_blocking(move || roots.iter().any(|root| !root.exists()))
            .await
            .map_err(|_| Problem::new("invalidContext", "Repository path check failed"))?;
    if !missing_clone && preference != Some(Preference::Github) {
        return Ok(None);
    }
    let request = match bind(settings, contexts) {
        Ok(Some(request)) => request,
        Ok(None) => return Ok(None),
        Err(problem) if preference == Some(Preference::Github) => return Err(problem),
        Err(_) => return Ok(None),
    };
    if !missing_clone {
        let mut missing_ref = false;
        for context in contexts {
            read_root(context, job).await?;
            missing_ref |= resolve(context, job).await?.is_none();
        }
        if !missing_ref {
            return Ok(None);
        }
        for context in contexts {
            verify_origin(context, &request, job).await?;
        }
    }
    crate::providers::ensure_enabled(&request.source)
        .map_err(|message| Problem::new("githubUnavailable", &message))?;
    Ok(Some(request))
}

async fn verify_origin(context: &Context, request: &Request, job: &Job) -> Result<(), Problem> {
    let output = job
        .output(&context.root, &["remote", "get-url", "origin"])
        .await?;
    let url = super::decode(&output)?;
    let current = Request::from_url(
        request.source.clone(),
        url.trim(),
        [request.base.clone(), request.head.clone()],
    )
    .map_err(super::remote::problem)?;
    if !current.repository.same_repo(&request.repository) {
        return Err(Problem::new(
            "invalidContext",
            "Git origin differs from the registered GitHub repository",
        ));
    }
    Ok(())
}
