use super::compare::{immutable, Request};
use super::http::Error;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, Weak};
use tokio::sync::OnceCell;

mod disk;
#[cfg(test)]
mod tests;

pub(crate) const MAX_BYTES: usize = 5 * 1024 * 1024;
const CACHE_BYTES: u64 = 200 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct Blob {
    pub bytes: Option<Vec<u8>>,
    pub size: Option<u64>,
    pub binary: bool,
}

pub(crate) struct Cache {
    root: PathBuf,
    capacity: u64,
    flights: Mutex<HashMap<String, Weak<OnceCell<Blob>>>>,
}

impl Cache {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self {
            root: root.join("github-blobs"),
            capacity: CACHE_BYTES,
            flights: Mutex::default(),
        }
    }

    fn flight(&self, key: &str) -> Result<Arc<OnceCell<Blob>>, Error> {
        let mut flights = self.flights.lock().map_err(|_| cache_error())?;
        flights.retain(|_, entry| entry.strong_count() > 0);
        if let Some(flight) = flights.get(key).and_then(Weak::upgrade) {
            return Ok(flight);
        }
        let flight = Arc::new(OnceCell::new());
        flights.insert(key.into(), Arc::downgrade(&flight));
        Ok(flight)
    }

    pub(crate) async fn read(
        &self,
        request: &Request,
        sha: &str,
        size: Option<u64>,
    ) -> Result<Blob, Error> {
        crate::providers::ensure_enabled(&request.source).map_err(Error::Message)?;
        if !immutable(sha) {
            return Err(Error::Message("Invalid GitHub blob SHA".into()));
        }
        if size.is_some_and(|size| size > MAX_BYTES as u64) {
            return Ok(Blob {
                bytes: None,
                size,
                binary: false,
            });
        }
        let revision =
            crate::credentials::metadata_revision(&request.source).map_err(Error::Message)?;
        let key = key(request, sha, revision);
        let flight = self.flight(&key)?;
        let blob = flight
            .get_or_try_init(|| self.load(request, &key, sha, revision))
            .await?;
        crate::providers::ensure_enabled(&request.source).map_err(Error::Message)?;
        if crate::credentials::revision(&request.source.id) != revision {
            return Err(Error::Message(
                "Source credentials changed; refresh comparison".into(),
            ));
        }
        Ok(blob.clone())
    }

    async fn load(
        &self,
        request: &Request,
        key: &str,
        sha: &str,
        revision: u64,
    ) -> Result<Blob, Error> {
        let root = self.root.clone();
        let cache_key = key.to_string();
        let capacity = self.capacity;
        let source = request.source.clone();
        let cached = tauri::async_runtime::spawn_blocking(move || {
            crate::credentials::if_current(&source.id, revision, || {
                crate::providers::ensure_enabled(&source).map_err(Error::Message)?;
                disk::read(&root, &cache_key, capacity)
            })
            .ok_or_else(|| {
                Error::Message("Source credentials changed; refresh comparison".into())
            })?
        })
        .await
        .map_err(|_| cache_error())??;
        if let Some(blob) = cached {
            return Ok(blob);
        }
        let http = request.connect().await?;
        let path = format!("{}/git/blobs/{sha}", request.repository.api_path());
        let blob = http.raw_blob(&path).await?;
        self.save(request, key, revision, &blob).await?;
        Ok(blob)
    }

    async fn save(
        &self,
        request: &Request,
        key: &str,
        revision: u64,
        blob: &Blob,
    ) -> Result<(), Error> {
        let source = request.source.clone();
        let root = self.root.clone();
        let key = key.to_string();
        let capacity = self.capacity;
        let blob = blob.clone();
        tauri::async_runtime::spawn_blocking(move || {
            crate::credentials::if_current(&source.id, revision, || {
                crate::providers::ensure_enabled(&source).map_err(Error::Message)?;
                disk::write(&root, &key, &blob, capacity)
            })
            .ok_or_else(|| {
                Error::Message("Source credentials changed; refresh comparison".into())
            })?
        })
        .await
        .map_err(|_| cache_error())?
    }
}

fn key(request: &Request, sha: &str, revision: u64) -> String {
    let scope = serde_json::json!([
        request.source.id,
        request.repository.host.to_ascii_lowercase(),
        request.repository.owner.to_ascii_lowercase(),
        request.repository.name.to_ascii_lowercase(),
        revision,
        sha.to_ascii_lowercase()
    ])
    .to_string();
    format!("{:x}", Sha256::digest(scope.as_bytes()))
}

fn cache_error() -> Error {
    Error::Message("GitHub blob cache unavailable".into())
}
