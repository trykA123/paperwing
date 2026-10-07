use super::*;
use crate::core::capabilities::RepositoryProvider;
use crate::git::{remote_refs, RefsResult};

fn configured_source(url: &str) -> Result<Option<Source>, String> {
    let Some(runtime) = APPLICATION.get() else {
        return Ok(None);
    };
    let sources = runtime
        .sources
        .lock()
        .map_err(|_| "Provider sources are unavailable")?;
    let host = crate::github::pulls::repository::remote_host(url).ok();
    let matches = |source: &&Source| {
        source.urls.iter().any(|saved| saved == url)
            || source.kind != "manual"
                && host
                    .as_ref()
                    .is_some_and(|host| source.host.eq_ignore_ascii_case(host))
    };
    let matching: Vec<_> = sources.iter().filter(matches).collect();
    Ok(matching
        .iter()
        .find(|source| source.enabled)
        .or_else(|| matching.first())
        .map(|source| (*source).clone()))
}

async fn refs_for_url(url: String) -> RefsResult {
    let source = match configured_source(&url) {
        Ok(Some(source)) => source,
        Ok(None) => return remote_refs::get_refs_many(vec![url]).await.remove(0),
        Err(error) => return remote_refs::failed_refs(url, error),
    };
    let host =
        crate::github::pulls::repository::remote_host(&url).unwrap_or_else(|_| source.host.clone());
    let lease = match acquire(&source, &host, None) {
        Ok(lease) => lease,
        Err(error) => return remote_refs::failed_refs(url, error),
    };
    match lease.run(lease.provider.refs(vec![url.clone()])).await {
        Ok(mut results) => results.remove(0),
        Err(error) => remote_refs::failed_refs(url, error.to_string()),
    }
}

pub(crate) async fn refs(urls: Vec<String>) -> Vec<RefsResult> {
    let names = urls.clone();
    crate::ordered::map_bounded(urls, 8, refs_for_url, move |index| {
        remote_refs::failed_refs(
            names[index].clone(),
            "Remote refs check did not finish".into(),
        )
    })
    .await
}
