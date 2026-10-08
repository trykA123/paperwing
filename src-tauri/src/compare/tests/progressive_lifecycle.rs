use super::*;

async fn deferred(
    fixture: &Fixture,
    service: &Service,
) -> (Opened, Arc<publication::EnrichmentControl>) {
    let control = Arc::new(publication::EnrichmentControl {
        listed: Default::default(),
        release: Default::default(),
        panic: false,
        after_fixed: false,
    });
    *service.enrichment_control.lock().unwrap() = Some(control.clone());
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
    tokio::time::timeout(Duration::from_secs(10), control.listed.notified())
        .await
        .unwrap();
    (started, control)
}

#[tokio::test]
async fn ending_a_deferred_generation_stops_publication_and_releases_resources() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    for action in ["cancel", "close", "reload", "refresh"] {
        let fixture = Fixture::new().await;
        fixture.write("file", b"old\n");
        fixture.commit("base").await;
        fixture.write("file", b"new\n");
        let service = fixture.service();
        let (started, control) = deferred(&fixture, &service).await;
        match action {
            "cancel" => {
                service.cancel(&started.id).await;
            }
            "close" => {
                service.close(&started.id).await;
            }
            "reload" => service.release_sessions().await,
            "refresh" => {
                *service.enrichment_control.lock().unwrap() = None;
                let next = service
                    .start(
                        &fixture.settings(),
                        &started.id,
                        Options::default(),
                        None,
                        None,
                    )
                    .await
                    .unwrap();
                assert!(next.generation > started.generation);
                service.wait(&started.id, next.generation).await.unwrap();
            }
            _ => unreachable!(),
        }
        control.release.notify_one();
        assert_eq!(
            service
                .progress(&started.id, started.generation, 0, 500)
                .unwrap_err()
                .kind,
            "staleGeneration"
        );
        assert_eq!(service.slots.available_permits(), 4);
        service.close(&started.id).await;
        assert!(flight::idle());
        #[cfg(target_os = "linux")]
        assert!(git::runner_idle());
    }
}

#[tokio::test]
async fn a_panicked_producer_fails_without_a_snapshot_and_closes_readers() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    fixture.write("file", b"old\n");
    fixture.commit("base").await;
    let service = fixture.service();
    let control = Arc::new(publication::EnrichmentControl {
        listed: Default::default(),
        release: Default::default(),
        panic: true,
        after_fixed: false,
    });
    *service.enrichment_control.lock().unwrap() = Some(control);
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
    assert_eq!(
        service
            .wait(&opened.id, started.generation)
            .await
            .err()
            .unwrap()
            .kind,
        "internal"
    );
    let progress = service
        .progress(&opened.id, started.generation, 0, 500)
        .unwrap();
    assert_eq!(progress.state, State::Failed);
    assert!(progress.snapshot.is_none());
    assert!(progress.outcome.is_none());
    let RowUpdate::Pending(row) = &progress.rows[0] else {
        panic!("missing pending row")
    };
    assert!(service
        .selected_content(&settings, &opened.id, started.generation, &row.id, "right")
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
        .is_err());
    service.close(&opened.id).await;
    assert!(flight::idle());
    #[cfg(target_os = "linux")]
    assert!(git::runner_idle());
}

#[tokio::test]
async fn an_unconsumed_producer_completes_and_retains_bounded_pages() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    for index in 0..600 {
        fixture.write(&format!("file-{index:04}"), b"old\n");
    }
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
    let started = service
        .start(&settings, &opened.id, Options::default(), None, None)
        .await
        .unwrap();
    service.wait(&opened.id, started.generation).await.unwrap();
    let mut after = 0;
    loop {
        let progress = service
            .progress(&opened.id, started.generation, after, 500)
            .unwrap();
        assert!(progress.rows.len() <= 500);
        assert!(
            serde_json::to_vec(&progress.rows).unwrap().len()
                <= crate::compare::progressive::PAGE_BYTES
        );
        assert_eq!(progress.state, State::Complete);
        after = progress.sequence;
        if !progress.more {
            break;
        }
    }
    assert_eq!(after, 1200);
    service.close(&opened.id).await;
    assert!(flight::idle());
}

#[tokio::test]
async fn pending_copy_checks_destination_only_rows_and_changed_root_authority() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    fixture.write("folder/source", b"old\n");
    fixture.commit("base").await;
    fixture.write("folder/retained", b"retained\n");
    let service = fixture.service();
    let (started, _) = deferred(&fixture, &service).await;
    let progress = service
        .progress(&started.id, started.generation, 0, 500)
        .unwrap();
    let folder = progress
        .rows
        .iter()
        .find_map(|row| match row {
            RowUpdate::Pending(row) if row.path == "folder" => Some(row),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        service
            .copy_ids(
                &fixture.settings(),
                &started.id,
                started.generation,
                &folder.id,
                "left"
            )
            .await
            .unwrap_err(),
        "Wait until this folder finishes checking"
    );
    let mut changed = fixture.settings();
    changed.workspace["root"] = serde_json::json!(fixture.0.join("new-root"));
    assert!(service
        .available(&changed, &started.id, started.generation)
        .is_err());
    assert!(service
        .write_context(
            &changed,
            &started.id,
            started.generation,
            &folder.id,
            "right",
            true
        )
        .await
        .is_err());
    service.close(&started.id).await;
}

#[tokio::test]
async fn fixed_rows_authorize_fresh_writes_while_budget_rows_and_folders_wait() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    fixture.write("folder/a-budget", b"old\n");
    fixture.write("folder/z-fixed", b"same\n");
    fixture.commit("base").await;
    fixture.write("folder/a-budget", b"new\n");
    let service = fixture.service();
    let settings = fixture.settings();
    let control = Arc::new(publication::EnrichmentControl {
        listed: Default::default(),
        release: Default::default(),
        panic: false,
        after_fixed: true,
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
    let started = service
        .start(&settings, &opened.id, Options::default(), None, None)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), control.listed.notified())
        .await
        .unwrap();
    let progress = service
        .progress(&opened.id, started.generation, 0, 500)
        .unwrap();
    assert_eq!(progress.state, State::Enriching);
    let final_rows: Vec<_> = progress
        .rows
        .iter()
        .filter_map(|row| match row {
            RowUpdate::Final(row) => Some(row),
            _ => None,
        })
        .collect();
    assert_eq!(final_rows.len(), 1);
    assert_eq!(final_rows[0].path, "folder/z-fixed");
    assert!(service
        .write_context(
            &settings,
            &opened.id,
            started.generation,
            &final_rows[0].id,
            "right",
            true
        )
        .await
        .is_ok());
    let folder = progress
        .rows
        .iter()
        .find_map(|row| match row {
            RowUpdate::Pending(row) if row.path == "folder" => Some(row),
            _ => None,
        })
        .unwrap();
    assert!(service
        .copy_ids(
            &settings,
            &opened.id,
            started.generation,
            &folder.id,
            "right"
        )
        .await
        .unwrap_err()
        .contains("folder finishes checking"));
    control.release.notify_one();
    service.wait(&opened.id, started.generation).await.unwrap();
    let progress = service
        .progress(&opened.id, started.generation, 0, 500)
        .unwrap();
    let final_paths: Vec<_> = progress
        .rows
        .iter()
        .filter_map(|row| match row {
            RowUpdate::Final(row) => Some(row.path.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(final_paths, ["folder/z-fixed", "folder/a-budget", "folder"]);
    service.close(&opened.id).await;
}

#[tokio::test]
async fn content_memory_bounds_reserve_interactive_capacity_and_release_cancelled_waiters() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    let first = fixture.job();
    let second = fixture.job();
    let third = fixture.job();
    let first_row = flight::acquire(&first, true, flight::ROW_BYTES)
        .await
        .unwrap();
    let second_row = flight::acquire(&second, true, flight::ROW_BYTES)
        .await
        .unwrap();
    let mut queued = Box::pin(flight::acquire(&third, true, flight::ROW_BYTES));
    assert!(futures_util::FutureExt::now_or_never(&mut queued).is_none());
    let selected = flight::acquire(&first, false, 8 * 1024 * 1024)
        .await
        .unwrap();
    let mut over_comparison = Box::pin(flight::acquire(&first, false, 1));
    assert!(futures_util::FutureExt::now_or_never(&mut over_comparison).is_none());
    first.cancel.store(true, Ordering::Relaxed);
    assert_eq!(over_comparison.await.err().unwrap().kind, "cancelled");
    drop(queued);
    drop(selected);
    drop(first_row);
    drop(second_row);
    assert!(flight::idle());
}

#[test]
fn closing_during_producer_registration_joins_every_owned_generation() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap();
    let _guard = runtime.block_on(git::TEST_RUNNER_LOCK.lock());
    let fixture = runtime.block_on(Fixture::new());
    fixture.write("file", b"old\n");
    runtime.block_on(fixture.commit("base"));
    let service = Arc::new(fixture.service());
    let (started, _) = runtime.block_on(deferred(&fixture, &service));
    let control = Arc::new(producer::RegistrationControl {
        arrived: std::sync::Barrier::new(2),
        release: std::sync::Barrier::new(2),
    });
    *service.registration_control.lock().unwrap() = Some(control.clone());
    let returned_early = std::thread::scope(|scope| {
        let runtime = &runtime;
        let service = &service;
        let id = &started.id;
        let starting = scope.spawn(|| {
            runtime.block_on(service.start(
                &fixture.settings(),
                &started.id,
                Options::default(),
                None,
                None,
            ))
        });
        control.arrived.wait();
        let (entered, ready) = std::sync::mpsc::channel();
        let (closed, completed) = std::sync::mpsc::channel();
        let closing = scope.spawn(move || {
            entered.send(()).unwrap();
            let result = runtime.block_on(service.close(id));
            closed.send(result).unwrap();
        });
        ready.recv().unwrap();
        let early = completed.recv_timeout(Duration::from_millis(100));
        control.release.wait();
        starting.join().unwrap().unwrap();
        closing.join().unwrap();
        let returned_early = early.is_ok();
        assert!(early.unwrap_or_else(|_| completed.recv().unwrap()));
        returned_early
    });
    assert!(
        !returned_early,
        "close returned before producer ownership was registered"
    );
    assert_eq!(service.slots.available_permits(), 4);
    assert!(flight::idle());
    #[cfg(target_os = "linux")]
    assert!(git::runner_idle());
}
