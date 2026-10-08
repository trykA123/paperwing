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

mod count_eligibility;
mod history;
mod github_source;
mod remote;
mod remote_service;
pub use github_source::Preference as CompareSource;
mod index_objects;
mod inventory;
mod line_counts;
mod object_id;
mod registration;
mod text_diff;
mod working_inventory;

use history::{history, HistorySource};
use inventory::{content, inventory, Entry, Kind, Resolved};
use object_id::ObjectFormat;
use registration::bind;
use text_diff::{binary, count_result, diff_metadata, line_counts, normalized};

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_at: Option<i64>,
}

impl Problem {
    fn new(kind: &str, message: &str) -> Self {
        Self {
            kind: kind.into(),
            side: None,
            message: git::safe(message),
            reason: (kind == "unavailable").then_some(UnavailableReason::Other),
            retry_at: None,
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

#[derive(Clone)]
struct Job {
    rust_counts: Option<count_eligibility::EolMode>,
    count_root: Option<PathBuf>,
    readers: Arc<Mutex<Vec<git::BatchReader>>>,
    context: String,
    cancel: Arc<AtomicBool>,
    #[cfg(target_os = "linux")]
    diff: Option<Arc<crate::linux_diff::Storage>>,
    #[cfg(target_os = "linux")]
    roots: Vec<crate::linux_guard::root::RootValue>,
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
        #[cfg(feature = "benchmark")]
        let _span = crate::benchmark::Span::new("compare.queue", "other");
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
        self.captured_output(root, args).await.map(|result| result.stdout)
    }
    async fn captured_output(&self, root: &Path, args: &[&str]) -> Result<git::Captured, Problem> {
        let result = self.run(root, args, &[0]).await?;
        if result.code != Some(0) {
            return Err(Problem::new(
                "gitError",
                &result.last_error(),
            ));
        }
        Ok(result)
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
    let top = String::from_utf8(top).map_err(|_| Problem::new("unsafePath", "Unsupported root encoding"))?;
    let top = top.strip_suffix('\n').unwrap_or(&top);
    #[cfg(windows)]
    let top = top.strip_suffix('\r').unwrap_or(top);
    let top = PathBuf::from(top);
    let top_identity = crate::platform::physical_identity(&top).map_err(|error| Problem::new("unsafePath", &error))?;
    let root_identity = crate::platform::physical_identity(&context.root).map_err(|error| Problem::new("unsafePath", &error))?;
    if top_identity != root_identity {
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
    let output = output.strip_suffix('\n').unwrap_or(&output);
    #[cfg(windows)]
    let output = output.strip_suffix('\r').unwrap_or(output);
    let path = PathBuf::from(output);
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

fn decode(bytes: &[u8]) -> Result<String, Problem> {
    String::from_utf8(bytes.to_vec()).map_err(|_| {
        Problem::unavailable(
            UnavailableReason::UnsupportedEncoding,
            "Non-UTF-8 Git paths are unsupported",
        )
    })
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
    source: &'static str,
    truncated: crate::github::compare::Truncated,
}

#[derive(Clone)]
struct Prepared {
    view: Snapshot,
    left: Resolved,
    right: Resolved,
    rows: Vec<FileRow>,
    history_source: Option<HistorySource>,
}

#[cfg(test)]
impl Prepared {
    async fn close_readers(&self) {
        self.left.reader.close().await;
        self.right.reader.close().await;
    }
}

async fn close_readers(readers: &Mutex<Vec<git::BatchReader>>) {
    let readers = std::mem::take(&mut *readers.lock().await);
    futures_util::future::join_all(readers.iter().map(git::BatchReader::close)).await;
}

#[derive(Default)]
struct Fetch {
    epoch: u64,
    problem: Option<Problem>,
}

#[cfg(all(test, target_os = "linux"))]
type WriteRootHook = Arc<dyn Fn() + Send + Sync>;

type DiffConfigurations = HashMap<PathBuf, Arc<tokio::sync::OnceCell<Vec<String>>>>;

pub struct Service {
    sessions: std::sync::Mutex<HashMap<String, Session>>,
    fetches: Mutex<HashMap<PathBuf, Arc<Mutex<Fetch>>>>,
    diff_configs: std::sync::Mutex<DiffConfigurations>,
    slots: Semaphore,
    #[cfg(target_os = "linux")]
    diff: std::sync::OnceLock<Arc<crate::linux_diff::Storage>>,
    counts: std::sync::OnceLock<Arc<count_eligibility::Eligibility>>,
    remote_cache: std::sync::OnceLock<Arc<crate::github::blob::Cache>>,
    remote_store: std::sync::OnceLock<crate::store::Store>,
    #[cfg(all(test, target_os = "linux"))]
    fresh_write_root_hook: std::sync::Mutex<Option<WriteRootHook>>,
}

enum SourcePrepared {
    Local(Box<Prepared>),
    Github(Box<remote::Prepared>),
}

struct Session {
    readers: Arc<Mutex<Vec<git::BatchReader>>>,
    left: Context,
    right: Context,
    generation: u64,
    cancel: Arc<AtomicBool>,
    prepared: Option<Arc<Prepared>>,
    remote: Option<Arc<remote::Prepared>>,
    notify: Arc<tokio::sync::Notify>,
}

impl Default for Service {
    fn default() -> Self {
        Self {
            sessions: std::sync::Mutex::new(HashMap::new()),
            fetches: Mutex::new(HashMap::new()),
            diff_configs: std::sync::Mutex::new(HashMap::new()),
            slots: Semaphore::new(4),
            #[cfg(target_os = "linux")]
            diff: std::sync::OnceLock::new(),
            counts: std::sync::OnceLock::new(),
            remote_cache: std::sync::OnceLock::new(),
            remote_store: std::sync::OnceLock::new(),
            #[cfg(all(test, target_os = "linux"))]
            fresh_write_root_hook: std::sync::Mutex::new(None),
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
    fn sessions(&self) -> std::sync::MutexGuard<'_, HashMap<String, Session>> {
        self.sessions.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn diff_configs(&self) -> std::sync::MutexGuard<'_, DiffConfigurations> {
        self.diff_configs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn reset_configuration(&self, contexts: &[Context]) {
        if let Some(counts) = self.counts.get() {
            counts.reset();
        }
        let mut configs = self.diff_configs();
        for context in contexts {
            configs.remove(&context.root);
        }
    }

    #[cfg(target_os = "linux")]
    pub(crate) async fn write_revocation(
        &self,
        settings: &crate::settings::Settings,
        id: &str,
        generation: u64,
    ) -> Result<Arc<AtomicBool>, String> {
        let (_, job) = self.snapshot(settings, id, generation).await.map_err(|problem| problem.message)?;
        job.check().map_err(|problem| problem.message)?;
        Ok(job.cancel)
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn configure_diff(&self, path: PathBuf) -> Result<(), String> {
        let _ = self.counts.set(Arc::new(count_eligibility::Eligibility::new(path.clone())));
        let storage = crate::linux_diff::Storage::new(path).map_err(|error| error.to_string())?;
        self.diff.set(Arc::new(storage)).map_err(|_| "Comparison diff storage is already configured".into())
    }
    async fn open(
        &self,
        settings: &crate::settings::Settings,
        left: Endpoint,
        right: Endpoint,
    ) -> Result<Opened, Problem> {
        let left = bind(settings, left).map_err(|problem| problem.side("left"))?;
        let right = bind(settings, right).map_err(|problem| problem.side("right"))?;
        let mut sessions = self.sessions();
        if sessions.len() >= 16 {
            return Err(Problem::new(
                "limitExceeded",
                "Close a comparison before opening another",
            ));
        }
        self.reset_configuration(&[left.clone(), right.clone()]);
        let id = format!("comparison-{}", NEXT.fetch_add(1, Ordering::Relaxed));
        sessions.insert(
            id.clone(),
            Session {
                readers: Arc::default(),
                left,
                right,
                generation: 0,
                cancel: Arc::new(AtomicBool::new(false)),
                prepared: None,
                remote: None,
                notify: Arc::default(),
            },
        );
        Ok(Opened { id, generation: 0 })
    }

    fn rebind(settings: &crate::settings::Settings, context: &Context) -> Result<(), Problem> {
        let current = bind(settings, context.endpoint.clone())?;
        let same_root = crate::platform::same_destination(&current.root, &context.root)
            .map_err(|error| Problem::new("unsafePath", &error))?;
        let same_workspace = crate::platform::same_destination(&current.workspace_root, &context.workspace_root)
            .map_err(|error| Problem::new("unsafePath", &error))?;
        if !same_root || !same_workspace {
            return Err(Problem::new(
                "staleContext",
                "Registered destination changed; reopen comparison",
            ));
        }
        Ok(())
    }

    async fn close(&self, id: &str) -> bool {
        let session = self.sessions().remove(id);
        if let Some(session) = session {
            session.cancel.store(true, Ordering::Relaxed);
            session.notify.notify_waiters();
            close_readers(&session.readers).await;
            true
        } else {
            false
        }
    }

    pub fn release_sessions(&self) -> impl std::future::Future<Output = ()> + Send + 'static {
        let sessions: Vec<_> = self.sessions().drain()
            .map(|(_, session)| session)
            .collect();
        for session in &sessions {
            session.cancel.store(true, Ordering::Relaxed);
            session.notify.notify_waiters();
        }
        async move {
            futures_util::future::join_all(sessions.iter().map(|session| close_readers(&session.readers))).await;
        }
    }

    async fn cancel(&self, id: &str) -> bool {
        let readers = {
            let mut sessions = self.sessions();
            let Some(session) = sessions.get_mut(id) else {
                return false;
            };
            session.cancel.store(true, Ordering::Relaxed);
            session.notify.notify_waiters();
            session.generation += 1;
            session.prepared = None;
            session.remote = None;
            session.readers.clone()
        };
        close_readers(&readers).await;
        true
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
        #[cfg(feature = "benchmark")]
        let _span = crate::benchmark::Span::new("compare.prepare", "other");
        let [left_context, right_context] = contexts;
        let left_safe = read_root(&left_context, job)
            .await
            .map_err(|problem| problem.side("left"))?;
        let right_safe = read_root(&right_context, job)
            .await
            .map_err(|problem| problem.side("right"))?;
        #[cfg(target_os = "linux")]
        let storage_job = {
            let mut captured = job.clone();
            if let Some(storage) = &captured.diff {
                captured.roots = storage.capture(vec![left_safe.clone(), right_safe.clone()], &captured.cancel).await
                    .map_err(|error| {
                        Problem::new(if error.cancelled { "cancelled" } else { "unavailable" }, error.message)
                    })?;
            }
            captured
        };
        #[cfg(target_os = "linux")]
        let job = &storage_job;
        let mut count_job = job.clone();
        let eligibility = self.counts.get_or_init(|| {
            #[cfg(target_os = "linux")]
            let eligibility = count_eligibility::Eligibility::default();
            #[cfg(not(target_os = "linux"))]
            let eligibility = count_eligibility::Eligibility::new(std::env::temp_dir());
            Arc::new(eligibility)
        });
        let fallback;
        let eligibility = if job.count_root.as_ref().is_some_and(|root| Some(root) != eligibility.storage.as_ref()) {
            fallback = count_eligibility::Eligibility::new(job.count_root.clone().ok_or_else(|| Problem::new("unavailable", "Count storage unavailable"))?);
            &fallback
        } else { eligibility.as_ref() };
        let left_counts = eligibility.configuration(&left_context.root, job).await?;
        let right_counts = eligibility.configuration(&right_context.root, job).await?;
        count_job.rust_counts = left_counts.filter(|mode| Some(*mode) == right_counts);
        let job = &count_job;
        let contexts = [&left_context, &right_context];
        let roots = [&left_safe.path, &right_safe.path];
        let mut states = Vec::new();
        let mut epochs = Vec::new();
        for root in roots {
            let state = self.fetch_state(root).await?;
            epochs.push(job.lock(&state).await?.epoch);
            states.push(state);
        }
        let mut diff_configs = Vec::new();
        for context in contexts {
            let config = {
                let mut configs = self.diff_configs();
                if configs.len() >= 32 {
                    configs.retain(|_, config| Arc::strong_count(config) > 1);
                }
                configs.entry(context.root.clone()).or_default().clone()
            };
            let values = config.get_or_try_init(|| async {
                let result = job.run(&context.root, &["config", "--get-regexp", "^diff\\.(algorithm|renamelimit)$"], &[0, 1]).await?;
                let mut values = Vec::new();
                for line in decode(&result.stdout)?.lines() {
                    if let Some((key, value)) = line.split_once(' ') {
                        values.extend(["-c".into(), format!("{key}={value}")]);
                    }
                }
                Ok::<_, Problem>(values)
            }).await?;
            diff_configs.push(values.clone());
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
                        &result.last_error(),
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
        let left_format = ObjectFormat::read(&left_context.root, job).await?;
        let right_format = ObjectFormat::read(&right_context.root, job).await?;
        let left_reader = git::BatchReader::new(left_context.root.clone(), job.cancel.clone());
        let right_reader = if left_context.root == right_context.root
            || left_reader.shares_directory(right_context.root.clone()).await {
            left_reader.clone()
        } else {
            git::BatchReader::new(right_context.root.clone(), job.cancel.clone())
        };
        {
            let mut readers = job.readers.lock().await;
            job.check()?;
            readers.extend([left_reader.clone(), right_reader.clone()]);
        }
        let mut left = Resolved {
            object_format: left_format,
            diff_config: diff_configs[0].clone(),
            reader: left_reader,
            context: left_context,
            safe: left_safe,
            commit: commits[0].take().unwrap(),
            files: BTreeMap::new(),
        };
        let mut right = Resolved {
            object_format: right_format,
            diff_config: diff_configs[1].clone(),
            reader: right_reader,
            context: right_context,
            safe: right_safe,
            commit: commits[1].take().unwrap(),
            files: BTreeMap::new(),
        };
        left.files = inventory(&left, job).await.map_err(|problem| problem.side("left"))?;
        right.files = inventory(&right, job).await.map_err(|problem| problem.side("right"))?;
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
                let identical_oid = matches!((left_entry, right_entry), (Some(left), Some(right)) if left.reason.is_none() && right.reason.is_none() && left.oid.is_some() && left.blob_id == right.blob_id && left.oid == right.oid && left.mode == right.mode)
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
            source: "local",
            truncated: Default::default(),
        };
        Ok(Prepared {
            view,
            left,
            right,
            rows,
            history_source,
        })
    }

    #[cfg(test)]
    async fn refresh(&self, settings: &crate::settings::Settings, id: &str, options: Options) -> Result<RefreshResult, Problem> {
        self.refresh_with_source(settings, id, options, None).await
    }

    async fn refresh_with_source(
        &self,
        settings: &crate::settings::Settings,
        id: &str,
        options: Options,
        source: Option<CompareSource>,
    ) -> Result<RefreshResult, Problem> {
        let (contexts, generation, job, previous, notify) = {
            let mut sessions = self.sessions();
            let session = sessions
                .get_mut(id)
                .ok_or_else(|| Problem::new("unknownSession", "Unknown comparison"))?;
            Self::rebind(settings, &session.left)?;
            Self::rebind(settings, &session.right)?;
            session.cancel.store(true, Ordering::Relaxed);
            session.notify.notify_waiters();
            session.cancel = Arc::new(AtomicBool::new(false));
            session.notify = Arc::default();
            session.generation += 1;
            session.prepared = None;
            session.remote = None;
            let previous = std::mem::take(&mut session.readers);
            (
                [session.left.clone(), session.right.clone()],
                session.generation,
                Job {
                    rust_counts: None,
                    count_root: self.counts.get().and_then(|counts| counts.storage.clone()),
                    readers: session.readers.clone(),
                    context: format!("compare:{id}"),
                    cancel: session.cancel.clone(),
                    #[cfg(target_os = "linux")] diff: self.diff.get().cloned(),
                    #[cfg(target_os = "linux")] roots: Vec::new(),
                    #[cfg(test)]
                    temporary_root: None,
                    #[cfg(test)]
                    inventory_started: None,
                },
                previous,
                session.notify.clone(),
            )
        };
        close_readers(&previous).await;
        self.reset_configuration(&contexts);
        let _permit = job.slot(&self.slots).await?;
        let cancelled = notify.notified();
        tokio::pin!(cancelled);
        cancelled.as_mut().enable();
        job.check()?;
        let result = tokio::select! {
            biased;
            _ = &mut cancelled => Err(Problem::new("cancelled", "Comparison cancelled")),
            result = self.prepare_source(settings, remote::Refresh { id, generation, endpoints: [contexts[0].endpoint.clone(), contexts[1].endpoint.clone()], options, job: &job }, contexts, source) => result,
        };
        if result.is_err() {
            close_readers(&job.readers).await;
        }
        job.check()?;
        let mut sessions = self.sessions();
        let session = sessions
            .get_mut(id)
            .filter(|session| session.generation == generation)
            .ok_or_else(|| Problem::new("staleGeneration", "Obsolete comparison result"))?;
        match result {
            Ok(SourcePrepared::Local(prepared)) => {
                let snapshot = Box::new(prepared.view.clone());
                session.prepared = Some(Arc::new(*prepared));
                Ok(RefreshResult::Ready { snapshot })
            }
            Ok(SourcePrepared::Github(prepared)) => {
                let snapshot = Box::new(prepared.view.clone());
                session.remote = Some(Arc::new(*prepared));
                Ok(RefreshResult::Ready { snapshot })
            }
            Err(problem) => match problem.kind.as_str() {
                "unavailable" => Ok(RefreshResult::Unavailable { problem }),
                "invalidRef" => Ok(RefreshResult::InvalidRef { problem }),
                "missingLeft" => Ok(RefreshResult::MissingLeft { problem }),
                "missingRight" => Ok(RefreshResult::MissingRight { problem }),
                "networkError" => Ok(RefreshResult::NetworkError { problem }),
                "githubUnavailable" | "githubNotFound" | "githubRateLimited" => Ok(RefreshResult::Unavailable { problem }),
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
        #[cfg(target_os = "linux")]
        let expected_root = resolved.safe.linux_value()?;
        #[cfg(all(test, target_os = "linux"))]
        if fresh {
            let hook = self.fresh_write_root_hook.lock().map_err(|_| "Root test control unavailable")?.clone();
            if let Some(hook) = hook { hook(); }
        }
        let safe = if fresh { read_root(&resolved.context, &job).await.map_err(|problem| problem.message)? } else { resolved.safe.clone() };
        #[cfg(target_os = "linux")]
        if safe.linux_value()? != expected_root {
            return Err("Repository root changed during validation; reopen the comparison".into());
        }
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
        let sessions = self.sessions();
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
                rust_counts: None,
                count_root: self.counts.get().and_then(|counts| counts.storage.clone()),
                readers: session.readers.clone(),
                context: format!("compare:{id}"),
                cancel: session.cancel.clone(),
                #[cfg(target_os = "linux")] diff: self.diff.get().cloned(),
                #[cfg(target_os = "linux")] roots: Vec::new(),
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
    registration::registered_clone_destination(settings, id, destination, url)
}

type SetRoots = (Vec<(String, PathBuf)>, std::collections::HashSet<crate::platform::DestinationKey>);

pub fn set_roots(settings: &crate::settings::Settings, set_id: &str) -> Result<SetRoots, String> {
    registration::set_roots(settings, set_id)
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
                registration::confined_destination(&context.workspace_root, root)?;
                if root.exists() && crate::platform::same_destination(&context.root, root)? {
                    let job = Job {
                        rust_counts: None,
                        count_root: None,
                        readers: Arc::default(),
                        context: "Recovery authorization".into(), cancel: Arc::new(AtomicBool::new(false)),
                        #[cfg(target_os = "linux")] diff: None,
                        #[cfg(target_os = "linux")] roots: Vec::new(),
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

pub struct Content {
    pub(crate) bytes: Vec<u8>,
}

impl tauri::ipc::IpcResponse for Content {
    fn body(self) -> tauri::Result<tauri::ipc::InvokeResponseBody> {
        tauri::ipc::IpcResponse::body(tauri::ipc::Response::new(self.bytes))
    }
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
    #[cfg(feature = "benchmark")]
    let _span = crate::benchmark::Span::new("ipc.open", "other");
    service.open(&saved(&app)?, left, right).await
}

#[tauri::command]
pub async fn comparison_refresh(
    app: tauri::AppHandle,
    service: tauri::State<'_, Service>,
    id: String,
    options: Options,
    source: Option<CompareSource>,
) -> Result<RefreshResult, Problem> {
    #[cfg(feature = "benchmark")]
    let _span = crate::benchmark::Span::new("ipc.refresh", "other");
    service.refresh_with_source(&saved(&app)?, &id, options, source).await
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
    #[cfg(feature = "benchmark")]
    let _span = crate::benchmark::Span::new("ipc.files", "other");
    if limit == 0 || limit > 512 || offset > FILE_LIMIT {
        return Err(Problem::new("limitExceeded", "Invalid inventory page"));
    }
    let settings = saved(&app)?;
    if let Some((prepared, job, _)) = service.remote_snapshot(&settings, &id, generation).await? {
        job.check()?;
        return Ok(prepared.files(offset, limit).await);
    }
    let (prepared, job) = service.snapshot(&settings, &id, generation).await?;
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
    #[cfg(feature = "benchmark")]
    let _span = crate::benchmark::Span::new("ipc.content", "other");
    let settings = saved(&app)?;
    if let Some(session) = service.remote_snapshot(&settings, &id, generation).await? {
        let content = service.remote_content(session, (&file_id, &side)).await?;
        service.remote_snapshot(&saved(&app)?, &id, generation).await?;
        return Ok(content);
    }
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
    Ok(Content { bytes })
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
    if let Some((prepared, job, _)) = service.remote_snapshot(&settings, &id, generation).await? {
        job.check()?;
        return Ok(prepared.commits(offset, limit));
    }
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
        .captured_output(
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
    let commits = unique_commits(&output)?;
    job.check()?;
    service.snapshot(&settings, &id, generation).await?;
    Ok(commits)
}

fn unique_commits(output: &git::Captured) -> Result<Vec<UniqueCommit>, Problem> {
    let fields: Vec<_> = output.stdout.split(|byte| *byte == 0).collect();
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
            subject: output.safe(&String::from_utf8_lossy(fields[2])),
            author: output.safe(&String::from_utf8_lossy(fields[3])),
            date: decode(fields[4])?,
        });
    }
    Ok(commits)
}

#[cfg(test)]
mod tests;

#[cfg(all(test, target_os = "linux"))]
pub(crate) struct NativeDiffTest {
    pub storage: Arc<crate::linux_diff::Storage>,
    pub roots: Vec<crate::linux_guard::root::RootValue>,
    pub cancel: Arc<AtomicBool>,
}
#[cfg(all(test, target_os = "linux"))]
pub(crate) async fn native_diff_counts(input: NativeDiffTest) -> Result<serde_json::Value, String> {
    let job=Job{
        rust_counts: None,
        count_root: None,
        readers: Arc::default(),
        context:"native-diff-control".into(),cancel:input.cancel,diff:Some(input.storage),roots:input.roots,temporary_root:None,inventory_started:None};
    let lines=line_counts(b"left\nsame\n",b"same\nright\nextra\n",&job).await.map_err(|problem|problem.message)?;
    let activity=serde_json::to_value(git::activity_snapshot()).map_err(|_|"Native diff activity unavailable")?;
    let commands=activity.as_array().ok_or("Native diff activity invalid")?.iter().filter(|entry|entry["context"]=="native-diff-control").collect::<Vec<_>>();
    if commands.len()!=1 || commands[0]["state"]!="completed" || commands[0]["argv"].as_array().is_none_or(|args|!args.iter().any(|arg|arg=="--no-index")) {return Err("Native diff Git counter mismatch".into());}
    #[cfg(feature="benchmark")]
    let counters=crate::benchmark::benchmark_snapshot().map_err(|_|"Native diff counters unavailable")?["commands"].clone();
    #[cfg(not(feature="benchmark"))]
    let counters=serde_json::json!({"diff":commands.len()});
    Ok(serde_json::json!({"lines":lines,"commands":counters,"gitOperations":commands.len()}))
}
