use super::git::run;
use crate::git::{execute_streaming, OutputPolicy, Request};
use sha2::{Digest, Sha256};
use std::sync::{atomic::AtomicBool, Arc, Mutex};
use std::time::Duration;

pub(super) async fn snapshot(path: &str) -> Result<(Vec<u8>, [u8; 32]), String> {
    let context = format!("Stash state: {path}");
    let index = run(
        path,
        &["write-tree"],
        &context,
        &[0],
        OutputPolicy::Metadata,
        Duration::from_secs(45),
    )
    .await?;
    Ok((index.stdout, status_hash(path, &context).await?))
}

async fn status_hash(path: &str, context: &str) -> Result<[u8; 32], String> {
    let digest = Arc::new(Mutex::new(Sha256::new()));
    let streamed = digest.clone();
    let output = execute_streaming(
        Request {
            args: &[
                "-C",
                path,
                "-c",
                "core.fsmonitor=false",
                "-c",
                "core.quotepath=false",
                "status",
                "--porcelain=v2",
                "-z",
                "--untracked-files=all",
                "--ignored=matching",
            ],
            context,
            expected: &[0],
            policy: OutputPolicy::Metadata,
            timeout: Duration::from_secs(45),
        },
        Arc::new(AtomicBool::new(false)),
        Arc::new(move |bytes| {
            streamed
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .update(bytes);
            false
        }),
    )
    .await?;
    if output.code != Some(0) {
        return Err(output.last_error());
    }
    let hash = digest
        .lock()
        .map_err(|_| "Stash snapshot is unavailable")?
        .clone()
        .finalize()
        .into();
    Ok(hash)
}
