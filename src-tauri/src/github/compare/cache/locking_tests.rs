use super::*;
use crate::github::compare::fixture::source;
use crate::platform::Fixture;
use std::time::Duration;

#[tokio::test]
async fn slow_metadata_transaction_does_not_block_revisions_and_invalidation_follows_enqueue_order()
{
    let _guard = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new("p40-metadata-lock");
    let path = fixture.0.join("cache.sqlite3");
    let store = Store::open(&path, &Default::default()).unwrap();
    store
        .enqueue(|connection| {
            connection.busy_timeout(Duration::from_secs(60))?;
            Ok(())
        })
        .unwrap()
        .await
        .unwrap()
        .unwrap();
    let source = source("gitext.company.com");
    let request = Request::from_url(
        source.clone(),
        &format!("https://{}/admin/repo.git", source.host),
        ["a".repeat(40), "b".repeat(40)],
    )
    .unwrap();
    let admission = Admission::new(&request).unwrap();
    let data = Comparison {
        base: request.base.clone(),
        head: request.head.clone(),
        merge_base: "c".repeat(40),
        files: Vec::new(),
        commits: Vec::new(),
        ahead: 0,
        behind: 0,
        truncated: Default::default(),
    };
    let key = comparisons::Key {
        host: source.host.clone(),
        owner: "admin".into(),
        repository: "repo".into(),
        base: data.base.clone(),
        head: data.head.clone(),
    };
    let blocker = rusqlite::Connection::open(&path).unwrap();
    blocker.execute_batch("BEGIN IMMEDIATE").unwrap();
    let waiting = crate::store::comparisons::tests::subscribe(&source.id);
    let writing = tokio::spawn({
        let store = store.clone();
        async move { remember(&store, key, &admission, &data).await }
    });
    let reached_transaction = tokio::time::timeout(Duration::from_secs(5), waiting).await;
    let id = source.id.clone();
    let invalidation = store.clone();
    let mut changing = tauri::async_runtime::spawn_blocking(move || {
        let before = crate::credentials::revision(&id);
        crate::credentials::advance_revision(&id, || invalidation.remove_source(&id)) > before
    });
    let changed_before_release = tokio::time::timeout(Duration::from_secs(5), &mut changing).await;
    let released = blocker.execute_batch("ROLLBACK");
    drop(blocker);
    let finished_before_release = changed_before_release.is_ok();
    let revision_result = match changed_before_release {
        Ok(result) => result,
        Err(_) => changing.await,
    };
    let writer_result = writing.await;
    let rows = store
        .enqueue(|connection| {
            Ok(connection.query_row::<i64, _, _>(
                "SELECT count(*) FROM github_comparison_scopes",
                [],
                |row| row.get(0),
            )?)
        })
        .unwrap()
        .await
        .unwrap()
        .unwrap();
    store.close();
    crate::store::comparisons::tests::writing(&source.id);
    released.unwrap();
    writer_result.unwrap();
    let changed = revision_result.unwrap() && finished_before_release;
    assert!(
        matches!(reached_transaction, Ok(Ok(()))),
        "metadata writer never reached its blocked transaction"
    );
    assert!(
        changed,
        "SQLite transaction blocked credential revision checks and changes"
    );
    assert_eq!(
        rows, 0,
        "old cache access survived queued credential invalidation"
    );
}
