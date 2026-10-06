use super::timings::{OPERATIONS, PHASES};
use super::*;
use crate::settings::{Settings, Source};
use std::path::{Path, PathBuf};
use std::sync::{atomic::AtomicBool, Arc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[test]
fn common_windows_layout_exports_a_complete_document_with_exact_ram() {
    let home = r"C:\Users\admin\source\repos";
    let repo = r"D:\git\work\app";
    let settings = Settings {
        sources: vec![Source {
            id: "source".into(),
            name: "GitHub Enterprise".into(),
            kind: "github".into(),
            host: "github.example".into(),
            orgs: Vec::new(),
            urls: Vec::new(),
            credential_managed: false,
        }],
        workspace: serde_json::json!({
            "root": home,
            "sets": [{"name": "work", "items": [{"name": "app", "path": repo}]}]
        }),
    };
    let paths = [PathBuf::from(repo.replace('\\', "/"))];
    let mut denylist = leak::Denylist::from_settings(&settings, &paths);
    denylist.add_path(Path::new(&home.replace('\\', "/")));
    let timings = timings(&serde_json::json!({
        "aggregates": {"git.process/status": {"count": 1, "totalMs": 1.0, "maxMs": 1.0}},
        "commands": {"status": 1},
        "events": [{"phase": "git.process", "operation": "status", "durationMs": 1.0}]
    }))
    .unwrap();
    let scale = scale::ScaleData {
        set_count: 1,
        repos_per_set: vec![1],
        repos: vec![scale::ScaleRepo {
            repo: format!("{}1", scale::REPO_PREFIX),
            ..scale::ScaleRepo::default()
        }],
    };
    let hardware = process::MachineHardware {
        logical_cores: 8,
        total_ram_bytes: 32_999_999_999,
        system_drive_type: "ssd",
        windows_build: Some(26_100),
    };
    let document = document_json(
        scale,
        timings,
        resources::ResourceSnapshot::default(),
        hardware,
        "2.50.1".into(),
        0,
        &denylist,
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(&document).unwrap();
    assert_eq!(value["machine"]["totalRamBytes"], 33_000_000_000_u64);
    assert_schema_strings(&value);
}

#[tokio::test]
async fn fixture_document_hides_set_repo_owner_and_path_strings() {
    let _serial = crate::test_support::serial().await;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let fixture = crate::test_support::tmp_root().join(format!("diagnostics-document-{stamp}"));
    let workspace = fixture.join("path-component-secret");
    let repo = workspace.join("acme-secret-repo");
    std::fs::create_dir_all(&repo).unwrap();
    run_git(&repo, &["init", "--initial-branch=main"]).await;
    run_git(&repo, &["config", "user.name", "Fixture User"]).await;
    run_git(&repo, &["config", "user.email", "fixture@example.test"]).await;
    run_git(
        &repo,
        &[
            "remote",
            "add",
            "origin",
            "https://github.example/AcmeOwner/acme-secret-repo.git",
        ],
    )
    .await;
    std::fs::write(repo.join("private-file.txt"), "fixture").unwrap();
    run_git(&repo, &["add", "private-file.txt"]).await;
    run_git(&repo, &["commit", "-m", "fixture"]).await;
    let settings = Settings {
        sources: vec![Source {
            id: "source-secret".into(),
            name: "GitHub private".into(),
            kind: "github".into(),
            host: "enterprise-secret.example".into(),
            orgs: vec!["account-secret".into()],
            urls: vec!["https://configured-owner.example/ConfiguredOwner/private-repo.git".into()],
            credential_managed: false,
        }],
        workspace: serde_json::json!({
            "root": workspace,
            "layout": "flat",
            "sets": [{"id":"set-id", "name":"ClientCorp", "items":[{
                "id":"repo-id", "repoId":"source-secret:repo-id", "name":"acme-secret-repo",
                "folder":"acme-secret-repo", "org":"AcmeOwner",
                "url":"https://github.example/AcmeOwner/acme-secret-repo.git",
                "ref":{"type":"branch", "name":"feature/private"}
            }]}]
        }),
    };
    let paths = scale::registered_paths(&settings);
    let denylist = leak::Denylist::from_settings(&settings, &paths);
    for planted in [
        "account-secret",
        "ClientCorp",
        "AcmeOwner",
        "path-component-secret",
        "acme-secret-repo",
        "enterprise-secret.example",
        "configured-owner.example",
        "private-repo",
    ] {
        assert!(
            denylist.contains(&serde_json::json!({"value": planted.to_ascii_lowercase()})),
            "missing {planted}"
        );
        assert_eq!(
            leak::serialize_checked(&serde_json::json!({"value": planted}), &denylist).unwrap_err(),
            leak::REFUSAL
        );
    }
    let scale = scale::collect(
        &settings,
        Arc::new(AtomicBool::new(false)),
        Arc::new(|_, _| {}),
    )
    .await
    .unwrap();
    let timings = timings(&benchmark::benchmark_snapshot().unwrap()).unwrap();
    let resources = sampler::Sampler::start().unwrap();
    assert!(
        resources.wait_for(Duration::from_secs(2), |snapshot| !snapshot
            .samples
            .is_empty())
    );
    let document = document_json(
        scale,
        timings,
        resources.snapshot().rounded(),
        process::hardware(),
        git_version().await.unwrap(),
        resources.uptime_ms(),
        &denylist,
    )
    .unwrap();
    assert!(!document.to_ascii_lowercase().contains("clientcorp"));
    assert!(!document.to_ascii_lowercase().contains("acme-secret-repo"));
    assert!(!document.to_ascii_lowercase().contains("acmeowner"));
    assert!(!document
        .to_ascii_lowercase()
        .contains("path-component-secret"));
    assert_schema_strings(&serde_json::from_str(&document).unwrap());
    write_sample_if_requested(&document);
    std::fs::remove_dir_all(fixture).unwrap();
}

async fn run_git(repo: &Path, command: &[&str]) {
    let path = repo.to_string_lossy();
    let mut args = vec!["-C", path.as_ref()];
    args.extend_from_slice(command);
    let request = git::Request {
        args: &args,
        context: "diagnostics-document-fixture",
        timeout: Duration::from_secs(30),
        expected: &[0],
        policy: git::OutputPolicy::Metadata,
    };
    let result = git::execute(request, None).await.unwrap();
    assert_eq!(result.code, Some(0));
}

fn assert_schema_strings(value: &serde_json::Value) {
    match value {
        serde_json::Value::String(text) => {
            let fixed = document::OS_FAMILIES.contains(&text.as_str())
                || document::DRIVE_TYPES.contains(&text.as_str())
                || PHASES.contains(&text.as_str())
                || OPERATIONS.contains(&text.as_str());
            let version = text.split('.').all(|part| {
                !part.is_empty() && part.chars().all(|character| character.is_ascii_digit())
            });
            let repo = text.strip_prefix(scale::REPO_PREFIX).is_some_and(|number| {
                !number.is_empty() && number.chars().all(|character| character.is_ascii_digit())
            });
            assert!(fixed || version || repo, "unexpected diagnostics string");
        }
        serde_json::Value::Array(values) => values.iter().for_each(assert_schema_strings),
        serde_json::Value::Object(values) => values.values().for_each(assert_schema_strings),
        _ => {}
    }
}

fn write_sample_if_requested(document: &str) {
    let Some(path) = std::env::var_os("SKEIN_DIAG_SAMPLE_PATH").map(PathBuf::from) else {
        return;
    };
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target");
    assert!(path.is_absolute() && path.starts_with(&target));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap();
    use std::io::Write;
    file.write_all(document.as_bytes()).unwrap();
}
