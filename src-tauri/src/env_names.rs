use std::env::VarError;
use std::ffi::OsString;

fn legacy(name: &str) -> String {
    format!("PAPERWING_{}", name.strip_prefix("SKEIN_").unwrap_or(name))
}

fn resolve(name: &str, lookup: impl Fn(&str) -> Option<OsString>) -> Option<OsString> {
    lookup(name).or_else(|| lookup(&legacy(name)))
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
    fn skein_name_wins_over_legacy_name() {
        let found = resolve("SKEIN_TEST_TMP", lookup(&[("SKEIN_TEST_TMP", "new"), ("PAPERWING_TEST_TMP", "old")]));
        assert_eq!(found, Some("new".into()));
    }

    #[test]
    fn legacy_name_is_read_when_skein_name_is_absent() {
        let found = resolve("SKEIN_TEST_TMP", lookup(&[("PAPERWING_TEST_TMP", "old")]));
        assert_eq!(found, Some("old".into()));
    }

    #[test]
    fn missing_names_resolve_to_none() {
        assert_eq!(resolve("SKEIN_TEST_TMP", lookup(&[])), None);
    }
}
