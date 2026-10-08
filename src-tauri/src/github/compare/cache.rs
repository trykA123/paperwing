use super::{fetch_resolved, resolve, Comparison, Error, Request};
use crate::settings::Source;
use crate::store::{comparisons, Store};
use sha2::{Digest, Sha256};

#[derive(Clone)]
struct Admission {
    source: Source,
    revision: u64,
    access: comparisons::Access,
}

impl Admission {
    fn new(request: &Request) -> Result<Self, Error> {
        let source = &request.source;
        let revision = crate::credentials::metadata_revision(source).map_err(Error::Message)?;
        let configuration = serde_json::to_vec(&(source, revision))
            .map_err(|_| Error::Message("GitHub comparison cache scope is unavailable".into()))?;
        Ok(Self {
            source: source.clone(),
            revision,
            access: comparisons::Access {
                source_id: source.id.clone(),
                scope: format!("{:x}", Sha256::digest(configuration)),
            },
        })
    }

    fn check(&self, store: &Store) -> Result<(), Error> {
        crate::providers::ensure_enabled(&self.source).map_err(Error::Message)?;
        if crate::store::providers::disabled_sources(store)
            .map_err(|_| Error::Message("Provider cache admission is unavailable".into()))?
            .contains(&self.source.id)
        {
            return Err(Error::Message(
                "GitHub source is disabled; enable it in Settings".into(),
            ));
        }
        if crate::credentials::metadata_revision(&self.source).map_err(Error::Message)?
            != self.revision
        {
            return Err(Error::Message(
                "Source credentials changed; refresh comparison".into(),
            ));
        }
        Ok(())
    }
}

pub(crate) async fn load(request: &Request, store: &Store) -> Result<Comparison, Error> {
    let admission = Admission::new(request)?;
    admission.check(store)?;
    let http = request.connect().await?;
    let key = comparisons::Key {
        host: request.repository.host.to_ascii_lowercase(),
        owner: request.repository.owner.to_ascii_lowercase(),
        repository: request.repository.name.to_ascii_lowercase(),
        base: resolve(&http, &request.repository, &request.base).await?,
        head: resolve(&http, &request.repository, &request.head).await?,
    };
    if let Some(data) = recall(store, &key, &admission).await? {
        admission.check(store)?;
        return Ok(data);
    }
    let data = fetch_resolved(
        &http,
        &request.repository,
        key.base.clone(),
        key.head.clone(),
    )
    .await?;
    admission.check(store)?;
    remember(store, key, &admission, &data).await;
    admission.check(store)?;
    Ok(data)
}

async fn recall(
    store: &Store,
    key: &comparisons::Key,
    admission: &Admission,
) -> Result<Option<Comparison>, Error> {
    let store = store.clone();
    let key = key.clone();
    let admission = admission.clone();
    tauri::async_runtime::spawn_blocking(move || {
        admission.check(&store)?;
        let body = store
            .read_blocking(|connection| comparisons::get(connection, &key, &admission.access))
            .ok()
            .flatten();
        admission.check(&store)?;
        Ok(body
            .and_then(|body| serde_json::from_str::<Comparison>(&body).ok())
            .filter(|data| {
                data.base == key.base
                    && data.head == key.head
                    && super::mapping::validate_cached(data).is_ok()
            }))
    })
    .await
    .map_err(|_| Error::Message("GitHub comparison cache read task failed".into()))?
}

async fn remember(store: &Store, key: comparisons::Key, admission: &Admission, data: &Comparison) {
    let Ok(body) = serde_json::to_string(data) else {
        return;
    };
    let entry = comparisons::Entry {
        key,
        body,
        fetched_at: crate::github::cache::now(),
    };
    let store = store.clone();
    let admission = admission.clone();
    let queued = tauri::async_runtime::spawn_blocking(move || {
        store.read_blocking(|_| Ok(()))?;
        admission
            .check(&store)
            .map_err(|_| crate::store::Error::Unavailable)?;
        let guard = store.clone();
        crate::credentials::if_current(&admission.source.id.clone(), admission.revision, || {
            store.enqueue(move |connection| {
                if crate::providers::ensure_enabled(&admission.source).is_err() {
                    return Ok(());
                }
                let disabled = crate::store::providers::disabled_sources(&guard)?;
                if disabled.contains(&admission.source.id)
                    || crate::credentials::revision(&admission.source.id) != admission.revision
                {
                    return Ok(());
                }
                comparisons::put(connection, &entry, &admission.access)
            })
        })
        .unwrap_or(Err(crate::store::Error::Unavailable))
    })
    .await;
    if let Ok(Ok(receiver)) = queued {
        let _ = receiver.await;
    }
}

#[cfg(test)]
mod locking_tests;
