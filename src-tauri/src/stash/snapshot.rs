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
    let args = [
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
    ];
    let output = execute_streaming(
        status_request(&args, context),
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

fn status_request<'a>(args: &'a [&'a str], context: &'a str) -> Request<'a> {
    Request {
        args,
        context,
        expected: &[0],
        policy: OutputPolicy::Metadata,
        timeout: Duration::from_secs(120),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_status_allows_the_same_two_minutes_as_stash_apply_regression() {
        let request = status_request(&["status"], "Stash state");
        assert_eq!(request.timeout, Duration::from_secs(120));
        assert_eq!(request.expected, &[0]);
        assert!(matches!(request.policy, OutputPolicy::Metadata));
    }
}
