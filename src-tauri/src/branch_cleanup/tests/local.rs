use super::*;

async fn delete(fixture: &Fixture, items: &[&str], expected: Vec<String>) -> Vec<BranchOutcome> {
    delete_merged_branches(fixture.path(), names(items), expected, Some("main".into()))
        .await
        .unwrap()
}

#[tokio::test]
async fn checks_oid_merge_and_protected_branches_with_reasons() {
    let fixture = fixture();
    branch_with_commit(&fixture, "done", true);
    branch_with_commit(&fixture, "wip", false);
    let stale_expected = fixture.tip("done");
    git_in(&fixture.work, &["branch", "stale", &stale_expected]);
    git_in(&fixture.work, &["switch", "-q", "stale"]);
    fixture.commit("s.txt", "moved");
    git_in(&fixture.work, &["switch", "-q", "main"]);
    git_in(
        &fixture.work,
        &["merge", "-q", "--no-ff", "-m", "merge stale", "stale"],
    );
    let main = fixture.tip("main");
    let expected = vec![
        fixture.tip("done"),
        fixture.tip("wip"),
        stale_expected,
        main.clone(),
        main.clone(),
        main,
    ];
    let results = delete(
        &fixture,
        &["done", "wip", "stale", "main", "HEAD", "-D"],
        expected,
    )
    .await;
    assert!(results[0].deleted, "{:?}", results[0].error);
    assert_eq!(error_of(&results[1]), "wip is not merged into main");
    assert_eq!(error_of(&results[2]), "stale changed since it was listed");
    assert_eq!(error_of(&results[3]), "main is the current branch");
    assert_eq!(error_of(&results[4]), "Invalid branch name: HEAD");
    assert_eq!(error_of(&results[5]), "Invalid branch name: -D");
    let branches = git_in(&fixture.work, &["branch", "--format=%(refname:short)"]);
    assert!(!branches.lines().any(|line| line == "done"));
    assert!(
        branches.lines().any(|line| line == "wip") && branches.lines().any(|line| line == "stale")
    );
    assert!(
        delete_merged_branches(fixture.path(), names(&["wip"]), vec![], None)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn deletes_when_head_is_on_a_third_branch_with_live_or_gone_upstream() {
    let fixture = fixture();
    branch_with_commit(&fixture, "live", true);
    branch_with_commit(&fixture, "gone", true);
    branch_with_commit(&fixture, "third", false);
    git_in(
        &fixture.work,
        &["push", "-q", "-u", "origin", "main", "live", "gone"],
    );
    git_in(&fixture.work, &["push", "-q", "origin", "--delete", "gone"]);
    git_in(&fixture.work, &["fetch", "-q", "--prune", "origin"]);
    git_in(&fixture.work, &["switch", "-q", "third"]);
    let expected = vec![fixture.tip("live"), fixture.tip("gone")];
    let results = delete_merged_branches(fixture.path(), names(&["live", "gone"]), expected, None)
        .await
        .unwrap();
    assert!(results.iter().all(|result| result.deleted), "{results:?}");
    let branches = git_in(&fixture.work, &["branch", "--format=%(refname:short)"]);
    assert_eq!(branches.lines().collect::<Vec<_>>(), vec!["main", "third"]);
}

#[tokio::test]
async fn refuses_a_branch_checked_out_in_another_worktree() {
    let fixture = fixture();
    branch_with_commit(&fixture, "done", true);
    let tree = fixture.root.join("tree");
    git_in(
        &fixture.work,
        &["worktree", "add", "-q", tree.to_str().unwrap(), "done"],
    );
    let results = delete(&fixture, &["done"], vec![fixture.tip("done")]).await;
    assert_eq!(error_of(&results[0]), "done is checked out in a worktree");
    assert!(!fixture.tip("done").is_empty());
}

#[tokio::test]
async fn refuses_a_branch_that_moved_after_listing() {
    let fixture = fixture();
    branch_with_commit(&fixture, "done", true);
    let listed = fixture.tip("done");
    git_in(&fixture.work, &["switch", "-q", "done"]);
    fixture.commit("later.txt", "later");
    git_in(&fixture.work, &["switch", "-q", "main"]);
    let results = delete(&fixture, &["done"], vec![listed]).await;
    assert_eq!(error_of(&results[0]), "done changed since it was listed");
    assert!(!fixture.tip("done").is_empty());
}

#[tokio::test]
async fn current_branch_is_never_deleted() {
    let fixture = fixture();
    git_in(&fixture.work, &["switch", "-q", "-c", "feature"]);
    let results = delete(&fixture, &["feature"], vec![fixture.tip("feature")]).await;
    assert_eq!(error_of(&results[0]), "feature is the current branch");
}

#[tokio::test]
async fn symbolic_alias_is_not_listed_and_never_deletes_its_target() {
    let fixture = fixture();
    git_in(&fixture.work, &["switch", "-q", "-c", "other"]);
    git_in(
        &fixture.work,
        &["symbolic-ref", "refs/heads/alias", "refs/heads/main"],
    );
    let list = merged_branches(fixture.path(), Some("main".into()), None)
        .await
        .unwrap();
    assert!(list.local.iter().all(|branch| branch.name != "alias"));
    let main = fixture.tip("main");
    let results = delete(&fixture, &["alias"], vec![main.clone()]).await;
    assert_eq!(error_of(&results[0]), "alias is a symbolic ref");
    assert_eq!(fixture.tip("main"), main);
}

#[tokio::test]
async fn removes_branch_config_section_on_delete() {
    let fixture = fixture();
    branch_with_commit(&fixture, "done", true);
    git_in(
        &fixture.work,
        &["push", "-q", "-u", "origin", "main", "done"],
    );
    git_in(&fixture.work, &["switch", "-q", "-c", "other"]);
    let results = delete(&fixture, &["done"], vec![fixture.tip("done")]).await;
    assert!(results[0].deleted, "{:?}", results[0].error);
    let config = git_in(&fixture.work, &["config", "--local", "--list"]);
    assert!(!config.contains("branch.done."), "{config}");
    let results = delete(&fixture, &["done"], vec![fixture.tip("main")]).await;
    assert!(!results[0].deleted);
}
