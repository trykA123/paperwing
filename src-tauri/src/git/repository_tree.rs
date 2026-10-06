#[path = "tree_identity.rs"]
mod identity;

use super::{execute, valid_path, valid_ref, valid_root, Captured, OutputPolicy, Request};
use serde::Serialize;
use std::time::Duration;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeRef {
    pub(super) name: String,
    pub(super) label: String,
    pub(super) sha: String,
    pub(super) current: bool,
    pub(super) symbolic: String,
}

#[derive(Serialize)]
pub struct TreeRemote {
    pub(super) name: String,
    pub(super) urls: Vec<String>,
    pub(super) refs: Vec<TreeRef>,
}

#[derive(Serialize)]
pub struct TreeStash {
    pub(super) name: String,
    pub(super) sha: String,
    pub(super) subject: String,
}

#[derive(Serialize)]
pub struct TreeSubmodule {
    pub(super) path: String,
    pub(super) sha: String,
    pub(super) url: Option<String>,
}

#[derive(Serialize, Default)]
pub struct RepositoryTree {
    pub(super) identity: String,
    pub(super) branches: Vec<TreeRef>,
    pub(super) remotes: Vec<TreeRemote>,
    pub(super) tags: Vec<TreeRef>,
    pub(super) stashes: Vec<TreeStash>,
    pub(super) submodules: Vec<TreeSubmodule>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) warning: Option<String>,
}

async fn tree_output(
    path: &str,
    args: &[&str],
    expected: &[i32],
    policy: OutputPolicy,
) -> Result<Captured, String> {
    let mut argv = vec!["-C", path];
    argv.extend_from_slice(args);
    let output = execute(
        Request {
            args: &argv,
            context: &format!("Tree: {path}"),
            expected,
            policy,
            timeout: Duration::from_secs(45),
        },
        None,
    )
    .await?;
    if !output.code.is_some_and(|code| expected.contains(&code)) {
        return Err(output.last_error());
    }
    Ok(output)
}

pub(super) async fn repository_tree(path: String) -> Result<RepositoryTree, String> {
    valid_root(&path)?;
    let expected_identity = identity::read(&path).await?;
    let mut tree = read_tree(&path).await?;
    if identity::read(&path).await? != expected_identity {
        return Err("Repository identity changed; reload its tree".into());
    }
    tree.identity = expected_identity;
    Ok(tree)
}

async fn read_tree(path: &str) -> Result<RepositoryTree, String> {
    let mut tree = RepositoryTree::default();
    let output = tree_output(
        path,
        &[
            "for-each-ref",
            "--format=%(refname)%09%(objectname)%09%(HEAD)%09%(symref)",
            "refs/heads",
            "refs/tags",
            "refs/remotes",
        ],
        &[0],
        OutputPolicy::Text,
    )
    .await?;
    let mut remote_refs = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let fields: Vec<_> = line.split('\t').collect();
        if fields.len() != 4 {
            continue;
        }
        let make_ref = |name: &str| TreeRef {
            name: name.into(),
            label: output.safe(name),
            sha: fields[1].into(),
            current: fields[2] == "*",
            symbolic: if fields[3].is_empty() {
                String::new()
            } else {
                output.safe(fields[3])
            },
        };
        if let Some(name) = fields[0].strip_prefix("refs/heads/") {
            tree.branches.push(make_ref(name));
        } else if let Some(name) = fields[0].strip_prefix("refs/tags/") {
            tree.tags.push(make_ref(name));
        } else if let Some(name) = fields[0].strip_prefix("refs/remotes/") {
            remote_refs.push((name.to_string(), make_ref(name)));
        }
    }
    let output = tree_output(path, &["remote"], &[0], OutputPolicy::Text).await?;
    for name in String::from_utf8_lossy(&output.stdout).lines() {
        valid_ref(name)?;
        let urls = tree_output(
            path,
            &["remote", "get-url", "--all", name],
            &[0],
            OutputPolicy::Text,
        )
        .await?;
        let refs = remote_refs
            .iter()
            .filter(|(reference, _)| reference.starts_with(&format!("{name}/")))
            .map(|(_, reference)| TreeRef {
                name: reference.name.clone(),
                label: reference.label.clone(),
                sha: reference.sha.clone(),
                current: reference.current,
                symbolic: reference.symbolic.clone(),
            })
            .collect();
        tree.remotes.push(TreeRemote {
            name: output.safe(name),
            urls: urls
                .safe(&String::from_utf8_lossy(&urls.stdout))
                .lines()
                .map(str::to_string)
                .collect(),
            refs,
        });
    }
    let mut warnings = Vec::new();
    if let Err(error) = read_stashes(path, &mut tree).await {
        warnings.push(format!("Stashes unavailable: {error}"));
    }
    if let Err(error) = read_submodules(path, &mut tree).await {
        warnings.push(format!("Submodules unavailable: {error}"));
    }
    if !warnings.is_empty() {
        tree.warning = Some(warnings.join("; "));
    }
    Ok(tree)
}

async fn read_stashes(path: &str, tree: &mut RepositoryTree) -> Result<(), String> {
    let output = tree_output(
        path,
        &["stash", "list", "--format=%gd%x09%H%x09%gs"],
        &[0],
        OutputPolicy::Text,
    )
    .await?;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let fields: Vec<_> = line.splitn(3, '\t').collect();
        if fields.len() == 3 {
            tree.stashes.push(TreeStash {
                name: fields[0].into(),
                sha: fields[1].into(),
                subject: output.safe(fields[2]),
            });
        }
    }
    Ok(())
}

async fn read_submodules(path: &str, tree: &mut RepositoryTree) -> Result<(), String> {
    let modules = std::path::Path::new(path).join(".gitmodules");
    if !modules.exists() {
        return Ok(());
    }
    let module_path = modules.to_str().ok_or("Unsupported .gitmodules path")?;
    valid_path(module_path, true)?;
    if !modules.is_file()
        || std::fs::metadata(&modules)
            .map_err(|_| ".gitmodules unavailable")?
            .len()
            > 256 * 1024
    {
        return Err("Unsupported or oversized .gitmodules".into());
    }
    let output = tree_output(
        path,
        &[
            "config",
            "--no-includes",
            "--file",
            module_path,
            "--null",
            "--get-regexp",
            "^submodule\\..*\\.(path|url)$",
        ],
        &[0, 1],
        OutputPolicy::Metadata,
    )
    .await?;
    let mut config = std::collections::BTreeMap::<String, (Option<String>, Option<String>)>::new();
    for entry in output.stdout.split(|byte| *byte == 0) {
        let text = String::from_utf8_lossy(entry);
        let Some((key, value)) = text.split_once('\n') else {
            continue;
        };
        let Some((name, field)) = key.rsplit_once('.') else {
            continue;
        };
        let pair = config.entry(name.into()).or_default();
        if field == "path" {
            pair.0 = Some(value.into());
        }
        if field == "url" {
            pair.1 = Some(output.safe(value));
        }
    }
    let declared: Vec<String> = config
        .values()
        .filter_map(|(path, _)| path.clone())
        .collect();
    if declared.is_empty() {
        return Ok(());
    }
    let mut args = vec!["ls-files", "--stage", "-z", "--"];
    args.extend(declared.iter().map(String::as_str));
    let listing = tree_output(path, &args, &[0], OutputPolicy::Metadata).await?;
    for entry in listing.stdout.split(|byte| *byte == 0) {
        let text = String::from_utf8_lossy(entry);
        let Some((metadata, relative)) = text.split_once('\t') else {
            continue;
        };
        let fields: Vec<_> = metadata.split(' ').collect();
        if fields.len() == 3 && fields[0] == "160000" && fields[2] == "0" {
            let url = config
                .values()
                .find(|(declared, _)| declared.as_deref() == Some(relative))
                .and_then(|(_, url)| url.clone());
            tree.submodules.push(TreeSubmodule {
                path: listing.safe(relative),
                sha: fields[1].into(),
                url,
            });
        }
    }
    Ok(())
}
