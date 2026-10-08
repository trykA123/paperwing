use super::{Connection, Http};
use crate::settings::Source;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

fn endpoints() -> &'static Mutex<HashMap<String, String>> {
    static ENDPOINTS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    ENDPOINTS.get_or_init(Mutex::default)
}

pub(crate) fn endpoint(source_id: &str) -> Option<String> {
    endpoints().lock().unwrap().get(source_id).cloned()
}

pub(crate) struct Binding(String);

impl Binding {
    pub(crate) fn new(source_id: &str, base: &str) -> Self {
        let url = reqwest::Url::parse(base).unwrap();
        assert_eq!(url.host_str(), Some("127.0.0.1"));
        assert!((42080..=42099).contains(&url.port().unwrap()));
        assert!(endpoints()
            .lock()
            .unwrap()
            .insert(source_id.into(), base.into())
            .is_none());
        Self(source_id.into())
    }
}

impl Drop for Binding {
    fn drop(&mut self) {
        endpoints().lock().unwrap().remove(&self.0);
    }
}

impl<'a> Http<'a> {
    pub(super) async fn fixture(
        source: &'a Source,
        base: String,
        expected: u64,
    ) -> Result<Self, (u16, String)> {
        Self::with_token(
            Connection {
                source,
                base,
                host: &source.host,
                expected,
            },
            async { Ok(None) },
            || crate::credentials::revision(&source.id),
        )
        .await
    }
}
