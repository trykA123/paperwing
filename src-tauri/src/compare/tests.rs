#[tokio::test]
async fn write_authority_rejects_history_stale_sessions_and_root_changes() {
    let _guard = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    fixture.write("file.txt", b"before"); fixture.commit("base").await;
    fixture.write("file.txt", b"after");
    let service = fixture.service(); let settings = fixture.settings();
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

mod review_fixes;
mod review_round2;
mod review_round3;
mod count_config;
mod cold_lifecycle;
#[cfg(feature = "benchmark")]
mod cold_measure;
mod cold_path;
mod equivalence;
mod legacy;
#[cfg(target_os = "linux")]
mod metadata_limits;

struct Fixture(PathBuf);
impl Fixture {
    async fn new() -> Self {
        Self::with_format(None).await
    }
    async fn with_format(format: Option<&str>) -> Self {
        let path = crate::test_support::tmp_root().join(format!(
            "skein-compare-{}-{}",
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
    fn service(&self) -> Service {
        let service=Service::default();
        #[cfg(target_os = "linux")]
        service.configure_diff(self.diff_data()).unwrap();
        service
    }
    #[cfg(target_os = "linux")]
    fn diff_data(&self) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
            .join(".skillify/evidence/paperwing/14/quota-repair/compare-native")
            .join(crate::test_support::tmp_root().file_name().unwrap()).join(self.0.file_name().unwrap())
    }
    fn job(&self) -> Job {
        Job {
            rust_counts: None,
            #[cfg(target_os = "linux")] count_root: Some(self.diff_data()),
            #[cfg(not(target_os = "linux"))] count_root: Some(std::env::temp_dir()),
            readers: Arc::default(),
            context: "compare-fixture".into(),
            cancel: Arc::new(AtomicBool::new(false)),
            #[cfg(target_os = "linux")] diff: Some(Arc::new(crate::linux_diff::Storage::new(self.diff_data()).unwrap())),
            #[cfg(target_os = "linux")] roots: vec![crate::linux_guard::Root::open(&self.0.join("repo"), &[]).unwrap().value().unwrap()],
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
        self.commit_staged(name).await
    }
    async fn stage_paths(&self, paths: Vec<String>) {
        let repo = self.0.join("repo");
        tokio::task::spawn_blocking(move || {
            let run = |args: &[&str], input: String| {
                let mut child = std::process::Command::new("git")
                    .current_dir(&repo)
                    .args(args)
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .spawn()
                    .unwrap();
                let mut stdin = child.stdin.take().unwrap();
                let writer = std::thread::spawn(move || {
                    std::io::Write::write_all(&mut stdin, input.as_bytes()).unwrap()
                });
                let output = child.wait_with_output().unwrap();
                writer.join().unwrap();
                assert!(output.status.success(), "git {args:?} failed");
                String::from_utf8(output.stdout).unwrap()
            };
            let hashes = run(
                &["-c", "core.autocrlf=false", "hash-object", "-w", "--stdin-paths"],
                paths.join("\n") + "\n",
            );
            let info: String = hashes
                .lines()
                .zip(&paths)
                .map(|(hash, path)| format!("100644 {hash}\t{path}\n"))
                .collect();
            assert_eq!(info.lines().count(), paths.len());
            run(&["update-index", "--add", "--index-info"], info);
            run(&["update-index", "--refresh"], String::new());
        })
        .await
        .unwrap();
    }
    async fn commit_staged(&self, name: &str) -> String {
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
    fixture.service()
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
    let json = include_str!("../../../src/lib/test-support/layout-vectors.json");
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
        Fixture(crate::test_support::tmp_root().join(format!("skein-layout-{}", std::process::id())));
    std::fs::create_dir_all(&fixture.0).unwrap();
    let mut settings = fixture.settings();
    settings.workspace["layout"] = "custom".into();
    settings.workspace["sets"][0]["items"][0] = serde_json::json!({"id":"item", "repoId":"source:repo", "name":"repo", "folder":"repo-folder", "org":"org", "ref":{"type":"branch", "name":"feature/x"}});
    for (template, expected) in vectors {
        assert_eq!(
            registration::template_segments_with_policy(&template, &values, true),
            expected,
            "{template:?}"
        );
        if cfg!(windows) && !template.contains(['\u{007f}', '\u{0085}'])
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

#[test]
fn tagged_layout_vectors_preserve_windows_policy_and_native_linux_tokens() {
    let vectors: Vec<serde_json::Value> = serde_json::from_str(include_str!("../../../src/lib/test-support/platform-layout-vectors.json")).unwrap();
    for vector in vectors {
        let text = |key| vector[key].as_str().unwrap();
        let windows = text("platform") == "windows";
        let values = BTreeMap::from([
            ("folder", text("folder").into()), ("repo", text("repo").into()), ("org", text("org").into()),
            ("set", registration::flat_value(text("set"), windows)),
            ("source", registration::flat_value(text("source"), windows)),
            ("ref", registration::flat_value(text("ref"), windows)),
        ]);
        let segments = if text("layout") == "flat" { vec![text("folder").to_string()] } else {
            registration::template_segments_with_policy(text("pathTemplate"), &values, windows)
        };
        assert_eq!(serde_json::to_value(&segments).unwrap(), vector["segments"], "{vector}");
        assert_eq!(registration::layout_destination(text("root"), &segments, windows), text("destination"), "{vector}");
        if windows != cfg!(windows) { continue; }
        let fixture = crate::platform::Fixture::new("tagged-layout");
        let settings = crate::settings::Settings {
            sources: vec![crate::settings::Source { id: "source".into(), name: text("source").into(), kind: "manual".into(), host: String::new(), orgs: vec![], urls: vec![], enabled: true, credential_managed: false }],
            workspace: serde_json::json!({
                "root": fixture.0, "layout": text("layout"), "pathTemplate": text("pathTemplate"),
                "sets": [{"id": "set", "name": text("set"), "items": [{"id":"item", "repoId":"source:repo", "name":text("repo"), "folder":text("folder"), "org":text("org"), "ref":{"type":"branch", "name":text("ref")}}]}]
            }),
        };
        let context = bind(&settings, Endpoint { set_id: "set".into(), item_id: "item".into(), reference: CompareRef::WorkingTree }).unwrap();
        assert_eq!(context.root, fixture.0.join(segments.iter().collect::<PathBuf>()));
    }
}

#[cfg(target_os = "linux")]
#[test]
fn registered_destinations_require_saved_owner_native_root_and_physical_membership() {
    let fixture = crate::platform::Fixture::new("registration");
    let workspace = fixture.0.join("workspace");
    std::fs::create_dir_all(workspace.join("Folder/.git")).unwrap();
    std::fs::create_dir_all(workspace.join("folder/.git")).unwrap();
    let mut settings = crate::settings::Settings { sources: vec![], workspace: serde_json::json!({
        "root": workspace, "layout": "flat", "sets": [{"id":"set", "name":"Set", "items":[
            {"id":"owner", "name":"Folder", "url":"https://example.test/repo", "repoId":"source:repo", "org":"o", "ref":{"type":"branch", "name":"main"}}
        ]}]
    }) };
    let url = "https://example.test/repo";
    assert!(registered_clone_destination(&settings, "owner", &workspace.join("Folder/"), url).is_ok());
    assert!(registered_clone_destination(&settings, "foreign", &workspace.join("Folder"), url).is_err());
    assert!(registered_clone_destination(&settings, "owner", &workspace.join("Folder"), "foreign-url").is_err());
    assert!(registered_clone_destination(&settings, "owner", &workspace.join("folder"), url).is_err());
    assert!(registered_clone_destination(&settings, "owner", &fixture.0.join("outside"), url).is_err());
    std::os::unix::fs::symlink(workspace.join("Folder"), workspace.join("linked")).unwrap();
    assert!(registered_clone_destination(&settings, "owner", &workspace.join("linked"), url).is_err());
    settings.workspace["sets"][0]["items"][0]["name"] = "Missing".into();
    assert!(registered_clone_destination(&settings, "owner", &workspace.join("Missing"), url).is_ok());
    assert!(registered_clone_destination(&settings, "owner", &workspace.join("missing"), url).is_err());
    assert!(registered_clone_destination(&settings, "owner", &workspace.join("../outside"), url).is_err());
    settings.workspace["root"] = fixture.0.join("absent").to_str().unwrap().into();
    assert!(registered_clone_destination(&settings, "owner", &fixture.0.join("absent/Missing"), url).is_err());
    settings.workspace["root"] = "C:\\Dev\\repos".into();
    let before = serde_json::to_vec(&settings).unwrap();
    assert!(registered_clone_destination(&settings, "owner", Path::new("C:\\Dev\\repos\\Missing"), url).is_err());
    assert_eq!(serde_json::to_vec(&settings).unwrap(), before);
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn native_repository_trailing_space_and_backslash_paths_compare_without_write_tickets() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = crate::platform::Fixture::new("native-compare");
    let repo = fixture.0.join("repo\\native:CON. ");
    std::fs::create_dir(&repo).unwrap();
    assert!(std::process::Command::new("git").arg("-C").arg(&repo).args(["init", "-q"]).status().unwrap().success());
    std::fs::write(repo.join("literal\\filename: "), b"native content\n").unwrap();
    let context = Context {
        endpoint: Endpoint { set_id: "set".into(), item_id: "item".into(), reference: CompareRef::WorkingTree },
        root: repo.clone(), workspace_root: fixture.0.clone(),
    };
    let job = Job {
        rust_counts: None,
        count_root: None,
        readers: Arc::default(),
        context: "native-read".into(), cancel: Arc::new(AtomicBool::new(false)), diff: Some(Arc::new(crate::linux_diff::Storage::new(fixture.0.join("diff-data")).unwrap())), roots: vec![crate::linux_guard::Root::open(&repo, &[]).unwrap().value().unwrap()], temporary_root: None, inventory_started: None };
    let safe = read_root(&context, &job).await.unwrap();
    assert_eq!(safe.read("literal\\filename: ").unwrap().unwrap().bytes, b"native content\n");
    assert!(safe.read(".git/config").is_err());
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
    fixture.git(&["config", "core.fsync", "none"]).await;
    let mut bytes = vec![b'x'; 8400];
    let mut paths = Vec::new();
    for index in 0u32..5000 {
        bytes[..4].copy_from_slice(&index.to_le_bytes());
        let path = format!("file-{index:04}.dat");
        fixture.write(&path, &bytes);
        paths.push(path);
    }
    fixture.stage_paths(paths).await;
    fixture.commit_staged("40 MiB scale").await;
    let index = std::fs::read(fixture.0.join("repo/.git/index")).unwrap();
    let started = std::time::Instant::now();
    let job = Job {
        rust_counts: None,
        context: "scale-ready".into(),
        ..fixture.job()
    };
    let service = fixture.service();
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
        rust_counts: None,
        inventory_started: Some(inventory_started.clone()),
        ..fixture.job()
    };
    let running_job = cancel_job.clone();
    let resolved = comparison.right.clone();
    let task = tokio::spawn(async move { inventory(&resolved, &running_job).await });
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
    let comparison = fixture.service()
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
    let service = fixture.service();
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
            .fetch_state(&crate::platform::canonical_path(&clone).unwrap())
            .await
            .unwrap()
            .lock()
            .await
            .epoch,
        1
    );
    assert_eq!(std::fs::read(clone.join(".git/HEAD")).unwrap(), head);
    assert_eq!(std::fs::read(clone.join(".git/index")).unwrap(), index);
    assert_eq!(
        std::fs::read_to_string(clone.join("file.txt"))
            .unwrap()
            .replace("\r\n", "\n"),
        "initial\n"
    );
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
    let service = fixture.service();
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
    let service = fixture.service();
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
        .sessions()
        .values()
        .map(|session| session.cancel.clone())
        .collect();
    let saved_workspace = settings.workspace.clone();
    service.release_sessions().await;
    assert!(flags.iter().all(|flag| flag.load(Ordering::Relaxed)));
    assert!(service.sessions().is_empty());
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
    let service = fixture.service();
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
    let service = fixture.service();
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
    let root = crate::platform::canonical_path(&fixture.0.join("repo")).unwrap();
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
    let service = fixture.service();
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
        rust_counts: None,
        count_root: Some(unsuitable.clone()),
        #[cfg(target_os = "linux")] diff: Some(Arc::new(crate::linux_diff::Storage::new(unsuitable.clone()).unwrap())),
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
    let resolved = Resolved {
        object_format: ObjectFormat::read(&context.root, &job).await.unwrap(),
        reader: git::BatchReader::new(context.root.clone(), job.cancel.clone()),
        diff_config: Vec::new(), context, safe: root, commit: sha, files: BTreeMap::new(),
    };
    let files = inventory(&resolved, &job).await.unwrap();
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

#[tokio::test]
async fn history_display_uses_operation_tokens_after_replacement() {
    let _serial = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    let _credentials = crate::git::CredentialFixture::new(std::collections::BTreeMap::from([
        ("managed-manual".into(), Ok(Some("synthetic-old-token".into()))),
    ]));
    fixture.git(&["config", "user.name", "synthetic-old-token author"]).await;
    fixture.write("file.txt", b"synthetic-old-token\0exact blob");
    fixture.git(&["config", "user.email", "fixture@example.test"]).await;
    fixture.git(&["add", "file.txt"]).await;
    fixture.git(&["-c", "core.hooksPath=", "commit", "-m", "synthetic-old-token subject"]).await;
    let output = fixture.job().captured_output(&fixture.0.join("repo"), &["log", "-1",
        "--format=>%x00%H%x00%s%x00%an%x00%aI", "-z"]).await.unwrap();
    crate::git::CredentialFixture::replace("managed-manual", "synthetic-new-token");
    let commits = unique_commits(&output).unwrap();
    let displayed = serde_json::to_string(&commits).unwrap();
    assert!(!displayed.contains("synthetic-old-token"));
    assert!(displayed.contains("[redacted] subject"));
    assert!(displayed.contains("[redacted] author"));
    let bytes = fixture.job().output(&fixture.0.join("repo"), &["cat-file", "blob", "HEAD:file.txt"]).await.unwrap();
    assert_eq!(bytes, b"synthetic-old-token\0exact blob");
}

#[cfg(target_os="linux")]
#[tokio::test]
async fn line_counts_refuse_missing_storage_configuration_and_source_authority() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture=Fixture::new().await;let mut job=fixture.job();job.diff=None;
    assert!(line_counts(b"l\n",b"r\n",&job).await.unwrap_err().message.contains("not configured"));
    job=fixture.job();job.roots.clear();assert!(line_counts(b"l\n",b"r\n",&job).await.is_err());assert!(!fixture.0.join("diff-data").exists());
}

#[cfg(target_os = "linux")]
#[path = "tests/linux_files.rs"]
mod linux_files;
