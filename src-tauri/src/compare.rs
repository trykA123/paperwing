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

mod classification;
mod listing;
mod ordered_commit;
mod publication;
mod resolving;
mod access;
mod progressive;
mod producer;
pub use progressive::{Hint, PendingRow, Progress, RowUpdate, State, Totals};
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
        let mut argv = vec!["--no-pager"];
        argv.extend(git::repo_command::RepoGit::at(root).no_optional_locks()
            .config("color.ui=false").config("core.untrackedCache=false")
            .config("diff.external=").config("core.hooksPath=").argv(args));
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
    inner: Arc<ServiceState>,
    owner: bool,
}

impl std::ops::Deref for Service {
    type Target = ServiceState;
    fn deref(&self) -> &ServiceState { &self.inner }
}

pub struct ServiceState {
    sessions: std::sync::Mutex<HashMap<String, Session>>,
    fetches: Mutex<HashMap<PathBuf, Arc<Mutex<Fetch>>>>,
    diff_configs: std::sync::Mutex<DiffConfigurations>,
    slots: Semaphore,
    interactive_slots: Semaphore,
    #[cfg(target_os = "linux")]
    diff: std::sync::OnceLock<Arc<crate::linux_diff::Storage>>,
    counts: std::sync::OnceLock<Arc<count_eligibility::Eligibility>>,
    remote_cache: std::sync::OnceLock<Arc<crate::github::blob::Cache>>,
    remote_store: std::sync::OnceLock<crate::store::Store>,
    #[cfg(test)]
    enrichment_control: std::sync::Mutex<Option<Arc<publication::EnrichmentControl>>>,
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
    progress: Option<progressive::Retained>,
    producer: Option<tokio::task::JoinHandle<()>>,
}

impl Default for Service {
    fn default() -> Self {
        Self { inner: Arc::new(ServiceState::default()), owner: true }
    }
}

impl Default for ServiceState {
    fn default() -> Self {
        Self {
            sessions: std::sync::Mutex::new(HashMap::new()),
            fetches: Mutex::new(HashMap::new()),
            diff_configs: std::sync::Mutex::new(HashMap::new()),
            slots: Semaphore::new(4),
            interactive_slots: Semaphore::new(4),
            #[cfg(target_os = "linux")]
            diff: std::sync::OnceLock::new(),
            counts: std::sync::OnceLock::new(),
            remote_cache: std::sync::OnceLock::new(),
            remote_store: std::sync::OnceLock::new(),
            #[cfg(test)]
            enrichment_control: std::sync::Mutex::new(None),
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
        let (_, job) = self.available(settings, id, generation).map_err(|problem| problem.message)?;
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
                progress: None,
                producer: None,
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
            if let Some(producer) = session.producer { let _ = producer.await; }
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
            for session in sessions {
                close_readers(&session.readers).await;
                if let Some(producer) = session.producer { let _ = producer.await; }
            }
        }
    }

    async fn cancel(&self, id: &str) -> bool {
        let (readers, producer) = {
            let mut sessions = self.sessions();
            let Some(session) = sessions.get_mut(id) else {
                return false;
            };
            session.cancel.store(true, Ordering::Relaxed);
            session.notify.notify_waiters();
            session.generation += 1;
            session.prepared = None;
            session.remote = None;
            session.progress = None;
            (session.readers.clone(), session.producer.take())
        };
        close_readers(&readers).await;
        if let Some(producer) = producer { let _ = producer.await; }
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
        let (mut left, mut right, job) = self.resolve_comparison(contexts, job).await?;
        self.transition(id, generation, State::Listing);
        left.files = inventory(&left, &job).await.map_err(|problem| problem.side("left"))?;
        right.files = inventory(&right, &job).await.map_err(|problem| problem.side("right"))?;
        let paths: BTreeSet<_> = left.files.keys().chain(right.files.keys()).cloned().collect();
        let over_limit = paths.len() > FILE_LIMIT;
        if over_limit {
            git::enrichment(diff_metadata(&left, &right, &job)).await?;
            return Err(Problem::new("limitExceeded", "Comparison exceeds inventory limit"));
        }
        let listed = Arc::new(listing::Listed::new(left, right, paths));
        self.publish_inventory((id, generation), listed.clone())?;
        #[cfg(test)]
        self.defer_enrichment(&job).await?;
        let metadata = git::enrichment(diff_metadata(&listed.left, &listed.right, &job)).await?;
        let classifier = classification::Classifier { left: &listed.left, right: &listed.right,
            metadata: &metadata, options: &options, job: &job };
        let rows = git::enrichment(ordered_commit::commit(self, (id, generation), &listed, classifier)).await?;
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
        let (history, history_source) = git::enrichment(history(&listed.left, &listed.right, &job)).await?;
        let endpoint = |side: &Resolved, basis: String| ResolvedEndpoint {
            endpoint: side.context.endpoint.clone(),
            commit: side.commit.clone(),
            history_basis: basis,
        };
        let view = Snapshot {
            id: id.into(),
            generation,
            left: endpoint(&listed.left, history.left_basis.clone()),
            right: endpoint(&listed.right, history.right_basis.clone()),
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
            left: listed.left.clone(),
            right: listed.right.clone(),
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
        let opened = self.start(settings, id, options, source, None).await?;
        self.wait(id, opened.generation).await
    }

    pub async fn copy_source_context(&self, settings: &crate::settings::Settings, id: &str, generation: u64, file_id: &str, side: &str) -> Result<Option<WriteContext>, String> {
        let (available, job) = self.available(settings, id, generation).map_err(|problem| problem.message)?;
        job.check().map_err(|problem| problem.message)?;
        if !available.selection(file_id).map_err(|problem| problem.message)?.1 {
            return Err("Wait until this file finishes checking".into());
        }
        let resolved = available.side(side).map_err(|problem| problem.message)?;
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
        let (available, job) = self.available(settings, id, generation).map_err(|problem| problem.message)?;
        job.check().map_err(|problem| problem.message)?;
        let (path, final_row) = available.selection(file_id).map_err(|problem| problem.message)?;
        if !final_row { return Err("Wait until this file finishes checking".into()); }
        let resolved = available.side(side).map_err(|problem| problem.message)?;
        if resolved.context.endpoint.reference != CompareRef::WorkingTree { return Err("Historical references are read-only".into()); }
        if resolved.files.get(path).is_some_and(|entry| entry.kind != Kind::File || entry.reason.is_some()) {
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
        safe.resolve_cached(path, false, &mut paths::ReadCache::default())?;
        job.check().map_err(|problem| problem.message)?;
        self.available(settings, id, generation).map_err(|problem| problem.message)?;
        Ok(WriteContext { root: resolved.context.root.clone(), path: path.to_string(), safe })
    }

    pub async fn copy_ids(&self, settings: &crate::settings::Settings, id: &str, generation: u64, file_id: &str, source: &str) -> Result<(Vec<String>, usize), String> {
        let (available, job) = self.available(settings, id, generation).map_err(|problem| problem.message)?;
        job.check().map_err(|problem| problem.message)?;
        let (path, _) = available.selection(file_id).map_err(|problem| problem.message)?;
        let resolved = available.side(source).map_err(|problem| problem.message)?;
        let entry = resolved.files.get(path).ok_or("Source is absent; copy never deletes destination files")?;
        let prefix = format!("{path}/");
        let rows: Vec<_> = available.rows().into_iter().filter(|(_, candidate, _)| *candidate == path
            || (entry.kind == Kind::Directory && candidate.starts_with(&prefix))).collect();
        if rows.iter().any(|(_, _, final_row)| !final_row) {
            return Err(if entry.kind == Kind::Directory { "Wait until this folder finishes checking" }
                else { "Wait until this file finishes checking" }.into());
        }
        let mut ids = Vec::new(); let mut retained = 0;
        for (file_id, path, _) in rows {
            let Some(source) = resolved.files.get(path) else { retained += 1; continue; };
            if source.kind == Kind::Directory { continue; }
            if source.kind != Kind::File || source.reason.is_some() { return Err(format!("Copy refuses linked, unavailable or submodule content: {path}")); }
            if ids.len() >= 128 { return Err("Copy scope exceeds 128 files; select a smaller folder".into()); }
            ids.push(file_id.to_string());
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
    service.selected_content(&settings, &id, generation, &file_id, &side).await
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

#[tauri::command]
pub async fn comparison_start(
    app: tauri::AppHandle,
    service: tauri::State<'_, Service>,
    id: String,
    options: Options,
) -> Result<Opened, Problem> {
    service
        .start(&saved(&app)?, &id, options, None, Some(app))
        .await
}

#[tauri::command]
pub async fn comparison_progress(
    service: tauri::State<'_, Service>,
    id: String,
    generation: u64,
    after: u64,
    limit: usize,
) -> Result<Progress, Problem> {
    service.progress(&id, generation, after, limit)
}
