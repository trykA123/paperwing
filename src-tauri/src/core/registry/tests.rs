use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

fn config(id: &str, host: &str, enabled: bool) -> ProviderConfig {
    ProviderConfig {
        key: ProviderKey {
            source_id: id.into(),
            host: host.into(),
        },
        enabled,
        configuration: host.into(),
    }
}

#[test]
fn disabled_sources_are_never_constructed_and_cannot_bypass_the_registry() {
    let registry = Registry::default();
    let constructed = AtomicUsize::new(0);
    let factory = |_: &ProviderKey| {
        constructed.fetch_add(1, Ordering::SeqCst);
    };
    let off = config("off", "enterprise.invalid", false);
    registry
        .configure(std::slice::from_ref(&off), factory)
        .unwrap();
    assert_eq!(constructed.load(Ordering::SeqCst), 0);
    let mut forged = off;
    forged.enabled = true;
    assert!(registry.acquire(&forged, factory).is_err());
    assert_eq!(constructed.load(Ordering::SeqCst), 0);
}

#[test]
fn each_configured_host_owns_one_instance_and_unchanged_settings_keep_it() {
    let registry = Registry::default();
    let constructed = AtomicUsize::new(0);
    let factory = |key: &ProviderKey| {
        constructed.fetch_add(1, Ordering::SeqCst);
        key.host.clone()
    };
    let configs = [
        config("cloud", "github.com", true),
        config("one", "one.invalid", true),
        config("two", "two.invalid", true),
    ];
    registry.configure(&configs, factory).unwrap();
    let first = registry.acquire(&configs[0], factory).unwrap();
    registry.configure(&configs, factory).unwrap();
    let again = registry.acquire(&configs[0], factory).unwrap();
    assert!(Arc::ptr_eq(&first.provider, &again.provider));
    assert_eq!(constructed.load(Ordering::SeqCst), 3);
    for config in configs {
        assert_eq!(
            *registry.acquire(&config, factory).unwrap().provider,
            config.key.host
        );
    }
}

#[tokio::test]
async fn disabling_cancels_in_flight_work_and_reenabling_constructs_a_fresh_instance() {
    let registry = Registry::default();
    let constructed = AtomicUsize::new(0);
    let factory = |_: &ProviderKey| constructed.fetch_add(1, Ordering::SeqCst);
    let mut source = config("source", "enterprise.invalid", true);
    registry
        .configure(std::slice::from_ref(&source), factory)
        .unwrap();
    let lease = registry.acquire(&source, factory).unwrap();
    let (started, ready) = tokio::sync::oneshot::channel();
    let running = tokio::spawn(async move {
        lease
            .run(async {
                started.send(()).unwrap();
                std::future::pending::<()>().await
            })
            .await
    });
    ready.await.unwrap();
    source.enabled = false;
    registry
        .configure(std::slice::from_ref(&source), factory)
        .unwrap();
    assert!(matches!(
        running.await.unwrap(),
        Err(RegistryError::Stopped)
    ));
    source.enabled = true;
    registry
        .configure(std::slice::from_ref(&source), factory)
        .unwrap();
    assert_eq!(*registry.acquire(&source, factory).unwrap().provider, 1);
}
