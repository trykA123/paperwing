use super::{cache, Commit, Source};
use crate::store::{
    commits::{self, CommitSet, Key},
    Store,
};

pub(super) struct Request<'a> {
    pub source: &'a Source,
    pub org: &'a str,
    pub name: &'a str,
    pub branch: &'a str,
    pub revision: u64,
}

const REF_EPOCH: u64 = 0;

pub(super) fn remember(store: &Store, request: &Request, list: &[Commit]) {
    let Ok(scope) = cache::scope_configuration(request.source) else {
        return;
    };
    let set = CommitSet {
        source_id: request.source.id.clone(),
        scope,
        repository: format!("{}/{}", request.org, request.name),
        branch: request.branch.to_string(),
        ref_epoch: REF_EPOCH,
        fetched_at: cache::now(),
        commits: list.to_vec(),
    };
    let admission = store.clone();
    crate::credentials::if_current(&request.source.id, request.revision, || {
        store.post(move |connection| {
            if crate::store::providers::disabled_sources(&admission)?.contains(&set.source_id) {
                return Ok(());
            }
            commits::put(connection, &set)
        });
    });
}

#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn recall(store: &Store, request: &Request) -> Option<Vec<Commit>> {
    let scope = cache::scope_configuration(request.source).ok()?;
    let repository = format!("{}/{}", request.org, request.name);
    let key = Key {
        source_id: &request.source.id,
        repository: &repository,
        branch: request.branch,
        ref_epoch: REF_EPOCH,
    };
    let set = store
        .read_blocking(|connection| commits::get(connection, &key))
        .ok()??;
    (set.scope == scope && set.fetched_at <= cache::now()).then_some(set.commits)
}

#[cfg(test)]
mod tests;
