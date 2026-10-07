use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, Mutex},
};
use tokio::sync::watch;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProviderKey {
    pub source_id: String,
    pub host: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderConfig {
    pub key: ProviderKey,
    pub enabled: bool,
    pub configuration: String,
}

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("Provider registry is unavailable")]
    Unavailable,
    #[error("Source {0} is disabled; enable it in Settings")]
    Disabled(String),
    #[error("Source provider stopped; retry the request")]
    Stopped,
}

struct Entry<P> {
    configuration: String,
    provider: Arc<P>,
    stop: watch::Sender<bool>,
}

impl<P> Drop for Entry<P> {
    fn drop(&mut self) {
        self.stop.send_replace(true);
    }
}

pub struct Lease<P> {
    pub provider: Arc<P>,
    stop: watch::Receiver<bool>,
}

impl<P> Lease<P> {
    pub async fn run<T>(&self, work: impl Future<Output = T>) -> Result<T, RegistryError> {
        let mut stop = self.stop.clone();
        if *stop.borrow() {
            return Err(RegistryError::Stopped);
        }
        tokio::select! {
            biased;
            _ = stop.changed() => Err(RegistryError::Stopped),
            result = work => Ok(result),
        }
    }
}

struct State<P> {
    enabled: HashMap<String, bool>,
    entries: HashMap<ProviderKey, Entry<P>>,
}

pub struct Registry<P>(Mutex<State<P>>);

impl<P> Default for Registry<P> {
    fn default() -> Self {
        Self(Mutex::new(State {
            enabled: HashMap::new(),
            entries: HashMap::new(),
        }))
    }
}

impl<P> Registry<P> {
    pub fn check_enabled(&self, source_id: &str) -> Result<(), RegistryError> {
        let state = self.0.lock().map_err(|_| RegistryError::Unavailable)?;
        if state.enabled.get(source_id) == Some(&false) {
            return Err(RegistryError::Disabled(source_id.into()));
        }
        Ok(())
    }

    pub fn configure(
        &self,
        configs: &[ProviderConfig],
        construct: impl Fn(&ProviderKey) -> P,
    ) -> Result<(), RegistryError> {
        let mut state = self.0.lock().map_err(|_| RegistryError::Unavailable)?;
        state.entries.retain(|key, entry| {
            configs.iter().any(|config| {
                config.enabled
                    && config.key.source_id == key.source_id
                    && config.configuration == entry.configuration
            })
        });
        state.enabled = configs
            .iter()
            .map(|config| (config.key.source_id.clone(), config.enabled))
            .collect();
        for config in configs.iter().filter(|config| config.enabled) {
            state
                .entries
                .entry(config.key.clone())
                .or_insert_with(|| entry(config, construct(&config.key)));
        }
        Ok(())
    }

    pub fn acquire(
        &self,
        config: &ProviderConfig,
        construct: impl FnOnce(&ProviderKey) -> P,
    ) -> Result<Lease<P>, RegistryError> {
        let mut state = self.0.lock().map_err(|_| RegistryError::Unavailable)?;
        if !config.enabled || state.enabled.get(&config.key.source_id) == Some(&false) {
            return Err(RegistryError::Disabled(config.key.source_id.clone()));
        }
        let entry = state
            .entries
            .entry(config.key.clone())
            .or_insert_with(|| entry(config, construct(&config.key)));
        Ok(Lease {
            provider: entry.provider.clone(),
            stop: entry.stop.subscribe(),
        })
    }
}

fn entry<P>(config: &ProviderConfig, provider: P) -> Entry<P> {
    Entry {
        configuration: config.configuration.clone(),
        provider: Arc::new(provider),
        stop: watch::channel(false).0,
    }
}

#[cfg(test)]
mod tests;
