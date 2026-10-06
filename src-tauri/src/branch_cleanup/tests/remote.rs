use super::*;

async fn delete(fixture: &Fixture, items: &[&str], expected: Vec<String>) -> Vec<BranchOutcome> {
    delete_remote_branches(
        fixture.path(),
        "origin".into(),
        names(items),
        expected,
        None,
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn one_leased_push_maps_each_branch() {
    let fixture = fixture();
    branch_with_commit(&fixture, "done", true);
    branch_with_commit(&fixture, "wip", false);
    branch_with_commit(&fixture, "raced", true);
    git_in(
        &fixture.work,
        &["push", "-q", "origin", "main", "done", "wip", "raced"],
    );
    let raced_seen = fixture.tip("raced");
    git_in(&fixture.work, &["switch", "-q", "raced"]);
    fixture.commit("r.txt", "extra");
    git_in(&fixture.work, &["push", "-q", "origin", "raced"]);
    git_in(&fixture.work, &["switch", "-q", "main"]);
    let main = fixture.tip("main");
    let expected = vec![
        fixture.tip("done"),
        fixture.tip("wip"),
        raced_seen,
        main.clone(),
        main,
    ];
    let results = delete(
        &fixture,
        &["done", "wip", "raced", "main", "master"],
        expected,
    )
    .await;
    assert!(results[0].deleted, "{:?}", results[0].error);
    assert_eq!(error_of(&results[1]), "wip is not merged into origin/main");
    assert_eq!(
        error_of(&results[2]),
        "raced changed on origin since it was listed; fetch, then refresh"
    );
    assert_eq!(error_of(&results[3]), "main is protected");
    assert_eq!(error_of(&results[4]), "master is protected");
    assert_eq!(fixture.remote_tip("done"), None);
    assert!(fixture.remote_tip("wip").is_some() && fixture.remote_tip("raced").is_some());
    assert!(fixture.remote_tip("main").is_some());
}

#[tokio::test]
async fn deletes_several_branches_in_one_call() {
    let fixture = fixture();
    for name in ["a", "b", "c"] {
        branch_with_commit(&fixture, name, true);
    }
    git_in(
        &fixture.work,
        &["push", "-q", "origin", "main", "a", "b", "c"],
    );
    let expected = vec![fixture.tip("a"), fixture.tip("b"), fixture.tip("c")];
    let results = delete(
        &fixture,
        &["a", "b", "c", "a"],
        [expected.clone(), vec![expected[0].clone()]].concat(),
    )
    .await;
    assert!(
        results[..3].iter().all(|result| result.deleted),
        "{results:?}"
    );
    assert_eq!(error_of(&results[3]), "a is listed twice");
    assert!(["a", "b", "c"]
        .iter()
        .all(|name| fixture.remote_tip(name).is_none()));
}

#[tokio::test]
async fn lease_mismatch_is_refused() {
    let fixture = fixture();
    branch_with_commit(&fixture, "done", true);
    git_in(&fixture.work, &["push", "-q", "origin", "main", "done"]);
    let other = git_in(&fixture.work, &["rev-parse", "main~1"]);
    let results = delete(&fixture, &["done"], vec![other]).await;
    assert_eq!(
        error_of(&results[0]),
        "done changed on origin since it was listed; fetch, then refresh"
    );
    assert!(fixture.remote_tip("done").is_some());
}

#[tokio::test]
async fn remote_rejection_reason_is_returned() {
    let fixture = fixture();
    branch_with_commit(&fixture, "done", true);
    git_in(&fixture.work, &["push", "-q", "origin", "main", "done"]);
    let hook = fixture.bare.join("hooks/update");
    std::fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
    std::fs::set_permissions(&hook, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let results = delete(&fixture, &["done"], vec![fixture.tip("done")]).await;
    assert_eq!(error_of(&results[0]), "hook declined");
    assert!(fixture.remote_tip("done").is_some());
}

#[tokio::test]
async fn merge_only_in_unpushed_local_main_is_not_deletable() {
    let fixture = fixture();
    branch_with_commit(&fixture, "done", false);
    git_in(&fixture.work, &["push", "-q", "origin", "done"]);
    git_in(
        &fixture.work,
        &["merge", "-q", "--no-ff", "-m", "merge done", "done"],
    );
    let results = delete(&fixture, &["done"], vec![fixture.tip("done")]).await;
    assert_eq!(error_of(&results[0]), "done is not merged into origin/main");
    assert!(fixture.remote_tip("done").is_some());
}

#[tokio::test]
async fn refuses_without_a_remote_base() {
    let fixture = fixture();
    branch_with_commit(&fixture, "done", true);
    git_in(&fixture.work, &["push", "-q", "origin", "done"]);
    git_in(&fixture.work, &["remote", "set-head", "origin", "-d"]);
    git_in(
        &fixture.work,
        &["update-ref", "-d", "refs/remotes/origin/main"],
    );
    let error = delete_remote_branches(
        fixture.path(),
        "origin".into(),
        names(&["done"]),
        vec![fixture.tip("done")],
        Some("main".into()),
    )
    .await
    .unwrap_err();
    assert!(error.contains("not found on origin"), "{error}");
    let error = delete_remote_branches(
        fixture.path(),
        "origin".into(),
        names(&["done"]),
        vec![fixture.tip("done")],
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(error, "Choose a remote base");
    assert!(fixture.remote_tip("done").is_some());
}

#[tokio::test]
async fn validates_remote_and_names() {
    let fixture = fixture();
    let call = |remote: &str, name: &str| {
        delete_remote_branches(
            fixture.path(),
            remote.into(),
            names(&[name]),
            vec![fixture.tip("main")],
            None,
        )
    };
    assert_eq!(
        call("nowhere", "x").await.unwrap_err(),
        "There is no remote named nowhere"
    );
    assert!(call("--all", "x").await.is_err());
    for name in ["HEAD", "-f", "refs/heads/x", "a..b"] {
        let results = call("origin", name).await.unwrap();
        assert!(
            error_of(&results[0]).starts_with("Invalid branch name"),
            "{name}"
        );
    }
}

#[tokio::test]
async fn stale_lease_on_a_vanished_remote_branch_says_it_is_gone() {
    let fixture = fixture();
    branch_with_commit(&fixture, "done", true);
    git_in(&fixture.work, &["push", "-q", "origin", "main", "done"]);
    git_in(&fixture.bare, &["update-ref", "-d", "refs/heads/done"]);
    let results = delete(&fixture, &["done"], vec![fixture.tip("done")]).await;
    assert_eq!(
        error_of(&results[0]),
        "done is already gone on origin; fetch to refresh"
    );
}
