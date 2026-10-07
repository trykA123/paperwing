use crate::github::http::{Error, Http, Response};
use crate::kernel::capabilities::ProviderFuture;
use reqwest::Method;
use serde_json::Value;

pub(super) enum Request<'a> {
    Metadata {
        path: &'a str,
        etag: Option<&'a str>,
    },
    Write {
        path: &'a str,
        body: Option<Value>,
    },
    Raw {
        path: &'a str,
    },
    Download {
        path: &'a str,
    },
}

pub(super) trait Transport: Send + Sync {
    fn send<'a>(&'a self, request: Request<'a>) -> ProviderFuture<'a, Result<Response, Error>>;
}

impl Transport for Http<'_> {
    fn send<'a>(&'a self, request: Request<'a>) -> ProviderFuture<'a, Result<Response, Error>> {
        Box::pin(async move {
            match request {
                Request::Metadata { path, etag } => self.conditional(path, etag).await,
                Request::Write { path, body } => self.send(Method::POST, path, body).await,
                Request::Raw { path } => self.raw(path).await,
                Request::Download { path } => self.download(path).await,
            }
        })
    }
}
