use std::cell::RefCell;

thread_local! {
    static RAW_TOKEN: RefCell<Option<(String, String)>> = const { RefCell::new(None) };
}

pub(super) struct RawToken;

impl RawToken {
    pub(super) fn new(source_id: &str, token: &str) -> Self {
        RAW_TOKEN.with_borrow_mut(|raw| {
            assert!(raw.replace((source_id.into(), token.into())).is_none());
        });
        Self
    }
}

impl Drop for RawToken {
    fn drop(&mut self) {
        RAW_TOKEN.with_borrow_mut(|raw| *raw = None);
    }
}

pub(super) fn read(source_id: &str) -> Option<String> {
    RAW_TOKEN.with_borrow(|raw| {
        raw.as_ref()
            .filter(|(id, _)| id == source_id)
            .map(|(_, token)| token.clone())
    })
}
