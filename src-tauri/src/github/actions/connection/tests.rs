use super::super::transport::Request;
use super::*;
use crate::{
    github::{
        http::{Error, Response},
        provider::GithubProvider,
    },
    kernel::registry::{Registry, RegistryError},
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::sync::Notify;

#[derive(Clone, Default)]
struct Counts {
    transports: Arc<AtomicUsize>,
    requests: Arc<AtomicUsize>,
}

struct CountingConnection {
    counts: Counts,
    pause: Option<(Arc<Notify>, Arc<Notify>)>,
}

struct CountingTransport {
    counts: Counts,
    pause: Option<(Arc<Notify>, Arc<Notify>)>,
}

impl Connection for CountingConnection {
    type Transport<'a> = CountingTransport;

    fn connect<'a>(
        &'a self,
        source: &'a Source,
        host: &'a str,
    ) -> ProviderFuture<'a, Result<CountingTransport, CiError>> {
        Box::pin(async move {
            assert!(source.enabled);
            assert_eq!(host, "enterprise.invalid");
            self.counts.transports.fetch_add(1, Ordering::SeqCst);
            Ok(CountingTransport {
                counts: self.counts.clone(),
                pause: self.pause.clone(),
            })
        })
    }
}

impl Transport for CountingTransport {
    fn send<'a>(&'a self, request: Request<'a>) -> ProviderFuture<'a, Result<Response, Error>> {
        Box::pin(async move {
            if let Some((started, resume)) = &self.pause {
                started.notify_one();
                resume.notified().await;
            }
            self.counts.requests.fetch_add(1, Ordering::SeqCst);
            assert!(matches!(
                request,
                Request::Metadata {
                    path: "/repos/admin/repo/actions/runs?per_page=100&page=1",
                    etag: None
                }
            ));
            Ok(Response {
                status: 200,
                headers: Default::default(),
                body: br#"{"workflow_runs":[]}"#.to_vec(),
                next: false,
            })
        })
    }
}

fn source(id: &str, enabled: bool) -> Source {
    serde_json::from_value(serde_json::json!({"id":id,"name":"admin","kind":"ghe","host":"enterprise.invalid","enabled":enabled})).unwrap()
}

fn repo() -> Repository {
    Repository {
        host: "enterprise.invalid".into(),
        owner: "admin".into(),
        name: "repo".into(),
    }
}

#[tokio::test]
async fn disabled_ci_source_constructs_no_provider_transport_or_requests() {
    let source = source("ci-disabled-count-source", false);
    let repo = repo();
    let counts = Counts::default();
    let constructions = AtomicUsize::new(0);
    let registry = Registry::default();
    let config = crate::providers::configuration(&source, &repo.host).unwrap();
    let construct = |key: &crate::kernel::registry::ProviderKey| {
        constructions.fetch_add(1, Ordering::SeqCst);
        GithubProvider::new(key.clone(), None)
    };
    registry
        .configure(std::slice::from_ref(&config), construct)
        .unwrap();
    assert!(matches!(
        registry.acquire(&config, construct),
        Err(RegistryError::Disabled(_))
    ));
    let mut stale = source.clone();
    stale.enabled = true;
    let stale = crate::providers::configuration(&stale, &repo.host).unwrap();
    assert!(matches!(
        registry.acquire(&stale, construct),
        Err(RegistryError::Disabled(_))
    ));
    let connection = CountingConnection {
        counts: counts.clone(),
        pause: None,
    };
    let result = run(&connection, (&source, &repo), |provider| {
        Box::pin(async move { provider.runs(&CiQuery::default()).await })
    })
    .await;
    assert!(result.unwrap_err().to_string().contains("disabled"));
    assert_eq!(constructions.load(Ordering::SeqCst), 0);
    assert_eq!(counts.transports.load(Ordering::SeqCst), 0);
    assert_eq!(counts.requests.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn enabled_ci_source_constructs_one_transport_and_sends_one_foreground_request() {
    let source = source("ci-enabled-count-source", true);
    let repo = repo();
    let counts = Counts::default();
    let connection = CountingConnection {
        counts: counts.clone(),
        pause: None,
    };
    let result = run(&connection, (&source, &repo), |provider| {
        Box::pin(async move { provider.runs(&CiQuery::default()).await })
    })
    .await
    .unwrap();
    assert!(matches!(result, CiPage::Updated { items, .. } if items.is_empty()));
    assert_eq!(counts.transports.load(Ordering::SeqCst), 1);
    assert_eq!(counts.requests.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn disabling_a_ci_lease_stops_deferred_dispatch_before_any_http_request() {
    let source = source("ci-cancelled-count-source", true);
    let repo = repo();
    let counts = Counts::default();
    let started = Arc::new(Notify::new());
    let resume = Arc::new(Notify::new());
    let connection = CountingConnection {
        counts: counts.clone(),
        pause: Some((started.clone(), resume.clone())),
    };
    let registry = Registry::default();
    let config = crate::providers::configuration(&source, &repo.host).unwrap();
    let lease = registry
        .acquire(&config, |key| GithubProvider::new(key.clone(), None))
        .unwrap();
    let operation = lease.run(run(&connection, (&source, &repo), |provider| {
        Box::pin(async move { provider.runs(&CiQuery::default()).await })
    }));
    tokio::pin!(operation);
    tokio::select! {
        _ = started.notified() => {},
        _ = &mut operation => panic!("CI dispatch must remain deferred"),
    }
    let mut disabled = source.clone();
    disabled.enabled = false;
    let disabled = crate::providers::configuration(&disabled, &repo.host).unwrap();
    registry
        .configure(&[disabled], |_| {
            panic!("disabled provider must not be constructed")
        })
        .unwrap();
    assert!(matches!(operation.await, Err(RegistryError::Stopped)));
    resume.notify_one();
    assert_eq!(counts.transports.load(Ordering::SeqCst), 1);
    assert_eq!(counts.requests.load(Ordering::SeqCst), 0);
    assert!(matches!(
        registry.acquire(&config, |_| panic!(
            "stale admission must not construct a provider"
        )),
        Err(RegistryError::Disabled(_))
    ));
}
