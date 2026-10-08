use super::*;

#[test]
fn pending_payload_has_no_status_lines_or_binary() {
    let row = RowUpdate::Pending(PendingRow {
        id: "file-1".into(),
        path: "file".into(),
        left: None,
        right: None,
        hint: Hint::ChangedId,
    });
    assert_eq!(
        serde_json::to_value(row).unwrap(),
        serde_json::json!({
            "phase":"pending", "id":"file-1", "path":"file", "left":null,
            "right":null, "hint":"changedId"
        })
    );
}

#[tokio::test]
async fn start_returns_while_producer_waits_and_cancel_invalidates_generation() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    fixture.write("file", b"before\n");
    fixture.commit("base").await;
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
    let slots = service.slots.acquire_many(4).await.unwrap();
    let started = service
        .start(&settings, &opened.id, Options::default(), None, None)
        .await
        .unwrap();
    let progress = service
        .progress(&started.id, started.generation, 0, 500)
        .unwrap();
    assert_eq!(progress.state, State::Resolving);
    assert_eq!(progress.sequence, 0);
    assert!(progress.totals.is_none());
    assert!(service
        .progress(&started.id, started.generation, 0, 501)
        .is_err());
    assert!(service.cancel(&started.id).await);
    assert_eq!(
        service
            .progress(&started.id, started.generation, 0, 1)
            .unwrap_err()
            .kind,
        "staleGeneration"
    );
    drop(slots);
    service.close(&started.id).await;
}

#[tokio::test]
async fn progressive_completion_returns_the_legacy_refresh_payload() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    fixture.write("folder/file", b"before\n");
    fixture.commit("base").await;
    fixture.write("folder/file", b"after\n");
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
    let started = service
        .start(&settings, &opened.id, Options::default(), None, None)
        .await
        .unwrap();
    let result = service.wait(&opened.id, started.generation).await.unwrap();
    let RefreshResult::Ready { snapshot } = result else {
        panic!("refresh failed")
    };
    let progress = service
        .progress(&opened.id, started.generation, 0, 500)
        .unwrap();
    assert_eq!(progress.state, State::Complete);
    assert_eq!(
        serde_json::to_value(progress.snapshot.unwrap()).unwrap(),
        serde_json::to_value(snapshot).unwrap()
    );
    assert_eq!(progress.totals.unwrap().pending, 0);
    assert_eq!(progress.rows.len(), 4);
    let mut pending_ids: Vec<_> = progress
        .rows
        .iter()
        .filter_map(|row| match row {
            RowUpdate::Pending(row) => Some(&row.id),
            _ => None,
        })
        .collect();
    let mut final_ids: Vec<_> = progress
        .rows
        .iter()
        .filter_map(|row| match row {
            RowUpdate::Final(row) => Some(&row.id),
            _ => None,
        })
        .collect();
    pending_ids.sort();
    final_ids.sort();
    assert_eq!(pending_ids, final_ids);
    service.close(&opened.id).await;
}

#[tokio::test]
async fn inventory_and_selected_content_are_ready_before_deferred_enrichment() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    fixture.write("folder/file", b"before\n");
    fixture.commit("base").await;
    fixture.write("folder/file", b"after\n");
    let service = fixture.service();
    let settings = fixture.settings();
    let control = Arc::new(publication::EnrichmentControl {
        listed: Default::default(),
        release: Default::default(),
        panic: false,
        after_fixed: false,
    });
    *service.enrichment_control.lock().unwrap() = Some(control.clone());
    let opened = service
        .open(
            &settings,
            fixture.context(CompareRef::Head).endpoint,
            fixture.context(CompareRef::WorkingTree).endpoint,
        )
        .await
        .unwrap();
    let started_at = std::time::Instant::now();
    let started = service
        .start(&settings, &opened.id, Options::default(), None, None)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), control.listed.notified())
        .await
        .unwrap();
    let listed_ms = started_at.elapsed().as_millis();
    let progress = service
        .progress(&opened.id, started.generation, 0, 500)
        .unwrap();
    assert_eq!(progress.state, State::Enriching);
    assert!(progress.snapshot.is_none());
    assert_eq!(progress.totals.as_ref().unwrap().pending, 1);
    assert_eq!(progress.totals.as_ref().unwrap().raw.total, 0);
    let row = progress
        .rows
        .iter()
        .find_map(|row| match row {
            RowUpdate::Pending(row) if row.path == "folder/file" => Some(row),
            _ => None,
        })
        .unwrap();
    assert!(service
        .snapshot(&settings, &opened.id, started.generation)
        .await
        .is_err());
    assert!(service
        .write_context(
            &settings,
            &opened.id,
            started.generation,
            &row.id,
            "right",
            true
        )
        .await
        .err()
        .unwrap()
        .contains("finishes checking"));
    assert!(service
        .copy_source_context(&settings, &opened.id, started.generation, &row.id, "left")
        .await
        .err()
        .unwrap()
        .contains("finishes checking"));
    let held_producer_slots = service.slots.acquire_many(3).await.unwrap();
    let content = service
        .selected_content(&settings, &opened.id, started.generation, &row.id, "right")
        .await
        .unwrap();
    assert_eq!(content.bytes, b"after\n");
    let content_ms = started_at.elapsed().as_millis();
    assert_eq!(
        service
            .progress(&opened.id, started.generation, 0, 500)
            .unwrap()
            .state,
        State::Enriching
    );
    drop(held_producer_slots);
    control.release.notify_one();
    service.wait(&opened.id, started.generation).await.unwrap();
    println!(
        "early publication: listed={listed_ms}ms content={content_ms}ms complete={}ms",
        started_at.elapsed().as_millis()
    );
    service.close(&opened.id).await;
}
