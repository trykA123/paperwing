use super::*;

#[test]
fn metadata_rejects_deleted_or_edited_source_snapshots_before_credentials() {
    let source: crate::settings::Source = serde_json::from_value(serde_json::json!({
        "id":"metadata-fixture", "name":"admin", "kind":"github", "host":"github.com", "orgs":["admin"]
    })).unwrap();
    let configured = HashMap::from([(source.id.clone(), source_configuration(&source))]);
    assert!(check_configuration(&source, Some(&configured)).is_ok());
    assert!(check_configuration(&source, Some(&HashMap::new())).is_err());
    let edits = [
        crate::settings::Source {
            host: "enterprise.invalid".into(),
            ..source.clone()
        },
        crate::settings::Source {
            orgs: vec!["another-owner".into()],
            ..source.clone()
        },
        crate::settings::Source {
            urls: vec!["git@github.com:admin/another.git".into()],
            ..source.clone()
        },
        crate::settings::Source {
            credential_managed: true,
            ..source.clone()
        },
    ];
    for edited in edits {
        assert!(check_configuration(&edited, Some(&configured)).is_err());
    }
}

#[tokio::test]
async fn credential_invalidation_does_not_block_the_async_runtime_while_the_store_opens_regression() {
    let store = crate::store::Store::pending();
    let started = std::time::Instant::now();
    let invalidating = invalidate_with("pending-invalidation-regression".into(), move || {
        store.remove_source("pending-invalidation-regression");
    });
    let (_, responsive) = tokio::join!(invalidating, async {
        tokio::task::yield_now().await;
        started.elapsed() < std::time::Duration::from_millis(250)
    });
    assert!(responsive, "store opening blocked the async runtime");
}
