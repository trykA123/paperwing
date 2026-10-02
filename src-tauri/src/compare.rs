use crate::{git, paths};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::time::Duration;
use tokio::sync::{Mutex, Semaphore};

const FILE_LIMIT: usize = 20_000;
const BYTE_LIMIT: usize = 64 * 1024 * 1024;
static NEXT: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum CompareRef {
    Branch { name: String },
    RemoteBranch { name: String },
    Tag { name: String },
    Commit { sha: String },
    Head,
    WorkingTree,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Endpoint {
    pub set_id: String,
    pub item_id: String,
    pub reference: CompareRef,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Options {
    #[serde(default)]
    pub normalize_eol: bool,
    #[serde(default)]
    pub ignore_whitespace: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UnavailableReason {
    NotCloned,
    NotRepository,
    UnsupportedEncoding,
    UnmergedIndex,
    RefreshRequired,
    UnbornHead,
    GitCapability,
    Other,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub kind: String,
    pub side: Option<String>,
    pub message: String,
    pub reason: Option<UnavailableReason>,
}

impl Problem {
    fn new(kind: &str, message: &str) -> Self {
        Self {
            kind: kind.into(),
            side: None,
            message: git::safe(message),
            reason: (kind == "unavailable").then_some(UnavailableReason::Other),
        }
    }
    fn side(mut self, side: &str) -> Self {
        self.side = Some(side.into());
        self
    }
    fn unavailable(reason: UnavailableReason, message: &str) -> Self {
        Self {
            reason: Some(reason),
            ..Self::new("unavailable", message)
        }
    }
}

#[derive(Clone, Debug)]
struct Context {
    endpoint: Endpoint,
    root: PathBuf,
    workspace_root: PathBuf,
}

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

fn template_segments(template: &str, values: &BTreeMap<&str, String>) -> Vec<String> {
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
    let mut expanded = String::new();
    let mut remainder = template.as_str();
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

fn bind(settings: &crate::settings::Settings, endpoint: Endpoint) -> Result<Context, Problem> {
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
    git::valid_path(
        workspace_root
            .to_str()
            .ok_or_else(|| Problem::new("unsafePath", "Unsupported root encoding"))?,
        false,
    )
    .map_err(|error| Problem::new("unsafePath", &error))?;
    let folder = item
        .get("folder")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or(text(item, "name")?);
    let segments = if workspace.get("layout").and_then(serde_json::Value::as_str) == Some("custom")
    {
        let flat = |value: &str| {
            let mut output = String::new();
            let mut separator = false;
            for character in value.chars() {
                if character == '/' || character == '\\' {
                    if !separator {
                        output.push('-');
                    }
                    separator = true;
                } else {
                    output.push(character);
                    separator = false;
                }
            }
            output
        };
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

#[derive(Clone)]
struct Job {
    context: String,
    cancel: Arc<AtomicBool>,
    #[cfg(test)]
    temporary_root: Option<PathBuf>,
    #[cfg(test)]
    inventory_started: Option<Arc<tokio::sync::Notify>>,
}

impl Job {
    fn check(&self) -> Result<(), Problem> {
        if self.cancel.load(Ordering::Relaxed) {
            Err(Problem::new("cancelled", "Comparison cancelled"))
        } else {
            Ok(())
        }
    }
    async fn slot<'a>(
        &self,
        slots: &'a Semaphore,
    ) -> Result<tokio::sync::SemaphorePermit<'a>, Problem> {
        loop {
            self.check()?;
            tokio::select! { result = slots.acquire() => return result.map_err(|_| Problem::new("unavailable", "Compare engine unavailable")), _ = tokio::time::sleep(Duration::from_millis(25)) => {} }
        }
    }
    async fn lock<'a, T>(
        &self,
        mutex: &'a Mutex<T>,
    ) -> Result<tokio::sync::MutexGuard<'a, T>, Problem> {
        loop {
            self.check()?;
            tokio::select! { guard = mutex.lock() => return Ok(guard), _ = tokio::time::sleep(Duration::from_millis(25)) => {} }
        }
    }
    async fn run(
        &self,
        root: &Path,
        args: &[&str],
        expected: &[i32],
    ) -> Result<git::Captured, Problem> {
        self.run_input(root, args, expected, None).await
    }

    async fn run_input(
        &self,
        root: &Path,
        args: &[&str],
        expected: &[i32],
        input: Option<&[u8]>,
    ) -> Result<git::Captured, Problem> {
        self.check()?;
        let root = root
            .to_str()
            .ok_or_else(|| Problem::new("unsafePath", "Unsupported root encoding"))?;
        let mut argv = vec![
            "--no-pager",
            "--no-optional-locks",
            "-c",
            "color.ui=false",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.untrackedCache=false",
            "-c",
            "diff.external=",
            "-c",
            "core.hooksPath=",
            "-C",
            root,
        ];
        argv.extend_from_slice(args);
        let request = git::Request {
            args: &argv,
            context: &self.context,
            timeout: Duration::from_secs(45),
            expected,
            policy: git::OutputPolicy::Metadata,
        };
        let result = if input.is_some() {
            git::execute_cancellable_input(request, self.cancel.clone(), input).await
        } else {
            git::execute_cancellable(request, self.cancel.clone()).await
        };
        result.map_err(|error| {
            Problem::new(
                if self.cancel.load(Ordering::Relaxed) {
                    "cancelled"
                } else {
                    "gitError"
                },
                &error,
            )
        })
    }
    async fn output(&self, root: &Path, args: &[&str]) -> Result<Vec<u8>, Problem> {
        let result = self.run(root, args, &[0]).await?;
        if result.code != Some(0) {
            return Err(Problem::new(
                "gitError",
                &git::last_error(&String::from_utf8_lossy(&result.stderr)),
            ));
        }
        Ok(result.stdout)
    }
}

async fn read_root(context: &Context, job: &Job) -> Result<paths::ReadRoot, Problem> {
    if !context.root.exists() {
        return Err(Problem::unavailable(
            UnavailableReason::NotCloned,
            "notCloned",
        ));
    }
    git::valid_path(
        context
            .root
            .to_str()
            .ok_or_else(|| Problem::new("unsafePath", "Unsupported root"))?,
        true,
    )
    .map_err(|error| Problem::new("unsafePath", &error))?;
    let workspace = std::fs::canonicalize(&context.workspace_root)
        .map_err(|_| Problem::new("unsafePath", "Workspace root unavailable"))?;
    if !std::fs::canonicalize(&context.root)
        .map_err(|_| Problem::new("unsafePath", "Root unavailable"))?
        .starts_with(workspace)
    {
        return Err(Problem::new(
            "unsafePath",
            "Repository is outside registered root",
        ));
    }
    let marker = context.root.join(".git");
    git::valid_path(
        marker
            .to_str()
            .ok_or_else(|| Problem::new("unsafePath", "Unsupported metadata"))?,
        true,
    )
    .map_err(|_| Problem::unavailable(UnavailableReason::NotRepository, "notRepository"))?;
    if marker.is_file() {
        let bytes = marker_bytes(&marker)?;
        if bytes.len() > 4096 {
            return Err(Problem::new("unsafePath", "Oversized gitdir marker"));
        }
        let value = decode(&bytes)?;
        let value = value
            .trim()
            .strip_prefix("gitdir: ")
            .ok_or_else(|| Problem::new("unsafePath", "Unsupported gitdir marker"))?;
        let metadata = context.root.join(value);
        git::valid_path(
            metadata
                .to_str()
                .ok_or_else(|| Problem::new("unsafePath", "Unsupported gitdir encoding"))?,
            true,
        )
        .map_err(|error| Problem::new("unsafePath", &error))?;
        let common = metadata.join("commondir");
        if common.exists() {
            git::valid_path(
                common
                    .to_str()
                    .ok_or_else(|| Problem::new("unsafePath", "Unsupported common metadata"))?,
                true,
            )
            .map_err(|error| Problem::new("unsafePath", &error))?;
            let bytes = marker_bytes(&common)?;
            if bytes.len() > 4096 {
                return Err(Problem::new("unsafePath", "Oversized common metadata"));
            }
            let value = decode(&bytes)?;
            let common = metadata.join(value.trim());
            if value.trim().contains("..") {
                return Err(Problem::new(
                    "unsafePath",
                    "Relative common metadata traversal is unsupported",
                ));
            }
            git::valid_path(
                common
                    .to_str()
                    .ok_or_else(|| Problem::new("unsafePath", "Unsupported common metadata"))?,
                true,
            )
            .map_err(|error| Problem::new("unsafePath", &error))?;
        }
    }
    let top = job
        .output(&context.root, &["rev-parse", "--show-toplevel"])
        .await?;
    let top = PathBuf::from(
        String::from_utf8(top)
            .map_err(|_| Problem::new("unsafePath", "Unsupported root encoding"))?
            .trim(),
    );
    if std::fs::canonicalize(top).ok() != std::fs::canonicalize(&context.root).ok() {
        return Err(Problem::new(
            "unsafePath",
            "Registered destination is not a repository root",
        ));
    }
    let mut metadata = Vec::new();
    for option in ["--git-dir", "--git-common-dir"] {
        let value = job
            .run(
                &context.root,
                &["rev-parse", "--path-format=absolute", option],
                &[0, 128, 129],
            )
            .await?;
        if value.code != Some(0) {
            return Err(Problem::unavailable(
                UnavailableReason::GitCapability,
                "Compare requires Git's absolute metadata-path capability (--path-format=absolute)",
            ));
        }
        metadata.push(metadata_path(&value.stdout)?);
    }
    paths::ReadRoot::new(&context.root, metadata)
        .map_err(|error| Problem::new("unsafePath", &error))
}

fn metadata_path(bytes: &[u8]) -> Result<PathBuf, Problem> {
    let output = decode(bytes)?;
    let path = PathBuf::from(output.trim());
    if output.lines().count() != 1 || !path.is_absolute() {
        return Err(Problem::unavailable(
            UnavailableReason::GitCapability,
            "Compare requires Git's absolute metadata-path capability (--path-format=absolute)",
        ));
    }
    Ok(path)
}

fn marker_bytes(path: &Path) -> Result<Vec<u8>, Problem> {
    use std::io::Read;
    let file = std::fs::File::open(path)
        .map_err(|_| Problem::new("unsafePath", "Unreadable metadata marker"))?;
    let mut bytes = Vec::new();
    file.take(4097)
        .read_to_end(&mut bytes)
        .map_err(|_| Problem::new("unsafePath", "Unreadable metadata marker"))?;
    if bytes.len() > 4096 {
        return Err(Problem::new("unsafePath", "Oversized metadata marker"));
    }
    Ok(bytes)
}

fn hex(value: &str) -> bool {
    value.len() >= 4 && value.len() <= 128 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

async fn resolve(context: &Context, job: &Job) -> Result<Option<String>, Problem> {
    let reference = match &context.endpoint.reference {
        CompareRef::Head | CompareRef::WorkingTree => "HEAD".to_string(),
        CompareRef::Branch { name }
        | CompareRef::RemoteBranch { name }
        | CompareRef::Tag { name } => {
            git::valid_ref(name).map_err(|error| Problem::new("invalidRef", &error))?;
            if matches!(context.endpoint.reference, CompareRef::RemoteBranch { .. })
                && !name.contains('/')
            {
                return Err(Problem::new(
                    "invalidRef",
                    "Remote branch must include its remote namespace",
                ));
            }
            let full = format!(
                "refs/{}/{name}",
                match context.endpoint.reference {
                    CompareRef::Branch { .. } => "heads",
                    CompareRef::RemoteBranch { .. } => "remotes",
                    _ => "tags",
                }
            );
            if job
                .run(&context.root, &["check-ref-format", &full], &[0, 1])
                .await?
                .code
                != Some(0)
            {
                return Err(Problem::new("invalidRef", "Invalid branch or tag"));
            }
            full
        }
        CompareRef::Commit { sha } => {
            if !hex(sha) {
                return Err(Problem::new(
                    "invalidRef",
                    "Commit must be an unambiguous hexadecimal object ID",
                ));
            }
            let objects = job
                .output(
                    &context.root,
                    &[
                        "rev-parse",
                        &format!("--disambiguate={}", sha.to_ascii_lowercase()),
                    ],
                )
                .await?;
            let objects: Vec<_> = objects
                .split(|byte| *byte == b'\n')
                .filter(|part| !part.is_empty())
                .collect();
            if objects.len() > 1 {
                return Err(Problem::new("invalidRef", "Ambiguous object ID"));
            }
            if objects.is_empty() {
                return Ok(None);
            }
            decode(objects[0])?.trim().to_string()
        }
    };
    let result = job
        .run(
            &context.root,
            &[
                "rev-parse",
                "--verify",
                "--end-of-options",
                &format!("{reference}^{{commit}}"),
            ],
            &[0, 128],
        )
        .await?;
    if result.code != Some(0) {
        if matches!(context.endpoint.reference, CompareRef::Commit { .. })
            || (reference.starts_with("refs/")
                && job
                    .run(
                        &context.root,
                        &["show-ref", "--verify", "--quiet", &reference],
                        &[0, 1],
                    )
                    .await?
                    .code
                    == Some(0))
        {
            return Err(Problem::new(
                "invalidRef",
                "Reference does not peel to a commit",
            ));
        }
        if matches!(
            context.endpoint.reference,
            CompareRef::Head | CompareRef::WorkingTree
        ) {
            let head = job
                .run(
                    &context.root,
                    &["symbolic-ref", "--quiet", "HEAD"],
                    &[0, 1, 128],
                )
                .await?;
            if head.code == Some(0) {
                let branch = decode(&head.stdout)?;
                if job
                    .run(
                        &context.root,
                        &["show-ref", "--verify", "--quiet", branch.trim()],
                        &[0, 1],
                    )
                    .await?
                    .code
                    == Some(1)
                {
                    return Err(Problem::unavailable(
                        UnavailableReason::UnbornHead,
                        "HEAD has no commit yet; comparison is unavailable without fetching",
                    ));
                }
            }
        }
        return Ok(None);
    }
    let commit = String::from_utf8(result.stdout)
        .map_err(|_| Problem::new("gitError", "Invalid object output"))?
        .trim()
        .to_string();
    if !hex(&commit) {
        return Err(Problem::new("gitError", "Invalid resolved commit"));
    }
    Ok(Some(commit))
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
enum Kind {
    File,
    Symlink,
    Gitlink,
    Directory,
}

#[derive(Clone, Debug)]
struct Entry {
    kind: Kind,
    oid: Option<String>,
    size: Option<u64>,
    mode: String,
    modified_ms: Option<u128>,
    reason: Option<String>,
    fingerprint: Option<u64>,
    source: String,
}

fn decode(bytes: &[u8]) -> Result<String, Problem> {
    String::from_utf8(bytes.to_vec()).map_err(|_| {
        Problem::unavailable(
            UnavailableReason::UnsupportedEncoding,
            "Non-UTF-8 Git paths are unsupported",
        )
    })
}

fn add_folders(entries: &mut BTreeMap<String, Entry>) {
    for path in entries.keys().cloned().collect::<Vec<_>>() {
        let mut parent = path.as_str();
        while let Some((prefix, _)) = parent.rsplit_once('/') {
            entries.entry(prefix.into()).or_insert(Entry {
                kind: Kind::Directory,
                oid: None,
                size: None,
                mode: "040000".into(),
                modified_ms: None,
                reason: None,
                fingerprint: None,
                source: "aggregate".into(),
            });
            parent = prefix;
        }
    }
}

async fn index_sizes(
    context: &Context,
    index: &HashMap<String, (String, String)>,
    job: &Job,
) -> Result<HashMap<String, usize>, Problem> {
    let objects: BTreeSet<_> = index
        .values()
        .filter(|(mode, _)| mode != "160000")
        .map(|(_, oid)| oid.clone())
        .collect();
    let objects: Vec<_> = objects.into_iter().collect();
    let mut sizes = HashMap::new();
    for chunk in objects.chunks(1024) {
        let input = format!("{}\n", chunk.join("\n"));
        let result = job
            .run_input(
                &context.root,
                &["cat-file", "--batch-check"],
                &[0],
                Some(input.as_bytes()),
            )
            .await?;
        if result.code != Some(0) {
            return Err(Problem::new("gitError", "Index object inspection failed"));
        }
        for line in decode(&result.stdout)?.lines() {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() != 3 || fields[1] != "blob" || !hex(fields[0]) {
                return Err(Problem::new("gitError", "Invalid index object metadata"));
            }
            sizes.insert(
                fields[0].into(),
                fields[2]
                    .parse()
                    .map_err(|_| Problem::new("gitError", "Invalid blob size"))?,
            );
        }
    }
    Ok(sizes)
}

async fn index_blobs(
    context: &Context,
    objects: BTreeSet<String>,
    job: &Job,
) -> Result<HashMap<String, Vec<u8>>, Problem> {
    if objects.is_empty() {
        return Ok(HashMap::new());
    }
    let input = format!(
        "{}\n",
        objects.iter().cloned().collect::<Vec<_>>().join("\n")
    );
    let result = job
        .run_input(
            &context.root,
            &["cat-file", "--batch"],
            &[0],
            Some(input.as_bytes()),
        )
        .await?;
    if result.code != Some(0) {
        return Err(Problem::new("gitError", "Index blob batch failed"));
    }
    let mut output = result.stdout.as_slice();
    let mut blobs = HashMap::new();
    for object in objects {
        let end = output
            .iter()
            .position(|byte| *byte == b'\n')
            .ok_or_else(|| Problem::new("gitError", "Invalid batch header"))?;
        let header = decode(&output[..end])?;
        let fields: Vec<_> = header.split_whitespace().collect();
        if fields.len() != 3 || fields[0] != object || fields[1] != "blob" {
            return Err(Problem::new("gitError", "Unexpected batch object"));
        }
        let size: usize = fields[2]
            .parse()
            .map_err(|_| Problem::new("gitError", "Invalid batch size"))?;
        output = &output[end + 1..];
        if size > paths::CONTENT_LIMIT || output.get(size) != Some(&b'\n') {
            return Err(Problem::new("gitError", "Invalid batch content"));
        }
        blobs.insert(object, output[..size].to_vec());
        output = &output[size + 1..];
    }
    if !output.is_empty() {
        return Err(Problem::new("gitError", "Unexpected batch tail"));
    }
    Ok(blobs)
}

async fn inventory(
    context: &Context,
    safe: &paths::ReadRoot,
    commit: &str,
    job: &Job,
) -> Result<BTreeMap<String, Entry>, Problem> {
    let working = matches!(context.endpoint.reference, CompareRef::WorkingTree);
    let mut entries = BTreeMap::new();
    if !working {
        let output = job
            .output(&context.root, &["ls-tree", "-r", "-z", "-l", commit, "--"])
            .await?;
        for record in output
            .split(|byte| *byte == 0)
            .filter(|part| !part.is_empty())
        {
            let tab = record
                .iter()
                .position(|byte| *byte == b'\t')
                .ok_or_else(|| Problem::new("gitError", "Invalid tree record"))?;
            let header = decode(&record[..tab])?;
            let fields: Vec<_> = header.split_whitespace().collect();
            if fields.len() != 4 || !hex(fields[2]) {
                return Err(Problem::new("gitError", "Invalid tree metadata"));
            }
            let kind = match fields[0] {
                "120000" => Kind::Symlink,
                "160000" => Kind::Gitlink,
                "100644" | "100755" => Kind::File,
                _ => return Err(Problem::new("unavailable", "Unsupported tree mode")),
            };
            entries.insert(
                decode(&record[tab + 1..])?,
                Entry {
                    kind,
                    oid: Some(fields[2].into()),
                    size: fields[3].parse().ok(),
                    mode: fields[0].into(),
                    modified_ms: None,
                    reason: None,
                    fingerprint: None,
                    source: "commitBlob".into(),
                },
            );
        }
    } else {
        let staged = job
            .output(&context.root, &["ls-files", "--stage", "-z", "--"])
            .await?;
        let mut index = HashMap::new();
        for record in staged
            .split(|byte| *byte == 0)
            .filter(|part| !part.is_empty())
        {
            let tab = record
                .iter()
                .position(|byte| *byte == b'\t')
                .ok_or_else(|| Problem::new("gitError", "Invalid index record"))?;
            let header = decode(&record[..tab])?;
            let fields: Vec<_> = header.split_whitespace().collect();
            if fields.len() != 3 || fields[2] != "0" {
                return Err(Problem::unavailable(
                    UnavailableReason::UnmergedIndex,
                    "Unmerged index is unsupported",
                ));
            }
            index.insert(
                decode(&record[tab + 1..])?,
                (fields[0].to_string(), fields[1].to_string()),
            );
        }
        let output = job
            .output(
                &context.root,
                &[
                    "ls-files",
                    "-z",
                    "--cached",
                    "--others",
                    "--exclude-standard",
                    "--",
                ],
            )
            .await?;
        let records: Vec<_> = output
            .split(|byte| *byte == 0)
            .filter(|part| !part.is_empty())
            .map(decode)
            .collect::<Result<BTreeSet<_>, _>>()?
            .into_iter()
            .collect();
        if records.len() > FILE_LIMIT {
            return Err(Problem::new("limitExceeded", "Too many comparison files"));
        }
        let sizes = index_sizes(context, &index, job).await?;
        let index = Arc::new(index);
        let mut cache = paths::ReadCache::default();
        let mut start = 0;
        while start < records.len() {
            job.check()?;
            let mut end = start;
            let mut size = 0;
            let mut objects = BTreeSet::new();
            while end < records.len() && end - start < 512 {
                let object = index.get(&records[end]).map(|(_, oid)| oid);
                let bytes = object
                    .and_then(|oid| sizes.get(oid))
                    .copied()
                    .filter(|size| *size <= paths::CONTENT_LIMIT)
                    .unwrap_or(0);
                if size + bytes > 4 * 1024 * 1024 {
                    break;
                }
                size += bytes;
                if let Some(oid) = object.filter(|oid| {
                    sizes
                        .get(*oid)
                        .is_some_and(|size| *size <= paths::CONTENT_LIMIT)
                }) {
                    objects.insert(oid.clone());
                }
                end += 1;
            }
            let blobs = index_blobs(context, objects, job).await?;
            let records = records[start..end].to_vec();
            let safe = safe.clone();
            let index = index.clone();
            let job = job.clone();
            let result = tokio::task::spawn_blocking(move || -> Result<_, Problem> {
                let mut entries = BTreeMap::new();
                for path in records {
                    job.check()?;
                    if let Some(directory) = path.strip_suffix('/') {
                        let reason = safe.resolve_cached(directory, false, &mut cache).err();
                        entries.insert(
                            directory.into(),
                            Entry {
                                kind: Kind::Gitlink,
                                oid: None,
                                size: None,
                                mode: "160000".into(),
                                modified_ms: None,
                                reason,
                                fingerprint: None,
                                source: "untrackedRepository".into(),
                            },
                        );
                        continue;
                    }
                    if entries.contains_key(&path) {
                        continue;
                    }
                    if index.get(&path).is_some_and(|(mode, _)| mode == "160000") {
                        let resolved = safe.resolve_cached(&path, false, &mut cache);
                        let reason = resolved.as_ref().err().cloned();
                        if !resolved.is_ok_and(|path| path.exists()) && reason.is_none() {
                            continue;
                        }
                        entries.insert(
                            path.clone(),
                            Entry {
                                kind: Kind::Gitlink,
                                oid: Some(index[&path].1.clone()),
                                size: None,
                                mode: "160000".into(),
                                modified_ms: None,
                                reason,
                                fingerprint: None,
                                source: "indexGitlink".into(),
                            },
                        );
                        continue;
                    }
                    let result = safe.read_cached(&path, &mut cache);
                    #[cfg(test)]
                    if let Some(started) = &job.inventory_started {
                        started.notify_one();
                    }
                    match result {
                        Ok(Some(file)) => {
                            let link = file.symlink
                                || index.get(&path).is_some_and(|(mode, _)| mode == "120000");
                            let mode = if link {
                                "120000".into()
                            } else {
                                index
                                    .get(&path)
                                    .map(|(mode, _)| mode.clone())
                                    .unwrap_or_else(|| "100644".into())
                            };
                            let oid = index.get(&path).and_then(|(_, oid)| {
                                blobs
                                    .get(oid)
                                    .filter(|bytes| **bytes == file.bytes)
                                    .map(|_| oid.clone())
                            });
                            entries.insert(
                                path,
                                Entry {
                                    kind: if link { Kind::Symlink } else { Kind::File },
                                    oid,
                                    size: Some(file.bytes.len() as u64),
                                    mode,
                                    modified_ms: file.modified_ms,
                                    reason: None,
                                    fingerprint: Some(fingerprint(&file.bytes)),
                                    source: "workingTree".into(),
                                },
                            );
                        }
                        Ok(None) => {}
                        Err(reason) => {
                            entries.insert(
                                path,
                                Entry {
                                    kind: Kind::File,
                                    oid: None,
                                    size: None,
                                    mode: "100644".into(),
                                    modified_ms: None,
                                    reason: Some(reason),
                                    fingerprint: None,
                                    source: "workingTree".into(),
                                },
                            );
                        }
                    }
                }
                Ok((entries, cache))
            })
            .await
            .map_err(|_| Problem::new("unavailable", "Working inventory task failed"))??;
            entries.extend(result.0);
            cache = result.1;
            start = end;
        }
    }
    if entries.len() > FILE_LIMIT {
        return Err(Problem::new("limitExceeded", "Too many comparison files"));
    }
    add_folders(&mut entries);
    Ok(entries)
}

fn fingerprint(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

#[derive(Clone)]
struct Resolved {
    context: Context,
    safe: paths::ReadRoot,
    commit: String,
    files: BTreeMap<String, Entry>,
}

async fn content(
    side: &Resolved,
    path: &str,
    entry: &Entry,
    job: &Job,
) -> Result<Vec<u8>, Problem> {
    job.check()?;
    if let Some(reason) = &entry.reason {
        return Err(Problem::new("unavailable", reason));
    }
    if entry.kind == Kind::Directory {
        return Err(Problem::new("unavailable", "Directory has no blob content"));
    }
    if entry.kind == Kind::Gitlink {
        return entry
            .oid
            .as_ref()
            .map(|oid| oid.as_bytes().to_vec())
            .ok_or_else(|| {
                Problem::new(
                    "unavailable",
                    "Untracked nested repository; contents are opaque",
                )
            });
    }
    if matches!(side.context.endpoint.reference, CompareRef::WorkingTree) {
        let safe = side.safe.clone();
        let path = path.to_string();
        let file = tokio::task::spawn_blocking(move || safe.read(&path))
            .await
            .map_err(|_| Problem::new("unavailable", "Content task failed"))?
            .map_err(|reason| Problem::new("unavailable", &reason))?
            .ok_or_else(|| {
                Problem::new(
                    "staleContent",
                    "Working-tree file was deleted; refresh required",
                )
            })?;
        if entry.fingerprint != Some(fingerprint(&file.bytes))
            || entry.size != Some(file.bytes.len() as u64)
            || entry.modified_ms != file.modified_ms
        {
            return Err(Problem::new(
                "staleContent",
                "Working-tree bytes changed; refresh required",
            ));
        }
        Ok(file.bytes)
    } else {
        if entry
            .size
            .is_none_or(|size| size > paths::CONTENT_LIMIT as u64)
        {
            return Err(Problem::new("unavailable", "Content exceeds read limit"));
        }
        let oid = entry
            .oid
            .as_ref()
            .ok_or_else(|| Problem::new("gitError", "Missing blob identity"))?;
        if !hex(oid) {
            return Err(Problem::new("gitError", "Invalid blob identity"));
        }
        let bytes = job
            .output(&side.context.root, &["cat-file", "blob", oid])
            .await?;
        if bytes.len() > paths::CONTENT_LIMIT {
            return Err(Problem::new("unavailable", "Content exceeds read limit"));
        }
        Ok(bytes)
    }
}

fn normalized(bytes: &[u8], options: &Options) -> Vec<u8> {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return bytes.to_vec();
    };
    if bytes.contains(&0) {
        return bytes.to_vec();
    }
    let text = if options.normalize_eol {
        text.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        text.to_string()
    };
    if options.ignore_whitespace {
        text.chars()
            .filter(|character| {
                *character == '\r' || *character == '\n' || !character.is_whitespace()
            })
            .collect::<String>()
            .into_bytes()
    } else {
        text.into_bytes()
    }
}

fn binary(bytes: &[u8]) -> bool {
    bytes.contains(&0) || std::str::from_utf8(bytes).is_err()
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
enum Status {
    Same,
    Different,
    LeftOnly,
    RightOnly,
    TypeConflict,
    Unavailable,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Lines {
    added: u64,
    removed: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SideInfo {
    kind: Kind,
    size: Option<u64>,
    modified_ms: Option<u128>,
    reason: Option<String>,
    source: String,
}

impl From<&Entry> for SideInfo {
    fn from(entry: &Entry) -> Self {
        Self {
            kind: entry.kind.clone(),
            size: entry.size,
            modified_ms: entry.modified_ms,
            reason: entry.reason.clone(),
            source: entry.source.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Rename {
    from: String,
    to: String,
    score: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRow {
    id: String,
    path: String,
    left: Option<SideInfo>,
    right: Option<SideInfo>,
    raw_status: Status,
    display_status: Status,
    raw_lines: Option<Lines>,
    display_lines: Option<Lines>,
    binary: Option<bool>,
    rename: Option<Rename>,
    reason: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    same: usize,
    different: usize,
    left_only: usize,
    right_only: usize,
    type_conflict: usize,
    unavailable: usize,
    total: usize,
}

impl Summary {
    fn add(&mut self, status: &Status) {
        self.total += 1;
        match status {
            Status::Same => self.same += 1,
            Status::Different => self.different += 1,
            Status::LeftOnly => self.left_only += 1,
            Status::RightOnly => self.right_only += 1,
            Status::TypeConflict => self.type_conflict += 1,
            Status::Unavailable => self.unavailable += 1,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct History {
    available: bool,
    reason: Option<String>,
    left_count: Option<u64>,
    right_count: Option<u64>,
    left_basis: String,
    right_basis: String,
}

#[derive(Clone)]
struct HistorySource {
    root: PathBuf,
    range: String,
}

async fn history(
    left: &Resolved,
    right: &Resolved,
    job: &Job,
) -> Result<(History, Option<HistorySource>), Problem> {
    let basis = |side: &Resolved| {
        if matches!(side.context.endpoint.reference, CompareRef::WorkingTree) {
            "workingTreeHead"
        } else {
            "commit"
        }
        .to_string()
    };
    let mut result = History {
        available: false,
        reason: None,
        left_count: None,
        right_count: None,
        left_basis: basis(left),
        right_basis: basis(right),
    };
    for side in [left, right] {
        if job
            .output(
                &side.context.root,
                &["rev-parse", "--is-shallow-repository"],
            )
            .await?
            == b"true\n"
        {
            result.reason = Some("shallowHistory".into());
            return Ok((result, None));
        }
    }
    let mut shared = None;
    for side in [left, right] {
        let other = if side.context.root == left.context.root {
            &right.commit
        } else {
            &left.commit
        };
        if job
            .run(
                &side.context.root,
                &["cat-file", "-e", &format!("{other}^{{commit}}")],
                &[0, 128],
            )
            .await?
            .code
            == Some(0)
        {
            shared = Some(side.context.root.clone());
            break;
        }
    }
    let Some(root) = shared else {
        result.reason = Some("historyObjectsUnavailable".into());
        return Ok((result, None));
    };
    if job
        .run(&root, &["merge-base", &left.commit, &right.commit], &[0, 1])
        .await?
        .code
        != Some(0)
    {
        result.reason = Some("unrelatedHistory".into());
        return Ok((result, None));
    }
    let range = format!("{}...{}", left.commit, right.commit);
    let counts = decode(
        &job.output(
            &root,
            &["rev-list", "--left-right", "--count", &range, "--"],
        )
        .await?,
    )?;
    let counts: Vec<_> = counts.split_whitespace().collect();
    if counts.len() != 2 {
        return Err(Problem::new("gitError", "Invalid history counts"));
    }
    result.left_count = counts[0].parse().ok();
    result.right_count = counts[1].parse().ok();
    if result.left_count.is_none() || result.right_count.is_none() {
        return Err(Problem::new("gitError", "Invalid history counts"));
    }
    result.available = true;
    Ok((result, Some(HistorySource { root, range })))
}

struct Temporary(PathBuf);
impl Temporary {
    fn new(_job: &Job) -> Result<Self, Problem> {
        #[cfg(not(test))]
        let parent = std::env::temp_dir();
        #[cfg(test)]
        let parent = _job
            .temporary_root
            .clone()
            .unwrap_or_else(std::env::temp_dir);
        git::valid_path(
            parent
                .to_str()
                .ok_or_else(|| Problem::new("unsafePath", "Unsupported temporary root"))?,
            true,
        )
        .map_err(|error| Problem::new("unsafePath", &error))?;
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = parent.join(format!(
            "paperwing-diff-{}-{nonce}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).map_err(|_| {
            Problem::new(
                "unavailable",
                "Could not create private diff materialization",
            )
        })?;
        Ok(Self(path))
    }
    fn write(&self, name: &str, bytes: &[u8]) -> Result<PathBuf, Problem> {
        use std::io::Write;
        let path = self.0.join(name);
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|_| Problem::new("unavailable", "Could not materialize diff content"))?;
        file.write_all(bytes)
            .map_err(|_| Problem::new("unavailable", "Could not materialize diff content"))?;
        Ok(path)
    }
}
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn line_counts(left: &[u8], right: &[u8], job: &Job) -> Result<Option<Lines>, Problem> {
    if binary(left) || binary(right) {
        return Ok(None);
    }
    let temporary = Temporary::new(job)?;
    let left = temporary.write("left", left)?;
    let right = temporary.write("right", right)?;
    let result = job
        .run(
            &temporary.0,
            &[
                "-c",
                "core.attributesFile=",
                "diff",
                "--no-index",
                "--no-ext-diff",
                "--no-textconv",
                "--no-renames",
                "--numstat",
                "-z",
                "--",
                left.to_str().unwrap(),
                right.to_str().unwrap(),
            ],
            &[0, 1],
        )
        .await?;
    if !matches!(result.code, Some(0 | 1)) {
        return Err(Problem::new("gitError", "No-index diff failed"));
    }
    if result.stdout.is_empty() {
        return Ok(Some(Lines {
            added: 0,
            removed: 0,
        }));
    }
    let fields: Vec<_> = result.stdout.splitn(3, |byte| *byte == b'\t').collect();
    if fields.len() != 3 {
        return Err(Problem::new("gitError", "Invalid numstat output"));
    }
    if fields[0] == b"-" || fields[1] == b"-" {
        return Ok(None);
    }
    Ok(Some(Lines {
        added: decode(fields[0])?
            .parse()
            .map_err(|_| Problem::new("gitError", "Invalid added count"))?,
        removed: decode(fields[1])?
            .parse()
            .map_err(|_| Problem::new("gitError", "Invalid removed count"))?,
    }))
}

#[derive(Default)]
struct DiffMetadata {
    lines: HashMap<String, Option<Lines>>,
    renames: HashMap<String, Rename>,
    reason: Option<String>,
}

fn count_result(
    result: Result<Option<Lines>, Problem>,
    reason: &mut Option<String>,
) -> Result<Option<Lines>, Problem> {
    match result {
        Ok(lines) => Ok(lines),
        Err(problem) if problem.kind == "cancelled" => Err(problem),
        Err(problem) => {
            let message = format!("Line counts unavailable: {}", problem.message);
            *reason = Some(
                reason
                    .as_ref()
                    .map(|reason| format!("{reason}; {message}"))
                    .unwrap_or(message),
            );
            Ok(None)
        }
    }
}

async fn diff_metadata(
    left: &Resolved,
    right: &Resolved,
    job: &Job,
) -> Result<DiffMetadata, Problem> {
    let mut metadata = DiffMetadata::default();
    if left.safe.path != right.safe.path {
        return Ok(metadata);
    }
    let working_left = matches!(left.context.endpoint.reference, CompareRef::WorkingTree);
    let working_right = matches!(right.context.endpoint.reference, CompareRef::WorkingTree);
    if working_left || working_right {
        metadata.reason =
            Some("Working-tree rename metadata unavailable; clean filters are not executed".into());
        return Ok(metadata);
    }
    let mut args = vec![
        "diff",
        "--no-ext-diff",
        "--no-textconv",
        "--ignore-submodules=all",
        "--find-renames",
        "-z",
    ];
    if working_left {
        args.extend(["-R", &right.commit]);
    } else {
        args.push(&left.commit);
        if !working_right {
            args.push(&right.commit);
        }
    }
    let mut names = args.clone();
    names.extend(["--name-status", "--"]);
    let output = job.output(&left.context.root, &names).await?;
    let fields: Vec<_> = output
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
        .collect();
    let mut index = 0;
    while index < fields.len() {
        let status = decode(fields[index])?;
        index += 1;
        let path = fields
            .get(index)
            .ok_or_else(|| Problem::new("gitError", "Invalid name-status record"))?;
        index += 1;
        if status.starts_with(['R', 'C']) {
            let to = decode(
                fields
                    .get(index)
                    .ok_or_else(|| Problem::new("gitError", "Invalid rename record"))?,
            )?;
            index += 1;
            let rename = Rename {
                from: decode(path)?,
                to: to.clone(),
                score: status[1..].into(),
            };
            metadata.renames.insert(rename.from.clone(), rename.clone());
            metadata.renames.insert(to, rename);
        }
    }
    let mut stats = args;
    stats.extend(["--numstat", "--"]);
    let output = job.output(&left.context.root, &stats).await?;
    let fields: Vec<_> = output.split(|byte| *byte == 0).collect();
    let mut index = 0;
    while index < fields.len() && !fields[index].is_empty() {
        let row: Vec<_> = fields[index].splitn(3, |byte| *byte == b'\t').collect();
        index += 1;
        if row.len() != 3 {
            return Err(Problem::new("gitError", "Invalid numstat record"));
        }
        let lines = if row[0] == b"-" || row[1] == b"-" {
            None
        } else {
            Some(Lines {
                added: decode(row[0])?
                    .parse()
                    .map_err(|_| Problem::new("gitError", "Invalid numstat count"))?,
                removed: decode(row[1])?
                    .parse()
                    .map_err(|_| Problem::new("gitError", "Invalid numstat count"))?,
            })
        };
        if row[2].is_empty() {
            index += 2;
            if index > fields.len() {
                return Err(Problem::new("gitError", "Invalid rename numstat record"));
            }
        } else {
            metadata.lines.insert(decode(row[2])?, lines);
        }
    }
    Ok(metadata)
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedEndpoint {
    endpoint: Endpoint,
    commit: String,
    history_basis: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    id: String,
    generation: u64,
    left: ResolvedEndpoint,
    right: ResolvedEndpoint,
    raw: Summary,
    display: Summary,
    history: History,
    file_count: usize,
    options: Options,
}

#[derive(Clone)]
struct Prepared {
    view: Snapshot,
    left: Resolved,
    right: Resolved,
    rows: Vec<FileRow>,
    history_source: Option<HistorySource>,
}

#[derive(Default)]
struct Fetch {
    epoch: u64,
    problem: Option<Problem>,
}

pub struct Service {
    sessions: Mutex<HashMap<String, Session>>,
    fetches: Mutex<HashMap<PathBuf, Arc<Mutex<Fetch>>>>,
    slots: Semaphore,
}

struct Session {
    left: Context,
    right: Context,
    generation: u64,
    cancel: Arc<AtomicBool>,
    prepared: Option<Arc<Prepared>>,
}

impl Default for Service {
    fn default() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            fetches: Mutex::new(HashMap::new()),
            slots: Semaphore::new(4),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Opened {
    id: String,
    generation: u64,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum RefreshResult {
    Ready { snapshot: Box<Snapshot> },
    Unavailable { problem: Problem },
    InvalidRef { problem: Problem },
    MissingLeft { problem: Problem },
    MissingRight { problem: Problem },
    NetworkError { problem: Problem },
}

impl Service {
    async fn open(
        &self,
        settings: &crate::settings::Settings,
        left: Endpoint,
        right: Endpoint,
    ) -> Result<Opened, Problem> {
        let left = bind(settings, left).map_err(|problem| problem.side("left"))?;
        let right = bind(settings, right).map_err(|problem| problem.side("right"))?;
        let mut sessions = self.sessions.lock().await;
        if sessions.len() >= 16 {
            return Err(Problem::new(
                "limitExceeded",
                "Close a comparison before opening another",
            ));
        }
        let id = format!("comparison-{}", NEXT.fetch_add(1, Ordering::Relaxed));
        sessions.insert(
            id.clone(),
            Session {
                left,
                right,
                generation: 0,
                cancel: Arc::new(AtomicBool::new(false)),
                prepared: None,
            },
        );
        Ok(Opened { id, generation: 0 })
    }

    fn rebind(settings: &crate::settings::Settings, context: &Context) -> Result<(), Problem> {
        let current = bind(settings, context.endpoint.clone())?;
        if current.root != context.root || current.workspace_root != context.workspace_root {
            return Err(Problem::new(
                "staleContext",
                "Registered destination changed; reopen comparison",
            ));
        }
        Ok(())
    }

    async fn close(&self, id: &str) -> bool {
        if let Some(session) = self.sessions.lock().await.remove(id) {
            session.cancel.store(true, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    pub async fn release_sessions(&self) {
        for (_, session) in self.sessions.lock().await.drain() {
            session.cancel.store(true, Ordering::Relaxed);
        }
    }

    async fn cancel(&self, id: &str) -> bool {
        if let Some(session) = self.sessions.lock().await.get_mut(id) {
            session.cancel.store(true, Ordering::Relaxed);
            session.generation += 1;
            session.prepared = None;
            true
        } else {
            false
        }
    }

    async fn fetch_state(&self, root: &Path) -> Result<Arc<Mutex<Fetch>>, Problem> {
        let mut fetches = self.fetches.lock().await;
        if fetches.len() >= 32 {
            fetches.retain(|_, state| Arc::strong_count(state) > 1);
        }
        if !fetches.contains_key(root) && fetches.len() >= 32 {
            return Err(Problem::new(
                "limitExceeded",
                "Too many concurrent repository contexts",
            ));
        }
        Ok(fetches.entry(root.to_path_buf()).or_default().clone())
    }

    async fn prepare(
        &self,
        id: &str,
        generation: u64,
        contexts: [Context; 2],
        options: Options,
        job: &Job,
    ) -> Result<Prepared, Problem> {
        let [left_context, right_context] = contexts;
        let left_safe = read_root(&left_context, job)
            .await
            .map_err(|problem| problem.side("left"))?;
        let right_safe = read_root(&right_context, job)
            .await
            .map_err(|problem| problem.side("right"))?;
        let contexts = [&left_context, &right_context];
        let roots = [&left_safe.path, &right_safe.path];
        let mut states = Vec::new();
        let mut epochs = Vec::new();
        for root in roots {
            let state = self.fetch_state(root).await?;
            epochs.push(job.lock(&state).await?.epoch);
            states.push(state);
        }
        let mut commits = Vec::new();
        for (index, context) in contexts.iter().enumerate() {
            commits.push(
                resolve(context, job)
                    .await
                    .map_err(|problem| problem.side(if index == 0 { "left" } else { "right" }))?,
            );
        }
        let mut attempted = BTreeSet::new();
        for index in 0..2 {
            if commits[index].is_some() {
                continue;
            }
            let side = if index == 0 { "left" } else { "right" };
            let mut state = job.lock(&states[index]).await?;
            if state.epoch == epochs[index]
                && attempted.insert(roots[index].clone())
                && resolve(contexts[index], job).await?.is_none()
            {
                let result = job
                    .run(
                        &contexts[index].root,
                        &[
                            "-c",
                            "gc.auto=0",
                            "-c",
                            "maintenance.auto=false",
                            "-c",
                            "protocol.ext.allow=never",
                            "-c",
                            "fetch.prune=false",
                            "-c",
                            "fetch.pruneTags=false",
                            "-c",
                            "remote.origin.prune=false",
                            "-c",
                            "remote.origin.pruneTags=false",
                            "fetch",
                            "--no-prune",
                            "--no-write-fetch-head",
                            "--no-auto-maintenance",
                            "--no-recurse-submodules",
                            "--",
                            "origin",
                        ],
                        &[0],
                    )
                    .await;
                state.epoch += 1;
                state.problem = match result {
                    Ok(result) if result.code == Some(0) => None,
                    Ok(result) => Some(Problem::new(
                        "networkError",
                        &git::last_error(&String::from_utf8_lossy(&result.stderr)),
                    )),
                    Err(problem) if problem.kind == "cancelled" => return Err(problem),
                    Err(problem) => Some(Problem::new("networkError", &problem.message)),
                };
            }
            commits[index] = resolve(contexts[index], job)
                .await
                .map_err(|problem| problem.side(side))?;
            if commits[index].is_none() {
                if let Some(problem) = &state.problem {
                    return Err(problem.clone().side(side));
                }
                return Err(Problem::new(
                    if index == 0 {
                        "missingLeft"
                    } else {
                        "missingRight"
                    },
                    "Reference is missing after one origin fetch",
                )
                .side(side));
            }
        }
        let mut left = Resolved {
            context: left_context,
            safe: left_safe,
            commit: commits[0].take().unwrap(),
            files: BTreeMap::new(),
        };
        let mut right = Resolved {
            context: right_context,
            safe: right_safe,
            commit: commits[1].take().unwrap(),
            files: BTreeMap::new(),
        };
        left.files = inventory(&left.context, &left.safe, &left.commit, job)
            .await
            .map_err(|problem| problem.side("left"))?;
        right.files = inventory(&right.context, &right.safe, &right.commit, job)
            .await
            .map_err(|problem| problem.side("right"))?;
        let metadata = diff_metadata(&left, &right, job).await?;
        let paths: BTreeSet<_> = left
            .files
            .keys()
            .chain(right.files.keys())
            .cloned()
            .collect();
        if paths.len() > FILE_LIMIT {
            return Err(Problem::new(
                "limitExceeded",
                "Comparison exceeds inventory limit",
            ));
        }
        let mut rows = Vec::new();
        let mut used = 0;
        for path in paths {
            job.check()?;
            let left_entry = left.files.get(&path);
            let right_entry = right.files.get(&path);
            let raw_status = match (left_entry, right_entry) {
                (left, right)
                    if [left, right]
                        .into_iter()
                        .flatten()
                        .any(|entry| entry.reason.is_some()) =>
                {
                    Status::Unavailable
                }
                (Some(left), Some(right)) if left.kind != right.kind => Status::TypeConflict,
                (Some(_), None) => Status::LeftOnly,
                (None, Some(_)) => Status::RightOnly,
                _ => Status::Same,
            };
            let mut row = FileRow {
                id: format!("file-{}", NEXT.fetch_add(1, Ordering::Relaxed)),
                path: path.clone(),
                left: left_entry.map(SideInfo::from),
                right: right_entry.map(SideInfo::from),
                display_status: raw_status.clone(),
                raw_status,
                raw_lines: None,
                display_lines: None,
                binary: None,
                rename: metadata.renames.get(&path).cloned(),
                reason: metadata.reason.clone(),
            };
            let leaves = [left_entry, right_entry]
                .into_iter()
                .flatten()
                .all(|entry| entry.kind != Kind::Directory);
            if [left_entry, right_entry]
                .into_iter()
                .flatten()
                .any(|entry| entry.source == "untrackedRepository")
            {
                row.reason = Some("Untracked nested repository; contents are opaque".into());
                if left_entry.is_some() && right_entry.is_some() && row.raw_status == Status::Same {
                    row.raw_status = Status::Unavailable;
                    row.display_status = Status::Unavailable;
                }
                rows.push(row);
                continue;
            }
            if leaves && row.raw_status != Status::TypeConflict {
                let identical_oid = matches!((left_entry, right_entry), (Some(left), Some(right)) if left.reason.is_none() && right.reason.is_none() && left.oid.is_some() && left.oid == right.oid && left.mode == right.mode)
                    && left.safe.path == right.safe.path;
                if !identical_oid {
                    let cost = [left_entry, right_entry]
                        .into_iter()
                        .flatten()
                        .map(|entry| entry.size.unwrap_or(0) as usize)
                        .sum::<usize>();
                    if cost > BYTE_LIMIT.saturating_sub(used) {
                        row.reason = Some("Comparison diff content budget exceeded".into());
                        row.raw_status = Status::Unavailable;
                        row.display_status = Status::Unavailable;
                        rows.push(row);
                        continue;
                    }
                    let mut bytes = Vec::new();
                    for (side, entry) in [(&left, left_entry), (&right, right_entry)] {
                        match entry {
                            Some(entry) => match content(side, &path, entry, job).await {
                                Ok(content) => {
                                    used += content.len();
                                    bytes.push(content);
                                }
                                Err(problem) if problem.kind == "cancelled" => return Err(problem),
                                Err(problem) => {
                                    row.reason = Some(problem.message);
                                    row.raw_status = Status::Unavailable;
                                    row.display_status = Status::Unavailable;
                                    break;
                                }
                            },
                            None => bytes.push(Vec::new()),
                        }
                    }
                    if used > BYTE_LIMIT {
                        row.reason = Some("Comparison diff content budget exceeded".into());
                        row.raw_status = Status::Unavailable;
                        row.display_status = Status::Unavailable;
                        rows.push(row);
                        continue;
                    }
                    if bytes.len() == 2 {
                        let opaque = [left_entry, right_entry]
                            .into_iter()
                            .flatten()
                            .any(|entry| entry.kind == Kind::Gitlink);
                        let is_binary = binary(&bytes[0]) || binary(&bytes[1]);
                        row.binary = Some(is_binary);
                        if let (Some(left), Some(right)) = (left_entry, right_entry) {
                            let mode_same = left.mode == right.mode;
                            row.raw_status = if bytes[0] == bytes[1] && mode_same {
                                Status::Same
                            } else {
                                Status::Different
                            };
                            row.display_status = if normalized(&bytes[0], &options)
                                == normalized(&bytes[1], &options)
                                && mode_same
                            {
                                Status::Same
                            } else {
                                Status::Different
                            };
                        }
                        if !opaque && !is_binary {
                            row.raw_lines = if row.raw_status == Status::Same {
                                Some(Lines {
                                    added: 0,
                                    removed: 0,
                                })
                            } else if !matches!(
                                left.context.endpoint.reference,
                                CompareRef::WorkingTree
                            ) && !matches!(
                                right.context.endpoint.reference,
                                CompareRef::WorkingTree
                            ) && metadata.lines.contains_key(&path)
                            {
                                metadata.lines[&path].clone()
                            } else {
                                count_result(
                                    line_counts(&bytes[0], &bytes[1], job).await,
                                    &mut row.reason,
                                )?
                            };
                            row.display_lines = if row.display_status == Status::Same {
                                Some(Lines {
                                    added: 0,
                                    removed: 0,
                                })
                            } else if options.normalize_eol || options.ignore_whitespace {
                                count_result(
                                    line_counts(
                                        &normalized(&bytes[0], &options),
                                        &normalized(&bytes[1], &options),
                                        job,
                                    )
                                    .await,
                                    &mut row.reason,
                                )?
                            } else {
                                row.raw_lines.clone()
                            };
                        }
                    }
                }
            }
            rows.push(row);
        }
        for index in (0..rows.len()).rev() {
            if ![rows[index].left.as_ref(), rows[index].right.as_ref()]
                .into_iter()
                .flatten()
                .any(|side| side.kind == Kind::Directory)
            {
                continue;
            }
            let prefix = format!("{}/", rows[index].path);
            let children: Vec<_> = rows
                .iter()
                .filter(|row| {
                    row.path.starts_with(&prefix) && !row.path[prefix.len()..].contains('/')
                })
                .cloned()
                .collect();
            if rows[index].raw_status == Status::Same {
                rows[index].raw_status = if children
                    .iter()
                    .any(|row| row.raw_status == Status::Unavailable)
                {
                    Status::Unavailable
                } else if children.iter().all(|row| row.raw_status == Status::Same) {
                    Status::Same
                } else {
                    Status::Different
                };
                rows[index].display_status = if children
                    .iter()
                    .any(|row| row.display_status == Status::Unavailable)
                {
                    Status::Unavailable
                } else if children
                    .iter()
                    .all(|row| row.display_status == Status::Same)
                {
                    Status::Same
                } else {
                    Status::Different
                };
            }
            for is_left in [true, false] {
                let info = if is_left {
                    &mut rows[index].left
                } else {
                    &mut rows[index].right
                };
                if let Some(info) = info.as_mut().filter(|side| side.kind == Kind::Directory) {
                    let sides: Vec<_> = children
                        .iter()
                        .filter_map(|row| {
                            if is_left {
                                row.left.as_ref()
                            } else {
                                row.right.as_ref()
                            }
                        })
                        .collect();
                    info.size = sides.iter().try_fold(0u64, |sum, side| {
                        side.size.and_then(|size| sum.checked_add(size))
                    });
                    info.modified_ms = sides.iter().filter_map(|side| side.modified_ms).max();
                }
            }
        }
        let mut raw = Summary::default();
        let mut display = Summary::default();
        for row in &rows {
            if [row.left.as_ref(), row.right.as_ref()]
                .into_iter()
                .flatten()
                .any(|side| side.kind != Kind::Directory)
            {
                raw.add(&row.raw_status);
                display.add(&row.display_status);
            }
        }
        let (history, history_source) = history(&left, &right, job).await?;
        let endpoint = |side: &Resolved, basis: String| ResolvedEndpoint {
            endpoint: side.context.endpoint.clone(),
            commit: side.commit.clone(),
            history_basis: basis,
        };
        let view = Snapshot {
            id: id.into(),
            generation,
            left: endpoint(&left, history.left_basis.clone()),
            right: endpoint(&right, history.right_basis.clone()),
            raw,
            display,
            history,
            file_count: rows.len(),
            options,
        };
        Ok(Prepared {
            view,
            left,
            right,
            rows,
            history_source,
        })
    }

    async fn refresh(
        &self,
        settings: &crate::settings::Settings,
        id: &str,
        options: Options,
    ) -> Result<RefreshResult, Problem> {
        let (contexts, generation, job) = {
            let mut sessions = self.sessions.lock().await;
            let session = sessions
                .get_mut(id)
                .ok_or_else(|| Problem::new("unknownSession", "Unknown comparison"))?;
            Self::rebind(settings, &session.left)?;
            Self::rebind(settings, &session.right)?;
            session.cancel.store(true, Ordering::Relaxed);
            session.cancel = Arc::new(AtomicBool::new(false));
            session.generation += 1;
            session.prepared = None;
            (
                [session.left.clone(), session.right.clone()],
                session.generation,
                Job {
                    context: format!("compare:{id}"),
                    cancel: session.cancel.clone(),
                    #[cfg(test)]
                    temporary_root: None,
                    #[cfg(test)]
                    inventory_started: None,
                },
            )
        };
        let _permit = job.slot(&self.slots).await?;
        let result = self.prepare(id, generation, contexts, options, &job).await;
        job.check()?;
        let mut sessions = self.sessions.lock().await;
        let session = sessions
            .get_mut(id)
            .filter(|session| session.generation == generation)
            .ok_or_else(|| Problem::new("staleGeneration", "Obsolete comparison result"))?;
        match result {
            Ok(prepared) => {
                let snapshot = Box::new(prepared.view.clone());
                session.prepared = Some(Arc::new(prepared));
                Ok(RefreshResult::Ready { snapshot })
            }
            Err(problem) => match problem.kind.as_str() {
                "unavailable" => Ok(RefreshResult::Unavailable { problem }),
                "invalidRef" => Ok(RefreshResult::InvalidRef { problem }),
                "missingLeft" => Ok(RefreshResult::MissingLeft { problem }),
                "missingRight" => Ok(RefreshResult::MissingRight { problem }),
                "networkError" => Ok(RefreshResult::NetworkError { problem }),
                _ => Err(problem),
            },
        }
    }

    pub async fn copy_source_context(&self, settings: &crate::settings::Settings, id: &str, generation: u64, file_id: &str, side: &str) -> Result<Option<WriteContext>, String> {
        let (prepared, job) = self.snapshot(settings, id, generation).await.map_err(|problem| problem.message)?;
        job.check().map_err(|problem| problem.message)?;
        let resolved = match side { "left" => &prepared.left, "right" => &prepared.right, _ => return Err("Unknown side".into()) };
        if resolved.context.endpoint.reference != CompareRef::WorkingTree { return Ok(None); }
        self.write_context(settings, id, generation, file_id, side, true).await.map(Some)
    }

    pub async fn write_context(
        &self,
        settings: &crate::settings::Settings,
        id: &str,
        generation: u64,
        file_id: &str,
        side: &str,
        fresh: bool,
    ) -> Result<WriteContext, String> {
        let (prepared, job) = self.snapshot(settings, id, generation).await.map_err(|problem| problem.message)?;
        job.check().map_err(|problem| problem.message)?;
        let row = prepared.rows.iter().find(|row| row.id == file_id).ok_or("Unknown file identity")?;
        let resolved = match side { "left" => &prepared.left, "right" => &prepared.right, _ => return Err("Unknown side".into()) };
        if resolved.context.endpoint.reference != CompareRef::WorkingTree { return Err("Historical references are read-only".into()); }
        if resolved.files.get(&row.path).is_some_and(|entry| entry.kind != Kind::File || entry.reason.is_some()) {
            return Err("Only regular, available working-tree files are writable".into());
        }
        let safe = if fresh { read_root(&resolved.context, &job).await.map_err(|problem| problem.message)? } else { resolved.safe.clone() };
        safe.resolve_cached(&row.path, false, &mut paths::ReadCache::default())?;
        Ok(WriteContext { root: resolved.context.root.clone(), path: row.path.clone(), safe })
    }

    pub async fn copy_ids(&self, settings: &crate::settings::Settings, id: &str, generation: u64, file_id: &str, source: &str) -> Result<(Vec<String>, usize), String> {
        let (prepared, job) = self.snapshot(settings, id, generation).await.map_err(|problem| problem.message)?;
        job.check().map_err(|problem| problem.message)?;
        let selected = prepared.rows.iter().find(|row| row.id == file_id).ok_or("Unknown file identity")?;
        let resolved = match source { "left" => &prepared.left, "right" => &prepared.right, _ => return Err("Unknown side".into()) };
        let entry = resolved.files.get(&selected.path).ok_or("Source is absent; copy never deletes destination files")?;
        let prefix = format!("{}/", selected.path);
        let mut ids = Vec::new(); let mut retained = 0;
        for row in &prepared.rows {
            if row.path != selected.path && !(entry.kind == Kind::Directory && row.path.starts_with(&prefix)) { continue; }
            let Some(source) = resolved.files.get(&row.path) else { retained += 1; continue; };
            if source.kind == Kind::Directory { continue; }
            if source.kind != Kind::File || source.reason.is_some() { return Err(format!("Copy refuses linked, unavailable or submodule content: {}", row.path)); }
            if ids.len() >= 128 { return Err("Copy scope exceeds 128 files; select a smaller folder".into()); }
            ids.push(row.id.clone());
        }
        if ids.is_empty() { return Err("No regular source files in this scope".into()); }
        Ok((ids, retained))
    }

    async fn snapshot(
        &self,
        settings: &crate::settings::Settings,
        id: &str,
        generation: u64,
    ) -> Result<(Arc<Prepared>, Job), Problem> {
        let sessions = self.sessions.lock().await;
        let session = sessions
            .get(id)
            .filter(|session| session.generation == generation)
            .ok_or_else(|| Problem::new("staleGeneration", "Unknown or obsolete comparison"))?;
        Self::rebind(settings, &session.left)?;
        Self::rebind(settings, &session.right)?;
        let prepared = session.prepared.clone().ok_or_else(|| {
            Problem::unavailable(
                UnavailableReason::RefreshRequired,
                "Refresh comparison first",
            )
        })?;
        Ok((
            prepared,
            Job {
                context: format!("compare:{id}"),
                cancel: session.cancel.clone(),
                #[cfg(test)]
                temporary_root: None,
                #[cfg(test)]
                inventory_started: None,
            },
        ))
    }
}

pub struct WriteContext {
    pub root: PathBuf,
    pub path: String,
    pub safe: paths::ReadRoot,
}

pub fn registered_clone_destination(settings: &crate::settings::Settings, id: &str, destination: &Path, url: &str) -> Result<(), String> {
    let sets = settings.workspace.get("sets").and_then(serde_json::Value::as_array).ok_or("No registered sets")?;
    for set in sets {
        let Some(set_id) = set.get("id").and_then(serde_json::Value::as_str) else { continue; };
        let Some(items) = set.get("items").and_then(serde_json::Value::as_array) else { continue; };
        for item in items {
            if item.get("id").and_then(serde_json::Value::as_str) != Some(id) || item.get("url").and_then(serde_json::Value::as_str) != Some(url) { continue; }
            let context = bind(settings, Endpoint { set_id: set_id.into(), item_id: id.into(), reference: CompareRef::WorkingTree })
                .map_err(|problem| problem.message)?;
            if context.root == destination { return Ok(()); }
        }
    }
    Err("Clone destination does not match a registered repository item".into())
}

type SetRoots = (Vec<(String, PathBuf)>, std::collections::HashSet<String>);

/// Destination folders of a saved set's items, and the lowercased folders used by every other set.
pub fn set_roots(settings: &crate::settings::Settings, set_id: &str) -> Result<SetRoots, String> {
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
            if id == set_id { own.push((item_id.to_string(), context.root)); } else { others.insert(context.root.to_string_lossy().to_lowercase()); }
        }
    }
    if !found { return Err("This set has not been saved yet".into()); }
    Ok((own, others))
}

pub async fn registered_write_root(settings: &crate::settings::Settings, root: &Path, relative: &str) -> Result<paths::ReadRoot, String> {
    paths::relative(relative)?;
    let sets = settings.workspace.get("sets").and_then(serde_json::Value::as_array).ok_or("No registered sets")?;
    for set in sets {
        let Some(set_id) = set.get("id").and_then(serde_json::Value::as_str) else { continue; };
        let Some(items) = set.get("items").and_then(serde_json::Value::as_array) else { continue; };
        for item in items {
            let Some(item_id) = item.get("id").and_then(serde_json::Value::as_str) else { continue; };
            let endpoint = Endpoint { set_id: set_id.into(), item_id: item_id.into(), reference: CompareRef::WorkingTree };
            if let Ok(context) = bind(settings, endpoint) {
                if context.root == root {
                    let job = Job {
                        context: "Recovery authorization".into(), cancel: Arc::new(AtomicBool::new(false)),
                        #[cfg(test)] temporary_root: None,
                        #[cfg(test)] inventory_started: None,
                    };
                    let safe = read_root(&context, &job).await.map_err(|problem| problem.message)?;
                    safe.resolve_cached(relative, false, &mut paths::ReadCache::default())?;
                    return Ok(safe);
                }
            }
        }
    }
    Err("Recovery root is no longer registered; automatic restore refused".into())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Content {
    generation: u64,
    side: String,
    kind: Kind,
    pub(crate) bytes: Vec<u8>,
    binary: bool,
}

#[derive(Serialize)]
pub struct UniqueCommit {
    side: String,
    sha: String,
    subject: String,
    author: String,
    date: String,
}

fn saved(app: &tauri::AppHandle) -> Result<crate::settings::Settings, Problem> {
    crate::settings::load_settings(app.clone())
        .map_err(|error| Problem::new("invalidContext", &error))
}

#[tauri::command]
pub async fn comparison_open(
    app: tauri::AppHandle,
    service: tauri::State<'_, Service>,
    left: Endpoint,
    right: Endpoint,
) -> Result<Opened, Problem> {
    service.open(&saved(&app)?, left, right).await
}

#[tauri::command]
pub async fn comparison_refresh(
    app: tauri::AppHandle,
    service: tauri::State<'_, Service>,
    id: String,
    options: Options,
) -> Result<RefreshResult, Problem> {
    service.refresh(&saved(&app)?, &id, options).await
}

#[tauri::command]
pub async fn comparison_close(
    service: tauri::State<'_, Service>,
    id: String,
) -> Result<bool, Problem> {
    Ok(service.close(&id).await)
}

#[tauri::command]
pub async fn comparison_cancel(
    service: tauri::State<'_, Service>,
    id: String,
) -> Result<bool, Problem> {
    Ok(service.cancel(&id).await)
}

#[tauri::command]
pub async fn comparison_files(
    app: tauri::AppHandle,
    service: tauri::State<'_, Service>,
    id: String,
    generation: u64,
    offset: usize,
    limit: usize,
) -> Result<Vec<FileRow>, Problem> {
    if limit == 0 || limit > 512 || offset > FILE_LIMIT {
        return Err(Problem::new("limitExceeded", "Invalid inventory page"));
    }
    let (prepared, job) = service.snapshot(&saved(&app)?, &id, generation).await?;
    job.check()?;
    Ok(prepared
        .rows
        .iter()
        .skip(offset)
        .take(limit)
        .cloned()
        .collect())
}

#[tauri::command]
pub async fn comparison_content(
    app: tauri::AppHandle,
    service: tauri::State<'_, Service>,
    id: String,
    generation: u64,
    file_id: String,
    side: String,
) -> Result<Content, Problem> {
    let settings = saved(&app)?;
    let (prepared, job) = service.snapshot(&settings, &id, generation).await?;
    let _permit = job.slot(&service.slots).await?;
    let row = prepared
        .rows
        .iter()
        .find(|row| row.id == file_id)
        .ok_or_else(|| Problem::new("unknownFile", "Unknown file identity"))?;
    let resolved = match side.as_str() {
        "left" => &prepared.left,
        "right" => &prepared.right,
        _ => return Err(Problem::new("invalidContext", "Unknown side")),
    };
    read_root(&resolved.context, &job).await?;
    let entry = resolved
        .files
        .get(&row.path)
        .ok_or_else(|| Problem::new("unavailable", "File absent on selected side"))?;
    let bytes = content(resolved, &row.path, entry, &job).await?;
    job.check()?;
    service.snapshot(&settings, &id, generation).await?;
    Ok(Content {
        generation,
        side,
        kind: entry.kind.clone(),
        binary: binary(&bytes),
        bytes,
    })
}

#[tauri::command]
pub async fn comparison_commits(
    app: tauri::AppHandle,
    service: tauri::State<'_, Service>,
    id: String,
    generation: u64,
    offset: usize,
    limit: usize,
) -> Result<Vec<UniqueCommit>, Problem> {
    if limit == 0 || limit > 200 || offset > FILE_LIMIT {
        return Err(Problem::new("limitExceeded", "Invalid commit page"));
    }
    let settings = saved(&app)?;
    let (prepared, job) = service.snapshot(&settings, &id, generation).await?;
    let _permit = job.slot(&service.slots).await?;
    let source = prepared.history_source.as_ref().ok_or_else(|| {
        Problem::new(
            "unavailable",
            prepared
                .view
                .history
                .reason
                .as_deref()
                .unwrap_or("History unavailable"),
        )
    })?;
    let output = job
        .output(
            &source.root,
            &[
                "log",
                "--left-right",
                "--no-show-signature",
                "--format=%m%x00%H%x00%s%x00%an%x00%aI",
                "-z",
                &format!("--max-count={limit}"),
                &format!("--skip={offset}"),
                &source.range,
                "--",
            ],
        )
        .await?;
    let fields: Vec<_> = output.split(|byte| *byte == 0).collect();
    let mut commits = Vec::new();
    for fields in fields[..fields.len().saturating_sub(1)].chunks(5) {
        if fields.len() != 5 {
            return Err(Problem::new("gitError", "Invalid unique-commit record"));
        }
        commits.push(UniqueCommit {
            side: match fields[0] {
                b"<" => "left",
                b">" => "right",
                _ => return Err(Problem::new("gitError", "Invalid commit direction")),
            }
            .into(),
            sha: decode(fields[1])?,
            subject: git::safe(&String::from_utf8_lossy(fields[2])),
            author: git::safe(&String::from_utf8_lossy(fields[3])),
            date: decode(fields[4])?,
        });
    }
    job.check()?;
    service.snapshot(&settings, &id, generation).await?;
    Ok(commits)
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn write_authority_rejects_history_stale_sessions_and_root_changes() {
        let _guard = crate::git::TEST_RUNNER_LOCK.lock().await;
        let fixture = Fixture::new().await;
        fixture.write("file.txt", b"before"); fixture.commit("base").await;
        fixture.write("file.txt", b"after");
        let service = Service::default(); let settings = fixture.settings();
        let opened = service.open(&settings, fixture.context(CompareRef::Head).endpoint,
            fixture.context(CompareRef::WorkingTree).endpoint).await.unwrap();
        let result = service.refresh(&settings, &opened.id, Options::default()).await.unwrap();
        let RefreshResult::Ready { snapshot } = result else { panic!("Comparison unavailable"); };
        let (prepared, _) = service.snapshot(&settings, &opened.id, snapshot.generation).await.unwrap();
        let row = prepared.rows.iter().find(|row| row.path == "file.txt").unwrap();
        assert!(service.write_context(&settings, &opened.id, snapshot.generation, &row.id, "left", true).await.is_err());
        let context = service.write_context(&settings, &opened.id, snapshot.generation, &row.id, "right", true).await.unwrap();
        assert_eq!(context.path, "file.txt");
        assert!(service.write_context(&settings, &opened.id, snapshot.generation + 1, &row.id, "right", false).await.is_err());
        assert!(service.write_context(&settings, &opened.id, snapshot.generation, "forged", "right", false).await.is_err());
        let mut changed = fixture.settings(); changed.workspace["root"] = serde_json::json!(fixture.0.join("different-root"));
        assert!(service.write_context(&changed, &opened.id, snapshot.generation, &row.id, "right", false).await.is_err());
        assert!(registered_write_root(&settings, &context.root, ".git/config").await.is_err());
        service.close(&opened.id).await;
        assert!(service.write_context(&settings, &opened.id, snapshot.generation, &row.id, "right", false).await.is_err());
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn pinned_repository_root_keeps_git_checkout_working() {
        let _guard = crate::git::TEST_RUNNER_LOCK.lock().await;
        let fixture = Fixture::new().await; fixture.write("file.txt", b"original"); fixture.commit("base").await;
        let guard = crate::file_guard::PinnedPath::existing_directory(&fixture.0.join("repo")).unwrap();
        fixture.write("file.txt", b"modified");
        fixture.git(&["checkout", "HEAD", "--", "file.txt"]).await;
        assert_eq!(std::fs::read(fixture.0.join("repo/file.txt")).unwrap(), b"original");
        assert!(std::fs::rename(fixture.0.join("repo"), fixture.0.join("moved")).is_err());
        drop(guard);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn fresh_clone_into_pinned_empty_destination_preserves_git_workflow() {
        let _guard = crate::git::TEST_RUNNER_LOCK.lock().await;
        let fixture = Fixture::new().await; fixture.write("file.txt", b"source"); fixture.commit("base").await;
        let destination = fixture.0.join("new-parent/clone");
        let guard = crate::file_guard::PinnedPath::ensure_directory(&destination).unwrap();
        let source = fixture.0.join("repo");
        fixture.job().output(&fixture.0, &["clone", "--no-hardlinks", "--", source.to_str().unwrap(), destination.to_str().unwrap()]).await.unwrap();
        assert_eq!(std::fs::read(destination.join("file.txt")).unwrap(), b"source");
        assert!(std::fs::rename(&destination, fixture.0.join("moved")).is_err());
        drop(guard);
    }

    use super::*;

    struct Fixture(PathBuf);
    impl Fixture {
        async fn new() -> Self {
            Self::with_format(None).await
        }
        async fn with_format(format: Option<&str>) -> Self {
            let path = std::env::temp_dir().join(format!(
                "paperwing-compare-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(path.join("repo")).unwrap();
            let fixture = Self(path);
            let option = format.map(|format| format!("--object-format={format}"));
            let mut args = vec!["init", "--initial-branch=main"];
            if let Some(option) = &option {
                args.push(option);
            }
            fixture.git(&args).await;
            fixture
        }
        fn job(&self) -> Job {
            Job {
                context: "compare-fixture".into(),
                cancel: Arc::new(AtomicBool::new(false)),
                temporary_root: None,
                inventory_started: None,
            }
        }
        async fn git(&self, args: &[&str]) -> Vec<u8> {
            self.job().output(&self.0.join("repo"), args).await.unwrap()
        }
        fn write(&self, path: &str, bytes: &[u8]) {
            let file = self.0.join("repo").join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, bytes).unwrap();
        }
        async fn commit(&self, name: &str) -> String {
            self.git(&["add", "."]).await;
            self.git(&[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.test",
                "-c",
                "core.hooksPath=",
                "commit",
                "-m",
                name,
            ])
            .await;
            decode(&self.git(&["rev-parse", "HEAD"]).await)
                .unwrap()
                .trim()
                .into()
        }
        fn context(&self, reference: CompareRef) -> Context {
            Context {
                endpoint: Endpoint {
                    set_id: "set".into(),
                    item_id: "item".into(),
                    reference,
                },
                root: self.0.join("repo"),
                workspace_root: self.0.clone(),
            }
        }
        fn settings(&self) -> crate::settings::Settings {
            crate::settings::Settings {
                sources: vec![],
                workspace: serde_json::json!({
                    "root": self.0, "layout": "flat", "sets": [{ "id": "set", "name": "Fixture", "items": [
                        { "id": "item", "name": "repo" }, { "id": "missing", "name": "not-cloned" }
                    ] }]
                }),
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    async fn prepared(
        fixture: &Fixture,
        left: CompareRef,
        right: CompareRef,
        options: Options,
    ) -> Prepared {
        Service::default()
            .prepare(
                "fixture",
                1,
                [fixture.context(left), fixture.context(right)],
                options,
                &fixture.job(),
            )
            .await
            .unwrap()
    }

    #[test]
    fn custom_layout_matches_typescript_goldens_and_registered_destination() {
        let source = include_str!("../../src/lib/workspace.test.js");
        let json = source
            .split_once("const layoutGoldens = JSON.parse(String.raw`")
            .unwrap()
            .1
            .split_once('`')
            .unwrap()
            .0;
        let vectors: Vec<(String, Vec<String>)> = serde_json::from_str(json).unwrap();
        let values = BTreeMap::from([
            ("folder", "repo-folder".into()),
            ("repo", "repo".into()),
            ("org", "org".into()),
            ("set", "Set-Name".into()),
            ("source", "Source-Name".into()),
            ("ref", "feature-x".into()),
        ]);
        let fixture =
            Fixture(std::env::temp_dir().join(format!("paperwing-layout-{}", std::process::id())));
        let mut settings = fixture.settings();
        settings.workspace["layout"] = "custom".into();
        settings.workspace["sets"][0]["items"][0] = serde_json::json!({"id":"item", "repoId":"source:repo", "name":"repo", "folder":"repo-folder", "org":"org", "ref":{"type":"branch", "name":"feature/x"}});
        for (template, expected) in vectors {
            assert_eq!(
                template_segments(&template, &values),
                expected,
                "{template:?}"
            );
            if !template.contains(['\u{007f}', '\u{0085}'])
                && !["{set}", "{source}"]
                    .iter()
                    .any(|token| template.contains(token))
            {
                settings.workspace["pathTemplate"] = template.clone().into();
                let context = bind(&settings, fixture.context(CompareRef::Head).endpoint).unwrap();
                assert_eq!(
                    context.root,
                    fixture.0.join(expected.iter().collect::<PathBuf>())
                );
            }
        }
    }

    #[tokio::test]
    async fn link_text_gitlinks_ignored_files_and_content_limits() {
        let _guard = git::TEST_RUNNER_LOCK.lock().await;
        let fixture = Fixture::new().await;
        fixture.git(&["config", "core.symlinks", "false"]).await;
        fixture.write("target.txt", b"never dereference this content\n");
        fixture.write("link", b"target.txt");
        fixture.write(".gitignore", b"ignored.txt\n");
        fixture.write("ignored.txt", b"excluded\n");
        fixture.write("large.txt", &vec![b'a'; paths::CONTENT_LIMIT + 1]);
        let initial = fixture.commit("initial").await;
        let blob = decode(&fixture.git(&["hash-object", "link"]).await)
            .unwrap()
            .trim()
            .to_string();
        fixture
            .git(&[
                "update-index",
                "--cacheinfo",
                &format!("120000,{blob},link"),
            ])
            .await;
        fixture
            .git(&[
                "update-index",
                "--add",
                "--cacheinfo",
                &format!("160000,{initial},nested"),
            ])
            .await;
        fixture
            .git(&[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.test",
                "commit",
                "-m",
                "opaque entries",
            ])
            .await;
        std::fs::create_dir(fixture.0.join("repo/nested")).unwrap();
        fixture
            .git(&["init", "--initial-branch=main", "vendor/foo"])
            .await;
        let committed = prepared(
            &fixture,
            CompareRef::Head,
            CompareRef::Head,
            Options::default(),
        )
        .await;
        assert!(!committed.rows.iter().any(|row| row.path == "ignored.txt"));
        assert!(!committed
            .rows
            .iter()
            .any(|row| row.path.starts_with("nested/")));
        assert_eq!(committed.left.files["link"].kind, Kind::Symlink);
        assert_eq!(
            content(
                &committed.left,
                "link",
                &committed.left.files["link"],
                &fixture.job()
            )
            .await
            .unwrap(),
            b"target.txt"
        );
        assert_eq!(
            content(
                &committed.left,
                "nested",
                &committed.left.files["nested"],
                &fixture.job()
            )
            .await
            .unwrap(),
            initial.as_bytes()
        );
        assert!(content(
            &committed.left,
            "large.txt",
            &committed.left.files["large.txt"],
            &fixture.job()
        )
        .await
        .is_err());
        let working = prepared(
            &fixture,
            CompareRef::Head,
            CompareRef::WorkingTree,
            Options::default(),
        )
        .await;
        assert_eq!(working.right.files["nested"].source, "indexGitlink");
        assert_eq!(working.right.files["link"].kind, Kind::Symlink);
        assert_eq!(
            content(
                &working.right,
                "link",
                &working.right.files["link"],
                &fixture.job()
            )
            .await
            .unwrap(),
            b"target.txt"
        );
        assert_eq!(
            working
                .rows
                .iter()
                .find(|row| row.path == "large.txt")
                .unwrap()
                .raw_status,
            Status::Unavailable
        );
        assert!(!working.rows.iter().any(|row| row.path == "ignored.txt"));
        let nested = working
            .rows
            .iter()
            .find(|row| row.path == "vendor/foo")
            .unwrap();
        assert_eq!(nested.raw_status, Status::RightOnly);
        assert_eq!(nested.right.as_ref().unwrap().kind, Kind::Gitlink);
        assert!(nested.reason.as_ref().unwrap().contains("opaque"));
        assert!(nested.raw_lines.is_none());
        assert!(!working
            .rows
            .iter()
            .any(|row| row.path.starts_with("vendor/foo/")));
        assert!(content(
            &working.right,
            "vendor/foo",
            &working.right.files["vendor/foo"],
            &fixture.job()
        )
        .await
        .unwrap_err()
        .message
        .contains("opaque"));
    }

    #[tokio::test]
    async fn scaled_working_inventory_is_lazy_bounded_and_cancellable() {
        let _guard = git::TEST_RUNNER_LOCK.lock().await;
        let fixture = Fixture::new().await;
        fixture.git(&["config", "core.autocrlf", "false"]).await;
        let mut bytes = vec![b'x'; 8400];
        for index in 0u32..5000 {
            bytes[..4].copy_from_slice(&index.to_le_bytes());
            fixture.write(&format!("file-{index:04}.dat"), &bytes);
        }
        fixture.commit("40 MiB scale").await;
        let index = std::fs::read(fixture.0.join("repo/.git/index")).unwrap();
        let started = std::time::Instant::now();
        let job = Job {
            context: "scale-ready".into(),
            ..fixture.job()
        };
        let service = Service::default();
        let comparison = service
            .prepare(
                "scale",
                1,
                [
                    fixture.context(CompareRef::Head),
                    fixture.context(CompareRef::WorkingTree),
                ],
                Options::default(),
                &job,
            )
            .await
            .unwrap();
        let elapsed = started.elapsed();
        assert_eq!(comparison.view.raw.same, 5000);
        assert_eq!(comparison.view.raw.unavailable, 0);
        assert!(
            elapsed < Duration::from_secs(30),
            "inventory took {elapsed:?}"
        );
        let activity: serde_json::Value = serde_json::to_value(git::activity_snapshot()).unwrap();
        let commands = activity
            .as_array()
            .unwrap()
            .iter()
            .filter(|entry| entry["context"] == "scale-ready")
            .count();
        assert!(commands <= 40, "{commands} Activity entries");
        let path = "file-4999.dat";
        let raw = content(&comparison.right, path, &comparison.right.files[path], &job)
            .await
            .unwrap();
        assert_eq!(
            raw,
            std::fs::read(fixture.0.join("repo").join(path)).unwrap()
        );
        assert_eq!(
            std::fs::read(fixture.0.join("repo/.git/index")).unwrap(),
            index
        );
        let inventory_started = Arc::new(tokio::sync::Notify::new());
        let cancel_job = Job {
            inventory_started: Some(inventory_started.clone()),
            ..fixture.job()
        };
        let running_job = cancel_job.clone();
        let context = fixture.context(CompareRef::WorkingTree);
        let safe = comparison.right.safe.clone();
        let commit = comparison.right.commit.clone();
        let task =
            tokio::spawn(async move { inventory(&context, &safe, &commit, &running_job).await });
        tokio::time::timeout(Duration::from_secs(10), inventory_started.notified())
            .await
            .expect("Working-tree blocking read must start before cancellation");
        let cancelled = std::time::Instant::now();
        cancel_job.cancel.store(true, Ordering::Relaxed);
        let result = tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result.unwrap_err().kind, "cancelled");
        println!("scale: 5000 files, 42000000 bytes, {elapsed:?}, {commands} Activity entries; cancellation {:?}", cancelled.elapsed());
        let changed = vec![0; paths::CONTENT_LIMIT];
        for index in 0..40 {
            fixture.write(&format!("file-{index:04}.dat"), &changed);
        }
        let comparison = service
            .prepare(
                "scale-budget",
                2,
                [
                    fixture.context(CompareRef::Head),
                    fixture.context(CompareRef::WorkingTree),
                ],
                Options::default(),
                &fixture.job(),
            )
            .await
            .unwrap();
        assert_eq!(comparison.view.raw.same, 4960);
        assert_eq!(comparison.view.raw.different, 31);
        assert_eq!(comparison.view.raw.unavailable, 9);
        let unavailable = comparison
            .rows
            .iter()
            .find(|row| row.raw_status == Status::Unavailable)
            .unwrap();
        assert!(unavailable.reason.as_ref().unwrap().contains("budget"));
        assert_eq!(
            content(
                &comparison.right,
                &unavailable.path,
                &comparison.right.files[&unavailable.path],
                &fixture.job()
            )
            .await
            .unwrap(),
            changed
        );
        assert_eq!(
            std::fs::read(fixture.0.join("repo/.git/index")).unwrap(),
            index
        );
        println!("scale budget: 4960 same, 31 different, 9 per-row unavailable; over-budget lazy original bytes preserved");
    }

    #[tokio::test]
    async fn sha_formats_ambiguous_prefixes_and_noncommit_objects() {
        let _guard = git::TEST_RUNNER_LOCK.lock().await;
        let fixture = Fixture::with_format(Some("sha256")).await;
        fixture.write("file.txt", b"same raw bytes\n");
        let sha = fixture.commit("sha256").await;
        assert_eq!(
            resolve(
                &fixture.context(CompareRef::Commit {
                    sha: sha.to_uppercase()
                }),
                &fixture.job()
            )
            .await
            .unwrap(),
            Some(sha.clone())
        );
        fixture.write("file.txt", b"different commit\n");
        let different = fixture.commit("hex ref target").await;
        for (command, length) in [("branch", 7), ("tag", 8)] {
            let prefix = &sha[..length];
            fixture.git(&[command, prefix, &different]).await;
            let selected = decode(
                &fixture
                    .git(&["rev-parse", "--verify", &format!("{prefix}^{{commit}}")])
                    .await,
            )
            .unwrap();
            assert_eq!(selected.trim(), different);
            assert_eq!(
                resolve(
                    &fixture.context(CompareRef::Commit { sha: prefix.into() }),
                    &fixture.job(),
                )
                .await
                .unwrap(),
                Some(sha.clone()),
                "hex-named {command} must not override the object ID"
            );
        }
        fixture.git(&["reset", "--hard", &sha]).await;
        let other = Fixture::new().await;
        other.write("file.txt", b"same raw bytes\n");
        other.commit("other format").await;
        let comparison = Service::default()
            .prepare(
                "formats",
                1,
                [
                    fixture.context(CompareRef::Head),
                    other.context(CompareRef::Head),
                ],
                Options::default(),
                &fixture.job(),
            )
            .await
            .unwrap();
        assert_eq!(comparison.view.raw.same, 1);
        let mut objects = HashMap::new();
        let mut ambiguous = None;
        for batch in 0..8 {
            let names: Vec<_> = (batch * 512..(batch + 1) * 512)
                .map(|index| format!("collision/{index}"))
                .collect();
            for name in &names {
                fixture.write(name, name.as_bytes());
            }
            let mut args = vec!["hash-object", "-w", "--"];
            args.extend(names.iter().map(String::as_str));
            let output = decode(&fixture.git(&args).await).unwrap();
            for oid in output.lines() {
                if let Some(previous) = objects.insert(oid[..4].to_string(), oid.to_string()) {
                    if previous != oid {
                        ambiguous = Some(oid[..4].to_string());
                        break;
                    }
                }
            }
            if ambiguous.is_some() {
                break;
            }
        }
        let prefix = ambiguous.expect("Fixture needs an actual ambiguous prefix");
        assert_eq!(
            resolve(
                &fixture.context(CompareRef::Commit { sha: prefix }),
                &fixture.job()
            )
            .await
            .unwrap_err()
            .kind,
            "invalidRef"
        );
        let blob = decode(&fixture.git(&["hash-object", "-w", "file.txt"]).await)
            .unwrap()
            .trim()
            .to_string();
        assert_eq!(
            resolve(
                &fixture.context(CompareRef::Commit { sha: blob }),
                &fixture.job()
            )
            .await
            .unwrap_err()
            .kind,
            "invalidRef"
        );
    }

    #[tokio::test]
    async fn fetch_recovers_missing_objects_without_checkout_or_index_changes() {
        let _guard = git::TEST_RUNNER_LOCK.lock().await;
        let fixture = Fixture::new().await;
        fixture.write("file.txt", b"initial\n");
        fixture.commit("initial").await;
        let clone = fixture.0.join("copy");
        fixture
            .git(&[
                "clone",
                "--no-local",
                "--",
                fixture.0.join("repo").to_str().unwrap(),
                clone.to_str().unwrap(),
            ])
            .await;
        fixture.write("file.txt", b"new upstream\n");
        fixture.commit("new").await;
        fixture.git(&["tag", "late"]).await;
        fixture.git(&["branch", "late-branch"]).await;
        let job = fixture.job();
        let preserved = decode(&job.output(&clone, &["rev-parse", "HEAD"]).await.unwrap())
            .unwrap()
            .trim()
            .to_string();
        for reference in ["refs/remotes/origin/obsolete", "refs/tags/private-tag"] {
            job.output(&clone, &["update-ref", reference, &preserved])
                .await
                .unwrap();
        }
        for option in [
            "fetch.prune",
            "fetch.pruneTags",
            "remote.origin.prune",
            "remote.origin.pruneTags",
        ] {
            job.output(&clone, &["config", option, "true"])
                .await
                .unwrap();
        }
        let head = std::fs::read(clone.join(".git/HEAD")).unwrap();
        let index = std::fs::read(clone.join(".git/index")).unwrap();
        let mut left = fixture.context(CompareRef::RemoteBranch {
            name: "origin/late-branch".into(),
        });
        left.root = clone.clone();
        let mut right = fixture.context(CompareRef::Head);
        right.root = clone.clone();
        let service = Service::default();
        let comparison = service
            .prepare(
                "recovered",
                1,
                [left, right],
                Options::default(),
                &fixture.job(),
            )
            .await
            .unwrap();
        assert_eq!(comparison.view.raw.different, 1);
        let mut tag = fixture.context(CompareRef::Tag {
            name: "late".into(),
        });
        tag.root = clone.clone();
        assert_eq!(
            resolve(&tag, &fixture.job()).await.unwrap().as_deref(),
            Some(comparison.left.commit.as_str())
        );
        let mut local_branch = tag.clone();
        local_branch.endpoint.reference = CompareRef::Branch {
            name: "late-branch".into(),
        };
        assert!(resolve(&local_branch, &fixture.job())
            .await
            .unwrap()
            .is_none());
        assert_eq!(
            service
                .fetch_state(&std::fs::canonicalize(&clone).unwrap())
                .await
                .unwrap()
                .lock()
                .await
                .epoch,
            1
        );
        assert_eq!(std::fs::read(clone.join(".git/HEAD")).unwrap(), head);
        assert_eq!(std::fs::read(clone.join(".git/index")).unwrap(), index);
        assert_eq!(std::fs::read(clone.join("file.txt")).unwrap(), b"initial\n");
        for reference in ["refs/remotes/origin/obsolete", "refs/tags/private-tag"] {
            assert_eq!(
                decode(
                    &job.output(&clone, &["rev-parse", "--verify", reference])
                        .await
                        .unwrap()
                )
                .unwrap()
                .trim(),
                preserved
            );
        }
        job.output(
            &clone,
            &[
                "config",
                "filter.sentinel.clean",
                "echo ran > filter-sentinel; cat",
            ],
        )
        .await
        .unwrap();
        job.output(&clone, &["config", "filter.sentinel.required", "true"])
            .await
            .unwrap();
        std::fs::write(clone.join(".gitattributes"), b"*.txt filter=sentinel\n").unwrap();
        std::fs::write(clone.join("file.txt"), b"raw changed bytes\r\n").unwrap();
        let mut head_context = fixture.context(CompareRef::Head);
        head_context.root = clone.clone();
        let mut working_context = head_context.clone();
        working_context.endpoint.reference = CompareRef::WorkingTree;
        let comparison = service
            .prepare(
                "no-filters",
                1,
                [head_context, working_context],
                Options::default(),
                &job,
            )
            .await
            .unwrap();
        let row = comparison
            .rows
            .iter()
            .find(|row| row.path == "file.txt")
            .unwrap();
        assert_eq!(row.raw_status, Status::Different);
        assert!(row.rename.is_none());
        assert!(row.reason.as_ref().unwrap().contains("clean filters"));
        assert_eq!(
            content(
                &comparison.right,
                "file.txt",
                &comparison.right.files["file.txt"],
                &job
            )
            .await
            .unwrap(),
            b"raw changed bytes\r\n"
        );
        assert!(!clone.join("filter-sentinel").exists());
        assert_eq!(std::fs::read(clone.join(".git/HEAD")).unwrap(), head);
        assert_eq!(std::fs::read(clone.join(".git/index")).unwrap(), index);
        let mutex = Arc::new(Mutex::new(()));
        let guard = mutex.lock().await;
        let blocked_job = fixture.job();
        let running_job = blocked_job.clone();
        let running_mutex = mutex.clone();
        let waiting =
            tokio::spawn(async move { running_job.lock(&running_mutex).await.map(|_| ()) });
        tokio::time::sleep(Duration::from_millis(50)).await;
        blocked_job.cancel.store(true, Ordering::Relaxed);
        assert_eq!(
            tokio::time::timeout(Duration::from_millis(500), waiting)
                .await
                .unwrap()
                .unwrap()
                .unwrap_err()
                .kind,
            "cancelled"
        );
        drop(guard);
        println!("recovery: pruning configs true; remote/tag refs retained, clean-filter sentinel absent, cancelled mutex waiter <500ms");
    }

    #[tokio::test]
    async fn divergent_history_counts_both_sides_and_cancel_obsoletes_running_refresh() {
        let _guard = git::TEST_RUNNER_LOCK.lock().await;
        let fixture = Fixture::new().await;
        fixture.write("file.txt", b"base\n");
        let base = fixture.commit("base").await;
        fixture.write("file.txt", b"left\nleft extra\n");
        let left = fixture.commit("left").await;
        fixture.git(&["checkout", "--detach", &base]).await;
        fixture.write("file.txt", b"right\n");
        let right = fixture.commit("right").await;
        let comparison = prepared(
            &fixture,
            CompareRef::Commit { sha: left },
            CompareRef::Commit { sha: right },
            Options::default(),
        )
        .await;
        assert_eq!(comparison.view.history.left_count, Some(1));
        assert_eq!(comparison.view.history.right_count, Some(1));
        assert_eq!(
            comparison.rows[0].raw_lines,
            Some(Lines {
                added: 1,
                removed: 2
            })
        );
        fixture.git(&["checkout", "--orphan", "unrelated"]).await;
        fixture.write("file.txt", b"orphan\n");
        let unrelated = fixture.commit("unrelated").await;
        let comparison = prepared(
            &fixture,
            CompareRef::Commit { sha: base },
            CompareRef::Commit { sha: unrelated },
            Options::default(),
        )
        .await;
        assert_eq!(
            comparison.view.history.reason.as_deref(),
            Some("unrelatedHistory")
        );
        let service = Service::default();
        let settings = fixture.settings();
        let opened = service
            .open(
                &settings,
                fixture.context(CompareRef::Head).endpoint,
                fixture.context(CompareRef::WorkingTree).endpoint,
            )
            .await
            .unwrap();
        let cancel = async {
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    if git::activity_snapshot().iter().any(|entry| {
                        serde_json::to_value(entry).unwrap()["context"]
                            == format!("compare:{}", opened.id)
                    }) {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert!(service.cancel(&opened.id).await);
        };
        let (refresh, ()) = tokio::join!(
            service.refresh(&settings, &opened.id, Options::default()),
            cancel
        );
        assert_eq!(refresh.err().unwrap().kind, "cancelled");
        assert!(service.snapshot(&settings, &opened.id, 1).await.is_err());
    }

    #[tokio::test]
    async fn compare_contract_unborn_reasons_capabilities_and_reload_reclamation() {
        let _guard = git::TEST_RUNNER_LOCK.lock().await;
        let fixture = Fixture::new().await;
        let settings = fixture.settings();
        let service = Service::default();
        let left = fixture.context(CompareRef::Head).endpoint;
        let right = fixture.context(CompareRef::WorkingTree).endpoint;
        let opened = service
            .open(&settings, left.clone(), right.clone())
            .await
            .unwrap();
        let result = service
            .refresh(&settings, &opened.id, Options::default())
            .await
            .unwrap();
        match result {
            RefreshResult::Unavailable { problem } => assert_eq!(
                serde_json::to_value(problem).unwrap()["reason"],
                "unbornHead"
            ),
            _ => panic!("Empty HEAD must be unavailable"),
        }
        let activity = serde_json::to_value(git::activity_snapshot()).unwrap();
        assert!(!activity
            .as_array()
            .unwrap()
            .iter()
            .filter(|entry| entry["context"] == format!("compare:{}", opened.id))
            .any(|entry| entry["argv"]
                .as_array()
                .unwrap()
                .iter()
                .any(|arg| arg == "fetch")));
        assert_eq!(
            serde_json::to_value(metadata_path(b"--path-format=absolute\n.git\n").unwrap_err())
                .unwrap()["reason"],
            "gitCapability"
        );
        assert_eq!(
            serde_json::to_value(decode(&[0xff]).unwrap_err()).unwrap()["reason"],
            "unsupportedEncoding"
        );
        for _ in 1..16 {
            service
                .open(&settings, left.clone(), right.clone())
                .await
                .unwrap();
        }
        assert_eq!(
            service
                .open(&settings, left.clone(), right.clone())
                .await
                .err()
                .unwrap()
                .kind,
            "limitExceeded"
        );
        let flags: Vec<_> = service
            .sessions
            .lock()
            .await
            .values()
            .map(|session| session.cancel.clone())
            .collect();
        let saved_workspace = settings.workspace.clone();
        service.release_sessions().await;
        assert!(flags.iter().all(|flag| flag.load(Ordering::Relaxed)));
        assert!(service.sessions.lock().await.is_empty());
        assert_eq!(settings.workspace, saved_workspace);
        let opened = service.open(&settings, left, right).await.unwrap();
        assert_eq!(
            serde_json::to_value(
                service
                    .snapshot(&settings, &opened.id, 0)
                    .await
                    .err()
                    .unwrap()
            )
            .unwrap()["reason"],
            "refreshRequired"
        );
        let invalid = resolve(
            &fixture.context(CompareRef::RemoteBranch {
                name: "origin".into(),
            }),
            &fixture.job(),
        )
        .await
        .unwrap_err();
        assert_eq!(invalid.kind, "invalidRef");
        fixture.write("file.txt", b"unmerged\n");
        fixture.commit("initial").await;
        let oid = decode(&fixture.git(&["rev-parse", "HEAD:file.txt"]).await)
            .unwrap()
            .trim()
            .to_string();
        let input = format!(
            "0 {}\tfile.txt\n100644 {oid} 1\tfile.txt\n",
            "0".repeat(oid.len())
        );
        fixture
            .job()
            .run_input(
                &fixture.0.join("repo"),
                &["update-index", "--index-info"],
                &[0],
                Some(input.as_bytes()),
            )
            .await
            .unwrap();
        let result = service
            .refresh(&settings, &opened.id, Options::default())
            .await
            .unwrap();
        match result {
            RefreshResult::Unavailable { problem } => assert_eq!(
                serde_json::to_value(problem).unwrap()["reason"],
                "unmergedIndex"
            ),
            _ => panic!("Unmerged index needs a typed unavailable reason"),
        }
        println!("contract: unborn HEAD no fetch, remote namespace explicit, typed reasons/capability diagnostic, 16-session reclamation cancels jobs without settings loss");
    }

    #[tokio::test]
    async fn registered_sessions_unavailable_cancel_and_stale_bytes() {
        let _guard = git::TEST_RUNNER_LOCK.lock().await;
        let fixture = Fixture::new().await;
        fixture.write("file.txt", b"original\r\n");
        fixture.commit("initial").await;
        let settings = fixture.settings();
        let service = Service::default();
        let left = fixture.context(CompareRef::Head).endpoint;
        let right = fixture.context(CompareRef::WorkingTree).endpoint;
        let mut forged = left.clone();
        forged.item_id = "unregistered".into();
        assert_eq!(
            service
                .open(&settings, forged, right.clone())
                .await
                .err()
                .unwrap()
                .kind,
            "invalidContext"
        );
        assert!(serde_json::from_value::<Endpoint>(serde_json::json!({"setId":"set","itemId":"item","reference":{"kind":"head"},"path":"C:/arbitrary"})).is_err());
        let opened = service
            .open(&settings, left.clone(), right.clone())
            .await
            .unwrap();
        let snapshot = match service
            .refresh(&settings, &opened.id, Options::default())
            .await
            .unwrap()
        {
            RefreshResult::Ready { snapshot } => snapshot,
            _ => panic!("Expected ready snapshot"),
        };
        let (prepared, job) = service
            .snapshot(&settings, &opened.id, snapshot.generation)
            .await
            .unwrap();
        let entry = &prepared.right.files["file.txt"];
        assert_eq!(
            content(&prepared.right, "file.txt", entry, &job)
                .await
                .unwrap(),
            b"original\r\n"
        );
        fixture.write("file.txt", b"external change\r\n");
        assert_eq!(
            content(&prepared.right, "file.txt", entry, &job)
                .await
                .unwrap_err()
                .kind,
            "staleContent"
        );
        assert!(service.cancel(&opened.id).await);
        assert_eq!(
            service
                .snapshot(&settings, &opened.id, snapshot.generation)
                .await
                .err()
                .unwrap()
                .kind,
            "staleGeneration"
        );
        assert!(service.close(&opened.id).await);
        assert!(!service.close(&opened.id).await);
        let mut missing = right;
        missing.item_id = "missing".into();
        let opened = service.open(&settings, left, missing).await.unwrap();
        match service
            .refresh(&settings, &opened.id, Options::default())
            .await
            .unwrap()
        {
            RefreshResult::Unavailable { problem } => {
                assert_eq!(problem.side.as_deref(), Some("right"));
                assert_eq!(problem.message, "notCloned");
                assert_eq!(
                    serde_json::to_value(problem).unwrap()["reason"],
                    "notCloned"
                );
            }
            _ => panic!("Not-cloned must be an unavailable result, not an IPC error"),
        }
    }

    #[tokio::test]
    async fn missing_refs_share_one_fetch_and_network_errors_are_separate() {
        let _guard = git::TEST_RUNNER_LOCK.lock().await;
        let fixture = Fixture::new().await;
        fixture.write("file.txt", b"fixture\n");
        fixture.commit("initial").await;
        fixture
            .git(&[
                "remote",
                "add",
                "origin",
                fixture.0.join("repo").to_str().unwrap(),
            ])
            .await;
        let service = Service::default();
        let contexts = [
            fixture.context(CompareRef::Tag {
                name: "absent".into(),
            }),
            fixture.context(CompareRef::Head),
        ];
        let job1 = fixture.job();
        let job2 = fixture.job();
        let (first, second) = tokio::join!(
            service.prepare("first", 1, contexts.clone(), Options::default(), &job1),
            service.prepare("second", 1, contexts, Options::default(), &job2)
        );
        assert_eq!(first.err().unwrap().kind, "missingLeft");
        assert_eq!(second.err().unwrap().kind, "missingLeft");
        let root = std::fs::canonicalize(fixture.0.join("repo")).unwrap();
        assert_eq!(
            service.fetch_state(&root).await.unwrap().lock().await.epoch,
            1
        );
        let problem = service
            .prepare(
                "right",
                1,
                [
                    fixture.context(CompareRef::Head),
                    fixture.context(CompareRef::Tag {
                        name: "absent".into(),
                    }),
                ],
                Options::default(),
                &fixture.job(),
            )
            .await
            .err()
            .unwrap();
        assert_eq!(problem.kind, "missingRight");
        fixture
            .git(&[
                "remote",
                "set-url",
                "origin",
                fixture.0.join("unavailable-origin").to_str().unwrap(),
            ])
            .await;
        let problem = service
            .prepare(
                "network",
                1,
                [
                    fixture.context(CompareRef::Tag {
                        name: "absent".into(),
                    }),
                    fixture.context(CompareRef::Head),
                ],
                Options::default(),
                &fixture.job(),
            )
            .await
            .err()
            .unwrap();
        assert_eq!(problem.kind, "networkError");
        assert_eq!(problem.side.as_deref(), Some("left"));
        let cancelled = fixture.job();
        cancelled.cancel.store(true, Ordering::Relaxed);
        assert_eq!(
            service
                .prepare(
                    "cancel",
                    1,
                    [
                        fixture.context(CompareRef::Head),
                        fixture.context(CompareRef::Head)
                    ],
                    Options::default(),
                    &cancelled
                )
                .await
                .err()
                .unwrap()
                .kind,
            "cancelled"
        );
    }

    #[tokio::test]
    async fn full_union_renames_binary_normalization_and_history() {
        let _guard = git::TEST_RUNNER_LOCK.lock().await;
        let fixture = Fixture::new().await;
        fixture.git(&["config", "core.autocrlf", "false"]).await;
        for (path, bytes) in [
            ("same.txt", b"same\n".as_slice()),
            ("old.txt", b"renamed\n"),
            ("left.txt", b"left\n"),
            ("binary.dat", b"\0left"),
            ("normalized.txt", b"a b\n"),
            ("node", b"file\n"),
        ] {
            fixture.write(path, bytes);
        }
        let left = fixture.commit("left").await;
        fixture.git(&["mv", "old.txt", "new.txt"]).await;
        std::fs::remove_file(fixture.0.join("repo/left.txt")).unwrap();
        std::fs::remove_file(fixture.0.join("repo/node")).unwrap();
        fixture.write("node/child.txt", b"child\n");
        fixture.write("right.txt", b"right\n");
        fixture.write("binary.dat", b"\0right");
        fixture.write("normalized.txt", b"ab\r\n");
        let right = fixture.commit("right").await;
        let head = std::fs::read(fixture.0.join("repo/.git/HEAD")).unwrap();
        let index = std::fs::read(fixture.0.join("repo/.git/index")).unwrap();
        let comparison = prepared(
            &fixture,
            CompareRef::Commit { sha: left },
            CompareRef::Commit { sha: right },
            Options {
                normalize_eol: true,
                ignore_whitespace: true,
            },
        )
        .await;
        let row = |path: &str| comparison.rows.iter().find(|row| row.path == path).unwrap();
        assert_eq!(row("same.txt").raw_status, Status::Same);
        assert_eq!(row("old.txt").raw_status, Status::LeftOnly);
        assert_eq!(row("new.txt").raw_status, Status::RightOnly);
        assert_eq!(row("old.txt").rename.as_ref().unwrap().to, "new.txt");
        assert_eq!(row("binary.dat").binary, Some(true));
        assert!(row("binary.dat").raw_lines.is_none());
        assert_eq!(row("normalized.txt").raw_status, Status::Different);
        assert_eq!(row("normalized.txt").display_status, Status::Same);
        assert_eq!(row("node").raw_status, Status::TypeConflict);
        assert_eq!(comparison.view.raw.type_conflict, 1);
        assert!(row("same.txt").left.as_ref().unwrap().modified_ms.is_none());
        assert_eq!(comparison.view.history.left_count, Some(0));
        assert_eq!(comparison.view.history.right_count, Some(1));
        assert_eq!(
            std::fs::read(fixture.0.join("repo/.git/HEAD")).unwrap(),
            head
        );
        assert_eq!(
            std::fs::read(fixture.0.join("repo/.git/index")).unwrap(),
            index
        );
        assert_eq!(
            std::fs::read(fixture.0.join("repo/normalized.txt")).unwrap(),
            b"ab\r\n"
        );
        let same = prepared(
            &fixture,
            CompareRef::Head,
            CompareRef::Head,
            Options::default(),
        )
        .await;
        assert_eq!(same.view.display.same, same.view.display.total);
        assert_eq!(same.view.history.left_count, Some(0));
    }

    #[tokio::test]
    async fn cross_repository_bytes_and_unrelated_shallow_history_are_honest() {
        let _guard = git::TEST_RUNNER_LOCK.lock().await;
        let fixture = Fixture::new().await;
        fixture.write("same.txt", b"equal\n");
        fixture.write("changed.txt", b"left\n");
        fixture.commit("left").await;
        let other = Fixture::new().await;
        other.write("same.txt", b"equal\n");
        other.write("changed.txt", b"right\nextra\n");
        other.write("orphan.txt", b"orphan\n");
        other.commit("unrelated").await;
        let service = Service::default();
        let job = fixture.job();
        let comparison = service
            .prepare(
                "cross",
                1,
                [
                    fixture.context(CompareRef::Head),
                    other.context(CompareRef::Head),
                ],
                Options::default(),
                &job,
            )
            .await
            .unwrap();
        assert_eq!(
            comparison
                .rows
                .iter()
                .find(|row| row.path == "same.txt")
                .unwrap()
                .raw_status,
            Status::Same
        );
        let changed = comparison
            .rows
            .iter()
            .find(|row| row.path == "changed.txt")
            .unwrap();
        assert_eq!(
            changed.raw_lines,
            Some(Lines {
                added: 2,
                removed: 1
            })
        );
        assert_eq!(comparison.view.raw.right_only, 1);
        assert!(!comparison.view.history.available);
        assert!(comparison.view.history.left_count.is_none());
        let unsuitable = fixture.0.join("unsuitable-temp");
        std::fs::write(&unsuitable, b"not a directory").unwrap();
        let partial_job = Job {
            temporary_root: Some(unsuitable),
            ..fixture.job()
        };
        assert!(line_counts(b"left\n", b"right\n", &partial_job)
            .await
            .is_err());
        let partial = service
            .prepare(
                "partial-counts",
                2,
                [
                    fixture.context(CompareRef::Head),
                    other.context(CompareRef::Head),
                ],
                Options {
                    normalize_eol: true,
                    ignore_whitespace: true,
                },
                &partial_job,
            )
            .await
            .unwrap();
        assert_eq!(partial.view.raw.same, 1);
        assert_eq!(partial.view.raw.different, 1);
        let changed = partial
            .rows
            .iter()
            .find(|row| row.path == "changed.txt")
            .unwrap();
        assert_eq!(changed.raw_status, Status::Different);
        assert!(changed.raw_lines.is_none() && changed.display_lines.is_none());
        assert!(changed
            .reason
            .as_ref()
            .unwrap()
            .contains("Line counts unavailable"));
        assert_eq!(
            std::fs::read(partial_job.temporary_root.as_ref().unwrap()).unwrap(),
            b"not a directory"
        );
        println!("partial numstat: unsuitable private TEMP, ready comparison; raw/display counts N/A with reason; originals preserved");
        fixture.write("same.txt", b"second\n");
        fixture.commit("second").await;
        let clone = fixture.0.join("shallow");
        let url = format!(
            "file:///{}",
            fixture.0.join("repo").to_string_lossy().replace('\\', "/")
        );
        fixture
            .git(&["clone", "--depth=1", "--", &url, clone.to_str().unwrap()])
            .await;
        let mut context = fixture.context(CompareRef::WorkingTree);
        context.root = clone;
        let comparison = service
            .prepare(
                "shallow",
                1,
                [context.clone(), context],
                Options::default(),
                &job,
            )
            .await
            .unwrap();
        assert_eq!(
            comparison.view.history.reason.as_deref(),
            Some("shallowHistory")
        );
        assert_eq!(comparison.view.history.left_basis, "workingTreeHead");
    }

    #[tokio::test]
    async fn refs_and_working_tree_inventory_are_read_only() {
        let _guard = git::TEST_RUNNER_LOCK.lock().await;
        let fixture = Fixture::new().await;
        fixture.write("same.txt", b"unchanged\n");
        fixture.write("folder/unicode-\u{e9}.txt", b"old\n");
        let sha = fixture.commit("initial").await;
        fixture.git(&["tag", "v1"]).await;
        fixture
            .git(&[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.test",
                "tag",
                "-a",
                "annotated",
                "-m",
                "tag",
                &sha,
            ])
            .await;
        let job = fixture.job();
        for reference in [
            CompareRef::Head,
            CompareRef::WorkingTree,
            CompareRef::Branch {
                name: "main".into(),
            },
            CompareRef::Tag { name: "v1".into() },
            CompareRef::Tag {
                name: "annotated".into(),
            },
            CompareRef::Commit {
                sha: sha[..8].into(),
            },
        ] {
            assert_eq!(
                resolve(&fixture.context(reference), &job).await.unwrap(),
                Some(sha.clone())
            );
        }
        for reference in [
            CompareRef::Branch {
                name: "main~1".into(),
            },
            CompareRef::Commit {
                sha: "HEAD^".into(),
            },
            CompareRef::Commit {
                sha: "--help".into(),
            },
        ] {
            assert_eq!(
                resolve(&fixture.context(reference), &job)
                    .await
                    .unwrap_err()
                    .kind,
                "invalidRef"
            );
        }
        fixture.write("folder/unicode-\u{e9}.txt", b"staged\r\n");
        fixture.git(&["add", "."]).await;
        fixture.write("folder/unicode-\u{e9}.txt", b"actual unstaged\r\n");
        fixture.write("untracked.txt", b"untracked\n");
        std::fs::remove_file(fixture.0.join("repo/same.txt")).unwrap();
        let index = std::fs::read(fixture.0.join("repo/.git/index")).unwrap();
        let head = std::fs::read(fixture.0.join("repo/.git/HEAD")).unwrap();
        let context = fixture.context(CompareRef::WorkingTree);
        let root = read_root(&context, &job).await.unwrap();
        let files = inventory(&context, &root, &sha, &job).await.unwrap();
        assert!(files.contains_key("untracked.txt"));
        assert!(!files.contains_key("same.txt"));
        assert!(files.contains_key("folder"));
        assert_eq!(
            files["folder/unicode-\u{e9}.txt"].size,
            Some(b"actual unstaged\r\n".len() as u64)
        );
        assert_eq!(
            std::fs::read(fixture.0.join("repo/.git/index")).unwrap(),
            index
        );
        assert_eq!(
            std::fs::read(fixture.0.join("repo/.git/HEAD")).unwrap(),
            head
        );
    }
}
