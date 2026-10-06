use super::*;

#[tokio::test]
async fn lists_merged_and_unmerged_without_current_or_base() {
    let fixture = fixture();
    branch_with_commit(&fixture, "done", true);
    branch_with_commit(&fixture, "wip", false);
    git_in(&fixture.work, &["switch", "-q", "-c", "here"]);
    git_in(&fixture.work, &["push", "-q", "origin", "main"]);
    let list = merged_branches(fixture.path(), None, None).await.unwrap();
    assert_eq!(
        (list.base.as_str(), list.base_name.as_str()),
        ("origin/main", "main")
    );
    assert_eq!(list.remote.as_deref(), Some("origin"));
    assert_eq!(list.current.as_deref(), Some("here"));
    assert!(find(&list, "done").merged);
    assert!(!find(&list, "wip").merged);
    assert!(list
        .local
        .iter()
        .all(|branch| branch.name != "here" && branch.name != "main"));
    assert_eq!(find(&list, "done").oid, fixture.tip("done"));
    assert_eq!(find(&list, "done").subject, "done");
    assert!(find(&list, "done").last_commit > 0);
}

#[tokio::test]
async fn base_falls_back_to_local_main_and_accepts_override() {
    let fixture = fixture();
    git_in(&fixture.work, &["remote", "remove", "origin"]);
    branch_with_commit(&fixture, "done", true);
    let list = merged_branches(fixture.path(), None, None).await.unwrap();
    assert_eq!((list.base.as_str(), list.remote.clone()), ("main", None));
    assert!(find(&list, "done").merged);
    let custom = merged_branches(fixture.path(), Some("done".into()), None)
        .await
        .unwrap();
    assert_eq!(custom.base_name, "done");
    assert!(merged_branches(fixture.path(), Some("nope".into()), None)
        .await
        .is_err());
    assert!(merged_branches(fixture.path(), Some("-x".into()), None)
        .await
        .is_err());
    assert!(merged_branches(fixture.path(), None, Some("origin".into()))
        .await
        .is_err());
}

#[tokio::test]
async fn gone_upstream_and_remote_listing() {
    let fixture = fixture();
    branch_with_commit(&fixture, "done", true);
    branch_with_commit(&fixture, "other", true);
    git_in(
        &fixture.work,
        &["push", "-q", "-u", "origin", "done", "other", "main"],
    );
    git_in(&fixture.work, &["push", "-q", "origin", "--delete", "done"]);
    git_in(&fixture.work, &["fetch", "-q", "--prune", "origin"]);
    let list = merged_branches(fixture.path(), None, Some("origin".into()))
        .await
        .unwrap();
    let done = find(&list, "done");
    assert_eq!(done.upstream.as_deref(), Some("origin/done"));
    assert!(done.upstream_gone);
    let other = find(&list, "other");
    assert!(!other.upstream_gone);
    let names: Vec<&str> = list
        .remote_branches
        .iter()
        .map(|branch| branch.name.as_str())
        .collect();
    assert_eq!(names, vec!["other"]);
    assert_eq!(list.remote_branches[0].oid, fixture.tip("other"));
}

#[tokio::test]
async fn protected_names_are_not_listed_and_missing_remote_base_empties_remote_list() {
    let fixture = fixture();
    branch_with_commit(&fixture, "done", true);
    git_in(&fixture.work, &["branch", "master", "main"]);
    git_in(&fixture.work, &["push", "-q", "origin", "main", "done"]);
    let list = merged_branches(fixture.path(), None, None).await.unwrap();
    assert!(list.local.iter().all(|branch| branch.name != "master"));
    assert_eq!(list.remote_base.as_deref(), Some("origin/main"));
    assert_eq!(list.remote_branches.len(), 1);
    git_in(&fixture.work, &["remote", "set-head", "origin", "-d"]);
    git_in(
        &fixture.work,
        &["update-ref", "-d", "refs/remotes/origin/main"],
    );
    let list = merged_branches(fixture.path(), None, None).await.unwrap();
    assert_eq!(list.remote_base, None);
    assert!(list.remote_branches.is_empty());
    assert_eq!(list.base, "main");
}
