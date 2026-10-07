#[cfg(test)]
pub(crate) mod test_fixture;
pub(crate) mod patch;
pub(crate) mod snapshot;
mod content;
mod index;
pub(crate) mod stage;
mod discard;
pub use snapshot::*;
pub use stage::*;
pub use discard::*;

use crate::git::{valid_ref, valid_root, Captured, OutputPolicy, Request};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{atomic::AtomicBool, Arc};
use std::time::Duration;

const MAX_FILES: usize = 2000;
const MAX_MESSAGE: usize = 64 * 1024;

#[derive(Serialize, Default, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChangeFile {
    path: String,
    orig_path: Option<String>,
    /// One-character index status, `.` when the index is unchanged.
    index: String,
    /// One-character working tree status, `.` when unchanged.
    worktree: String,
    /// `ordinary`, `renamed`, `unmerged` or `untracked`.
    kind: String,
    staged_added: Option<u32>,
    staged_removed: Option<u32>,
}

#[derive(Serialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RepoChanges {
    branch: Option<String>,
    detached: bool,
    unborn: bool,
    head: String,
    files: Vec<ChangeFile>,
    truncated: bool,
    skipped: usize,
    author: Option<String>,
    author_error: Option<String>,
    staged_files: usize,
    staged_added: u64,
    staged_removed: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeContent {
    original: String,
    modified: String,
    original_label: String,
    modified_label: String,
    binary: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitResult {
    sha: String,
    subject: String,
}

/// Runs git in `path` with literal pathspecs and without fsmonitor hooks; fails on any unexpected exit code.
pub(crate) async fn run(path: &str, args: &[&str], context: &str, expected: &[i32], policy: OutputPolicy, input: Option<&[u8]>, timeout: Duration) -> Result<Captured, String> {
    run_with_env(path, args, context, expected, policy, input, timeout, &[]).await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_with_env(path: &str, args: &[&str], context: &str, expected: &[i32], policy: OutputPolicy, input: Option<&[u8]>, timeout: Duration, envs: &[(&str, &str)]) -> Result<Captured, String> {
    let mut argv = vec!["-C", path, "-c", "core.fsmonitor=false", "-c", "core.quotepath=false", "--literal-pathspecs"];
    argv.extend_from_slice(args);
    let output = crate::git::execute_cancellable_input_env(Request { args: &argv, context, expected, policy, timeout }, Arc::new(AtomicBool::new(false)), input, envs).await?;
    if !output.code.is_some_and(|code| expected.contains(&code)) {
        return Err(output.last_error());
    }
    Ok(output)
}

pub(crate) async fn quick(path: &str, args: &[&str], context: &str, expected: &[i32]) -> Result<Captured, String> {
    run(path, args, context, expected, OutputPolicy::Text, None, Duration::from_secs(45)).await
}

pub(crate) fn idle_check() -> Result<(), String> {
    if crate::clone::busy() { return Err("A clone, fetch or pull is running; try again when it finishes".into()); }
    Ok(())
}

fn repo_paths(files: &[String]) -> Result<Vec<u8>, String> {
    if files.is_empty() { return Err("Select at least one file".into()); }
    if files.len() > MAX_FILES { return Err("Too many files in one operation".into()); }
    let mut input = Vec::new();
    for file in files {
        crate::paths::relative(file)?;
        input.extend_from_slice(file.as_bytes());
        input.push(0);
    }
    Ok(input)
}

fn parse_status(bytes: &[u8]) -> RepoChanges {
    let mut changes = RepoChanges::default();
    let tokens: Vec<&[u8]> = bytes.split(|byte| *byte == 0).collect();
    let mut i = 0;
    while i < tokens.len() {
        let token = tokens[i];
        i += 1;
        if token.is_empty() { continue; }
        let renamed = token.starts_with(b"2 ");
        let Ok(text) = std::str::from_utf8(token) else {
            changes.skipped += 1;
            if renamed { i += 1; }
            continue;
        };
        if let Some(rest) = text.strip_prefix("# branch.oid ") {
            changes.unborn = rest == "(initial)";
            if !changes.unborn { changes.head = rest.chars().take(8).collect(); }
            continue;
        }
        if let Some(rest) = text.strip_prefix("# branch.head ") {
            if rest == "(detached)" { changes.detached = true; } else { changes.branch = Some(rest.to_string()); }
            continue;
        }
        if text.starts_with('#') { continue; }
        let mut file = ChangeFile::default();
        let xy: &str;
        if let Some(path) = text.strip_prefix("? ") {
            file.path = path.to_string();
            file.kind = "untracked".into();
            xy = "?.";
        } else if text.starts_with("1 ") {
            let fields: Vec<&str> = text.splitn(9, ' ').collect();
            if fields.len() != 9 { continue; }
            xy = fields[1];
            file.path = fields[8].to_string();
            file.kind = "ordinary".into();
        } else if renamed {
            let fields: Vec<&str> = text.splitn(10, ' ').collect();
            let original = tokens.get(i).and_then(|bytes| std::str::from_utf8(bytes).ok()).map(str::to_string);
            i += 1;
            if fields.len() != 10 { continue; }
            xy = fields[1];
            file.path = fields[9].to_string();
            file.orig_path = original;
            file.kind = "renamed".into();
        } else if text.starts_with("u ") {
            let fields: Vec<&str> = text.splitn(11, ' ').collect();
            if fields.len() != 11 { continue; }
            xy = fields[1];
            file.path = fields[10].to_string();
            file.kind = "unmerged".into();
        } else {
            continue;
        }
        if changes.files.len() >= MAX_FILES {
            changes.truncated = true;
            break;
        }
        let mut marks = xy.chars();
        file.index = marks.next().unwrap_or('.').to_string();
        file.worktree = marks.next().unwrap_or('.').to_string();
        changes.files.push(file);
    }
    changes
}

fn parse_numstat(bytes: &[u8]) -> HashMap<String, (Option<u32>, Option<u32>)> {
    let mut stats = HashMap::new();
    let tokens: Vec<&[u8]> = bytes.split(|byte| *byte == 0).collect();
    let mut i = 0;
    while i < tokens.len() {
        let text = String::from_utf8_lossy(tokens[i]).into_owned();
        i += 1;
        let mut parts = text.splitn(3, '\t');
        let (Some(added), Some(removed), Some(rest)) = (parts.next(), parts.next(), parts.next()) else { continue };
        let path = if rest.is_empty() {
            i += 1;
            let Some(renamed) = tokens.get(i) else { break };
            i += 1;
            String::from_utf8_lossy(renamed).into_owned()
        } else {
            rest.to_string()
        };
        stats.insert(path, (added.parse().ok(), removed.parse().ok()));
    }
    stats
}

#[tauri::command]
pub async fn repo_changes(path: String) -> Result<RepoChanges, String> {
    valid_root(&path)?;
    let status = run(&path, &["status", "--porcelain=v2", "-z", "--branch", "--untracked-files=all"], &format!("Changes: {path}"), &[0], OutputPolicy::Metadata, None, Duration::from_secs(45)).await?;
    let mut changes = parse_status(&status.stdout);
    changes.branch = changes.branch.map(|branch| status.safe(&branch));
    let numstat = run(&path, &["diff", "--cached", "--numstat", "-z", "-M", "--no-ext-diff", "--no-textconv"], &format!("Staged summary: {path}"), &[0], OutputPolicy::Metadata, None, Duration::from_secs(45)).await?;
    let stats = parse_numstat(&numstat.stdout);
    for file in &mut changes.files {
        if file.kind == "untracked" || file.index == "." || file.index == "U" { continue; }
        changes.staged_files += 1;
        if let Some((added, removed)) = stats.get(&file.path) {
            file.staged_added = *added;
            file.staged_removed = *removed;
            changes.staged_added += u64::from(added.unwrap_or(0));
            changes.staged_removed += u64::from(removed.unwrap_or(0));
        }
    }
    match quick(&path, &["var", "GIT_AUTHOR_IDENT"], &format!("Identity: {path}"), &[0]).await {
        Ok(output) => {
            let ident = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let words: Vec<&str> = ident.split(' ').collect();
            changes.author = Some(output.safe(&if words.len() > 2 { words[..words.len() - 2].join(" ") } else { ident.clone() }));
        }
        Err(error) => changes.author_error = Some(error),
    }
    Ok(changes)
}

/// Reads one blob from the object database; `None` when the revision has no such file.
async fn blob(path: &str, spec: &str) -> Result<Option<Vec<u8>>, String> {
    let probe = run(path, &["rev-parse", "--verify", "--quiet", spec], &format!("Probe {spec}"), &[0, 1], OutputPolicy::Metadata, None, Duration::from_secs(45)).await?;
    if probe.code == Some(1) { return Ok(None); }
    let output = run(path, &["cat-file", "blob", spec], &format!("Read {spec}"), &[0], OutputPolicy::Metadata, None, Duration::from_secs(45)).await?;
    if output.code == Some(0) {
        if output.stdout.len() > crate::paths::CONTENT_LIMIT { return Err("The file is too large to preview".into()); }
        return Ok(Some(output.stdout));
    }
    Err(output.last_error())
}

fn text_of(bytes: Option<Vec<u8>>) -> (String, bool) {
    let bytes = bytes.unwrap_or_default();
    if bytes.contains(&0) { return (String::new(), true); }
    (String::from_utf8_lossy(&bytes).into_owned(), false)
}

/// Both sides of one file's change, for a read-only side-by-side compare.
#[tauri::command]
pub async fn change_content(path: String, file: String, orig_path: Option<String>, area: String) -> Result<ChangeContent, String> {
    valid_root(&path)?;
    crate::paths::relative(&file)?;
    if let Some(original) = &orig_path { crate::paths::relative(original)?; }
    let before = orig_path.as_deref().unwrap_or(&file);
    let staged_original = if area == "staged" {
        blob(&path, &format!("HEAD:{before}")).await?
    } else if area == "unstaged" {
        blob(&path, &format!(":0:{file}")).await?
    } else {
        None
    };
    let working = {
        let (root, relative, index_bytes) = (path.clone(), file.clone(), staged_original.clone());
        async move {
            let (r, rel) = (root.clone(), relative.clone());
            let read = tauri::async_runtime::spawn_blocking(move || {
                crate::paths::ReadRoot::new(std::path::Path::new(&r), Vec::new())?.read(&rel)
            }).await.map_err(|_| "Could not read the file".to_string())??;
            match read {
                Some(bytes) => {
                    if content::refuse_filters(&root, &relative).await.is_err() {
                        return Ok::<_, String>(Some(bytes.bytes));
                    }
                    match content::clean(&root, &relative, &bytes.bytes, index_bytes.as_deref()).await {
                        Ok(cleaned) => Ok(Some(cleaned)),
                        Err(_) => Ok(Some(bytes.bytes)),
                    }
                }
                None => Ok(None),
            }
        }
    };
    let (original, modified, original_label, modified_label) = match area.as_str() {
        "staged" => (staged_original, blob(&path, &format!(":0:{file}")).await?, "HEAD", "Staged"),
        "unstaged" => (staged_original, working.await?, "Staged", "Working tree"),
        "untracked" => (None, working.await?, "New file", "Working tree"),
        _ => return Err("Unknown diff area".into()),
    };
    let (original, original_binary) = text_of(original);
    let (modified, modified_binary) = text_of(modified);
    Ok(ChangeContent {
        original, modified, original_label: original_label.into(), modified_label: modified_label.into(),
        binary: original_binary || modified_binary,
    })
}

#[tauri::command]
pub async fn stage_paths(path: String, files: Vec<String>) -> Result<(), String> {
    valid_root(&path)?;
    idle_check()?;
    let input = repo_paths(&files)?;
    run(&path, &["add", "-A", "--pathspec-from-file=-", "--pathspec-file-nul"], &format!("Stage: {path}"), &[0], OutputPolicy::Text, Some(&input), Duration::from_secs(60)).await?;
    Ok(())
}

#[tauri::command]
pub async fn unstage_paths(path: String, files: Vec<String>) -> Result<(), String> {
    valid_root(&path)?;
    idle_check()?;
    let input = repo_paths(&files)?;
    let born = quick(&path, &["rev-parse", "--verify", "--quiet", "HEAD"], &format!("HEAD probe: {path}"), &[0, 1]).await?.code == Some(0);
    let args: &[&str] = if born {
        &["restore", "--staged", "--pathspec-from-file=-", "--pathspec-file-nul"]
    } else {
        &["rm", "--cached", "-r", "-q", "--ignore-unmatch", "--pathspec-from-file=-", "--pathspec-file-nul"]
    };
    run(&path, args, &format!("Unstage: {path}"), &[0], OutputPolicy::Text, Some(&input), Duration::from_secs(60)).await?;
    Ok(())
}

/// Commits exactly what is already staged. Never stages, amends or pushes.
#[tauri::command]
pub async fn commit_staged(path: String, message: String) -> Result<CommitResult, String> {
    valid_root(&path)?;
    idle_check()?;
    let message = message.trim();
    if message.is_empty() { return Err("Write a commit message".into()); }
    if message.len() > MAX_MESSAGE || message.contains('\0') { return Err("The commit message is too long or invalid".into()); }
    let staged = quick(&path, &["diff", "--cached", "--quiet", "--no-ext-diff"], &format!("Staged probe: {path}"), &[0, 1]).await?;
    if staged.code == Some(0) { return Err("Nothing is staged".into()); }
    let input = format!("{message}\n");
    run(&path, &["commit", "-F", "-"], &format!("Commit: {path}"), &[0], OutputPolicy::Text, Some(input.as_bytes()), Duration::from_secs(120)).await?;
    let sha = quick(&path, &["rev-parse", "--short", "HEAD"], &format!("Commit id: {path}"), &[0]).await?;
    let subject = quick(&path, &["log", "-1", "--format=%s"], &format!("Commit subject: {path}"), &[0]).await?;
    Ok(CommitResult {
        sha: String::from_utf8_lossy(&sha.stdout).trim().to_string(),
        subject: subject.safe(String::from_utf8_lossy(&subject.stdout).trim()),
    })
}

#[tauri::command]
pub async fn create_branch(path: String, name: String, start: Option<String>, switch: bool) -> Result<(), String> {
    valid_root(&path)?;
    idle_check()?;
    valid_ref(&name).map_err(|_| "Invalid branch name".to_string())?;
    if name.eq_ignore_ascii_case("head") { return Err("Invalid branch name".into()); }
    let format = quick(&path, &["check-ref-format", "--branch", &name], &format!("Branch name: {path}"), &[0, 1, 128]).await?;
    if format.code != Some(0) { return Err("Invalid branch name".into()); }
    let reference = format!("refs/heads/{name}");
    let exists = quick(&path, &["show-ref", "--verify", "--quiet", &reference], &format!("Branch probe: {path}"), &[0, 1]).await?;
    if exists.code == Some(0) { return Err(exists.safe(&format!("A branch named {name} already exists"))); }
    let start = start.map(|value| value.trim().to_string()).filter(|value| !value.is_empty());
    if let Some(start) = &start {
        valid_ref(start).map_err(|_| "Invalid start point".to_string())?;
        let target = format!("{start}^{{commit}}");
        let found = quick(&path, &["rev-parse", "--verify", "--quiet", &target], &format!("Start point: {path}"), &[0, 1]).await?;
        if found.code != Some(0) { return Err(found.safe(&format!("Start point {start} was not found"))); }
    }
    let mut args = if switch { vec!["switch", "-c", &name] } else { vec!["branch", &name] };
    if let Some(start) = &start { args.push(start); }
    run(&path, &args, &format!("New branch: {path}"), &[0], OutputPolicy::Text, None, Duration::from_secs(60)).await?;
    Ok(())
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PushResult {
    remote: String,
    branch: String,
    upstream_set: bool,
}

#[derive(Serialize, Debug)]
pub struct DeletedBranch {
    sha: String,
}

/// Pushes the current branch. Never forces; publishes new branches with an upstream.
#[tauri::command]
pub async fn push_branch(path: String) -> Result<PushResult, String> {
    valid_root(&path)?;
    idle_check()?;
    let head = quick(&path, &["symbolic-ref", "--short", "-q", "HEAD"], &format!("Current branch: {path}"), &[0, 1]).await?;
    if head.code != Some(0) { return Err("HEAD is detached; switch to a branch before pushing".into()); }
    let branch = String::from_utf8_lossy(&head.stdout).trim().to_string();
    valid_ref(&branch).map_err(|_| "Unsupported branch name".to_string())?;
    let upstream = quick(&path, &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"], &format!("Upstream: {path}"), &[0, 128]).await?;
    if upstream.code == Some(0) {
        let key = format!("branch.{branch}.remote");
        let configured = quick(&path, &["config", "--get", &key], &format!("Upstream remote: {path}"), &[0, 1]).await?;
        let remote = String::from_utf8_lossy(&configured.stdout).trim().to_string();
        if remote.is_empty() || remote == "." { return Err("The upstream is a local branch; there is nothing to push to".into()); }
        valid_ref(&remote).map_err(|_| "Unsupported remote name".to_string())?;
        run(&path, &["-c", "push.default=upstream", "push"], &format!("Push: {path}"), &[0], OutputPolicy::Text, None, Duration::from_secs(180)).await?;
        return Ok(PushResult { remote, branch, upstream_set: false });
    }
    let remotes = quick(&path, &["remote"], &format!("Remotes: {path}"), &[0]).await?;
    let names: Vec<String> = String::from_utf8_lossy(&remotes.stdout).lines().map(|line| line.trim().to_string()).filter(|line| !line.is_empty()).collect();
    let remote = match names.as_slice() {
        [] => return Err("No remote is configured for this repository".into()),
        [only] => only.clone(),
        many if many.iter().any(|name| name == "origin") => "origin".to_string(),
        _ => return Err("Several remotes and no origin; push from a terminal to choose one".into()),
    };
    valid_ref(&remote).map_err(|_| "Unsupported remote name".to_string())?;
    run(&path, &["push", "--set-upstream", &remote, &branch], &format!("Publish: {path}"), &[0], OutputPolicy::Text, None, Duration::from_secs(180)).await?;
    Ok(PushResult { remote, branch, upstream_set: true })
}

#[derive(Debug, Serialize)]
pub struct BranchError {
    kind: &'static str,
    message: String,
}

impl From<String> for BranchError {
    fn from(message: String) -> Self { Self { kind: "failed", message } }
}

impl std::fmt::Display for BranchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { self.message.fmt(formatter) }
}

async fn deletion_base(path: &str, name: &str) -> Result<String, String> {
    let upstream = quick(path, &["rev-parse", "--verify", "--quiet", &format!("{name}@{{upstream}}")], &format!("Branch upstream: {path}"), &[0, 1, 128]).await?;
    Ok(if upstream.code == Some(0) { String::from_utf8_lossy(&upstream.stdout).trim().into() } else { "HEAD".into() })
}

async fn branch_checked_out(path: &str, reference: &str) -> Result<bool, String> {
    let worktrees = run(path, &["worktree", "list", "--porcelain", "-z"], &format!("Branch worktree probe: {path}"), &[0], OutputPolicy::Metadata, None, Duration::from_secs(45)).await?;
    let branch = format!("branch {reference}");
    Ok(worktrees.stdout.split(|byte| *byte == 0).any(|field| field == branch.as_bytes()))
}

/// Deletes a local branch only; remotes are never touched. `force` allows unmerged branches.
#[tauri::command]
pub async fn delete_branch(path: String, name: String, force: bool) -> Result<DeletedBranch, BranchError> {
    valid_root(&path)?;
    idle_check()?;
    valid_ref(&name).map_err(|_| "Invalid branch name".to_string())?;
    let reference = format!("refs/heads/{name}");
    let exists = quick(&path, &["show-ref", "--verify", "--quiet", &reference], &format!("Branch probe: {path}"), &[0, 1]).await?;
    if exists.code != Some(0) { return Err(exists.safe(&format!("There is no local branch named {name}")).into()); }
    let tip = quick(&path, &["rev-parse", "--short", &reference], &format!("Branch tip: {path}"), &[0]).await?;
    if !force && !branch_checked_out(&path, &reference).await? {
        let base = deletion_base(&path, &name).await?;
        let merged = quick(&path, &["merge-base", "--is-ancestor", &reference, &base], &format!("Branch merge probe: {path}"), &[0, 1]).await?;
        if merged.code == Some(1) {
            return Err(BranchError { kind: "notMerged", message: merged.safe(&format!("The branch {name} is not fully merged")) });
        }
    }
    let flag = if force { "-D" } else { "-d" };
    run(&path, &["branch", flag, &name], &format!("Delete local branch: {path}"), &[0], OutputPolicy::Text, None, Duration::from_secs(45)).await?;
    Ok(DeletedBranch { sha: String::from_utf8_lossy(&tip.stdout).trim().to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn git_in(dir: &std::path::Path, args: &[&str]) {
        let status = std::process::Command::new("git").arg("-C").arg(dir).args(args).output().unwrap();
        assert!(status.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&status.stderr));
    }

    pub(super) fn repo() -> std::path::PathBuf {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("skein-commit-{}-{nonce}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        git_in(&dir, &["init", "-q", "-b", "main"]);
        git_in(&dir, &["config", "user.name", "Test User"]);
        git_in(&dir, &["config", "user.email", "test@example.test"]);
        git_in(&dir, &["config", "commit.gpgsign", "false"]);
        dir
    }

    #[test]
    fn status_records_cover_renames_untracked_and_branch_headers() {
        let bytes = b"# branch.oid 0123456789abcdef\0# branch.head main\0\
1 .M N... 100644 100644 100644 aaa bbb dir/changed.txt\0\
2 R. N... 100644 100644 100644 aaa bbb R100 new name.txt\0old name.txt\0\
u UU N... 100644 100644 100644 100644 a b c conflict.txt\0\
? untracked.txt\0";
        let changes = parse_status(bytes);
        assert_eq!(changes.branch.as_deref(), Some("main"));
        assert_eq!(changes.head, "01234567");
        assert_eq!(changes.files.len(), 4);
        assert_eq!((changes.files[0].index.as_str(), changes.files[0].worktree.as_str()), (".", "M"));
        assert_eq!(changes.files[1].path, "new name.txt");
        assert_eq!(changes.files[1].orig_path.as_deref(), Some("old name.txt"));
        assert_eq!(changes.files[2].kind, "unmerged");
        assert_eq!(changes.files[3].kind, "untracked");
    }

    #[test]
    fn numstat_handles_plain_renamed_and_binary_entries() {
        let stats = parse_numstat(b"3\t1\ta.txt\0-\t-\timage.png\x002\t0\t\0old.txt\0new.txt\0");
        assert_eq!(stats["a.txt"], (Some(3), Some(1)));
        assert_eq!(stats["image.png"], (None, None));
        assert_eq!(stats["new.txt"], (Some(2), Some(0)));
    }

    #[tokio::test]
    async fn stage_unstage_commit_and_branch_flow() {
        let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
        let dir = repo();
        let path = dir.to_str().unwrap().to_string();
        std::fs::write(dir.join("a.txt"), "one\n").unwrap();
        git_in(&dir, &["add", "a.txt"]);
        git_in(&dir, &["commit", "-q", "-m", "initial"]);
        std::fs::write(dir.join("a.txt"), "one\ntwo\n").unwrap();
        std::fs::write(dir.join("b.txt"), "new\n").unwrap();

        let before = repo_changes(path.clone()).await.unwrap();
        assert_eq!(before.branch.as_deref(), Some("main"));
        assert_eq!(before.files.len(), 2);
        assert_eq!(before.staged_files, 0);
        assert_eq!(before.author.as_deref(), Some("Test User <test@example.test>"));

        let unstaged = change_content(path.clone(), "a.txt".into(), None, "unstaged".into()).await.unwrap();
        assert_eq!((unstaged.original.as_str(), unstaged.modified.as_str()), ("one\n", "one\ntwo\n"));
        let untracked = change_content(path.clone(), "b.txt".into(), None, "untracked".into()).await.unwrap();
        assert_eq!((untracked.original.as_str(), untracked.modified.as_str()), ("", "new\n"));

        assert!(commit_staged(path.clone(), "nothing".into()).await.is_err());
        stage_paths(path.clone(), vec!["a.txt".into()]).await.unwrap();
        let staged = repo_changes(path.clone()).await.unwrap();
        assert_eq!((staged.staged_files, staged.staged_added, staged.staged_removed), (1, 1, 0));
        let staged_content = change_content(path.clone(), "a.txt".into(), None, "staged".into()).await.unwrap();
        assert_eq!((staged_content.original.as_str(), staged_content.modified.as_str()), ("one\n", "one\ntwo\n"));
        assert!(!staged_content.binary);
        std::fs::write(dir.join("blob.bin"), [0u8, 1, 2]).unwrap();
        assert!(change_content(path.clone(), "blob.bin".into(), None, "untracked".into()).await.unwrap().binary);
        std::fs::remove_file(dir.join("blob.bin")).unwrap();

        unstage_paths(path.clone(), vec!["a.txt".into()]).await.unwrap();
        assert_eq!(repo_changes(path.clone()).await.unwrap().staged_files, 0);
        stage_paths(path.clone(), vec!["a.txt".into()]).await.unwrap();

        assert!(commit_staged(path.clone(), "  ".into()).await.is_err());
        let result = commit_staged(path.clone(), "Add second line\n\nBody text".into()).await.unwrap();
        assert_eq!(result.subject, "Add second line");
        let after = repo_changes(path.clone()).await.unwrap();
        assert_eq!(after.files.len(), 1);
        assert_eq!(after.files[0].path, "b.txt");
        assert_eq!(after.files[0].kind, "untracked");

        assert!(create_branch(path.clone(), "bad name".into(), None, true).await.is_err());
        assert!(create_branch(path.clone(), "main".into(), None, true).await.is_err());
        assert!(create_branch(path.clone(), "feature/x".into(), Some("missing-ref".into()), true).await.is_err());
        create_branch(path.clone(), "feature/x".into(), None, true).await.unwrap();
        assert_eq!(repo_changes(path.clone()).await.unwrap().branch.as_deref(), Some("feature/x"));
        create_branch(path.clone(), "keep-here".into(), Some("main".into()), false).await.unwrap();
        assert_eq!(repo_changes(path.clone()).await.unwrap().branch.as_deref(), Some("feature/x"));

        assert!(stage_paths(path.clone(), vec!["../outside.txt".into()]).await.is_err());
        assert!(stage_paths(path.clone(), vec![".git/config".into()]).await.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn unborn_repository_can_stage_unstage_and_commit() {
        let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
        let dir = repo();
        let path = dir.to_str().unwrap().to_string();
        std::fs::write(dir.join("first.txt"), "hello\n").unwrap();
        let changes = repo_changes(path.clone()).await.unwrap();
        assert!(changes.unborn);
        stage_paths(path.clone(), vec!["first.txt".into()]).await.unwrap();
        assert_eq!(repo_changes(path.clone()).await.unwrap().staged_files, 1);
        unstage_paths(path.clone(), vec!["first.txt".into()]).await.unwrap();
        assert_eq!(repo_changes(path.clone()).await.unwrap().staged_files, 0);
        stage_paths(path.clone(), vec!["first.txt".into()]).await.unwrap();
        commit_staged(path.clone(), "First".into()).await.unwrap();
        assert!(!repo_changes(path.clone()).await.unwrap().unborn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn publish_push_and_local_only_branch_deletion() {
        let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
        let dir = repo();
        let remote = std::env::temp_dir().join(format!("skein-remote-{}-{}", std::process::id(), dir.file_name().unwrap().to_string_lossy()));
        std::fs::create_dir_all(&remote).unwrap();
        git_in(&remote, &["init", "-q", "--bare", "-b", "main"]);
        let path = dir.to_str().unwrap().to_string();
        std::fs::write(dir.join("a.txt"), "one\n").unwrap();
        git_in(&dir, &["add", "a.txt"]);
        git_in(&dir, &["commit", "-q", "-m", "initial"]);

        assert!(push_branch(path.clone()).await.unwrap_err().contains("No remote"));
        git_in(&dir, &["remote", "add", "origin", remote.to_str().unwrap()]);
        let published = push_branch(path.clone()).await.unwrap();
        assert_eq!((published.remote.as_str(), published.branch.as_str(), published.upstream_set), ("origin", "main", true));

        std::fs::write(dir.join("a.txt"), "one\ntwo\n").unwrap();
        git_in(&dir, &["commit", "-qam", "second"]);
        let again = push_branch(path.clone()).await.unwrap();
        assert!(!again.upstream_set);
        let remote_head = std::process::Command::new("git").arg("-C").arg(&remote).args(["rev-parse", "main"]).output().unwrap();
        let local_head = std::process::Command::new("git").arg("-C").arg(&dir).args(["rev-parse", "HEAD"]).output().unwrap();
        assert_eq!(remote_head.stdout, local_head.stdout);

        create_branch(path.clone(), "topic".into(), None, true).await.unwrap();
        std::fs::write(dir.join("a.txt"), "one\ntwo\nthree\n").unwrap();
        git_in(&dir, &["commit", "-qam", "topic work"]);
        assert!(push_branch(path.clone()).await.unwrap().upstream_set);
        assert!(delete_branch(path.clone(), "topic".into(), false).await.is_err());
        git_in(&dir, &["switch", "-q", "main"]);
        assert!(delete_branch(path.clone(), "missing".into(), false).await.unwrap_err().message.contains("no local branch"));
        let deleted = delete_branch(path.clone(), "topic".into(), false).await.unwrap();
        assert!(!deleted.sha.is_empty());
        let heads = std::process::Command::new("git").arg("-C").arg(&dir).args(["branch", "--list", "topic"]).output().unwrap();
        assert!(heads.stdout.is_empty());
        let remote_heads = std::process::Command::new("git").arg("-C").arg(&remote).args(["branch", "--list", "topic"]).output().unwrap();
        assert!(!remote_heads.stdout.is_empty(), "the remote branch must be untouched");

        create_branch(path.clone(), "unmerged".into(), None, true).await.unwrap();
        std::fs::write(dir.join("b.txt"), "x\n").unwrap();
        git_in(&dir, &["add", "b.txt"]);
        git_in(&dir, &["commit", "-qm", "only here"]);
        git_in(&dir, &["switch", "-q", "main"]);
        let refused = delete_branch(path.clone(), "unmerged".into(), false).await.unwrap_err();
        assert_eq!(refused.kind, "notMerged", "{refused}");
        delete_branch(path.clone(), "unmerged".into(), true).await.unwrap();
        assert!(delete_branch(path.clone(), "main".into(), true).await.is_err());

        git_in(&dir, &["checkout", "-q", "--detach"]);
        assert!(push_branch(path.clone()).await.unwrap_err().contains("detached"));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&remote);
    }
}

#[cfg(test)]
mod hardening_tests {
    use super::*;

    #[tokio::test]
    async fn missing_blobs_use_plumbing_for_added_deleted_and_unborn_files() {
        let _serial = crate::test_support::serial().await;
        let dir = super::tests::repo();
        let path = dir.to_str().unwrap();
        assert_eq!(blob(path, "HEAD:missing.txt").await.unwrap(), None);
        std::fs::write(dir.join("added.txt"), "added\n").unwrap();
        super::tests::git_in(&dir, &["add", "added.txt"]);
        assert_eq!(blob(path, ":0:missing.txt").await.unwrap(), None);
        assert_eq!(blob(path, ":0:added.txt").await.unwrap(), Some(b"added\n".to_vec()));
        let added = change_content(path.into(), "added.txt".into(), None, "staged".into()).await.unwrap();
        assert_eq!((added.original.as_str(), added.modified.as_str()), ("", "added\n"));
        super::tests::git_in(&dir, &["commit", "-qm", "added"]);
        super::tests::git_in(&dir, &["rm", "-q", "added.txt"]);
        let deleted = change_content(path.into(), "added.txt".into(), None, "staged".into()).await.unwrap();
        assert_eq!((deleted.original.as_str(), deleted.modified.as_str()), ("added\n", ""));
        assert!(blob(dir.join("missing-repo").to_str().unwrap(), "HEAD:missing.txt").await.is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod path_tests {
    use super::*;

    #[tokio::test]
    async fn blob_probes_keep_newlines_in_linux_filenames() {
        if cfg!(windows) { return; }
        let _serial = crate::test_support::serial().await;
        let dir = super::tests::repo();
        std::fs::write(dir.join("line\nname.txt"), "value\n").unwrap();
        super::tests::git_in(&dir, &["add", "line\nname.txt"]);
        assert_eq!(blob(dir.to_str().unwrap(), ":0:line\nname.txt").await.unwrap(), Some(b"value\n".to_vec()));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn checked_out_branch_refusal_does_not_offer_force_for_an_unmerged_upstream() {
        let _serial = crate::test_support::serial().await;
        let dir = super::tests::repo();
        super::tests::git_in(&dir, &["commit", "-qm", "base", "--allow-empty"]);
        super::tests::git_in(&dir, &["switch", "-qc", "topic"]);
        super::tests::git_in(&dir, &["branch", "--set-upstream-to=main", "topic"]);
        super::tests::git_in(&dir, &["commit", "-qm", "unmerged", "--allow-empty"]);
        let error = delete_branch(dir.to_str().unwrap().into(), "topic".into(), false).await.unwrap_err();
        assert_eq!(error.kind, "failed");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
