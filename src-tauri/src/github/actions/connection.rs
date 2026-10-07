use super::{client::Client, transport::Transport};
use crate::{
    github::{
        http::Http,
        pulls::{self, repository::Repository},
    },
    kernel::{
        capabilities::{CiProvider, ProviderFuture},
        ci::*,
    },
    settings::Source,
};

type Provider<'a> =
    dyn CiProvider<Request = CiDispatch, Run = CiActionResult, Error = CiError> + 'a;

pub(super) trait Connection {
    type Transport<'a>: Transport
    where
        Self: 'a;

    fn connect<'a>(
        &'a self,
        source: &'a Source,
        host: &'a str,
    ) -> ProviderFuture<'a, Result<Self::Transport<'a>, CiError>>;
}

pub(super) struct StoredConnection;

impl Connection for StoredConnection {
    type Transport<'a> = Http<'a>;

    fn connect<'a>(
        &'a self,
        source: &'a Source,
        host: &'a str,
    ) -> ProviderFuture<'a, Result<Http<'a>, CiError>> {
        Box::pin(async move {
            pulls::connect(source, host)
                .await
                .map_err(|error| CiError::message(crate::git::safe(&error.to_string())))
        })
    }
}

pub(super) async fn run<T>(
    connection: &impl Connection,
    target: (&Source, &Repository),
    operation: impl for<'a> FnOnce(&'a Provider<'a>) -> ProviderFuture<'a, Result<T, CiError>>,
) -> Result<T, CiError> {
    let (source, repo) = target;
    let lease = crate::providers::acquire(source, &repo.host, None).map_err(CiError::message)?;
    lease
        .run(async {
            let transport = connection.connect(source, &repo.host).await?;
            let client = Client {
                transport: &transport,
                repo,
            };
            operation(&client).await
        })
        .await
        .map_err(|error| CiError::message(error.to_string()))?
}

#[cfg(test)]
mod tests;
