use crate::kernel::registry::{Lease, ProviderConfig, ProviderKey, Registry};
use crate::{
    github::provider::GithubProvider,
    settings::Source,
    store::{providers, Store},
};
use std::sync::{Arc, Mutex, OnceLock};
mod refs;
pub(crate) use refs::refs;
use tauri::{AppHandle, Manager, Runtime as TauriRuntime};

static APPLICATION: OnceLock<Runtime> = OnceLock::new();

#[derive(Clone)]
pub(crate) struct Runtime {
    pub registry: Arc<Registry<GithubProvider>>,
    store: Store,
    sources: Arc<Mutex<Vec<Source>>>,
}

pub(crate) fn install<R: TauriRuntime>(app: &AppHandle<R>, store: Store) {
    let runtime = Runtime::new(store);
    let _ = APPLICATION.set(runtime.clone());
    app.manage(runtime);
}

pub(crate) fn configuration(source: &Source, host: &str) -> Result<ProviderConfig, String> {
    Ok(ProviderConfig {
        key: ProviderKey {
            source_id: source.id.clone(),
            host: host.to_ascii_lowercase(),
        },
        enabled: source.enabled,
        configuration: serde_json::to_string(&(
            &source.kind,
            &source.host,
            &source.orgs,
            &source.urls,
        ))
        .map_err(|error| error.to_string())?,
    })
}

impl Runtime {
    pub(crate) fn new(store: Store) -> Self {
        Self {
            registry: Arc::new(Registry::default()),
            store,
            sources: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub(crate) fn configure(&self, sources: &[Source]) -> Result<(), String> {
        let disabled = providers::configure(
            &self.store,
            sources
                .iter()
                .map(|source| (source.id.clone(), source.enabled))
                .collect(),
        )
        .map_err(|error| format!("Could not configure provider cache admission: {error}"))?;
        let configs = sources
            .iter()
            .map(|source| configuration(source, &source.host))
            .collect::<Result<Vec<_>, _>>()?;
        self.registry
            .configure(&configs, |key| {
                GithubProvider::new(key.clone(), Some(self.store.clone()))
            })
            .map_err(|error| error.to_string())?;
        *self
            .sources
            .lock()
            .map_err(|_| "Provider sources are unavailable")? = sources.to_vec();
        self.purge_disabled(disabled, sources);
        Ok(())
    }

    fn purge_disabled(&self, disabled: Vec<String>, sources: &[Source]) {
        if !disabled.is_empty() {
            let store = self.store.clone();
            let urls = sources
                .iter()
                .flat_map(|source| {
                    source
                        .urls
                        .iter()
                        .map(|url| (source.id.clone(), url.clone()))
                })
                .collect();
            tauri::async_runtime::spawn_blocking(move || {
                if let Err(error) = providers::purge(&store, disabled, urls) {
                    eprintln!("Disabled provider cache cleanup failed: {error}");
                }
            });
        }
    }

    fn acquire(&self, source: &Source, host: &str) -> Result<Lease<GithubProvider>, String> {
        self.registry
            .acquire(&configuration(source, host)?, |key| {
                GithubProvider::new(key.clone(), Some(self.store.clone()))
            })
            .map_err(|error| error.to_string())
    }
}

pub(crate) fn acquire(
    source: &Source,
    host: &str,
    store: Option<Store>,
) -> Result<Lease<GithubProvider>, String> {
    crate::settings::valid_id(&source.id)?;
    if let Some(runtime) = APPLICATION.get() {
        return runtime.acquire(source, host);
    }
    static STANDALONE: OnceLock<Registry<GithubProvider>> = OnceLock::new();
    STANDALONE
        .get_or_init(Registry::default)
        .acquire(&configuration(source, host)?, |key| {
            GithubProvider::new(key.clone(), store)
        })
        .map_err(|error| error.to_string())
}

pub(crate) fn ensure_enabled(source: &Source) -> Result<(), String> {
    if !is_enabled(source)? {
        return Err(format!(
            "Source {} is disabled; enable it in Settings",
            source.id
        ));
    }
    Ok(())
}

pub(crate) fn is_enabled(source: &Source) -> Result<bool, String> {
    if !source.enabled {
        return Ok(false);
    }
    if let Some(runtime) = APPLICATION.get() {
        return match runtime.registry.check_enabled(&source.id) {
            Ok(()) => Ok(true),
            Err(crate::kernel::registry::RegistryError::Disabled(_)) => Ok(false),
            Err(error) => Err(error.to_string()),
        };
    }
    Ok(true)
}
