use std::env::VarError;
use std::ffi::OsString;

fn resolve(name: &str, lookup: impl Fn(&str) -> Option<OsString>) -> Option<OsString> {
    lookup(name)
}

pub(crate) fn var_os(name: &str) -> Option<OsString> {
    resolve(name, |key| std::env::var_os(key))
}

pub(crate) fn var(name: &str) -> Result<String, VarError> {
    var_os(name).ok_or(VarError::NotPresent)?.into_string().map_err(VarError::NotUnicode)
}

#[cfg(test)]
mod tests {
    use super::resolve;
    use std::ffi::OsString;

    fn lookup(present: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<OsString> {
        move |key| present.iter().find(|(name, _)| *name == key).map(|(_, value)| value.into())
    }

    #[test]
    fn skein_name_is_read() {
        let found = resolve("SKEIN_TEST_TMP", lookup(&[("SKEIN_TEST_TMP", "value")]));
        assert_eq!(found, Some("value".into()));
    }

    #[test]
    fn unrelated_names_are_ignored() {
        let found = resolve("SKEIN_TEST_TMP", lookup(&[("OTHER_TEST_TMP", "value")]));
        assert_eq!(found, None);
    }

    #[test]
    fn missing_names_resolve_to_none() {
        assert_eq!(resolve("SKEIN_TEST_TMP", lookup(&[])), None);
    }
}
