use crate::kernel::capabilities::{CiProvider, ProviderFuture};

struct ExistingProvider;

impl CiProvider for ExistingProvider {
    type Request = String;
    type Run = String;
    type Error = String;

    fn start(&self, request: String) -> ProviderFuture<'_, Result<String, String>> {
        Box::pin(async move { Ok(request) })
    }

    fn cancel<'a>(&'a self, _run: &'a String) -> ProviderFuture<'a, Result<(), String>> {
        Box::pin(async { Ok(()) })
    }
}

#[tokio::test]
async fn existing_start_cancel_providers_keep_their_associated_types_and_methods() {
    let run = ExistingProvider
        .start("existing request".into())
        .await
        .unwrap();
    ExistingProvider.cancel(&run).await.unwrap();
    assert_eq!(run, "existing request");
}
