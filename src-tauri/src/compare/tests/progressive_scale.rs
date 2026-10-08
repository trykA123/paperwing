use super::*;

#[tokio::test]
async fn twenty_thousand_rows_publish_inventory_content_and_bounded_final_pages() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    for index in 0..FILE_LIMIT {
        fixture.write(&format!("file-{index:05}"), b"old\n");
    }
    let base = fixture.commit("base").await;
    for index in 0..FILE_LIMIT {
        fixture.write(&format!("file-{index:05}"), b"new\n");
    }
    fixture.commit("head").await;
    let service = fixture.service();
    let settings = fixture.settings();
    let opened = service
        .open(
            &settings,
            fixture.context(CompareRef::Commit { sha: base }).endpoint,
            fixture.context(CompareRef::Head).endpoint,
        )
        .await
        .unwrap();
    let control = Arc::new(publication::EnrichmentControl {
        listed: Default::default(),
        release: Default::default(),
        panic: false,
        after_fixed: false,
    });
    *service.enrichment_control.lock().unwrap() = Some(control.clone());
    let start = std::time::Instant::now();
    let started = service
        .start(&settings, &opened.id, Options::default(), None, None)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(60), control.listed.notified())
        .await
        .unwrap();
    let first = service
        .progress(&opened.id, started.generation, 0, 500)
        .unwrap();
    let listed_ms = start.elapsed().as_millis();
    assert_eq!(first.state, State::Enriching);
    assert_eq!(first.totals.as_ref().unwrap().pending, FILE_LIMIT);
    assert!(first.more);
    assert_eq!(first.rows.len(), 500);
    let RowUpdate::Pending(row) = &first.rows[0] else {
        panic!("not pending")
    };
    assert_eq!(
        service
            .selected_content(&settings, &opened.id, started.generation, &row.id, "right")
            .await
            .unwrap()
            .bytes,
        b"new\n"
    );
    let content_ms = start.elapsed().as_millis();
    control.release.notify_one();
    let result = service.wait(&opened.id, started.generation).await.unwrap();
    let complete_ms = start.elapsed().as_millis();
    let RefreshResult::Ready { snapshot } = result else {
        panic!("not ready")
    };
    assert_eq!(snapshot.raw.different, FILE_LIMIT);
    let mut after = 0;
    let mut pending = std::collections::HashSet::new();
    let mut final_ids = std::collections::HashSet::new();
    let mut pages = 0;
    loop {
        let progress = service
            .progress(&opened.id, started.generation, after, 500)
            .unwrap();
        assert!(progress.rows.len() <= 500);
        assert!(
            serde_json::to_vec(&progress.rows).unwrap().len()
                <= crate::compare::progressive::PAGE_BYTES
        );
        assert_eq!(progress.totals.as_ref().unwrap().pending, 0);
        for row in &progress.rows {
            match row {
                RowUpdate::Pending(row) => {
                    assert!(pending.insert(row.id.clone()));
                }
                RowUpdate::Final(row) => {
                    assert!(pending.contains(&row.id));
                    assert!(final_ids.insert(row.id.clone()));
                }
            }
        }
        pages += 1;
        after = progress.sequence;
        if !progress.more {
            break;
        }
    }
    assert_eq!(after, (FILE_LIMIT * 2) as u64);
    assert_eq!(pending, final_ids);
    assert_eq!(pages, 80);
    let (prepared, _) = service
        .snapshot(&settings, &opened.id, started.generation)
        .await
        .unwrap();
    use sha2::Digest;
    let fingerprint = format!(
        "{:x}",
        sha2::Sha256::digest(serde_json::to_vec(&equivalence::result(&prepared)).unwrap())
    );
    println!("progressive trace: cache=empty prewarm=off rows={FILE_LIMIT} listed={listed_ms}ms content={content_ms}ms complete={complete_ms}ms pages={pages} fingerprint={fingerprint}");
    service.close(&opened.id).await;
    assert!(flight::idle());
    #[cfg(target_os = "linux")]
    assert!(git::runner_idle());
}
