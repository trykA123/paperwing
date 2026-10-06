use super::*;
use std::path::{Path, PathBuf};

fn git_out(dir: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn scratch(label: &str) -> PathBuf {
    let base = crate::env_names::var_os("SKEIN_TEST_TMP")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = base.join(format!(
        "skein-tags-{label}-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn repo_with_remote() -> (PathBuf, PathBuf) {
    let bare = scratch("bare");
    git_out(&bare, &["init", "-q", "--bare", "-b", "main"]);
    let dir = scratch("work");
    git_out(&dir, &["init", "-q", "-b", "main"]);
    for (key, value) in [
        ("user.name", "Test User"),
        ("user.email", "test@example.test"),
        ("commit.gpgsign", "false"),
        ("core.autocrlf", "false"),
        ("tag.gpgsign", "false"),
    ] {
        git_out(&dir, &["config", key, value]);
    }
    git_out(&dir, &["commit", "-q", "--allow-empty", "-m", "first"]);
    git_out(&dir, &["remote", "add", "origin", bare.to_str().unwrap()]);
    (dir, bare)
}

fn request(name: &str, message: Option<&str>) -> CreateTagRequest {
    CreateTagRequest {
        name: name.into(),
        message: message.map(str::to_string),
        ..Default::default()
    }
}

fn path(dir: &Path) -> String {
    dir.to_str().unwrap().to_string()
}

#[tokio::test]
async fn creates_lightweight_and_annotated_tags_and_lists_them() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let (dir, _bare) = repo_with_remote();
    let head = git_out(&dir, &["rev-parse", "HEAD"]);
    let light = create_tag(path(&dir), request("v1", None)).await.unwrap();
    let annotated = create_tag(path(&dir), request("v2", Some("Release two\n\nbody")))
        .await
        .unwrap();
    assert!(!light.annotated && annotated.annotated);
    assert_eq!(light.commit, head);
    let tags = list_tags(path(&dir)).await.unwrap();
    assert_eq!(tags.len(), 2);
    assert_eq!(
        (
            tags[0].name.as_str(),
            tags[0].annotated,
            tags[0].commit.as_str(),
            tags[0].subject.clone()
        ),
        ("v1", false, head.as_str(), None)
    );
    assert_eq!(
        (
            tags[1].annotated,
            tags[1].commit.as_str(),
            tags[1].subject.as_deref()
        ),
        (true, head.as_str(), Some("Release two"))
    );
    assert_ne!(tags[1].object, head);
}

#[tokio::test]
async fn annotated_message_keeps_hash_lines() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let (dir, _bare) = repo_with_remote();
    create_tag(path(&dir), request("v1", Some("#123 fixed\nreal line")))
        .await
        .unwrap();
    let body = git_out(&dir, &["cat-file", "-p", "refs/tags/v1"]);
    assert!(body.ends_with("#123 fixed\nreal line"), "{body}");
}

#[tokio::test]
async fn tags_a_given_ref_instead_of_head() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let (dir, _bare) = repo_with_remote();
    let first = git_out(&dir, &["rev-parse", "HEAD"]);
    git_out(&dir, &["commit", "-q", "--allow-empty", "-m", "second"]);
    let created = create_tag(
        path(&dir),
        CreateTagRequest {
            target: Some("HEAD~1".into()),
            ..request("old", None)
        },
    )
    .await
    .unwrap();
    assert_eq!(created.commit, first);
    for target in ["nope", "--all", "-x"] {
        assert!(
            create_tag(
                path(&dir),
                CreateTagRequest {
                    target: Some(target.into()),
                    ..request("bad", None)
                }
            )
            .await
            .is_err(),
            "{target}"
        );
    }
}

#[tokio::test]
async fn refuses_duplicate_names_unless_moving() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let (dir, _bare) = repo_with_remote();
    let first = git_out(&dir, &["rev-parse", "HEAD"]);
    create_tag(path(&dir), request("v1", None)).await.unwrap();
    git_out(&dir, &["commit", "-q", "--allow-empty", "-m", "second"]);
    let refused = create_tag(path(&dir), request("v1", None))
        .await
        .unwrap_err();
    assert!(refused.contains("already exists"));
    assert_eq!(git_out(&dir, &["rev-parse", "v1^{commit}"]), first);
    let moved = create_tag(
        path(&dir),
        CreateTagRequest {
            move_existing: true,
            ..request("v1", Some("moved"))
        },
    )
    .await
    .unwrap();
    assert_eq!(moved.previous_object.as_deref(), Some(first.as_str()));
    assert_eq!(
        git_out(&dir, &["rev-parse", "v1^{commit}"]),
        git_out(&dir, &["rev-parse", "HEAD"])
    );
}

#[tokio::test]
async fn rejects_invalid_tag_and_remote_names() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let (dir, _bare) = repo_with_remote();
    for name in [
        "-x", "HEAD", "head", "a..b", "", "a b", "a~1", "x.lock", "--delete",
    ] {
        assert!(
            create_tag(path(&dir), request(name, None)).await.is_err(),
            "{name}"
        );
        assert!(delete_tag(path(&dir), name.into()).await.is_err(), "{name}");
    }
    create_tag(path(&dir), request("v1", None)).await.unwrap();
    for remote in ["-x", "", "https://example.invalid/x.git", "missing"] {
        assert!(
            push_tag(path(&dir), remote.into(), "v1".into(), None)
                .await
                .is_err(),
            "{remote}"
        );
        assert!(
            delete_remote_tag(path(&dir), remote.into(), "v1".into())
                .await
                .is_err(),
            "{remote}"
        );
    }
}

#[tokio::test]
async fn pushes_one_tag_only_and_deletes_it_remotely() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let (dir, bare) = repo_with_remote();
    git_out(&dir, &["push", "-q", "origin", "main"]);
    create_tag(path(&dir), request("v1", Some("one")))
        .await
        .unwrap();
    create_tag(path(&dir), request("v2", None)).await.unwrap();
    push_tag(path(&dir), "origin".into(), "v1".into(), None)
        .await
        .unwrap();
    assert_eq!(git_out(&bare, &["tag", "--list"]), "v1");
    delete_tag(path(&dir), "v1".into()).await.unwrap();
    assert_eq!(git_out(&dir, &["tag", "--list"]), "v2");
    assert_eq!(git_out(&bare, &["tag", "--list"]), "v1");
    delete_remote_tag(path(&dir), "origin".into(), "v1".into())
        .await
        .unwrap();
    assert_eq!(git_out(&bare, &["tag", "--list"]), "");
    assert!(delete_tag(path(&dir), "v1".into()).await.is_err());
}

#[tokio::test]
async fn moved_tag_needs_a_matching_lease_to_push() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let (dir, bare) = repo_with_remote();
    git_out(&dir, &["push", "-q", "origin", "main"]);
    let first = create_tag(path(&dir), request("v1", None)).await.unwrap();
    push_tag(path(&dir), "origin".into(), "v1".into(), None)
        .await
        .unwrap();
    git_out(&dir, &["commit", "-q", "--allow-empty", "-m", "second"]);
    let moved = create_tag(
        path(&dir),
        CreateTagRequest {
            move_existing: true,
            ..request("v1", None)
        },
    )
    .await
    .unwrap();
    assert!(push_tag(path(&dir), "origin".into(), "v1".into(), None)
        .await
        .is_err());
    assert!(push_tag(
        path(&dir),
        "origin".into(),
        "v1".into(),
        Some(moved.object.clone())
    )
    .await
    .is_err());
    assert!(push_tag(
        path(&dir),
        "origin".into(),
        "v1".into(),
        Some(String::new())
    )
    .await
    .is_err());
    assert!(
        push_tag(path(&dir), "origin".into(), "v1".into(), Some("zz".into()))
            .await
            .is_err()
    );
    assert_eq!(git_out(&bare, &["rev-parse", "refs/tags/v1"]), first.object);
    let pushed = push_tag(
        path(&dir),
        "origin".into(),
        "v1".into(),
        moved.previous_object,
    )
    .await
    .unwrap();
    assert!(pushed.forced);
    assert_eq!(git_out(&bare, &["rev-parse", "refs/tags/v1"]), moved.object);
}

#[tokio::test]
async fn signing_is_left_to_git_config() {
    let _runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    let (dir, _bare) = repo_with_remote();
    assert!(!signs_tags(&path(&dir)).await.unwrap());
    git_out(&dir, &["config", "tag.gpgSign", "true"]);
    assert!(signs_tags(&path(&dir)).await.unwrap());
    let refused = create_tag(path(&dir), request("v1", None))
        .await
        .unwrap_err();
    assert!(refused.contains("signs tags"));
    assert!(list_tags(path(&dir)).await.unwrap().is_empty());
}

#[test]
fn message_cleaning_trims_and_bounds() {
    assert_eq!(
        clean_message(Some("  hi \n".into())).unwrap().as_deref(),
        Some("hi")
    );
    assert_eq!(clean_message(Some("   ".into())).unwrap(), None);
    assert!(clean_message(Some("a".repeat(MAX_MESSAGE + 1))).is_err());
}
