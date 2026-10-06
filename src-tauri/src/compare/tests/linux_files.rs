use super::super::*;
use crate::linux_files::{
    AsyncContent, Context as FileContext, Environment, Request, Service as Files,
};
use std::os::unix::fs::PermissionsExt;

struct Native {
    _serial: crate::test_support::Serial,
    path: PathBuf,
    settings: Arc<std::sync::Mutex<serde_json::Value>>,
    comparisons: Service,
    files: Arc<Files>,
    environment: Environment,
    id: String,
    generation: u64,
    rows: Vec<FileRow>,
}
impl Native {
    async fn new() -> Self {
        let serial = crate::test_support::serial().await;
        let path = crate::test_support::tmp_root().join(format!(
            "paperwing-write-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        for name in ["left", "right", "data"] {
            std::fs::create_dir(path.join(name)).unwrap();
        }
        for side in ["left", "right"] {
            let output = std::process::Command::new("git")
                .args(["init", "--initial-branch=main"])
                .arg(path.join(side))
                .output()
                .unwrap();
            assert!(output.status.success());
            let commit = std::process::Command::new("git")
                .arg("-C")
                .arg(path.join(side))
                .args([
                    "-c",
                    "user.name=admin",
                    "-c",
                    "user.email=admin@example.test",
                    "-c",
                    "core.hooksPath=",
                    "-c",
                    "commit.gpgSign=false",
                    "commit",
                    "--allow-empty",
                    "-m",
                    "fixture",
                ])
                .output()
                .unwrap();
            assert!(
                commit.status.success(),
                "{}",
                String::from_utf8_lossy(&commit.stderr)
            );
            std::fs::write(
                path.join(side).join("file.txt"),
                if side == "left" {
                    b"\xef\xbb\xbfleft\r\n".as_slice()
                } else {
                    b"\xef\xbb\xbfright\r\n"
                },
            )
            .unwrap();
            std::fs::create_dir_all(path.join(side).join("folder/nested")).unwrap();
            std::fs::write(path.join(side).join("folder/nested/a.txt"), side).unwrap();
            std::fs::write(path.join(side).join("folder/nested/b.txt"), side).unwrap();
            std::fs::write(path.join(side).join("folder/nested/c.txt"), side).unwrap();
        }
        std::fs::write(path.join("left/new.txt"), b"new-left").unwrap();
        std::fs::write(path.join("right/other.txt"), b"new-right").unwrap();
        std::fs::write(path.join("right/folder/retained.txt"), b"retained").unwrap();
        std::fs::create_dir(path.join("left/missing")).unwrap();
        std::fs::write(path.join("left/missing/a.txt"), b"a").unwrap();
        std::fs::write(path.join("left/missing/b.txt"), b"b").unwrap();
        let value = serde_json::json!({"sources":[], "workspace":{"root":path,"layout":"flat","sets":[{"id":"set","name":"Fixture","items":[{"id":"left","name":"left"},{"id":"right","name":"right"}]}]}});
        let settings = Arc::new(std::sync::Mutex::new(value));
        let saved = settings.clone();
        let environment = Environment::fixture(path.join("data"), move || {
            serde_json::from_value(saved.lock().unwrap().clone()).map_err(|error| error.to_string())
        });
        let comparisons = Service::default();
        comparisons.configure_diff(path.join("data")).unwrap();
        let endpoint = |side: &str| Endpoint {
            set_id: "set".into(),
            item_id: side.into(),
            reference: CompareRef::WorkingTree,
        };
        let opened = comparisons
            .open(
                &environment.load().unwrap(),
                endpoint("left"),
                endpoint("right"),
            )
            .await
            .unwrap();
        let RefreshResult::Ready { snapshot } = comparisons
            .refresh(&environment.load().unwrap(), &opened.id, Options::default())
            .await
            .unwrap()
        else {
            panic!("Fixture comparison failed");
        };
        let (prepared, _) = comparisons
            .snapshot(
                &environment.load().unwrap(),
                &opened.id,
                snapshot.generation,
            )
            .await
            .unwrap();
        Self {
            _serial: serial,
            path,
            settings,
            comparisons,
            files: Arc::new(Files::default()),
            environment,
            id: opened.id,
            generation: snapshot.generation,
            rows: prepared.rows.clone(),
        }
    }
    async fn refresh(&mut self) {
        let RefreshResult::Ready { snapshot } = self
            .comparisons
            .refresh(
                &self.environment.load().unwrap(),
                &self.id,
                Options::default(),
            )
            .await
            .unwrap()
        else {
            panic!("Fixture refresh failed");
        };
        self.generation = snapshot.generation;
        let (prepared, _) = self
            .comparisons
            .snapshot(&self.environment.load().unwrap(), &self.id, self.generation)
            .await
            .unwrap();
        self.rows = prepared.rows.clone();
    }
    fn context(&self) -> FileContext<'_> {
        FileContext {
            comparisons: &self.comparisons,
            environment: &self.environment,
        }
    }
    fn request(&self, path: &str, side: &str) -> Request {
        Request {
            session: self.id.clone(),
            generation: self.generation,
            file_id: self
                .rows
                .iter()
                .find(|row| row.path == path)
                .unwrap()
                .id
                .clone(),
            side: side.into(),
        }
    }
    async fn open(&self, path: &str, side: &str) -> serde_json::Value {
        serde_json::to_value(
            self.files
                .open(self.context(), self.request(path, side))
                .await
                .unwrap(),
        )
        .unwrap()
    }
    async fn preview(&self, path: &str, side: &str) -> serde_json::Value {
        let content = Content { fixture: self };
        serde_json::to_value(
            self.files
                .preview(self.context(), self.request(path, side), content)
                .await
                .unwrap(),
        )
        .unwrap()
    }
    async fn apply(&self, preview: &serde_json::Value) -> serde_json::Value {
        serde_json::to_value(
            self.files
                .apply(self.context(), preview["id"].as_str().unwrap(), true)
                .await
                .unwrap(),
        )
        .unwrap()
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.path).unwrap();
    }
}
struct Content<'a> {
    fixture: &'a Native,
}
impl AsyncContent for Content<'_> {
    async fn read(&mut self, file_id: &str, side: &str) -> Result<Vec<u8>, String> {
        let (prepared, job) = self
            .fixture
            .comparisons
            .snapshot(
                &self.fixture.environment.load()?,
                &self.fixture.id,
                self.fixture.generation,
            )
            .await
            .map_err(|error| error.message)?;
        let row = prepared.rows.iter().find(|row| row.id == file_id).unwrap();
        let resolved = if side == "left" {
            &prepared.left
        } else {
            &prepared.right
        };
        content(
            resolved,
            &row.path,
            resolved.files.get(&row.path).unwrap(),
            &job,
        )
        .await
        .map_err(|error| error.message)
    }
}
#[tokio::test]
async fn linux_open_save_preserves_bom_crlf_and_repeated_save_bytes() {
    let fixture = Native::new().await;
    let opened = fixture.open("file.txt", "right").await;
    assert_eq!(
        opened["bytes"],
        serde_json::json!(b"\xef\xbb\xbfright\r\n".to_vec())
    );
    let ticket = opened["ticket"].as_str().unwrap();
    for bytes in [
        b"\xef\xbb\xbfedited\r\n".to_vec(),
        b"\xef\xbb\xbfagain\r\n".to_vec(),
    ] {
        let record = fixture
            .files
            .save(fixture.context(), ticket, bytes.clone())
            .await
            .unwrap();
        assert_eq!(
            std::fs::read(fixture.path.join("right/file.txt")).unwrap(),
            bytes
        );
        assert_eq!(serde_json::to_value(record).unwrap()["stage"], "applied");
    }
}
#[tokio::test]
async fn linux_stale_generation_refuses_save() {
    let fixture = Native::new().await;
    let opened = fixture.open("file.txt", "right").await;
    fixture.comparisons.cancel(&fixture.id).await;
    assert!(fixture
        .files
        .save(
            fixture.context(),
            opened["ticket"].as_str().unwrap(),
            b"bad".to_vec()
        )
        .await
        .is_err());
    assert_eq!(
        std::fs::read(fixture.path.join("right/file.txt")).unwrap(),
        b"\xef\xbb\xbfright\r\n"
    );
}
#[tokio::test]
async fn linux_external_edit_between_open_and_save_is_retained() {
    let fixture = Native::new().await;
    let opened = fixture.open("file.txt", "right").await;
    std::fs::write(fixture.path.join("right/file.txt"), b"external").unwrap();
    assert!(fixture
        .files
        .save(
            fixture.context(),
            opened["ticket"].as_str().unwrap(),
            b"bad".to_vec()
        )
        .await
        .is_err());
    assert_eq!(
        std::fs::read(fixture.path.join("right/file.txt")).unwrap(),
        b"external"
    );
}
#[tokio::test]
async fn linux_replaced_root_refuses_save_and_new_tickets() {
    let fixture = Native::new().await;
    let opened = fixture.open("file.txt", "right").await;
    std::fs::rename(fixture.path.join("right"), fixture.path.join("old-right")).unwrap();
    std::fs::create_dir(fixture.path.join("right")).unwrap();
    std::fs::write(fixture.path.join("right/file.txt"), b"replacement").unwrap();
    assert!(fixture
        .files
        .save(
            fixture.context(),
            opened["ticket"].as_str().unwrap(),
            b"bad".to_vec()
        )
        .await
        .is_err());
    assert!(fixture
        .files
        .open(fixture.context(), fixture.request("file.txt", "right"))
        .await
        .is_err());
    assert_eq!(
        std::fs::read(fixture.path.join("right/file.txt")).unwrap(),
        b"replacement"
    );
}
#[tokio::test]
async fn linux_copy_creates_overwrites_both_directions_and_retains_destination_only_files() {
    let mut fixture = Native::new().await;
    for (path, side, expected) in [
        ("file.txt", "right", b"\xef\xbb\xbfleft\r\n".as_slice()),
        ("file.txt", "left", b"\xef\xbb\xbfleft\r\n".as_slice()),
        ("new.txt", "right", b"new-left".as_slice()),
        ("other.txt", "left", b"new-right".as_slice()),
    ] {
        let preview = fixture.preview(path, side).await;
        assert_eq!(fixture.apply(&preview).await[0]["state"], "applied");
        assert_eq!(
            std::fs::read(fixture.path.join(side).join(path)).unwrap(),
            expected
        );
        fixture.refresh().await;
    }
    let preview = fixture.preview("folder", "right").await;
    assert_eq!(preview["retained"], 1);
    assert!(fixture
        .apply(&preview)
        .await
        .as_array()
        .unwrap()
        .iter()
        .all(|outcome| outcome["state"] == "applied"));
    assert_eq!(
        std::fs::read(fixture.path.join("right/folder/retained.txt")).unwrap(),
        b"retained"
    );
}
#[tokio::test]
async fn linux_folder_copy_returns_partial_outcomes_after_external_edit() {
    let fixture = Native::new().await;
    let preview = fixture.preview("folder", "right").await;
    std::fs::write(fixture.path.join("right/folder/nested/b.txt"), b"external").unwrap();
    let outcomes = fixture.apply(&preview).await;
    assert_eq!(outcomes[0]["state"], "applied");
    assert_eq!(outcomes[1]["state"], "failed");
    assert_eq!(outcomes[2]["state"], "notAttempted");
    assert_eq!(
        std::fs::read(fixture.path.join("right/folder/nested/b.txt")).unwrap(),
        b"external"
    );
    assert_eq!(
        std::fs::read(fixture.path.join("right/folder/nested/c.txt")).unwrap(),
        b"right"
    );
    assert_eq!(fixture.files.copy_count(), 0);
}
#[tokio::test]
async fn linux_folder_copy_creates_shared_missing_parents_with_recovery_records() {
    let fixture = Native::new().await;
    let preview = fixture.preview("missing", "right").await;
    assert!(fixture
        .apply(&preview)
        .await
        .as_array()
        .unwrap()
        .iter()
        .all(|outcome| outcome["state"] == "applied"));
    assert_eq!(
        std::fs::read(fixture.path.join("right/missing/a.txt")).unwrap(),
        b"a"
    );
    assert_eq!(
        std::fs::read(fixture.path.join("right/missing/b.txt")).unwrap(),
        b"b"
    );
    let rows = serde_json::to_value(fixture.files.list(&fixture.environment).unwrap()).unwrap();
    assert!(rows
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["stage"] == "parentLinked"));
}
#[tokio::test]
async fn linux_undo_restores_exact_bytes_and_refuses_later_external_edits() {
    let fixture = Native::new().await;
    let opened = fixture.open("file.txt", "right").await;
    let ticket = opened["ticket"].as_str().unwrap();
    let record = serde_json::to_value(
        fixture
            .files
            .save(fixture.context(), ticket, b"saved".to_vec())
            .await
            .unwrap(),
    )
    .unwrap();
    let id = record["id"].as_str().unwrap();
    assert_eq!(
        serde_json::to_value(fixture.files.undo(&fixture.environment, id).await.unwrap()).unwrap()
            ["stage"],
        "undone"
    );
    assert_eq!(
        std::fs::read(fixture.path.join("right/file.txt")).unwrap(),
        b"\xef\xbb\xbfright\r\n"
    );
    fixture.files.close(ticket).unwrap();
    let opened = fixture.open("file.txt", "right").await;
    let record = serde_json::to_value(
        fixture
            .files
            .save(
                fixture.context(),
                opened["ticket"].as_str().unwrap(),
                b"saved-again".to_vec(),
            )
            .await
            .unwrap(),
    )
    .unwrap();
    std::fs::write(fixture.path.join("right/file.txt"), b"external").unwrap();
    assert!(fixture
        .files
        .undo(&fixture.environment, record["id"].as_str().unwrap())
        .await
        .is_err());
    assert_eq!(
        std::fs::read(fixture.path.join("right/file.txt")).unwrap(),
        b"external"
    );
}
#[tokio::test]
async fn linux_tickets_and_copy_plans_release_on_close_cancel_and_page_load() {
    let fixture = Native::new().await;
    let first = fixture.open("file.txt", "right").await;
    let _second = fixture.open("file.txt", "left").await;
    assert_eq!(fixture.files.ticket_count(), 2);
    fixture
        .files
        .close(first["ticket"].as_str().unwrap())
        .unwrap();
    assert_eq!(fixture.files.ticket_count(), 1);
    let preview = fixture.preview("file.txt", "right").await;
    fixture
        .files
        .cancel(preview["id"].as_str().unwrap())
        .unwrap();
    assert_eq!(fixture.files.copy_count(), 0);
    let _preview = fixture.preview("file.txt", "right").await;
    fixture.comparisons.release_sessions().await;
    fixture.files.release_tickets().await;
    assert_eq!(fixture.files.ticket_count(), 0);
    assert_eq!(fixture.files.copy_count(), 0);
    assert!(fixture
        .files
        .save(
            fixture.context(),
            first["ticket"].as_str().unwrap(),
            b"bad".to_vec()
        )
        .await
        .is_err());
}
#[tokio::test]
async fn linux_settings_changes_refuse_save_and_stale_copy_sources_refuse_apply() {
    let fixture = Native::new().await;
    let preview = fixture.preview("file.txt", "right").await;
    std::fs::write(fixture.path.join("left/file.txt"), b"changed-source").unwrap();
    assert_eq!(fixture.apply(&preview).await[0]["state"], "failed");
    let opened = fixture.open("file.txt", "right").await;
    fixture.settings.lock().unwrap()["workspace"]["sets"] = serde_json::json!([]);
    assert!(fixture
        .files
        .save(
            fixture.context(),
            opened["ticket"].as_str().unwrap(),
            b"bad".to_vec()
        )
        .await
        .is_err());
}

#[tokio::test]
async fn linux_editor_ticket_limits_refuse_the_thirty_third_ticket_and_oversized_saves() {
    let fixture = Native::new().await;
    let mut opened = Vec::new();
    for _ in 0..32 {
        opened.push(fixture.open("file.txt", "right").await);
    }
    assert!(fixture
        .files
        .open(fixture.context(), fixture.request("file.txt", "right"))
        .await
        .unwrap_err()
        .contains("Too many"));
    assert!(fixture
        .files
        .save(
            fixture.context(),
            opened[0]["ticket"].as_str().unwrap(),
            vec![b'a'; 2 * 1024 * 1024 + 1]
        )
        .await
        .unwrap_err()
        .contains("limit"));
    for ticket in opened {
        fixture
            .files
            .close(ticket["ticket"].as_str().unwrap())
            .unwrap();
    }
    assert_eq!(fixture.files.ticket_count(), 0);
}

#[tokio::test]
async fn linux_hunk_save_and_created_file_undo_preserve_exact_destination_format() {
    let fixture = Native::new().await;
    let opened = fixture.open("new.txt", "right").await;
    assert_eq!(opened["exists"], false);
    let record = serde_json::to_value(
        fixture
            .files
            .save(
                fixture.context(),
                opened["ticket"].as_str().unwrap(),
                b"\xef\xbb\xbfnew hunk\r\n".to_vec(),
            )
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        std::fs::read(fixture.path.join("right/new.txt")).unwrap(),
        b"\xef\xbb\xbfnew hunk\r\n"
    );
    fixture
        .files
        .undo(&fixture.environment, record["id"].as_str().unwrap())
        .await
        .unwrap();
    assert!(!fixture.path.join("right/new.txt").exists());
}

#[tokio::test]
async fn linux_cleanup_preserves_open_editor_records_until_close() {
    let fixture = Native::new().await;
    let opened = fixture.open("file.txt", "right").await;
    let ticket = opened["ticket"].as_str().unwrap();
    let record = serde_json::to_value(
        fixture
            .files
            .save(fixture.context(), ticket, b"saved".to_vec())
            .await
            .unwrap(),
    )
    .unwrap();
    let id = record["id"].as_str().unwrap().to_string();
    fixture.files.undo(&fixture.environment, &id).await.unwrap();
    assert!(fixture
        .files
        .cleanup(&fixture.environment, std::slice::from_ref(&id), true)
        .unwrap_err()
        .contains("Close editors"));
    fixture.files.close(ticket).unwrap();
    assert_eq!(
        fixture
            .files
            .cleanup(&fixture.environment, &[id], true)
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn linux_close_during_parent_creation_revokes_file_publication_and_retains_parent_record() {
    let fixture = Native::new().await;
    let opened = fixture.open("missing/a.txt", "right").await;
    let ticket = opened["ticket"].as_str().unwrap().to_string();
    let files = fixture.files.clone();
    let closed = ticket.clone();
    crate::linux_guard::mutation::set_parent_hook(Some(Box::new(move |phase| {
        if phase == "parentSynced" {
            files.close(&closed).unwrap();
        }
        Ok(())
    })));
    let result = fixture
        .files
        .save(fixture.context(), &ticket, b"saved".to_vec())
        .await;
    crate::linux_guard::mutation::set_parent_hook(None);
    assert!(result.is_err());
    assert!(!fixture.path.join("right/missing/a.txt").exists());
    assert!(fixture.path.join("right/missing").is_dir());
    assert_eq!(fixture.files.ticket_count(), 0);
    assert!(
        serde_json::to_value(fixture.files.list(&fixture.environment).unwrap())
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["stage"] == "parentRetained")
    );
}

#[tokio::test]
async fn linux_cancel_during_copy_retains_completed_files_and_stops_the_remaining_batch() {
    let mut fixture = Native::new().await;
    let preview = fixture.preview("folder", "right").await;
    let files = fixture.files.clone();
    let id = preview["id"].as_str().unwrap().to_string();
    let settings = fixture.settings.clone();
    let copied = fixture.path.join("right/folder/nested/a.txt");
    fixture.environment = Environment::fixture(fixture.path.join("data"), move || {
        if std::fs::read(&copied).unwrap() == b"left" {
            files.cancel(&id).unwrap();
        }
        serde_json::from_value(settings.lock().unwrap().clone()).map_err(|error| error.to_string())
    });
    let outcomes = fixture.apply(&preview).await;
    assert_eq!(outcomes[0]["state"], "applied");
    assert_eq!(outcomes[1]["state"], "failed");
    assert_eq!(outcomes[2]["state"], "notAttempted");
    assert_eq!(
        std::fs::read(fixture.path.join("right/folder/nested/b.txt")).unwrap(),
        b"right"
    );
    assert_eq!(fixture.files.copy_count(), 0);
}

#[tokio::test]
async fn linux_copy_preview_accepts_thirty_two_mib_of_combined_source_and_destination_bytes() {
    let mut fixture = Native::new().await;
    for side in ["left", "right"] {
        std::fs::create_dir(fixture.path.join(side).join("payloads")).unwrap();
        for index in 0..8 {
            let mut bytes = vec![if side == "left" { b'L' } else { b'R' }; 2 * 1024 * 1024];
            bytes[0] = 0;
            std::fs::write(
                fixture
                    .path
                    .join(side)
                    .join(format!("payloads/{index}.bin")),
                bytes,
            )
            .unwrap();
        }
    }
    fixture.refresh().await;
    let preview = fixture.preview("payloads", "right").await;
    assert_eq!(preview["files"].as_array().unwrap().len(), 8);
    assert_eq!(
        preview["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|file| file["bytes"].as_u64().unwrap())
            .sum::<u64>(),
        16 * 1024 * 1024
    );
    fixture
        .files
        .cancel(preview["id"].as_str().unwrap())
        .unwrap();
    assert_eq!(fixture.files.copy_count(), 0);
}

#[tokio::test]
async fn linux_source_layout_changes_after_validation_refuse_publication() {
    let fixture = Native::new().await;
    fixture.settings.lock().unwrap()["sources"] = serde_json::json!([
        {"id":"fixture-source","name":"Before","kind":"manual"}
    ]);
    let opened = fixture.open("file.txt", "right").await;
    let settings = fixture.settings.clone();
    let changed = AtomicBool::new(false);
    let environment = Environment::fixture(fixture.path.join("data"), move || {
        let mut current = settings.lock().unwrap();
        let value = serde_json::from_value(current.clone()).map_err(|error| error.to_string())?;
        if !changed.swap(true, Ordering::AcqRel) {
            current["sources"][0]["name"] = "After".into();
        }
        Ok(value)
    });
    let result = fixture
        .files
        .save(
            FileContext {
                comparisons: &fixture.comparisons,
                environment: &environment,
            },
            opened["ticket"].as_str().unwrap(),
            b"must-be-retained".to_vec(),
        )
        .await;
    assert!(result.is_err());
    assert_eq!(
        std::fs::read(fixture.path.join("right/file.txt")).unwrap(),
        b"\xef\xbb\xbfright\r\n"
    );
}

#[tokio::test]
async fn linux_fresh_git_root_reads_refuse_adopting_a_replacement_root_with_equal_bytes() {
    let fixture = Native::new().await;
    std::fs::write(fixture.path.join("left/file.txt"), b"\xef\xbb\xbfright\r\n").unwrap();
    let base = fixture.path.clone();
    *fixture.comparisons.fresh_write_root_hook.lock().unwrap() = Some(Arc::new(move || {
        std::fs::rename(base.join("right"), base.join("right-displaced")).unwrap();
        std::fs::rename(base.join("left"), base.join("right")).unwrap();
    }));
    let result = fixture
        .files
        .open(fixture.context(), fixture.request("file.txt", "right"))
        .await;
    assert!(result.is_err());
    assert_eq!(fixture.files.ticket_count(), 0);
    assert_eq!(
        std::fs::read(fixture.path.join("right/file.txt")).unwrap(),
        b"\xef\xbb\xbfright\r\n"
    );
}
